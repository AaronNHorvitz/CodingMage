//! Read-only, source-bound reconciliation of a detached native Setup export.

use std::{
    fs,
    io::Read as _,
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments};

const MAX_INTENT: u64 = 64 * 1024;
const MAX_TERMINAL: u64 = 1024 * 1024;
const MAX_DESTINATION: u64 = 1024 * 1024;

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
        .is_none_or(|name| name != "setup-export-intent.json")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
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
}
