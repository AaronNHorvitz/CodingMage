//! Bounded, read-only directory snapshots for native file selection.

use std::{
    fs::{self, File},
    os::{fd::AsRawFd as _, unix::fs::MetadataExt as _},
};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde_json::{Value, json};

use crate::{CliError, ParsedArguments};

const MAX_ENTRIES: usize = 2_000;
const MAX_SCANNED: usize = 4_096;
const MAX_RESPONSE_BYTES: usize = 1_048_575;

struct ListedEntry {
    name: String,
    kind: &'static str,
}

pub(super) fn list(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["directory"])?;
    let directory = parsed.absolute_path("directory")?;
    let before = fs::symlink_metadata(&directory).map_err(|_| CliError::InvalidArgument)?;
    if !before.is_dir() || before.file_type().is_symlink() {
        return Err(CliError::InvalidArgument);
    }
    let held = File::from(
        open(
            &directory,
            OFlag::O_RDONLY
                | OFlag::O_DIRECTORY
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK
                | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| CliError::InvalidArgument)?,
    );
    let opened = held.metadata().map_err(|_| CliError::InvalidArgument)?;
    if !opened.is_dir() || (opened.dev(), opened.ino()) != (before.dev(), before.ino()) {
        return Err(CliError::StaleObservation);
    }

    // Linux procfs exposes the already-held descriptor as a directory. A replaced
    // selected name cannot redirect the iteration to another directory.
    let descriptor_path = format!("/proc/self/fd/{}", held.as_raw_fd());
    let read = fs::read_dir(descriptor_path).map_err(|_| CliError::InvalidArgument)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for (index, item) in read.enumerate() {
        if index >= MAX_SCANNED {
            truncated = true;
            break;
        }
        let item = item.map_err(|_| CliError::StaleObservation)?;
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') || name.is_empty() {
            continue;
        }
        let kind = item.file_type().map_err(|_| CliError::StaleObservation)?;
        let kind = if kind.is_dir() {
            "directory"
        } else if kind.is_file() {
            "file"
        } else if kind.is_symlink() {
            "symlink"
        } else {
            continue;
        };
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(ListedEntry { name, kind });
    }
    let after = fs::symlink_metadata(&directory).map_err(|_| CliError::StaleObservation)?;
    if !after.is_dir()
        || after.file_type().is_symlink()
        || (after.dev(), after.ino()) != (opened.dev(), opened.ino())
    {
        return Err(CliError::StaleObservation);
    }
    entries.sort_by(|left, right| {
        rank(left.kind)
            .cmp(&rank(right.kind))
            .then(left.name.cmp(&right.name))
    });
    let entries = entries
        .into_iter()
        .map(|entry| json!({"name": entry.name, "kind": entry.kind}))
        .collect::<Vec<Value>>();
    let response = serde_json::to_vec(&json!({
        "schema_version": 1,
        "directory": directory,
        "entries": entries,
        "truncated": truncated,
    }))
    .map_err(|_| CliError::Internal)?;
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(CliError::Refused);
    }
    String::from_utf8(response).map_err(|_| CliError::Internal)
}

const fn rank(kind: &str) -> u8 {
    match kind.as_bytes().first() {
        Some(b'd') => 0,
        Some(b'f') => 1,
        _ => 2,
    }
}
