//! Read-only project loading through the existing configuration and task-plan contracts.
//!
//! Opening a project validates the selected configuration with `codingmage-core`, parses the
//! task source with `codingmage-plan`, and records digests. It starts no process and changes no
//! file.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_core::{Config, ConfigLoadError, load_config};
use codingmage_plan::{PlanError, TaskPlan};
use sha2::{Digest, Sha256};

/// Largest task source the interface reads.
pub const MAX_TASK_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

/// One opened project.
#[derive(Clone, Debug)]
pub struct Project {
    /// Absolute configuration path.
    pub config_path: PathBuf,
    /// Validated configuration.
    pub config: Config,
    /// Parsed task source, or the reason it could not be parsed.
    pub plan: Result<LoadedPlan, PlanLoadError>,
}

/// Parsed task source with its digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedPlan {
    /// Strict parse.
    pub plan: TaskPlan,
    /// SHA-256 of the exact bytes read.
    pub source_sha256: String,
    /// Bytes read from disk.
    pub byte_length: usize,
}

/// Why a project could not be opened.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenError {
    /// The configuration path is not absolute.
    RelativePath,
    /// The configuration failed the existing loader.
    Config(ConfigLoadError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativePath => formatter.write_str("the configuration path must be absolute"),
            Self::Config(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// Why the task source could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanLoadError {
    /// The task source file is missing, linked, oversized or unreadable.
    Unavailable,
    /// The strict grammar rejected the source.
    Invalid(PlanError),
}

impl fmt::Display for PlanLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str(
                "the task source is missing, a symbolic link, larger than 8 MiB or unreadable",
            ),
            Self::Invalid(error) => write!(formatter, "the task source is invalid: {error}"),
        }
    }
}

impl std::error::Error for PlanLoadError {}

impl Project {
    /// Opens one configuration without side effects.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the configuration cannot be validated. A task-source failure is
    /// retained inside the project so the workspace can show it as a real failure state.
    pub fn open(config_path: &Path) -> Result<Self, OpenError> {
        if !config_path.is_absolute() {
            return Err(OpenError::RelativePath);
        }
        let config = load_config(config_path).map_err(OpenError::Config)?;
        let plan = load_plan(&config);
        Ok(Self {
            config_path: config_path.to_path_buf(),
            config,
            plan,
        })
    }

    /// Re-reads the task source without touching the configuration.
    pub fn reload_plan(&mut self) {
        self.plan = load_plan(&self.config);
    }

    /// Absolute task-source path.
    #[must_use]
    pub fn task_source_path(&self) -> PathBuf {
        self.config.target_path.join(&self.config.task_source)
    }
}

/// Parses task-source bytes from the configured location.
///
/// # Errors
///
/// Returns [`PlanLoadError`] for unavailable or invalid sources.
pub fn load_plan(config: &Config) -> Result<LoadedPlan, PlanLoadError> {
    let path = config.target_path.join(&config.task_source);
    parse_plan_file(&path)
}

/// Parses one task-source file.
///
/// # Errors
///
/// Returns [`PlanLoadError`] for unavailable or invalid sources.
pub fn parse_plan_file(path: &Path) -> Result<LoadedPlan, PlanLoadError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PlanLoadError::Unavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_TASK_SOURCE_BYTES
    {
        return Err(PlanLoadError::Unavailable);
    }
    let bytes = fs::read(path).map_err(|_| PlanLoadError::Unavailable)?;
    parse_plan_bytes(&bytes)
}

/// Parses task-source bytes.
///
/// # Errors
///
/// Returns [`PlanLoadError::Invalid`] when the strict grammar rejects the bytes.
pub fn parse_plan_bytes(bytes: &[u8]) -> Result<LoadedPlan, PlanLoadError> {
    let plan = TaskPlan::parse(bytes).map_err(PlanLoadError::Invalid)?;
    Ok(LoadedPlan {
        source_sha256: hex(&Sha256::digest(bytes)),
        byte_length: bytes.len(),
        plan,
    })
}

/// Lower-case hexadecimal encoding.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// SHA-256 of one bounded regular file.
///
/// # Errors
///
/// Returns `None` when the file is missing, linked, not regular or larger than `max_bytes`.
#[must_use]
pub fn file_sha256(path: &Path, max_bytes: u64) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(hex(&Sha256::digest(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_reports_config_and_plan_failures_distinctly() {
        assert_eq!(
            Project::open(Path::new("relative.toml")).unwrap_err(),
            OpenError::RelativePath
        );
        assert!(matches!(
            Project::open(Path::new("/nonexistent/codingmage.toml")).unwrap_err(),
            OpenError::Config(ConfigLoadError::Unavailable)
        ));
        assert!(matches!(
            parse_plan_bytes(b"not a plan"),
            Err(PlanLoadError::Invalid(_))
        ));
        let plan = parse_plan_bytes(
            b"# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the fixture task safely.\n",
        )
        .unwrap();
        assert_eq!(plan.plan.items.len(), 2);
        assert_eq!(plan.source_sha256.len(), 64);
    }
}
