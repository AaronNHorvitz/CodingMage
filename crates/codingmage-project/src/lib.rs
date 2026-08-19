//! Reusable project task sources, verification profiles, and isolation policy.

use std::{
    collections::BTreeSet,
    fmt, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use codingmage_plan::TaskPlan;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_SNAPSHOT_BYTES: u64 = 8 * 1024 * 1024;

/// Canonical local task source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskSource {
    /// Strict Markdown task plan.
    Markdown(PathBuf),
    /// Content-addressed local snapshot of one GitHub issue.
    GitHubIssueSnapshot(PathBuf),
}

/// Minimal local issue snapshot; remote content never becomes authority directly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubIssueSnapshot {
    /// Snapshot schema version.
    pub version: u16,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repository: String,
    /// Issue number.
    pub issue: u64,
    /// Exact remote version identity.
    pub remote_version: String,
    /// SHA-256 of the separately retained issue body.
    pub body_sha256: String,
    /// Canonical Markdown task plan derived and reviewed locally.
    pub canonical_plan: String,
}

/// Loaded canonical plan plus task-source provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedTaskSource {
    /// Strict parsed plan.
    pub plan: TaskPlan,
    /// Stable source family.
    pub source_kind: &'static str,
    /// SHA-256 of source bytes.
    pub source_sha256: String,
}

/// Loads one bounded local task source.
///
/// # Errors
///
/// Returns [`ProjectError`] for linked, oversized, malformed, stale, or unsupported input.
pub fn load_task_source(source: &TaskSource) -> Result<LoadedTaskSource, ProjectError> {
    let path = match source {
        TaskSource::Markdown(path) | TaskSource::GitHubIssueSnapshot(path) => path,
    };
    let metadata = fs::symlink_metadata(path).map_err(|_| ProjectError::TaskSource)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_SNAPSHOT_BYTES
    {
        return Err(ProjectError::TaskSource);
    }
    let bytes = fs::read(path).map_err(|_| ProjectError::TaskSource)?;
    match source {
        TaskSource::Markdown(_) => Ok(LoadedTaskSource {
            plan: TaskPlan::parse(&bytes).map_err(|_| ProjectError::TaskSource)?,
            source_kind: "markdown",
            source_sha256: hex(Sha256::digest(&bytes).as_ref()),
        }),
        TaskSource::GitHubIssueSnapshot(_) => {
            let snapshot: GitHubIssueSnapshot =
                serde_json::from_slice(&bytes).map_err(|_| ProjectError::TaskSource)?;
            validate_snapshot(&snapshot)?;
            Ok(LoadedTaskSource {
                plan: TaskPlan::parse(snapshot.canonical_plan.as_bytes())
                    .map_err(|_| ProjectError::TaskSource)?,
                source_kind: "github_issue_snapshot",
                source_sha256: hex(Sha256::digest(&bytes).as_ref()),
            })
        }
    }
}

fn validate_snapshot(snapshot: &GitHubIssueSnapshot) -> Result<(), ProjectError> {
    if snapshot.version != 1
        || snapshot.issue == 0
        || !component(&snapshot.owner)
        || !component(&snapshot.repository)
        || !component(&snapshot.remote_version)
        || !digest(&snapshot.body_sha256)
    {
        return Err(ProjectError::TaskSource);
    }
    Ok(())
}

/// Typed deterministic verification profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GateProfile {
    /// Rust workspace format, lint, and test profile.
    Rust,
    /// Python compile and test profile.
    Python,
    /// Node package test profile.
    Node,
    /// Documentation-only validation profile.
    Documentation,
    /// Explicit literal commands reviewed by the project owner.
    Custom(Vec<LiteralCommand>),
}

/// Required verification depth declared by a project.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TestTier {
    /// Changed-file and focused unit checks.
    Focused,
    /// Package or workspace integration checks.
    Package,
    /// Security, mutation, and recovery checks.
    Security,
    /// Packaging, platform, and release-candidate checks.
    Release,
}

/// One shell-free verification command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralCommand {
    /// Absolute executable path.
    pub executable: PathBuf,
    /// Literal argument vector.
    pub arguments: Vec<String>,
}

impl GateProfile {
    /// Returns the exact shell-free command profile.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::GateProfile`] for empty, relative, NUL-bearing, or excessive custom
    /// definitions.
    pub fn commands(&self) -> Result<Vec<LiteralCommand>, ProjectError> {
        let commands = match self {
            Self::Rust => vec![
                command("/usr/bin/cargo", &["fmt", "--all", "--", "--check"]),
                command(
                    "/usr/bin/cargo",
                    &["clippy", "--workspace", "--", "-D", "warnings"],
                ),
                command("/usr/bin/cargo", &["test", "--workspace", "--all-targets"]),
            ],
            Self::Python => vec![
                command("/usr/bin/python3", &["-m", "compileall", "-q", "."]),
                command("/usr/bin/python3", &["-m", "unittest", "discover"]),
            ],
            Self::Node => vec![command("/usr/bin/npm", &["test", "--", "--runInBand"])],
            Self::Documentation => vec![command("/usr/bin/python3", &["scripts/docs_check.py"])],
            Self::Custom(commands) => commands.clone(),
        };
        if commands.is_empty()
            || commands.len() > 128
            || commands.iter().any(|entry| {
                !entry.executable.is_absolute()
                    || entry.arguments.len() > 256
                    || entry
                        .arguments
                        .iter()
                        .any(|argument| argument.contains('\0'))
            })
        {
            return Err(ProjectError::GateProfile);
        }
        Ok(commands)
    }
}

fn command(executable: &str, arguments: &[&str]) -> LiteralCommand {
    LiteralCommand {
        executable: PathBuf::from(executable),
        arguments: arguments.iter().map(ToString::to_string).collect(),
    }
}

/// One project's explicit artifacts and denied operation families.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPolicy {
    /// Repository root.
    pub repository: PathBuf,
    /// Private state root.
    pub state: PathBuf,
    /// Owned worktree root.
    pub worktrees: PathBuf,
    /// Credential namespace label, not a credential value.
    pub credential_namespace: String,
    /// Session namespace label.
    pub session_namespace: String,
    /// Evidence namespace label.
    pub evidence_namespace: String,
    /// Expected relative artifact paths.
    pub expected_artifacts: BTreeSet<PathBuf>,
    /// Required verification depth for a completion candidate.
    pub required_test_tiers: BTreeSet<TestTier>,
    /// Stable prohibited operation labels.
    pub prohibited_operations: BTreeSet<String>,
    /// Verification profile.
    pub gates: GateProfile,
}

impl ProjectPolicy {
    /// Validates internal separation and explicit declarations.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Isolation`] for missing, overlapping, linked, or ambiguous policy.
    pub fn validate(&self) -> Result<(), ProjectError> {
        let roots = [&self.repository, &self.state, &self.worktrees]
            .into_iter()
            .map(|path| {
                let metadata = fs::symlink_metadata(path).map_err(|_| ProjectError::Isolation)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ProjectError::Isolation);
                }
                fs::canonicalize(path).map_err(|_| ProjectError::Isolation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if roots.iter().enumerate().any(|(index, left)| {
            roots
                .iter()
                .skip(index + 1)
                .any(|right| left.starts_with(right) || right.starts_with(left))
        }) || !component(&self.credential_namespace)
            || !component(&self.session_namespace)
            || !component(&self.evidence_namespace)
            || self.expected_artifacts.is_empty()
            || self.required_test_tiers.is_empty()
            || self.prohibited_operations.is_empty()
            || self.expected_artifacts.iter().any(|path| {
                path.is_absolute()
                    || path
                        .components()
                        .any(|part| !matches!(part, std::path::Component::Normal(_)))
            })
        {
            return Err(ProjectError::Isolation);
        }
        self.gates.commands()?;
        Ok(())
    }
}

/// Verifies two projects share no authority namespace or filesystem root.
///
/// # Errors
///
/// Returns [`ProjectError::Isolation`] on any overlap.
pub fn verify_isolation(left: &ProjectPolicy, right: &ProjectPolicy) -> Result<(), ProjectError> {
    left.validate()?;
    right.validate()?;
    let left_roots = [&left.repository, &left.state, &left.worktrees];
    let right_roots = [&right.repository, &right.state, &right.worktrees];
    if left_roots.iter().any(|left| {
        right_roots
            .iter()
            .any(|right| left.starts_with(right) || right.starts_with(left))
    }) || left.credential_namespace == right.credential_namespace
        || left.session_namespace == right.session_namespace
        || left.evidence_namespace == right.evidence_namespace
    {
        return Err(ProjectError::Isolation);
    }
    Ok(())
}

/// Registry of pairwise-isolated projects with exact in-process ownership.
#[derive(Clone, Debug)]
pub struct ProjectRegistry {
    projects: Arc<Vec<ProjectPolicy>>,
    active: Arc<Mutex<BTreeSet<usize>>>,
}

impl ProjectRegistry {
    /// Creates a registry after pairwise isolation validation.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Isolation`] for an empty registry or any overlapping pair.
    pub fn new(projects: Vec<ProjectPolicy>) -> Result<Self, ProjectError> {
        if projects.is_empty() {
            return Err(ProjectError::Isolation);
        }
        for (index, left) in projects.iter().enumerate() {
            left.validate()?;
            for right in projects.iter().skip(index + 1) {
                verify_isolation(left, right)?;
            }
        }
        Ok(Self {
            projects: Arc::new(projects),
            active: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }

    /// Claims one exact project until the returned lease is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Ownership`] for an unknown or already-active project.
    pub fn claim(&self, index: usize) -> Result<ProjectLease, ProjectError> {
        if self.projects.get(index).is_none() {
            return Err(ProjectError::Ownership);
        }
        let mut active = self.active.lock().map_err(|_| ProjectError::Ownership)?;
        if !active.insert(index) {
            return Err(ProjectError::Ownership);
        }
        Ok(ProjectLease {
            index,
            projects: Arc::clone(&self.projects),
            active: Arc::clone(&self.active),
        })
    }
}

/// Exact project ownership released automatically on drop.
#[derive(Debug)]
pub struct ProjectLease {
    index: usize,
    projects: Arc<Vec<ProjectPolicy>>,
    active: Arc<Mutex<BTreeSet<usize>>>,
}

impl ProjectLease {
    /// Returns the exact leased policy.
    #[must_use]
    pub fn policy(&self) -> &ProjectPolicy {
        &self.projects[self.index]
    }
}

impl Drop for ProjectLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.index);
        }
    }
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// Stable project-adapter error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectError {
    /// Task source is unsafe or malformed.
    TaskSource,
    /// Gate profile is unsafe or malformed.
    GateProfile,
    /// Project authority overlaps or is incomplete.
    Isolation,
    /// Exact project is unknown or already owned.
    Ownership,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TaskSource => "codingmage.project.task_source",
            Self::GateProfile => "codingmage.project.gate_profile",
            Self::Isolation => "codingmage.project.isolation",
            Self::Ownership => "codingmage.project.ownership",
        })
    }
}

impl std::error::Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("codingmage-project-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn plan() -> &'static str {
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the fixture task safely.\n"
    }

    #[test]
    fn markdown_and_github_snapshot_produce_local_canonical_plans() {
        let root = root("sources");
        let markdown = root.join("TASKS.md");
        fs::write(&markdown, plan()).unwrap();
        let loaded = load_task_source(&TaskSource::Markdown(markdown)).unwrap();
        assert_eq!(loaded.source_kind, "markdown");

        let snapshot_path = root.join("issue.json");
        let snapshot = GitHubIssueSnapshot {
            version: 1,
            owner: "owner".to_owned(),
            repository: "repo".to_owned(),
            issue: 1,
            remote_version: "etag-1".to_owned(),
            body_sha256: "a".repeat(64),
            canonical_plan: plan().to_owned(),
        };
        fs::write(&snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let loaded = load_task_source(&TaskSource::GitHubIssueSnapshot(snapshot_path)).unwrap();
        assert_eq!(loaded.source_kind, "github_issue_snapshot");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn typed_and_custom_gate_profiles_are_literal_and_bounded() {
        for profile in [
            GateProfile::Rust,
            GateProfile::Python,
            GateProfile::Node,
            GateProfile::Documentation,
        ] {
            assert!(!profile.commands().unwrap().is_empty());
        }
        assert_eq!(
            GateProfile::Custom(vec![LiteralCommand {
                executable: PathBuf::from("relative"),
                arguments: Vec::new(),
            }])
            .commands(),
            Err(ProjectError::GateProfile)
        );
    }

    #[test]
    fn unrelated_projects_are_isolated_and_every_namespace_matters() {
        let root = root("isolation");
        let policy = |label: &str| {
            let repository = root.join(format!("{label}-repository"));
            let state = root.join(format!("{label}-state"));
            let worktrees = root.join(format!("{label}-worktrees"));
            for path in [&repository, &state, &worktrees] {
                fs::create_dir(path).unwrap();
            }
            ProjectPolicy {
                repository,
                state,
                worktrees,
                credential_namespace: format!("{label}-credentials"),
                session_namespace: format!("{label}-sessions"),
                evidence_namespace: format!("{label}-evidence"),
                expected_artifacts: BTreeSet::from([PathBuf::from("target/artifact")]),
                required_test_tiers: BTreeSet::from([TestTier::Focused, TestTier::Package]),
                prohibited_operations: BTreeSet::from(["force-push".to_owned()]),
                gates: GateProfile::Rust,
            }
        };
        let left = policy("left");
        let mut right = policy("right");
        verify_isolation(&left, &right).unwrap();
        right.session_namespace.clone_from(&left.session_namespace);
        assert_eq!(
            verify_isolation(&left, &right),
            Err(ProjectError::Isolation)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_projects_run_sequentially_and_concurrently_without_cross_ownership() {
        let root = root("registry");
        let policy = |label: &str| {
            let repository = root.join(format!("{label}-repository"));
            let state = root.join(format!("{label}-state"));
            let worktrees = root.join(format!("{label}-worktrees"));
            for path in [&repository, &state, &worktrees] {
                fs::create_dir(path).unwrap();
            }
            ProjectPolicy {
                repository,
                state,
                worktrees,
                credential_namespace: format!("{label}-credentials"),
                session_namespace: format!("{label}-sessions"),
                evidence_namespace: format!("{label}-evidence"),
                expected_artifacts: BTreeSet::from([PathBuf::from("artifact")]),
                required_test_tiers: BTreeSet::from([TestTier::Focused]),
                prohibited_operations: BTreeSet::from(["force-push".to_owned()]),
                gates: GateProfile::Documentation,
            }
        };
        let registry = ProjectRegistry::new(vec![policy("left"), policy("right")]).unwrap();
        {
            let left = registry.claim(0).unwrap();
            assert_eq!(registry.claim(0).unwrap_err(), ProjectError::Ownership);
            assert!(left.policy().state.ends_with("left-state"));
        }
        registry.claim(0).unwrap();

        let left_registry = registry.clone();
        let right_registry = registry.clone();
        let left = std::thread::spawn(move || {
            let lease = left_registry.claim(0).unwrap();
            lease.policy().evidence_namespace.clone()
        });
        let right = std::thread::spawn(move || {
            let lease = right_registry.claim(1).unwrap();
            lease.policy().evidence_namespace.clone()
        });
        assert_eq!(left.join().unwrap(), "left-evidence");
        assert_eq!(right.join().unwrap(), "right-evidence");
        fs::remove_dir_all(root).unwrap();
    }
}
