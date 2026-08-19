//! Deterministic, content-free soak schedules and invariant accounting.

use std::{collections::BTreeSet, fmt};

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
    /// Provider quota pause and bounded resume.
    Quota,
    /// Network loss at an external boundary.
    NetworkLoss,
    /// Active provider process exits unexpectedly.
    AgentCrash,
    /// Coordinator restarts from durable state.
    ServiceRestart,
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
}

impl fmt::Display for SoakError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "codingmage.soak.invalid_configuration",
            Self::InvariantViolation => "codingmage.soak.invariant_violation",
        })
    }
}

impl std::error::Error for SoakError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_config(cycles: u64) -> CampaignConfig {
        let kinds = [
            FaultKind::Quota,
            FaultKind::NetworkLoss,
            FaultKind::AgentCrash,
            FaultKind::ServiceRestart,
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
        assert_eq!(report.injected_faults, 9);
        assert_eq!(report.fault_coverage.len(), 9);
        assert_eq!(report.peak_retained_events, 128);
        report.verify().unwrap();
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
