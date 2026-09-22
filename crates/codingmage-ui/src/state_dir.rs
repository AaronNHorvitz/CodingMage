//! Private interface state: recent repositories, launch records and admissions.
//!
//! Interface state never lives inside a target repository or a coordinator campaign directory.
//! Files are created with `0600` and directories with `0700`.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Largest private state document the interface reads.
pub const MAX_STATE_BYTES: u64 = 1024 * 1024;
/// Maximum retained recent repositories.
pub const MAX_RECENT: usize = 12;

/// Failure to read or write private interface state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    /// The location is unavailable or not writable.
    Unavailable,
    /// The document was linked, oversized or malformed.
    Invalid,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "private interface state is unavailable",
            Self::Invalid => "private interface state is invalid",
        })
    }
}

impl std::error::Error for StateError {}

/// Returns the per-user interface configuration directory.
///
/// # Errors
///
/// Returns [`StateError::Unavailable`] when neither `XDG_CONFIG_HOME` nor `HOME` is set.
pub fn user_config_dir() -> Result<PathBuf, StateError> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Ok(path.join("codingmage-ui"));
        }
    }
    let home = std::env::var_os("HOME").ok_or(StateError::Unavailable)?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(StateError::Unavailable);
    }
    Ok(home.join(".config").join("codingmage-ui"))
}

/// Creates a private directory tree.
///
/// # Errors
///
/// Returns [`StateError::Unavailable`] when creation or permission tightening fails.
pub fn ensure_private_dir(path: &Path) -> Result<(), StateError> {
    fs::create_dir_all(path).map_err(|_| StateError::Unavailable)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| StateError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StateError::Unavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| StateError::Unavailable)?;
    }
    Ok(())
}

/// Atomically writes a private document.
///
/// # Errors
///
/// Returns [`StateError::Unavailable`] on any I/O failure.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), StateError> {
    let parent = path.parent().ok_or(StateError::Unavailable)?;
    ensure_private_dir(parent)?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(StateError::Invalid);
    }
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| StateError::Unavailable)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| StateError::Unavailable);
    drop(file);
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, path).map_err(|_| StateError::Unavailable)
}

/// Reads a private document.
///
/// # Errors
///
/// Returns [`StateError::Invalid`] for linked or oversized files and
/// [`StateError::Unavailable`] when the file cannot be read.
pub fn read_private(path: &Path) -> Result<Vec<u8>, StateError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StateError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_STATE_BYTES
    {
        return Err(StateError::Invalid);
    }
    fs::read(path).map_err(|_| StateError::Unavailable)
}

/// Recently opened configurations.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecentProjects {
    /// Document version.
    #[serde(default = "recent_version")]
    pub version: u16,
    /// Absolute configuration paths, most recent first.
    #[serde(default)]
    pub configs: Vec<PathBuf>,
}

const fn recent_version() -> u16 {
    1
}

impl RecentProjects {
    /// Loads the recent list; a missing file yields an empty list.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the document exists but is invalid.
    pub fn load(directory: &Path) -> Result<Self, StateError> {
        let path = directory.join("recent.json");
        if !path.exists() {
            return Ok(Self {
                version: 1,
                configs: Vec::new(),
            });
        }
        let bytes = read_private(&path)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
        if value.version != 1 || value.configs.iter().any(|path| !path.is_absolute()) {
            return Err(StateError::Invalid);
        }
        Ok(value)
    }

    /// Records one configuration path as most recent and persists the bounded list.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when persistence fails.
    pub fn remember(&mut self, directory: &Path, config: &Path) -> Result<(), StateError> {
        if !config.is_absolute() {
            return Err(StateError::Invalid);
        }
        self.configs.retain(|existing| existing != config);
        self.configs.insert(0, config.to_path_buf());
        self.configs.truncate(MAX_RECENT);
        self.version = 1;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| StateError::Invalid)?;
        write_private(&directory.join("recent.json"), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_round_trips_bounded_and_private() {
        let root = std::env::temp_dir().join(format!("codingmage-ui-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut recent = RecentProjects::load(&root).unwrap();
        for index in 0..(MAX_RECENT + 3) {
            recent
                .remember(&root, &PathBuf::from(format!("/tmp/config-{index}.toml")))
                .unwrap();
        }
        assert_eq!(recent.configs.len(), MAX_RECENT);
        assert_eq!(
            recent.configs[0],
            PathBuf::from(format!("/tmp/config-{}.toml", MAX_RECENT + 2))
        );
        let reloaded = RecentProjects::load(&root).unwrap();
        assert_eq!(reloaded, recent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(root.join("recent.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert_eq!(
            recent.remember(&root, Path::new("relative.toml")),
            Err(StateError::Invalid)
        );
        fs::write(
            root.join("recent.json"),
            b"{\"version\":1,\"configs\":[\"relative\"]}",
        )
        .unwrap();
        assert_eq!(RecentProjects::load(&root), Err(StateError::Invalid));
        fs::remove_dir_all(root).unwrap();
    }
}

/// Per-configuration memory: the campaign specification last selected for a repository.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMemory {
    /// Document version.
    #[serde(default = "recent_version")]
    pub version: u16,
    /// Absolute campaign specification path last selected.
    #[serde(default)]
    pub campaign_spec: Option<PathBuf>,
}

impl ProjectMemory {
    fn path(directory: &Path, config: &Path) -> PathBuf {
        use sha2::{Digest as _, Sha256};
        let digest = Sha256::digest(config.as_os_str().as_encoded_bytes());
        directory
            .join("projects")
            .join(format!("{}.json", crate::project::hex(&digest)))
    }

    /// Loads the memory for one configuration; missing means empty.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the document exists but is invalid.
    pub fn load(directory: &Path, config: &Path) -> Result<Self, StateError> {
        let path = Self::path(directory, config);
        if !path.exists() {
            return Ok(Self {
                version: 1,
                campaign_spec: None,
            });
        }
        let bytes = read_private(&path)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
        if value.version != 1
            || value
                .campaign_spec
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return Err(StateError::Invalid);
        }
        Ok(value)
    }

    /// Persists the memory for one configuration.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when persistence fails.
    pub fn save(&self, directory: &Path, config: &Path) -> Result<(), StateError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| StateError::Invalid)?;
        write_private(&Self::path(directory, config), &bytes)
    }
}
