//! Project snapshots from the public coordinator command and legacy local parsers.
//!
//! Native Open actions decode a strict `project-open` response. Local parsing remains for
//! isolated fixtures and guided-configuration recovery, which is still an open migration.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_core::{Config, ConfigLoadError, load_config};
use codingmage_plan::{PlanError, TaskPlan};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::backend::BackendError;

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
    /// The coordinator read failed.
    Backend(BackendError),
    /// The coordinator returned an unexpected project snapshot.
    Contract,
    /// The coordinator uses an unsupported project snapshot schema.
    UnsupportedSchema,
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativePath => formatter.write_str("the configuration path must be absolute"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Backend(error) => write!(formatter, "{}", error.code()),
            Self::Contract => {
                formatter.write_str("project snapshot did not match the expected contract")
            }
            Self::UnsupportedSchema => {
                formatter.write_str("project snapshot uses an unsupported schema")
            }
        }
    }
}

impl std::error::Error for OpenError {}

/// Why a current task plan cannot be shown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanLoadError {
    /// The task source file is missing, linked, oversized or unreadable.
    Unavailable,
    /// The strict grammar rejected the source.
    Invalid(PlanError),
    /// The parsed task source exceeded the native command output bound.
    ProjectionTooLarge,
    /// A later coordinator diagnosis observed different task-source bytes.
    Stale,
}

impl fmt::Display for PlanLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str(
                "the task source is missing, a symbolic link, larger than 8 MiB or unreadable",
            ),
            Self::Invalid(error) => write!(formatter, "the task source is invalid: {error}"),
            Self::ProjectionTooLarge => {
                formatter.write_str("the task source projection exceeds 8 MiB")
            }
            Self::Stale => formatter.write_str("the task source changed after it was opened"),
        }
    }
}

impl std::error::Error for PlanLoadError {}

impl Project {
    /// Decodes one versioned read-only snapshot returned by `codingmage project-open`.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported schema, unexpected selected path, malformed digest or plan payload.
    pub fn from_snapshot(config_path: &Path, bytes: &[u8]) -> Result<Self, OpenError> {
        let schema: SnapshotSchema =
            serde_json::from_slice(bytes).map_err(|_| OpenError::Contract)?;
        if schema.schema_version != 1 {
            return Err(OpenError::UnsupportedSchema);
        }
        let snapshot: ProjectSnapshot =
            serde_json::from_slice(bytes).map_err(|_| OpenError::Contract)?;
        debug_assert_eq!(snapshot.schema_version, 1);
        if snapshot.config_path != config_path
            || snapshot.config.version != 1
            || !snapshot.config.target_path.is_absolute()
            || !valid_sha256(&snapshot.config_sha256)
        {
            return Err(OpenError::Contract);
        }
        let plan = match snapshot.plan {
            PlanSnapshot::Loaded {
                plan,
                source_sha256,
                byte_length,
            } => {
                if !valid_sha256(&source_sha256)
                    || plan.version != 1
                    || plan.source_sha256 != source_sha256
                    || byte_length == 0
                    || byte_length as u64 > MAX_TASK_SOURCE_BYTES
                {
                    return Err(OpenError::Contract);
                }
                Ok(LoadedPlan {
                    plan,
                    source_sha256,
                    byte_length,
                })
            }
            PlanSnapshot::Invalid { code } => Err(PlanLoadError::Invalid(
                parse_plan_error_code(&code).ok_or(OpenError::Contract)?,
            )),
            PlanSnapshot::Unavailable => Err(PlanLoadError::Unavailable),
            PlanSnapshot::ProjectionTooLarge => Err(PlanLoadError::ProjectionTooLarge),
        };
        Ok(Self {
            config_path: snapshot.config_path,
            config: snapshot.config,
            plan,
        })
    }

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

#[derive(Deserialize)]
struct SnapshotSchema {
    schema_version: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSnapshot {
    schema_version: u16,
    config_path: PathBuf,
    config_sha256: String,
    config: Config,
    plan: PlanSnapshot,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum PlanSnapshot {
    Loaded {
        plan: TaskPlan,
        source_sha256: String,
        byte_length: usize,
    },
    Invalid {
        code: String,
    },
    Unavailable,
    ProjectionTooLarge,
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_plan_error_code(code: &str) -> Option<PlanError> {
    Some(match code {
        "codingmage.plan.invalid_source" => PlanError::InvalidSource,
        "codingmage.plan.duplicate_id" => PlanError::DuplicateId,
        "codingmage.plan.missing_parent" => PlanError::MissingParent,
        "codingmage.plan.malformed_hierarchy" => PlanError::MalformedHierarchy,
        "codingmage.plan.missing_goal" => PlanError::MissingGoal,
        "codingmage.plan.invalid_dependency" => PlanError::InvalidDependency,
        "codingmage.plan.conflicting_state" => PlanError::ConflictingState,
        _ => return None,
    })
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
