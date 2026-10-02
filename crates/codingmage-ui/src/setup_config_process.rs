//! Durable one-shot supervision for a public guided configuration write.
//!
//! A private, exact input snapshot exists before dispatch. The helper owns the coordinator
//! command and terminal record after it starts, independent of the native window lifetime.

use std::{
    fmt, fs,
    io::{Read as _, Write as _},
    os::unix::fs::{MetadataExt as _, OpenOptionsExt as _},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use codingmage_core::{Config, RepositoryAuthorization};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
    unistd::geteuid,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    backend::{BackendError, CoordinatorBinary},
    project::hex,
    state_dir::{StateError, ensure_private_dir, write_private},
};

const INTENT_NAME: &str = "setup-config-intent.json";
const LOCK_NAME: &str = "setup-config-intent.lock";
const MAX_INPUT_BYTES: usize = 512 * 1024;
const MAX_RECORD_BYTES: u64 = 1024 * 1024;
const MAX_HELPER_REQUEST: u64 = 64 * 1024;
const MAX_RECEIPT_BYTES: usize = 16 * 1024;

/// Snapshot of one explicit native configuration request.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigIntent {
    schema_version: u16,
    /// Random request identity, used for response and terminal-record binding.
    pub request_id: String,
    /// Selected repository path, validated by the public coordinator command.
    pub repository: PathBuf,
    /// Requested public configuration path.
    pub destination: PathBuf,
    /// Exact displayed coordinator argument vector.
    pub arguments: Vec<String>,
    /// Private candidate, never included in argv, errors or Debug output.
    candidate: String,
    /// Digest of the exact candidate bytes.
    pub sha256: String,
}

impl fmt::Debug for ConfigIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigIntent")
            .field("request_id", &self.request_id)
            .field("repository", &self.repository)
            .field("destination", &self.destination)
            .field("arguments", &self.arguments)
            .field("sha256", &self.sha256)
            .finish_non_exhaustive()
    }
}

impl ConfigIntent {
    /// Digest of the exact private intent record submitted to the helper.
    pub(crate) fn fingerprint(&self) -> Result<String, StateError> {
        let bytes = serde_json::to_vec(self).map_err(|_| StateError::Invalid)?;
        Ok(hex(&Sha256::digest(bytes)))
    }

    fn result_path(&self, directory: &Path) -> Result<PathBuf, StateError> {
        if !valid_id(&self.request_id) {
            return Err(StateError::Invalid);
        }
        Ok(directory.join(format!("setup-config-result-{}.json", self.request_id)))
    }

    /// Exact candidate byte count.
    pub(crate) fn byte_count(&self) -> usize {
        self.candidate.len()
    }

    pub(crate) fn matches_config(&self, config: &Config) -> bool {
        toml::from_str::<Config>(&self.candidate).is_ok_and(|expected| &expected == config)
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
    result: TerminalResult,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "status", deny_unknown_fields)]
enum TerminalResult {
    Succeeded { receipt: Vec<u8> },
    Failed { code: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigReceipt {
    schema_version: u64,
    repository_id: String,
    head: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

fn valid_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_hex(value: &str, count: usize) -> bool {
    value.len() == count && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_head(value: &str) -> bool {
    if let Some(object) = value.strip_prefix("oid:") {
        return valid_hex(object, 40) || valid_hex(object, 64);
    }
    let Some((reference, object)) = value
        .strip_prefix("ref:")
        .and_then(|head| head.rsplit_once('@'))
    else {
        return false;
    };
    reference.starts_with("refs/")
        && reference.len() <= 1024
        && !reference.contains("..")
        && !reference.contains("@{")
        && !reference.ends_with(['.', '/'])
        && !reference
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
        && (object == "unborn" || valid_hex(object, 40) || valid_hex(object, 64))
}

/// Fixed private intent location; only one unresolved configuration write is allowed.
pub(crate) fn intent_path(directory: &Path) -> PathBuf {
    directory.join(INTENT_NAME)
}

fn lock(directory: &Path) -> Result<fs::File, StateError> {
    ensure_private_dir(directory)?;
    let file = fs::File::from(
        open(
            &directory.join(LOCK_NAME),
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| StateError::Unavailable)?,
    );
    let metadata = file.metadata().map_err(|_| StateError::Unavailable)?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
    {
        return Err(StateError::Invalid);
    }
    file.try_lock().map_err(|_| StateError::Unavailable)?;
    Ok(file)
}

fn read_held(path: &Path, limit: u64) -> Result<Vec<u8>, StateError> {
    let file = fs::File::from(
        open(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| StateError::Unavailable)?,
    );
    let held = file.metadata().map_err(|_| StateError::Unavailable)?;
    if !held.is_file()
        || held.len() > limit
        || held.nlink() != 1
        || held.uid() != geteuid().as_raw()
        || held.mode() & 0o777 != 0o600
    {
        return Err(StateError::Invalid);
    }
    let mut bytes = Vec::new();
    (&file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StateError::Unavailable)?;
    let named = fs::symlink_metadata(path).map_err(|_| StateError::Unavailable)?;
    let after = file.metadata().map_err(|_| StateError::Unavailable)?;
    if bytes.len() as u64 > limit
        || named.file_type().is_symlink()
        || !named.is_file()
        || (named.dev(), named.ino(), named.len()) != (held.dev(), held.ino(), held.len())
        || (after.dev(), after.ino(), after.len()) != (held.dev(), held.ino(), held.len())
        || bytes.len() as u64 != held.len()
    {
        return Err(StateError::Invalid);
    }
    Ok(bytes)
}

fn parse_intent(bytes: &[u8]) -> Result<ConfigIntent, StateError> {
    let intent: ConfigIntent = serde_json::from_slice(bytes).map_err(|_| StateError::Invalid)?;
    let repository = intent.repository.to_str().ok_or(StateError::Invalid)?;
    let destination = intent.destination.to_str().ok_or(StateError::Invalid)?;
    if intent.schema_version != 1
        || !valid_id(&intent.request_id)
        || !intent.repository.is_absolute()
        || !intent.destination.is_absolute()
        || intent.candidate.is_empty()
        || intent.candidate.len() > MAX_INPUT_BYTES
        || !valid_hex(&intent.sha256, 64)
        || hex(&Sha256::digest(intent.candidate.as_bytes())) != intent.sha256
        || intent.arguments.len() != 7
        || intent.arguments[0] != "setup-write-config"
        || intent.arguments[1] != "--repo"
        || intent.arguments[2] != repository
        || intent.arguments[3] != "--output"
        || intent.arguments[4] != destination
        || intent.arguments[5] != "--overwrite"
        || !matches!(intent.arguments[6].as_str(), "true" | "false")
    {
        return Err(StateError::Invalid);
    }
    Ok(intent)
}

fn load_at(path: &Path) -> Result<ConfigIntent, StateError> {
    if path.file_name().is_none_or(|name| name != INTENT_NAME) {
        return Err(StateError::Invalid);
    }
    parse_intent(&read_held(path, MAX_RECORD_BYTES)?)
}

fn load_matching(path: &Path, digest: &str) -> Result<ConfigIntent, StateError> {
    let bytes = read_held(path, MAX_RECORD_BYTES)?;
    if path.file_name().is_none_or(|name| name != INTENT_NAME)
        || !valid_hex(digest, 64)
        || hex(&Sha256::digest(&bytes)) != digest
    {
        return Err(StateError::Invalid);
    }
    parse_intent(&bytes)
}

/// Persists a single candidate before the worker can dispatch it.
///
/// # Errors
///
/// Refuses an unresolved prior intent, malformed paths, oversized input or unavailable state.
pub(crate) fn prepare(
    directory: &Path,
    repository: &Path,
    destination: &Path,
    arguments: Vec<String>,
    candidate: String,
) -> Result<ConfigIntent, StateError> {
    if !repository.is_absolute()
        || !destination.is_absolute()
        || candidate.is_empty()
        || candidate.len() > MAX_INPUT_BYTES
    {
        return Err(StateError::Invalid);
    }
    let mut random = [0_u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_| StateError::Unavailable)?;
    let intent = ConfigIntent {
        schema_version: 1,
        request_id: hex(&random),
        repository: repository.to_path_buf(),
        destination: destination.to_path_buf(),
        arguments,
        sha256: hex(&Sha256::digest(candidate.as_bytes())),
        candidate,
    };
    let bytes = serde_json::to_vec(&intent).map_err(|_| StateError::Invalid)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES || parse_intent(&bytes).is_err() {
        return Err(StateError::Invalid);
    }
    ensure_private_dir(directory)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(intent_path(directory))
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StateError::Invalid
            } else {
                StateError::Unavailable
            }
        })?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| StateError::Unavailable)?;
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)?;
    Ok(intent)
}

/// Loads an unresolved native configuration intent.
///
/// # Errors
///
/// Refuses an invalid or linked private record.
pub(crate) fn load(directory: &Path) -> Result<Option<ConfigIntent>, StateError> {
    let path = intent_path(directory);
    match fs::symlink_metadata(&path) {
        Ok(_) => load_at(&path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(StateError::Unavailable),
    }
}

/// Removes only the exact inspected intent and terminal record under its lock.
///
/// # Errors
///
/// Refuses a changed private record or an active helper.
pub(crate) fn clear(directory: &Path, intent: &ConfigIntent) -> Result<(), StateError> {
    let _lock = lock(directory)?;
    let path = intent_path(directory);
    if load_at(&path)? != *intent {
        return Err(StateError::Invalid);
    }
    let result = intent.result_path(directory)?;
    match fs::symlink_metadata(&result) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(result).map_err(|_| StateError::Unavailable)?;
        }
        Ok(_) => return Err(StateError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(StateError::Unavailable),
    }
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)?;
    fs::remove_file(path).map_err(|_| StateError::Unavailable)?;
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StateError::Unavailable)
}

/// Reads a terminal helper outcome without treating an absent record as a failure or success.
///
/// # Errors
///
/// Refuses a malformed or mismatched terminal record.
pub(crate) fn outcome(
    directory: &Path,
    intent: &ConfigIntent,
) -> Result<Option<Result<Vec<u8>, BackendError>>, StateError> {
    let path = intent.result_path(directory)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StateError::Unavailable),
    }
    let bytes = read_held(&path, MAX_RECEIPT_BYTES as u64 + 4096)?;
    let record: TerminalRecord = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
    if record.schema_version != 1 || record.request_id != intent.request_id {
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

/// Verifies the exact submitted bytes at the named destination and the command receipt.
pub(crate) fn receipt_matches(intent: &ConfigIntent, bytes: &[u8]) -> bool {
    let Ok(receipt) = serde_json::from_slice::<ConfigReceipt>(bytes) else {
        return false;
    };
    if receipt.schema_version != 1
        || !receipt.written
        || !receipt.repository_id.starts_with("repo-")
        || !valid_head(&receipt.head)
        || receipt.bytes != intent.byte_count()
        || receipt.sha256 != intent.sha256
    {
        return false;
    }
    read_held(&intent.destination, MAX_INPUT_BYTES as u64)
        .is_ok_and(|published| published == intent.candidate.as_bytes())
}

fn receipt_identity_matches(intent: &ConfigIntent, bytes: &[u8]) -> bool {
    let Ok(receipt) = serde_json::from_slice::<ConfigReceipt>(bytes) else {
        return false;
    };
    let Ok(config) = toml::from_str::<Config>(&intent.candidate) else {
        return false;
    };
    let Some(source) = std::env::current_exe()
        .ok()
        .and_then(|binary| binary.parent().map(Path::to_path_buf))
        .and_then(|parent| fs::canonicalize(parent).ok())
    else {
        return false;
    };
    let Ok(authority) = RepositoryAuthorization::authorize(&config, &source) else {
        return false;
    };
    receipt.repository_id == authority.identity().repository_id.as_str()
        && receipt.head == authority.identity().initial_head
        && authority.revalidate().is_ok()
}

/// Starts the detached helper and waits only on its worker supervisor.
///
/// # Errors
///
/// Returns spawn/record errors without cancelling a helper that already started.
pub(crate) fn run(
    helper: &Path,
    binary: &CoordinatorBinary,
    intent_path: &Path,
    intent_sha256: &str,
    deadline: Duration,
) -> Result<Vec<u8>, BackendError> {
    let intent = load_matching(intent_path, intent_sha256).map_err(|_| BackendError::Spawn)?;
    let request = HelperRequest {
        schema_version: 1,
        intent_path: intent_path.to_path_buf(),
        intent_sha256: intent_sha256.to_owned(),
        binary_path: binary.path().to_path_buf(),
        deadline_ms: u64::try_from(deadline.as_millis()).map_err(|_| BackendError::Spawn)?,
    };
    let bytes = serde_json::to_vec(&request).map_err(|_| BackendError::Spawn)?;
    if bytes.len() as u64 > MAX_HELPER_REQUEST {
        return Err(BackendError::Spawn);
    }
    let mut child = Command::new(helper)
        .arg("--setup-config-helper")
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
    outcome(intent_path.parent().ok_or(BackendError::Spawn)?, &intent)
        .map_err(|_| BackendError::Spawn)?
        .ok_or(BackendError::Spawn)?
}

/// Executes one bound public coordinator command and records its terminal outcome.
#[must_use]
pub(crate) fn helper_main() -> ExitCode {
    let mut bytes = Vec::new();
    if std::io::stdin()
        .take(MAX_HELPER_REQUEST + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_HELPER_REQUEST
    {
        return ExitCode::from(2);
    }
    let Ok(request) = serde_json::from_slice::<HelperRequest>(&bytes) else {
        return ExitCode::from(2);
    };
    if request.schema_version != 1 || request.deadline_ms == 0 || request.deadline_ms > 60_000 {
        return ExitCode::from(2);
    }
    let Some(directory) = request.intent_path.parent() else {
        return ExitCode::from(2);
    };
    let Ok(_lock) = lock(directory) else {
        return ExitCode::from(2);
    };
    let Ok(intent) = load_matching(&request.intent_path, &request.intent_sha256) else {
        return ExitCode::from(2);
    };
    let Ok(result_path) = intent.result_path(directory) else {
        return ExitCode::from(2);
    };
    match fs::symlink_metadata(&result_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) | Err(_) => return ExitCode::from(2),
    }
    let Ok(binary) = CoordinatorBinary::at(&request.binary_path) else {
        return ExitCode::from(2);
    };
    let result = binary.run_with_private_input(
        &intent.arguments,
        intent.candidate.as_bytes().to_vec(),
        Duration::from_millis(request.deadline_ms),
        &Arc::new(AtomicBool::new(false)),
    );
    let result = match result {
        Ok(receipt)
            if receipt.len() <= MAX_RECEIPT_BYTES
                && receipt_matches(&intent, &receipt)
                && receipt_identity_matches(&intent, &receipt) =>
        {
            TerminalResult::Succeeded { receipt }
        }
        Ok(_) => TerminalResult::Failed {
            code: "codingmage.ui.malformed_configuration_receipt".to_owned(),
        },
        Err(error) => TerminalResult::Failed { code: error.code() },
    };
    let record = TerminalRecord {
        schema_version: 1,
        request_id: intent.request_id,
        result,
    };
    let Ok(record) = serde_json::to_vec(&record) else {
        return ExitCode::from(2);
    };
    if write_private(&result_path, &record).is_err() {
        return ExitCode::from(3);
    }
    if fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .is_err()
    {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_intent_refuses_changed_bytes_and_duplicate_write() {
        let root =
            std::env::temp_dir().join(format!("codingmage-config-intent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repository = root.join("repo");
        let output = root.join("config.toml");
        let args = vec![
            "setup-write-config".to_owned(),
            "--repo".to_owned(),
            repository.display().to_string(),
            "--output".to_owned(),
            output.display().to_string(),
            "--overwrite".to_owned(),
            "false".to_owned(),
        ];
        let intent = prepare(
            &root,
            &repository,
            &output,
            args.clone(),
            "version = 1\n".to_owned(),
        )
        .unwrap();
        assert_eq!(load(&root).unwrap(), Some(intent.clone()));
        assert_eq!(
            prepare(
                &root,
                &repository,
                &output,
                args,
                "version = 1\n".to_owned()
            ),
            Err(StateError::Invalid)
        );
        let digest = intent.fingerprint().unwrap();
        assert_eq!(load_matching(&intent_path(&root), &digest).unwrap(), intent);
        fs::write(intent_path(&root), b"changed").unwrap();
        assert_eq!(
            load_matching(&intent_path(&root), &digest),
            Err(StateError::Invalid)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_terminal_record_and_head_are_refused() {
        let root =
            std::env::temp_dir().join(format!("codingmage-config-terminal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repository = root.join("repo");
        let output = root.join("config.toml");
        let intent = prepare(
            &root,
            &repository,
            &output,
            vec![
                "setup-write-config".to_owned(),
                "--repo".to_owned(),
                repository.display().to_string(),
                "--output".to_owned(),
                output.display().to_string(),
                "--overwrite".to_owned(),
                "false".to_owned(),
            ],
            "version = 1\n".to_owned(),
        )
        .unwrap();
        assert!(valid_head(
            "ref:refs/heads/main@0123456789012345678901234567890123456789"
        ));
        assert!(valid_head("oid:0123456789012345678901234567890123456789"));
        assert!(!valid_head("ref:refs/heads/main@not-an-object"));
        assert!(!valid_head(
            "ref:refs/heads/../other@0123456789012345678901234567890123456789"
        ));
        let result = intent.result_path(&root).unwrap();
        write_private(&result, b"{\"schema_version\":1,\"request_id\":\"wrong\",\"result\":{\"status\":\"Succeeded\",\"receipt\":[]}}").unwrap();
        assert!(matches!(outcome(&root, &intent), Err(StateError::Invalid)));
        clear(&root, &intent).unwrap();
        assert!(!result.exists());
        assert!(load(&root).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
