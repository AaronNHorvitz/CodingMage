//! Read-only, source-bound reconciliation of a detached native Setup export.

use std::{
    fs,
    io::Read as _,
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
};

use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, open, openat},
    sys::stat::{Mode, SFlag, fstatat},
    unistd::{UnlinkatFlags, geteuid, unlinkat},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments};

const MAX_INTENT: u64 = 64 * 1024;
const MAX_TERMINAL: u64 = 1024 * 1024;
const MAX_DESTINATION: u64 = 1024 * 1024;
const INTENT_NAME: &str = "setup-export-intent.json";
const LOCK_NAME: &str = "setup-export-intent.lock";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportIntent {
    schema_version: u16,
    request_id: String,
    config_path: PathBuf,
    repository_id: String,
    source: PathBuf,
    destination: PathBuf,
    arguments: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalRecord {
    schema_version: u16,
    request_id: String,
    repository_id: String,
    result: TerminalResult,
}

#[derive(Deserialize)]
#[serde(tag = "status", deny_unknown_fields)]
enum TerminalResult {
    Succeeded { receipt: Vec<u8> },
    Failed { code: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportReceipt {
    schema_version: u64,
    repository_id: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    digest
}

// Keep a no-follow descriptor across the read and confirm that its named leaf did not move.
fn read_named(path: &Path, limit: u64) -> Result<Vec<u8>, CliError> {
    let file = fs::File::from(
        open(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| CliError::StaleObservation)?,
    );
    let held = file.metadata().map_err(|_| CliError::StaleObservation)?;
    if !held.is_file() || held.len() > limit {
        return Err(CliError::Refused);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::StaleObservation)?;
    let named = fs::symlink_metadata(path).map_err(|_| CliError::StaleObservation)?;
    if bytes.len() as u64 != held.len()
        || !named.is_file()
        || named.file_type().is_symlink()
        || (named.dev(), named.ino(), named.len()) != (held.dev(), held.ino(), held.len())
    {
        return Err(CliError::StaleObservation);
    }
    Ok(bytes)
}

/// Verifies one prior export without changing its private notice or destination.
pub(super) fn export(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "intent",
            "intent-sha256",
            "request",
            "config",
            "repository-id",
        ],
    )?;
    let intent_path = parsed.absolute_path("intent")?;
    let config_path = parsed.absolute_path("config")?;
    let expected_digest = parsed.value("intent-sha256")?;
    let request = parsed.value("request")?;
    let repository_id = parsed.value("repository-id")?;
    if intent_path
        .file_name()
        .is_none_or(|name| name != INTENT_NAME)
        || !lower_hex(expected_digest, 64)
        || !lower_hex(request, 32)
        || repository_id.is_empty()
    {
        return Err(CliError::InvalidArgument);
    }
    let intent_bytes = read_named(&intent_path, MAX_INTENT)?;
    if hex_digest(&intent_bytes) != expected_digest {
        return Err(CliError::StaleObservation);
    }
    let intent: ExportIntent =
        serde_json::from_slice(&intent_bytes).map_err(|_| CliError::InvalidArgument)?;
    if intent.schema_version != 1
        || intent.request_id != request
        || intent.repository_id != repository_id
        || intent.config_path != config_path
        || !intent.source.is_absolute()
        || !intent.destination.is_absolute()
        || intent
            .arguments
            .first()
            .is_none_or(|arg| arg != "setup-export-copy")
    {
        return Err(CliError::StaleObservation);
    }
    let result_path = intent_path
        .parent()
        .ok_or(CliError::InvalidArgument)?
        .join(format!("setup-export-result-{request}.json"));
    let terminal_bytes = read_named(&result_path, MAX_TERMINAL)?;
    let terminal: TerminalRecord =
        serde_json::from_slice(&terminal_bytes).map_err(|_| CliError::InvalidArgument)?;
    if terminal.schema_version != 1
        || terminal.request_id != request
        || terminal.repository_id != repository_id
    {
        return Err(CliError::StaleObservation);
    }
    let receipt_bytes = match terminal.result {
        TerminalResult::Succeeded { receipt } if receipt.len() <= 16 * 1024 => receipt,
        TerminalResult::Failed { code } if code.len() <= 160 => return Err(CliError::Refused),
        TerminalResult::Succeeded { .. } | TerminalResult::Failed { .. } => {
            return Err(CliError::InvalidArgument);
        }
    };
    let receipt: ExportReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(|_| CliError::InvalidArgument)?;
    if receipt.schema_version != 1
        || !receipt.written
        || receipt.repository_id != repository_id
        || receipt.bytes == 0
        || receipt.bytes as u64 > MAX_DESTINATION
        || !lower_hex(&receipt.sha256, 64)
    {
        return Err(CliError::StaleObservation);
    }
    let published = read_named(&intent.destination, MAX_DESTINATION)?;
    if published.len() != receipt.bytes || hex_digest(&published) != receipt.sha256 {
        return Err(CliError::StaleObservation);
    }
    // One more observation narrows the interval before the UI's separate verified cleanup.
    if hex_digest(&read_named(&intent_path, MAX_INTENT)?) != expected_digest
        || read_named(&result_path, MAX_TERMINAL)? != terminal_bytes
    {
        return Err(CliError::StaleObservation);
    }
    serde_json::to_string(&json!({
        "schema_version": 1,
        "request_id": request,
        "repository_id": repository_id,
        "intent_sha256": expected_digest,
        "destination_sha256": receipt.sha256,
        "verified": true
    }))
    .map_err(|_| CliError::Internal)
}

// The command removes only entries under the configuration's private UI state directory.
// Keep a directory descriptor throughout validation and removal so a replaced ancestor cannot
// redirect the unlink operations.
fn clear_directory(path: &Path, config: &Path) -> Result<fs::File, CliError> {
    let parent = path.parent().ok_or(CliError::InvalidArgument)?;
    let projects = parent.parent().ok_or(CliError::InvalidArgument)?;
    let root = projects.parent().ok_or(CliError::InvalidArgument)?;
    let config_hash = hex_digest(config.as_os_str().as_encoded_bytes());
    if path.file_name().is_none_or(|name| name != INTENT_NAME)
        || projects.file_name().is_none_or(|name| name != "projects")
        || parent
            .file_name()
            .is_none_or(|name| name != std::ffi::OsStr::new(&config_hash))
    {
        return Err(CliError::InvalidArgument);
    }
    for directory in [root, projects, parent] {
        let metadata = fs::symlink_metadata(directory).map_err(|_| CliError::StaleObservation)?;
        let forbidden_mode = if directory == parent { 0o077 } else { 0o022 };
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != geteuid().as_raw()
            || metadata.mode() & forbidden_mode != 0
        {
            return Err(CliError::Refused);
        }
    }
    let dir = fs::File::from(
        open(
            parent,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| CliError::StaleObservation)?,
    );
    let held = dir.metadata().map_err(|_| CliError::StaleObservation)?;
    let named = fs::symlink_metadata(parent).map_err(|_| CliError::StaleObservation)?;
    if (held.dev(), held.ino()) != (named.dev(), named.ino()) {
        return Err(CliError::StaleObservation);
    }
    Ok(dir)
}

fn read_child(directory: &fs::File, name: &str, limit: u64) -> Result<Vec<u8>, CliError> {
    let file = fs::File::from(
        openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| CliError::StaleObservation)?,
    );
    let held = file.metadata().map_err(|_| CliError::StaleObservation)?;
    if !held.is_file()
        || held.len() > limit
        || held.nlink() != 1
        || held.uid() != geteuid().as_raw()
        || held.mode() & 0o077 != 0
    {
        return Err(CliError::Refused);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::StaleObservation)?;
    let named = fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .map_err(|_| CliError::StaleObservation)?;
    if bytes.len() as u64 != held.len()
        || !SFlag::from_bits_truncate(named.st_mode).contains(SFlag::S_IFREG)
        || (held.dev(), held.ino(), held.len())
            != (named.st_dev, named.st_ino, named.st_size.cast_unsigned())
    {
        return Err(CliError::StaleObservation);
    }
    Ok(bytes)
}

fn lock_is_named(directory: &fs::File, lock: &fs::File) -> Result<bool, CliError> {
    let held = lock.metadata().map_err(|_| CliError::StaleObservation)?;
    let named = fstatat(directory, LOCK_NAME, AtFlags::AT_SYMLINK_NOFOLLOW)
        .map_err(|_| CliError::StaleObservation)?;
    Ok(
        SFlag::from_bits_truncate(named.st_mode).contains(SFlag::S_IFREG)
            && (held.dev(), held.ino()) == (named.st_dev, named.st_ino),
    )
}

fn locked_directory(path: &Path, config: &Path) -> Result<(fs::File, fs::File), CliError> {
    let directory = clear_directory(path, config)?;
    let lock = fs::File::from(
        openat(
            &directory,
            LOCK_NAME,
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| CliError::StaleObservation)?,
    );
    let metadata = lock.metadata().map_err(|_| CliError::StaleObservation)?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(CliError::Refused);
    }
    lock.try_lock().map_err(|_| CliError::Refused)?;
    if !lock_is_named(&directory, &lock)? {
        return Err(CliError::StaleObservation);
    }
    Ok((directory, lock))
}

/// Clears one exact private Setup export notice after an explicit manual inspection or
/// after verifying the public recovery receipt. The exported destination is never removed.
pub(super) fn clear(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "intent",
            "intent-sha256",
            "request",
            "config",
            "repository-id",
            "verified",
        ],
    )?;
    let verified = match parsed.value("verified")? {
        "true" => true,
        "false" => false,
        _ => return Err(CliError::InvalidArgument),
    };
    clear_bound(&parsed, arguments, verified)
}

fn clear_bound(
    parsed: &ParsedArguments,
    arguments: &[String],
    verified: bool,
) -> Result<String, CliError> {
    let path = parsed.absolute_path("intent")?;
    let config = parsed.absolute_path("config")?;
    let digest = parsed.value("intent-sha256")?;
    let request = parsed.value("request")?;
    let repository = parsed.value("repository-id")?;
    if !lower_hex(digest, 64) || !lower_hex(request, 32) || repository.is_empty() {
        return Err(CliError::InvalidArgument);
    }
    let (directory, lock) = locked_directory(&path, &config)?;
    let intent_bytes = read_child(&directory, INTENT_NAME, MAX_INTENT)?;
    if hex_digest(&intent_bytes) != digest {
        return Err(CliError::StaleObservation);
    }
    let intent: ExportIntent =
        serde_json::from_slice(&intent_bytes).map_err(|_| CliError::InvalidArgument)?;
    if intent.schema_version != 1
        || intent.request_id != request
        || intent.config_path != config
        || intent.repository_id != repository
        || !intent.source.is_absolute()
        || !intent.destination.is_absolute()
        || intent
            .arguments
            .first()
            .is_none_or(|arg| arg != "setup-export-copy")
    {
        return Err(CliError::StaleObservation);
    }
    let result_name = format!("setup-export-result-{request}.json");
    let result_bytes = match fstatat(
        &directory,
        result_name.as_str(),
        AtFlags::AT_SYMLINK_NOFOLLOW,
    ) {
        Ok(_) => Some(read_child(&directory, &result_name, MAX_TERMINAL)?),
        Err(Errno::ENOENT) => None,
        Err(_) => return Err(CliError::StaleObservation),
    };
    if read_child(&directory, INTENT_NAME, MAX_INTENT)? != intent_bytes {
        return Err(CliError::StaleObservation);
    }
    if !lock_is_named(&directory, &lock)? {
        return Err(CliError::StaleObservation);
    }
    if let Some(result_bytes) = result_bytes {
        // Recheck the result's type and identity immediately before removing the named leaf.
        if read_child(&directory, &result_name, MAX_TERMINAL)? != result_bytes {
            return Err(CliError::StaleObservation);
        }
        if verified {
            let recovery: Vec<String> = arguments
                .chunks_exact(2)
                .filter(|pair| pair[0] != "--verified")
                .flat_map(|pair| pair.iter().cloned())
                .collect();
            export(&recovery)?;
        }
        unlinkat(&directory, result_name.as_str(), UnlinkatFlags::NoRemoveDir)
            .map_err(|_| CliError::StaleObservation)?;
        directory.sync_all().map_err(|_| CliError::UncertainWrite)?;
    } else if verified {
        return Err(CliError::StaleObservation);
    }
    if read_child(&directory, INTENT_NAME, MAX_INTENT)? != intent_bytes {
        return Err(CliError::StaleObservation);
    }
    unlinkat(&directory, INTENT_NAME, UnlinkatFlags::NoRemoveDir)
        .map_err(|_| CliError::StaleObservation)?;
    directory.sync_all().map_err(|_| CliError::UncertainWrite)?;
    serde_json::to_string(&json!({
        "schema_version": 1,
        "request_id": request,
        "repository_id": repository,
        "intent_sha256": digest,
        "cleared": true,
        "verified": verified
    }))
    .map_err(|_| CliError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{PermissionsExt as _, symlink},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn exact_export_recovery_refuses_stale_foreign_malformed_and_linked_state() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-cli-export-recovery-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let intent_path = root.join("setup-export-intent.json");
        let result_path = root.join(format!("setup-export-result-{}.json", "a".repeat(32)));
        let config_path = root.join("project.toml");
        let destination = root.join("export.toml");
        fs::write(&destination, b"published bytes").unwrap();
        let intent = json!({
            "schema_version": 1,
            "request_id": "a".repeat(32),
            "config_path": config_path,
            "repository_id": "repo-synthetic",
            "source": config_path,
            "destination": destination,
            "arguments": ["setup-export-copy"]
        });
        let intent_bytes = serde_json::to_vec(&intent).unwrap();
        fs::write(&intent_path, &intent_bytes).unwrap();
        let receipt = serde_json::to_vec(&json!({
            "schema_version": 1,
            "repository_id": "repo-synthetic",
            "written": true,
            "bytes": 15,
            "sha256": hex_digest(b"published bytes")
        }))
        .unwrap();
        let terminal = json!({
            "schema_version": 1,
            "request_id": "a".repeat(32),
            "repository_id": "repo-synthetic",
            "result": {"status": "Succeeded", "receipt": receipt}
        });
        fs::write(&result_path, serde_json::to_vec(&terminal).unwrap()).unwrap();
        let arguments = vec![
            "--intent".to_owned(),
            intent_path.to_str().unwrap().to_owned(),
            "--intent-sha256".to_owned(),
            hex_digest(&intent_bytes),
            "--request".to_owned(),
            "a".repeat(32),
            "--config".to_owned(),
            config_path.to_str().unwrap().to_owned(),
            "--repository-id".to_owned(),
            "repo-synthetic".to_owned(),
        ];
        let success: serde_json::Value =
            serde_json::from_str(&export(&arguments).unwrap()).unwrap();
        assert_eq!(success["verified"], true);
        assert_eq!(
            success["destination_sha256"],
            hex_digest(b"published bytes")
        );

        let mut foreign = arguments.clone();
        foreign[9] = "repo-other".to_owned();
        assert!(export(&foreign).is_err());
        let mut stale = arguments.clone();
        stale[3] = "0".repeat(64);
        assert!(export(&stale).is_err());
        fs::write(&destination, b"changed bytes").unwrap();
        assert!(export(&arguments).is_err());
        fs::write(&destination, b"published bytes").unwrap();
        fs::write(&result_path, b"{bad json").unwrap();
        assert!(export(&arguments).is_err());
        fs::remove_file(&result_path).unwrap();
        symlink(&destination, &result_path).unwrap();
        assert!(export(&arguments).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn clear_refuses_stale_running_and_linked_records_then_clears_only_exact_notice() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-cli-export-clear-{}-{nonce}",
            std::process::id()
        ));
        let config = root.join("project.toml");
        let destination = root.join("export.toml");
        let directory = root
            .join("codingmage-ui")
            .join("projects")
            .join(hex_digest(config.as_os_str().as_encoded_bytes()));
        fs::create_dir_all(&directory).unwrap();
        for path in [
            root.join("codingmage-ui"),
            root.join("codingmage-ui/projects"),
        ] {
            fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&config, b"source bytes").unwrap();
        fs::write(&destination, b"published bytes").unwrap();
        let path = directory.join(INTENT_NAME);
        let request = "a".repeat(32);
        let result = directory.join(format!("setup-export-result-{request}.json"));
        let intent = json!({
            "schema_version": 1,
            "request_id": request,
            "config_path": config,
            "repository_id": "repo-synthetic",
            "source": config,
            "destination": destination,
            "arguments": ["setup-export-copy"]
        });
        let bytes = serde_json::to_vec(&intent).unwrap();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let receipt = serde_json::to_vec(&json!({
            "schema_version": 1,
            "repository_id": "repo-synthetic",
            "written": true,
            "bytes": 15,
            "sha256": hex_digest(b"published bytes")
        }))
        .unwrap();
        let terminal_bytes = serde_json::to_vec(&json!({
            "schema_version": 1,
            "request_id": request,
            "repository_id": "repo-synthetic",
            "result": {"status": "Succeeded", "receipt": receipt}
        }))
        .unwrap();
        fs::write(&result, &terminal_bytes).unwrap();
        fs::set_permissions(&result, std::fs::Permissions::from_mode(0o600)).unwrap();
        let args = vec![
            "--intent".to_owned(),
            path.to_str().unwrap().to_owned(),
            "--intent-sha256".to_owned(),
            hex_digest(&bytes),
            "--request".to_owned(),
            request.clone(),
            "--config".to_owned(),
            config.to_str().unwrap().to_owned(),
            "--repository-id".to_owned(),
            "repo-synthetic".to_owned(),
            "--verified".to_owned(),
            "true".to_owned(),
        ];
        let mut stale = args.clone();
        stale[3] = "0".repeat(64);
        assert!(clear(&stale).is_err());
        let mut foreign = args.clone();
        foreign[9] = "repo-foreign".to_owned();
        assert!(clear(&foreign).is_err());
        fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(clear(&args), Err(CliError::Refused));
        fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(LOCK_NAME))
            .unwrap();
        fs::set_permissions(
            directory.join(LOCK_NAME),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        lock.try_lock().unwrap();
        assert_eq!(clear(&args), Err(CliError::Refused));
        drop(lock);
        fs::remove_file(&result).unwrap();
        symlink(&destination, &result).unwrap();
        assert!(clear(&args).is_err());
        fs::remove_file(&result).unwrap();
        let mut manual = args.clone();
        manual[11] = "false".to_owned();
        let manual_receipt: serde_json::Value =
            serde_json::from_str(&clear(&manual).unwrap()).unwrap();
        assert_eq!(manual_receipt["cleared"], true);
        assert_eq!(manual_receipt["verified"], false);
        assert!(!path.exists());
        assert!(destination.exists());
        fs::write(&path, &bytes).unwrap();
        fs::write(&result, &terminal_bytes).unwrap();
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&result, std::fs::Permissions::from_mode(0o600)).unwrap();
        let verified: serde_json::Value = serde_json::from_str(&clear(&args).unwrap()).unwrap();
        assert_eq!(verified["cleared"], true);
        assert_eq!(verified["verified"], true);
        assert!(!path.exists());
        assert!(!result.exists());
        assert!(destination.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
