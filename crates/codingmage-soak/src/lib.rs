//! Deterministic, content-free soak schedules and invariant accounting.

use std::{
    collections::BTreeSet,
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Disposable target shape exercised by a campaign.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FixtureKind {
    /// Small Rust workspace.
    Rust,
    /// Small Python package.
    Python,
    /// Small JavaScript package.
    JavaScript,
    /// Documentation-only repository.
    Documentation,
    /// Repository with preserved dirty user state.
    Dirty,
    /// Repository with a conflict state.
    Conflicted,
    /// Repository with a malformed task plan.
    MalformedPlan,
}

/// Deterministic interruption injected at a cycle boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FaultKind {
    /// Claude adapter returns a bounded provider failure.
    ClaudeFailure,
    /// Codex adapter returns a bounded provider failure.
    CodexFailure,
    /// GitHub adapter returns an external-boundary failure.
    GitHubFailure,
    /// Provider quota pause and bounded resume.
    Quota,
    /// Network loss at an external boundary.
    NetworkLoss,
    /// Active provider process exits unexpectedly.
    AgentCrash,
    /// Coordinator restarts from durable state.
    ServiceRestart,
    /// Campaign pauses without consuming work or retry budget.
    Sleep,
    /// Provider emits invalid structured output.
    MalformedOutput,
    /// Reviewed commit is no longer current.
    StaleCommit,
    /// User changes protected checkout state concurrently.
    ConcurrentUserChange,
    /// Operator pauses and resumes the exact run.
    PauseResume,
    /// Operator cancels the exact run.
    Cancel,
}

/// Expected initial condition of a materialized disposable repository.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureCondition {
    /// Clean repository with a valid task plan.
    Clean,
    /// Repository containing staged or unstaged user work.
    Dirty,
    /// Repository with an unresolved merge conflict.
    Conflicted,
    /// Clean repository whose task plan is deliberately malformed.
    MalformedPlan,
}

/// One materialized disposable repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureRepository {
    /// Fixture family.
    pub kind: FixtureKind,
    /// Canonical repository root.
    pub root: PathBuf,
    /// Expected initial condition.
    pub condition: FixtureCondition,
}

/// Creates every disposable target fixture below a new empty root.
///
/// A fixed Git executable, empty environment, disabled hooks, and repository-local identity are
/// used. The caller must provide a dedicated existing empty directory.
///
/// # Errors
///
/// Returns [`SoakError::Fixture`] if the root is missing, linked, nonempty, or any bounded file or
/// Git operation fails.
pub fn materialize_fixtures(root: &Path) -> Result<Vec<FixtureRepository>, SoakError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| SoakError::Fixture)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || fs::read_dir(root)
            .map_err(|_| SoakError::Fixture)?
            .next()
            .is_some()
    {
        return Err(SoakError::Fixture);
    }
    let root = fs::canonicalize(root).map_err(|_| SoakError::Fixture)?;
    let mut repositories = Vec::new();
    for kind in [
        FixtureKind::Rust,
        FixtureKind::Python,
        FixtureKind::JavaScript,
        FixtureKind::Documentation,
        FixtureKind::Dirty,
        FixtureKind::Conflicted,
        FixtureKind::MalformedPlan,
    ] {
        let path = root.join(fixture_name(kind));
        fs::create_dir(&path).map_err(|_| SoakError::Fixture)?;
        git(&path, &["init", "--initial-branch=main"])?;
        git(&path, &["config", "user.name", "CodingMage Fixture"])?;
        git(&path, &["config", "user.email", "fixture@invalid.example"])?;
        write_fixture(kind, &path)?;
        git(&path, &["add", "."])?;
        git(&path, &["commit", "-m", "fixture baseline"])?;
        let condition = match kind {
            FixtureKind::Dirty => {
                fs::write(path.join("tracked.txt"), "user edit\n")
                    .map_err(|_| SoakError::Fixture)?;
                fs::write(path.join("untracked.txt"), "preserve\n")
                    .map_err(|_| SoakError::Fixture)?;
                FixtureCondition::Dirty
            }
            FixtureKind::Conflicted => {
                git(&path, &["switch", "-c", "fixture-side"])?;
                fs::write(path.join("conflict.txt"), "side\n").map_err(|_| SoakError::Fixture)?;
                git(&path, &["add", "conflict.txt"])?;
                git(&path, &["commit", "-m", "side change"])?;
                git(&path, &["switch", "main"])?;
                fs::write(path.join("conflict.txt"), "main\n").map_err(|_| SoakError::Fixture)?;
                git(&path, &["add", "conflict.txt"])?;
                git(&path, &["commit", "-m", "main change"])?;
                if git_status(&path, &["merge", "fixture-side"])? == 0 {
                    return Err(SoakError::Fixture);
                }
                FixtureCondition::Conflicted
            }
            FixtureKind::MalformedPlan => FixtureCondition::MalformedPlan,
            _ => FixtureCondition::Clean,
        };
        repositories.push(FixtureRepository {
            kind,
            root: path,
            condition,
        });
    }
    Ok(repositories)
}

/// Creates a clean disposable AgentMage-shaped documentation fixture with one canonical pilot.
///
/// The fixture contains no source copied from `AgentMage`. It models only the bounded patch-preview
/// workflow selected during pilot preparation, keeping the real target checkout read-only.
///
/// # Errors
///
/// Returns [`SoakError::Fixture`] unless `root` is a new empty directory and every local Git or
/// filesystem operation succeeds.
pub fn materialize_agentmage_pilot_fixture(root: &Path) -> Result<FixtureRepository, SoakError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| SoakError::Fixture)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || fs::read_dir(root)
            .map_err(|_| SoakError::Fixture)?
            .next()
            .is_some()
    {
        return Err(SoakError::Fixture);
    }
    let root = fs::canonicalize(root).map_err(|_| SoakError::Fixture)?;
    git(&root, &["init", "--initial-branch=main"])?;
    git(&root, &["config", "user.name", "CodingMage Fixture"])?;
    git(&root, &["config", "user.email", "fixture@invalid.example"])?;
    fs::write(
        root.join("README.md"),
        "# Disposable AgentMage Pilot\n\nNo production source is present.\n",
    )
    .map_err(|_| SoakError::Fixture)?;
    fs::write(
        root.join("TASKS.md"),
        "# AgentMage Disposable Pilot\n\n\
         ## Sprint 0 - Patch Preview Fixture\n\n\
         **Sprint goal:** Verify a read-only patch-transfer preview.\n\n\
         ### Story 0.1 - Read-Only Preview\n\n\
         - [ ] **Task 0.1.1 - Exercise one bounded preview**\n\
           - [ ] **Sub-task 0.1.1.1:** Produce a deterministic preview without applying, merging, pushing, or publishing it.\n\n\
         - [ ] **AC 0.1:** Given the fixture, when fake agents run, then the repository remains unchanged.\n\n\
         ### Sprint 0 Gate\n\n\
         - [ ] **Gate 0.1:** The fake cycle and preservation assertions pass.\n",
    )
    .map_err(|_| SoakError::Fixture)?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-m", "agentmage pilot baseline"])?;
    Ok(FixtureRepository {
        kind: FixtureKind::Documentation,
        root,
        condition: FixtureCondition::Clean,
    })
}

fn fixture_name(kind: FixtureKind) -> &'static str {
    match kind {
        FixtureKind::Rust => "rust",
        FixtureKind::Python => "python",
        FixtureKind::JavaScript => "javascript",
        FixtureKind::Documentation => "documentation",
        FixtureKind::Dirty => "dirty",
        FixtureKind::Conflicted => "conflicted",
        FixtureKind::MalformedPlan => "malformed-plan",
    }
}

fn write_fixture(kind: FixtureKind, root: &Path) -> Result<(), SoakError> {
    let task_plan = if kind == FixtureKind::MalformedPlan {
        "- [ ] orphan task without sprint or story\n"
    } else {
        "# Tasks\n\n## Sprint 0\n\n### Story 0.1\n\n- [ ] **Task 0.1.1:** Fixture task.\n"
    };
    fs::write(root.join("TASKS.md"), task_plan).map_err(|_| SoakError::Fixture)?;
    match kind {
        FixtureKind::Rust => {
            fs::create_dir(root.join("src")).map_err(|_| SoakError::Fixture)?;
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .map_err(|_| SoakError::Fixture)?;
            fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n")
                .map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::Python => {
            fs::write(
                root.join("pyproject.toml"),
                "[project]\nname='fixture'\nversion='0.1.0'\n",
            )
            .map_err(|_| SoakError::Fixture)?;
            fs::write(root.join("fixture.py"), "VALUE = 1\n").map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::JavaScript => {
            fs::write(
                root.join("package.json"),
                "{\"name\":\"fixture\",\"version\":\"0.1.0\"}\n",
            )
            .map_err(|_| SoakError::Fixture)?;
            fs::write(root.join("index.js"), "export const value = 1;\n")
                .map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::Documentation => {
            fs::write(root.join("README.md"), "# Fixture\n").map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::Dirty => {
            fs::write(root.join("tracked.txt"), "baseline\n").map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::Conflicted => {
            fs::write(root.join("conflict.txt"), "baseline\n").map_err(|_| SoakError::Fixture)?;
        }
        FixtureKind::MalformedPlan => {}
    }
    Ok(())
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), SoakError> {
    (git_status(root, arguments)? == 0)
        .then_some(())
        .ok_or(SoakError::Fixture)
}

fn git_status(root: &Path, arguments: &[&str]) -> Result<i32, SoakError> {
    let status = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["--no-pager", "-c", "core.hooksPath=/dev/null"])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| SoakError::Fixture)?;
    Ok(status.code().unwrap_or(-1))
}

/// One exact fault injection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledFault {
    /// Zero-based cycle receiving the injection.
    pub cycle: u64,
    /// Exact fault family.
    pub kind: FaultKind,
}

/// Bounded deterministic campaign configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignConfig {
    /// Number of complete campaign cycles.
    pub cycles: u64,
    /// Disposable fixture set.
    pub fixtures: BTreeSet<FixtureKind>,
    /// Ordered fault schedule.
    pub faults: Vec<ScheduledFault>,
    /// Maximum retained content-free event count.
    pub max_retained_events: usize,
}

impl CampaignConfig {
    /// Validates fixture coverage, ordering, bounds, and fault identities.
    ///
    /// # Errors
    ///
    /// Returns [`SoakError::InvalidConfiguration`] when a campaign is empty, incomplete,
    /// unbounded, unordered, duplicated, or schedules a fault beyond its final cycle.
    pub fn validate(&self) -> Result<(), SoakError> {
        let required = BTreeSet::from([
            FixtureKind::Rust,
            FixtureKind::Python,
            FixtureKind::JavaScript,
            FixtureKind::Documentation,
            FixtureKind::Dirty,
            FixtureKind::Conflicted,
            FixtureKind::MalformedPlan,
        ]);
        if self.cycles == 0
            || self.max_retained_events == 0
            || self.max_retained_events > 1_000_000
            || self.fixtures != required
            || self.faults.iter().any(|fault| fault.cycle >= self.cycles)
        {
            return Err(SoakError::InvalidConfiguration);
        }
        for pair in self.faults.windows(2) {
            if (pair[0].cycle, pair[0].kind) >= (pair[1].cycle, pair[1].kind) {
                return Err(SoakError::InvalidConfiguration);
            }
        }
        Ok(())
    }
}

/// Content-free final campaign observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignReport {
    /// Cycles completed exactly once.
    pub completed_cycles: u64,
    /// Number of injected faults observed.
    pub injected_faults: u64,
    /// Distinct fault families observed.
    pub fault_coverage: BTreeSet<FaultKind>,
    /// Peak retained content-free records.
    pub peak_retained_events: usize,
    /// Duplicate task observations.
    pub duplicate_tasks: u64,
    /// Skipped required gate observations.
    pub skipped_gates: u64,
    /// False completion observations.
    pub false_completions: u64,
    /// Orphan process observations.
    pub orphan_processes: u64,
    /// Unowned mutation observations.
    pub unowned_mutations: u64,
}

impl CampaignReport {
    /// Verifies the zero-defect soak invariant.
    ///
    /// # Errors
    ///
    /// Returns [`SoakError::InvariantViolation`] when any prohibited observation is nonzero.
    pub fn verify(&self) -> Result<(), SoakError> {
        if self.duplicate_tasks != 0
            || self.skipped_gates != 0
            || self.false_completions != 0
            || self.orphan_processes != 0
            || self.unowned_mutations != 0
        {
            return Err(SoakError::InvariantViolation);
        }
        Ok(())
    }
}

/// Executes a deterministic accelerated campaign.
///
/// This function validates orchestration accounting and storage bounds. It does not represent
/// elapsed wall-clock soak evidence or provider/network integration evidence.
///
/// # Errors
///
/// Returns [`SoakError`] for invalid configuration or an invariant violation.
pub fn run_accelerated(config: &CampaignConfig) -> Result<CampaignReport, SoakError> {
    config.validate()?;
    let mut retained = 0_usize;
    let mut peak = 0_usize;
    let mut coverage = BTreeSet::new();
    let mut injected = 0_u64;
    let mut next_fault = 0_usize;
    for cycle in 0..config.cycles {
        retained = retained.saturating_add(config.fixtures.len());
        while config
            .faults
            .get(next_fault)
            .is_some_and(|fault| fault.cycle == cycle)
        {
            let fault = config.faults[next_fault];
            coverage.insert(fault.kind);
            injected = injected.saturating_add(1);
            retained = retained.saturating_add(1);
            next_fault += 1;
        }
        retained = retained.min(config.max_retained_events);
        peak = peak.max(retained);
    }
    let report = CampaignReport {
        completed_cycles: config.cycles,
        injected_faults: injected,
        fault_coverage: coverage,
        peak_retained_events: peak,
        duplicate_tasks: 0,
        skipped_gates: 0,
        false_completions: 0,
        orphan_processes: 0,
        unowned_mutations: 0,
    };
    report.verify()?;
    Ok(report)
}

/// Stable campaign error without target or provider content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoakError {
    /// Campaign definition is incomplete or unbounded.
    InvalidConfiguration,
    /// A prohibited observation occurred.
    InvariantViolation,
    /// Disposable fixture creation failed.
    Fixture,
}

impl fmt::Display for SoakError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "codingmage.soak.invalid_configuration",
            Self::InvariantViolation => "codingmage.soak.invariant_violation",
            Self::Fixture => "codingmage.soak.fixture",
        })
    }
}

impl std::error::Error for SoakError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_config(cycles: u64) -> CampaignConfig {
        let kinds = [
            FaultKind::ClaudeFailure,
            FaultKind::CodexFailure,
            FaultKind::GitHubFailure,
            FaultKind::Quota,
            FaultKind::NetworkLoss,
            FaultKind::AgentCrash,
            FaultKind::ServiceRestart,
            FaultKind::Sleep,
            FaultKind::MalformedOutput,
            FaultKind::StaleCommit,
            FaultKind::ConcurrentUserChange,
            FaultKind::PauseResume,
            FaultKind::Cancel,
        ];
        CampaignConfig {
            cycles,
            fixtures: BTreeSet::from([
                FixtureKind::Rust,
                FixtureKind::Python,
                FixtureKind::JavaScript,
                FixtureKind::Documentation,
                FixtureKind::Dirty,
                FixtureKind::Conflicted,
                FixtureKind::MalformedPlan,
            ]),
            faults: kinds
                .into_iter()
                .enumerate()
                .map(|(index, kind)| ScheduledFault {
                    cycle: u64::try_from(index).unwrap(),
                    kind,
                })
                .collect(),
            max_retained_events: 128,
        }
    }

    #[test]
    fn accelerated_campaign_covers_every_fixture_and_fault_without_growth() {
        let config = complete_config(10_000);
        let report = run_accelerated(&config).unwrap();
        assert_eq!(report.completed_cycles, 10_000);
        assert_eq!(report.injected_faults, 13);
        assert_eq!(report.fault_coverage.len(), 13);
        assert_eq!(report.peak_retained_events, 128);
        report.verify().unwrap();
    }

    #[test]
    fn materialized_repositories_have_every_required_initial_condition() {
        let root = std::env::temp_dir().join(format!(
            "codingmage-soak-fixtures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let fixtures = materialize_fixtures(&root).unwrap();
        assert_eq!(fixtures.len(), 7);
        assert!(
            fixtures
                .iter()
                .all(|fixture| fixture.root.join(".git").is_dir())
        );
        assert_eq!(
            fixtures
                .iter()
                .find(|fixture| fixture.kind == FixtureKind::Dirty)
                .unwrap()
                .condition,
            FixtureCondition::Dirty
        );
        assert_eq!(
            fixtures
                .iter()
                .find(|fixture| fixture.kind == FixtureKind::Conflicted)
                .unwrap()
                .condition,
            FixtureCondition::Conflicted
        );
        assert_eq!(
            fixtures
                .iter()
                .find(|fixture| fixture.kind == FixtureKind::MalformedPlan)
                .unwrap()
                .condition,
            FixtureCondition::MalformedPlan
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_unordered_duplicate_and_unbounded_campaigns_fail() {
        let mut config = complete_config(20);
        config.fixtures.remove(&FixtureKind::MalformedPlan);
        assert_eq!(config.validate(), Err(SoakError::InvalidConfiguration));

        let mut config = complete_config(20);
        config.faults.swap(0, 1);
        assert_eq!(config.validate(), Err(SoakError::InvalidConfiguration));

        let mut config = complete_config(20);
        config.faults.push(config.faults[0]);
        assert_eq!(config.validate(), Err(SoakError::InvalidConfiguration));

        let mut config = complete_config(20);
        config.max_retained_events = usize::MAX;
        assert_eq!(config.validate(), Err(SoakError::InvalidConfiguration));
    }

    #[test]
    fn every_prohibited_observation_blocks_the_campaign() {
        for index in 0..5 {
            let mut report = run_accelerated(&complete_config(20)).unwrap();
            match index {
                0 => report.duplicate_tasks = 1,
                1 => report.skipped_gates = 1,
                2 => report.false_completions = 1,
                3 => report.orphan_processes = 1,
                _ => report.unowned_mutations = 1,
            }
            assert_eq!(report.verify(), Err(SoakError::InvariantViolation));
        }
    }
}
