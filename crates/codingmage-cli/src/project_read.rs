//! Read-only, bounded project snapshot for the native selection boundary.

use std::{
    fs::{self, File},
    io::Read as _,
    os::unix::fs::MetadataExt as _,
    path::Path,
};

use codingmage_core::{Config, parse_config_bytes};
use codingmage_plan::TaskPlan;
use nix::{
    fcntl::{OFlag, open as open_file},
    sys::stat::Mode,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
// The CLI entry point appends a newline; keep the complete stdout within the worker cap.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024 - 1;

pub(super) fn open(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config"])?;
    let config_path = parsed.absolute_file("config")?;
    let before = bounded_regular_file(&config_path, MAX_CONFIG_BYTES).ok_or(CliError::Config)?;
    let config = parse_config_bytes(&before).map_err(|_| CliError::Config)?;
    let after = bounded_regular_file(&config_path, MAX_CONFIG_BYTES).ok_or(CliError::Config)?;
    if before != after {
        return Err(CliError::StaleObservation);
    }
    let plan = load_plan(&config);
    let envelope = |plan: Value| {
        json!({
            "schema_version": 1,
            "config_path": config_path,
            "config_sha256": hex_digest(&before),
            "config": config,
            "plan": plan,
        })
    };
    let mut response = serde_json::to_vec(&envelope(plan)).map_err(|_| CliError::Internal)?;
    if response.len() > MAX_RESPONSE_BYTES {
        response = serde_json::to_vec(&envelope(json!({"state": "projection_too_large"})))
            .map_err(|_| CliError::Internal)?;
    }
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(CliError::Refused);
    }
    String::from_utf8(response).map_err(|_| CliError::Internal)
}

fn load_plan(config: &Config) -> Value {
    let path = config.target_path.join(&config.task_source);
    let Some(bytes) = bounded_task_source(&config.target_path, &path) else {
        return json!({"state": "unavailable"});
    };
    match TaskPlan::parse(&bytes) {
        Ok(plan) => json!({
            "state": "loaded",
            "source_sha256": hex_digest(&bytes),
            "byte_length": bytes.len(),
            "plan": plan,
        }),
        Err(error) => json!({"state": "invalid", "code": error.to_string()}),
    }
}

fn bounded_task_source(root: &Path, path: &Path) -> Option<Vec<u8>> {
    let canonical_root = fs::canonicalize(root).ok()?;
    let canonical_path = fs::canonicalize(path).ok()?;
    if !canonical_path.starts_with(canonical_root) {
        return None;
    }
    let bytes = bounded_regular_file(path, MAX_SOURCE_BYTES)?;
    (fs::canonicalize(path).ok()? == canonical_path).then_some(bytes)
}

fn bounded_regular_file(path: &Path, maximum: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.len() > maximum
    {
        return None;
    }
    let file = File::from(
        open_file(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .ok()?,
    );
    let opened = file.metadata().ok()?;
    if !opened.is_file()
        || opened.nlink() != 1
        || opened.dev() != metadata.dev()
        || opened.ino() != metadata.ino()
    {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes).ok()?;
    let after = fs::symlink_metadata(path).ok()?;
    (bytes.len() as u64 <= maximum
        && after.is_file()
        && !after.file_type().is_symlink()
        && after.nlink() == 1
        && after.dev() == opened.dev()
        && after.ino() == opened.ino())
    .then_some(bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}
