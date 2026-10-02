//! Durable one-shot supervision for a public Setup export command.
//!
//! The private intent is written before the helper starts. Once started, the helper owns the
//! bounded coordinator invocation and its terminal record independently of the native window.

use std::{
    fs,
    io::{Read as _, Write as _},
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    backend::{BackendError, CoordinatorBinary},
    project::hex,
    state_dir::{StateError, ensure_private_dir, project_private_dir, read_private, write_private},
};

const INTENT_NAME: &str = "setup-export-intent.json";
const LOCK_NAME: &str = "setup-export-intent.lock";
const MAX_HELPER_INPUT: u64 = 64 * 1024;
const MAX_RECEIPT_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportReceipt {
    schema_version: u64,
    repository_id: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

/// Immutable, private record of one user-requested export.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportIntent {
    /// Record format.
    pub schema_version: u16,
    /// Random correlation identity.
    pub request_id: String,
    /// Exact opened configuration.
    pub config_path: PathBuf,
    /// Observed repository identity.
    pub repository_id: String,
    /// Selected source at submission.
    pub source: PathBuf,
    /// Destination at submission.
    pub destination: PathBuf,
    /// Exact public coordinator command.
    pub arguments: Vec<String>,
}

impl ExportIntent {
    pub(crate) fn fingerprint(&self) -> Result<String, StateError> {
        let bytes = serde_json::to_vec(self).map_err(|_| StateError::Invalid)?;
        Ok(hex(&Sha256::digest(bytes)))
    }

    fn result_path(&self, intent_path: &Path) -> Result<PathBuf, StateError> {
        if !valid_id(&self.request_id) {
            return Err(StateError::Invalid);
        }
        Ok(intent_path
            .parent()
            .ok_or(StateError::Invalid)?
            .join(format!("setup-export-result-{}.json", self.request_id)))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HelperRequest {
    schema_version: u16,
    intent_path: PathBuf,
    intent_sha256: String,
    binary_path: PathBuf,
    deadline_ms: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TerminalRecord {
    schema_version: u16,
    request_id: String,
    repository_id: String,
    result: TerminalResult,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "status", deny_unknown_fields)]
enum TerminalResult {
    Succeeded { receipt: Vec<u8> },
    Failed { code: String },
}

fn valid_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn intent_lock(path: &Path) -> Result<fs::File, StateError> {
    let parent = path.parent().ok_or(StateError::Invalid)?;
    ensure_private_dir(parent)?;
    let descriptor = open(
        &parent.join(LOCK_NAME),
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|_| StateError::Unavailable)?;
    let file = fs::File::from(descriptor);
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(StateError::Invalid);
    }
    file.try_lock().map_err(|_| StateError::Unavailable)?;
    Ok(file)
}

/// Private intent location for one opened configuration.
pub(crate) fn intent_path(directory: &Path, config: &Path) -> PathBuf {
    project_private_dir(directory, config).join(INTENT_NAME)
}

/// Creates one durable intent without replacing an unresolved earlier one.
///
/// # Errors
///
/// Refuses missing private state, existing intent or a failed durable write.
pub(crate) fn prepare(
    directory: &Path,
    config: &Path,
    repository_id: &str,
    source: &Path,
    destination: &Path,
    arguments: Vec<String>,
) -> Result<ExportIntent, StateError> {
    if !config.is_absolute() || !source.is_absolute() || !destination.is_absolute() {
        return Err(StateError::Invalid);
    }
    let mut random = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_| StateError::Unavailable)?;
    let intent = ExportIntent {
        schema_version: 1,
        request_id: hex(&random),
        config_path: config.to_path_buf(),
        repository_id: repository_id.to_owned(),
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        arguments,
    };
    let path = intent_path(directory, config);
    let bytes = serde_json::to_vec(&intent).map_err(|_| StateError::Invalid)?;
    ensure_private_dir(path.parent().ok_or(StateError::Invalid)?)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StateError::Invalid
            } else {
                StateError::Unavailable
            }
        })?;
    if file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        return Err(StateError::Unavailable);
    }
    fs::File::open(path.parent().ok_or(StateError::Invalid)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)?;
    Ok(intent)
}

/// Loads an unresolved intent for the exact configuration, if one exists.
///
/// # Errors
///
/// Refuses a malformed or linked private record.
pub(crate) fn load(directory: &Path, config: &Path) -> Result<Option<ExportIntent>, StateError> {
    let path = intent_path(directory, config);
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StateError::Unavailable),
    }
    let intent = load_at(&path)?;
    if intent.config_path != config {
        return Err(StateError::Invalid);
    }
    Ok(Some(intent))
}

fn load_at(path: &Path) -> Result<ExportIntent, StateError> {
    if path.file_name().is_none_or(|name| name != INTENT_NAME) {
        return Err(StateError::Invalid);
    }
    let bytes = read_private(path)?;
    parse_intent(&bytes)
}

fn load_matching_at(path: &Path, digest: &str) -> Result<ExportIntent, StateError> {
    if path.file_name().is_none_or(|name| name != INTENT_NAME) {
        return Err(StateError::Invalid);
    }
    let bytes = read_private(path)?;
    if digest.len() != 64 || hex(&Sha256::digest(&bytes)) != digest {
        return Err(StateError::Invalid);
    }
    parse_intent(&bytes)
}

fn parse_intent(bytes: &[u8]) -> Result<ExportIntent, StateError> {
    let intent: ExportIntent = serde_json::from_slice(bytes).map_err(|_| StateError::Invalid)?;
    if intent.schema_version != 1
        || !valid_id(&intent.request_id)
        || !intent.config_path.is_absolute()
        || !intent.source.is_absolute()
        || !intent.destination.is_absolute()
        || intent
            .arguments
            .first()
            .is_none_or(|arg| arg != "setup-export-copy")
    {
        return Err(StateError::Invalid);
    }
    Ok(intent)
}

/// Removes a completed or definitely unqueued intent and its terminal record only when its ID
/// still matches. The result is removed first so an interrupted clear retains the intent and
/// cannot leave an untracked result behind.
///
/// # Errors
///
/// Refuses a changed or unreadable private record.
pub(crate) fn clear(directory: &Path, intent: &ExportIntent) -> Result<(), StateError> {
    let path = intent_path(directory, &intent.config_path);
    let _lock = intent_lock(&path)?;
    if load_at(&path)? != *intent {
        return Err(StateError::Invalid);
    }
    let result = intent.result_path(&path)?;
    match fs::symlink_metadata(&result) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(result).map_err(|_| StateError::Unavailable)?;
        }
        Ok(_) => return Err(StateError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(StateError::Unavailable),
    }
    fs::File::open(path.parent().ok_or(StateError::Invalid)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)?;
    fs::remove_file(&path).map_err(|_| StateError::Unavailable)?;
    fs::File::open(path.parent().ok_or(StateError::Invalid)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)
}

/// Reads the terminal helper record for an exact intent.
///
/// # Errors
///
/// Refuses malformed, linked or mismatched result state.
pub(crate) fn outcome(
    directory: &Path,
    intent: &ExportIntent,
) -> Result<Option<Result<Vec<u8>, BackendError>>, StateError> {
    let path = intent.result_path(&intent_path(directory, &intent.config_path))?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StateError::Unavailable),
    }
    let bytes = read_private(&path)?;
    let record: TerminalRecord = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
    if record.schema_version != 1
        || record.request_id != intent.request_id
        || record.repository_id != intent.repository_id
    {
        return Err(StateError::Invalid);
    }
    Ok(Some(match record.result {
        TerminalResult::Succeeded { receipt } if receipt.len() <= MAX_RECEIPT_BYTES => Ok(receipt),
        TerminalResult::Failed { code } if code.len() <= 160 => Err(BackendError::Command {
            code,
            exit_code: None,
        }),
        TerminalResult::Succeeded { .. } | TerminalResult::Failed { .. } => {
            return Err(StateError::Invalid);
        }
    }))
}

fn receipt_matches(intent: &ExportIntent, bytes: &[u8]) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    let Ok(receipt) = serde_json::from_slice::<ExportReceipt>(bytes) else {
        return false;
    };
    if receipt.schema_version != 1
        || !receipt.written
        || receipt.repository_id != intent.repository_id
        || receipt.bytes == 0
        || receipt.bytes > 1024 * 1024
    {
        return false;
    }
    let Ok(descriptor) = open(
        &intent.destination,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) else {
        return false;
    };
    let file = fs::File::from(descriptor);
    let Ok(held) = file.metadata() else {
        return false;
    };
    if !held.is_file() || held.len() != receipt.bytes as u64 {
        return false;
    }
    let mut published = Vec::with_capacity(receipt.bytes);
    if file
        .take((receipt.bytes + 1) as u64)
        .read_to_end(&mut published)
        .is_err()
        || published.len() != receipt.bytes
        || hex(&Sha256::digest(&published)) != receipt.sha256
    {
        return false;
    }
    fs::symlink_metadata(&intent.destination).is_ok_and(|named| {
        named.is_file()
            && named.dev() == held.dev()
            && named.ino() == held.ino()
            && named.len() == held.len()
    })
}

fn recovery_error(suffix: &str) -> BackendError {
    BackendError::Command {
        code: format!("codingmage.ui.setup_export_{suffix}"),
        exit_code: None,
    }
}

/// Inspects the exact private export intent and terminal record off the render thread.
///
/// Success does not clear the notice; the caller must accept the bound response and request
/// a second verified cleanup. A missing outcome remains unknown.
pub(crate) fn recover(
    path: &Path,
    digest: &str,
    request_id: &str,
) -> Result<Vec<u8>, BackendError> {
    let intent = load_matching_at(path, digest).map_err(|_| recovery_error("invalid_intent"))?;
    if intent.request_id != request_id {
        return Err(recovery_error("stale_intent"));
    }
    let directory = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| recovery_error("invalid_intent"))?;
    let receipt = outcome(directory, &intent)
        .map_err(|_| recovery_error("invalid_outcome"))?
        .ok_or_else(|| recovery_error("outcome_unknown"))??;
    if !receipt_matches(&intent, &receipt) {
        return Err(recovery_error("invalid_receipt"));
    }
    Ok(Vec::new())
}

/// Clears only the exact private intent after explicit inspection, rechecking successful
/// destination bytes when cleanup follows a verified response.
pub(crate) fn clear_notice(
    path: &Path,
    config_path: &Path,
    observed_repository_id: &str,
    digest: &str,
    request_id: &str,
    verified: bool,
) -> Result<Vec<u8>, BackendError> {
    let intent = load_matching_at(path, digest).map_err(|_| recovery_error("invalid_intent"))?;
    if intent.request_id != request_id
        || intent.config_path != config_path
        || intent.repository_id != observed_repository_id
    {
        return Err(recovery_error("stale_intent"));
    }
    let directory = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| recovery_error("invalid_intent"))?;
    if verified {
        let receipt = outcome(directory, &intent)
            .map_err(|_| recovery_error("invalid_outcome"))?
            .ok_or_else(|| recovery_error("outcome_unknown"))??;
        if !receipt_matches(&intent, &receipt) {
            return Err(recovery_error("invalid_receipt"));
        }
    }
    clear(directory, &intent).map_err(|_| recovery_error("clear_failed"))?;
    Ok(Vec::new())
}

/// Starts an isolated helper and waits only on a detached worker supervisor.
///
/// # Errors
///
/// Returns spawn or private-record failures without canceling a started helper.
pub(crate) fn run(
    helper: &Path,
    binary: &CoordinatorBinary,
    intent_path: &Path,
    intent_sha256: &str,
    deadline: Duration,
) -> Result<Vec<u8>, BackendError> {
    let intent = load_matching_at(intent_path, intent_sha256).map_err(|_| BackendError::Spawn)?;
    let request = HelperRequest {
        schema_version: 1,
        intent_path: intent_path.to_path_buf(),
        intent_sha256: intent_sha256.to_owned(),
        binary_path: binary.path().to_path_buf(),
        deadline_ms: u64::try_from(deadline.as_millis()).map_err(|_| BackendError::Spawn)?,
    };
    let bytes = serde_json::to_vec(&request).map_err(|_| BackendError::Spawn)?;
    if bytes.len() as u64 > MAX_HELPER_INPUT {
        return Err(BackendError::Spawn);
    }
    let mut child = Command::new(helper)
        .arg("--setup-export-helper")
        .env_clear()
        .envs(
            ["HOME", "PATH", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME"]
                .into_iter()
                .filter_map(|name| std::env::var_os(name).map(|value| (name, value))),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| BackendError::Spawn)?;
    let submitted = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(&bytes).is_ok());
    let status = child.wait().map_err(|_| BackendError::Spawn)?;
    if !submitted || !status.success() {
        return Err(BackendError::Spawn);
    }
    let directory = intent_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or(BackendError::Spawn)?;
    outcome(directory, &intent)
        .map_err(|_| BackendError::Spawn)?
        .ok_or(BackendError::Spawn)?
}

/// Executes the bounded public command and durably records its terminal result.
#[must_use]
pub(crate) fn helper_main() -> ExitCode {
    let mut bytes = Vec::new();
    if std::io::stdin()
        .take(MAX_HELPER_INPUT + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_HELPER_INPUT
    {
        return ExitCode::from(2);
    }
    let Ok(request) = serde_json::from_slice::<HelperRequest>(&bytes) else {
        return ExitCode::from(2);
    };
    if request.schema_version != 1 || request.deadline_ms == 0 || request.deadline_ms > 60_000 {
        return ExitCode::from(2);
    }
    let Ok(_lock) = intent_lock(&request.intent_path) else {
        return ExitCode::from(2);
    };
    let Ok(intent) = load_matching_at(&request.intent_path, &request.intent_sha256) else {
        return ExitCode::from(2);
    };
    let Ok(binary) = CoordinatorBinary::at(&request.binary_path) else {
        return ExitCode::from(2);
    };
    let result = binary.run(
        &intent.arguments,
        Duration::from_millis(request.deadline_ms),
        &Arc::new(AtomicBool::new(false)),
    );
    let result = match result {
        Ok(receipt) if receipt.len() <= MAX_RECEIPT_BYTES => TerminalResult::Succeeded { receipt },
        Ok(_) => TerminalResult::Failed {
            code: BackendError::OutputTooLarge.code(),
        },
        Err(error) => TerminalResult::Failed { code: error.code() },
    };
    let record = TerminalRecord {
        schema_version: 1,
        request_id: intent.request_id.clone(),
        repository_id: intent.repository_id.clone(),
        result,
    };
    let Ok(result_path) = intent.result_path(&request.intent_path) else {
        return ExitCode::from(2);
    };
    let Ok(record) = serde_json::to_vec(&record) else {
        return ExitCode::from(2);
    };
    if write_private(&result_path, &record).is_err() {
        return ExitCode::from(3);
    }
    if fs::File::open(result_path.parent().expect("result has a parent"))
        .and_then(|directory| directory.sync_all())
        .is_err()
    {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn helper_refuses_intent_changed_after_submission() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-setup-export-intent-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let config = root.join("project.toml");
        let source = config.clone();
        let destination = root.join("export.toml");
        let intent = prepare(
            &root,
            &config,
            "synthetic-repository",
            &source,
            &destination,
            vec!["setup-export-copy".to_owned()],
        )
        .unwrap();
        let path = intent_path(&root, &config);
        let digest = intent.fingerprint().unwrap();
        assert_eq!(load_matching_at(&path, &digest).unwrap(), intent);
        let mut changed = intent.clone();
        changed.destination = root.join("other.toml");
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert_eq!(load_matching_at(&path, &digest), Err(StateError::Invalid));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_cleanup_rechecks_destination_and_manual_clear_keeps_identity() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-export-recovery-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let config = root.join("project.toml");
        let destination = root.join("export.toml");
        fs::write(&config, b"original configuration").unwrap();
        fs::write(&destination, b"original configuration").unwrap();
        let intent = prepare(
            &root,
            &config,
            "repo-synthetic",
            &config,
            &destination,
            vec!["setup-export-copy".to_owned()],
        )
        .unwrap();
        let path = intent_path(&root, &config);
        let digest = intent.fingerprint().unwrap();
        let receipt = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "repository_id": intent.repository_id,
            "written": true,
            "bytes": 22,
            "sha256": hex(&Sha256::digest(b"original configuration")),
        }))
        .unwrap();
        let record = TerminalRecord {
            schema_version: 1,
            request_id: intent.request_id.clone(),
            repository_id: intent.repository_id.clone(),
            result: TerminalResult::Succeeded { receipt },
        };
        write_private(
            &intent.result_path(&path).unwrap(),
            &serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert_eq!(
            recover(&path, &digest, &intent.request_id).unwrap(),
            Vec::<u8>::new()
        );
        let replacement = root.join("replacement.toml");
        fs::write(&replacement, b"changed destination").unwrap();
        fs::rename(&replacement, &destination).unwrap();
        let result_path = intent.result_path(&path).unwrap();
        let assert_refused = |config_path: &Path, repository: &str, request: &str, verified| {
            assert!(
                clear_notice(&path, config_path, repository, &digest, request, verified).is_err()
            );
            assert!(path.exists());
            assert!(result_path.exists());
        };
        assert_refused(&config, "repo-synthetic", &intent.request_id, true);
        assert_refused(&config, "repo-synthetic", "other-request", false);
        assert_refused(&config, "other-repository", &intent.request_id, false);
        assert_refused(
            &root.join("other.toml"),
            "repo-synthetic",
            &intent.request_id,
            false,
        );
        clear_notice(
            &path,
            &config,
            "repo-synthetic",
            &digest,
            &intent.request_id,
            false,
        )
        .unwrap();
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
