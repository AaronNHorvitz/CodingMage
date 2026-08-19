use serde::{Deserialize, Serialize};

use crate::{EffectClass, EventKind, EventOutcome, JournalRecord};

/// Exact content-free identities expected by a durable checkpoint.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentitySet {
    /// Opaque worktree identity.
    pub worktree: Option<String>,
    /// Canonical branch identity.
    pub branch: Option<String>,
    /// Full commit object identity.
    pub commit: Option<String>,
    /// Exact process start identity, not a PID alone.
    pub process: Option<String>,
    /// Exact provider session identity.
    pub agent_session: Option<String>,
    /// Exact resolved model identity.
    pub model: Option<String>,
    /// Exact deterministic gate identity.
    pub gate: Option<String>,
    /// Exact immutable evidence identity.
    pub evidence: Option<String>,
}

impl IdentitySet {
    /// Derives exact recovery expectations from one accepted durable record.
    #[must_use]
    pub fn from_record(record: &JournalRecord) -> Self {
        Self {
            worktree: record
                .event
                .identities
                .worktree
                .as_ref()
                .map(ToString::to_string),
            branch: record.event.identities.branch.clone(),
            commit: record.event.identities.commit.clone(),
            process: record.event.identities.process.clone(),
            agent_session: record
                .event
                .identities
                .agent_session
                .as_ref()
                .map(ToString::to_string),
            model: record.event.identities.model.clone(),
            gate: record.event.identities.gate.clone(),
            evidence: record.event.evidence.last().map(ToString::to_string),
        }
    }
}

/// Live identities observed without performing a state-changing action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiveObservation {
    /// Repository identity matches the authorized durable repository.
    pub repository_matches: bool,
    /// Exact observed external identities.
    pub identities: IdentitySet,
}

/// Safe restart action selected without replaying an external effect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDecision {
    /// Continue the next read-only or idempotent phase.
    Resume,
    /// Observe the uncertain external state before any action.
    Reobserve,
    /// Stop because exact identity or durable state is contradictory.
    Block,
}

/// Machine-readable reason for the recovery decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryReason {
    /// No events exist yet.
    EmptyJournal,
    /// Last operation was durably observed.
    CompletedEffect,
    /// Next phase is safe to repeat.
    RecoverablePhase,
    /// A state-changing effect has uncertain completion.
    UncertainEffect,
    /// Authorized repository identity changed.
    RepositoryMismatch,
    /// One or more exact live identities disagree.
    IdentityMismatch,
    /// Journal explicitly records a blocker.
    RecordedBlocker,
}

/// Reconciles durable and live state without invoking Git, a provider, or another effect.
#[must_use]
pub fn reconcile_after_restart(
    records: &[JournalRecord],
    expected: &IdentitySet,
    observed: &LiveObservation,
) -> (RecoveryDecision, RecoveryReason) {
    if !observed.repository_matches {
        return (RecoveryDecision::Block, RecoveryReason::RepositoryMismatch);
    }
    if !identities_match(expected, &observed.identities) {
        return (RecoveryDecision::Block, RecoveryReason::IdentityMismatch);
    }
    let Some(last) = records.last() else {
        return (RecoveryDecision::Resume, RecoveryReason::EmptyJournal);
    };
    if matches!(last.event.kind, EventKind::RecoveryBlocked { .. }) {
        return (RecoveryDecision::Block, RecoveryReason::RecordedBlocker);
    }
    if matches!(last.event.kind, EventKind::EffectObserved { .. })
        && last.event.outcome == EventOutcome::Succeeded
    {
        return (RecoveryDecision::Resume, RecoveryReason::CompletedEffect);
    }
    match &last.event.kind {
        EventKind::Transition {
            effect: EffectClass::StateChanging,
            ..
        } if last.event.outcome != EventOutcome::Failed => {
            (RecoveryDecision::Reobserve, RecoveryReason::UncertainEffect)
        }
        EventKind::Transition { .. }
        | EventKind::GateObserved { .. }
        | EventKind::ControlApplied { .. }
        | EventKind::RetryScheduled { .. }
        | EventKind::ExternalBoundaryChanged { .. } => {
            (RecoveryDecision::Resume, RecoveryReason::RecoverablePhase)
        }
        EventKind::EffectObserved { .. } => {
            (RecoveryDecision::Reobserve, RecoveryReason::UncertainEffect)
        }
        EventKind::RecoveryBlocked { .. } => {
            (RecoveryDecision::Block, RecoveryReason::RecordedBlocker)
        }
    }
}

fn identities_match(expected: &IdentitySet, observed: &IdentitySet) -> bool {
    [
        (&expected.worktree, &observed.worktree),
        (&expected.branch, &observed.branch),
        (&expected.commit, &observed.commit),
        (&expected.process, &observed.process),
        (&expected.agent_session, &observed.agent_session),
        (&expected.model, &observed.model),
        (&expected.gate, &observed.gate),
        (&expected.evidence, &observed.evidence),
    ]
    .into_iter()
    .all(|(expected, observed)| expected.is_none() || expected == observed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JournalEvent, RedactedField};
    use codingmage_contracts::{EvidenceId, RepositoryId, RunId, TaskId};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn record(effect: EffectClass, outcome: EventOutcome) -> JournalRecord {
        let event = JournalEvent {
            timestamp_ms: 1,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            repository_id: RepositoryId::new("repo-1").unwrap(),
            identities: crate::DurableIdentities {
                worktree: Some(codingmage_contracts::WorktreeId::new("worktree-1").unwrap()),
                branch: Some("codingmage/task-1".to_owned()),
                commit: Some("a".repeat(40)),
                process: Some("pid-10-start-20".to_owned()),
                agent_session: Some(codingmage_contracts::AttemptId::new("session-1").unwrap()),
                model: Some("model-1".to_owned()),
                gate: Some("gate-1".to_owned()),
            },
            kind: EventKind::Transition {
                phase: "implementation".to_owned(),
                effect,
            },
            outcome,
            evidence: vec![EvidenceId::new("evidence-1").unwrap()],
            redactions: vec![RedactedField::new("provider_output").unwrap()],
        };
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-recovery-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut journal = crate::Journal::open(&root, "owner").unwrap();
        let value = journal.append(event).unwrap().clone();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
        value
    }

    fn identities() -> IdentitySet {
        IdentitySet {
            worktree: Some("worktree-1".to_owned()),
            branch: Some("codingmage/task-1".to_owned()),
            commit: Some("0123456789abcdef".to_owned()),
            process: Some("pid-10-start-20".to_owned()),
            agent_session: Some("session-1".to_owned()),
            model: Some("model-1".to_owned()),
            gate: Some("gate-1".to_owned()),
            evidence: Some("evidence-1".to_owned()),
        }
    }

    #[test]
    fn every_transition_effect_class_has_fail_closed_recovery() {
        let expected = identities();
        let observed = LiveObservation {
            repository_matches: true,
            identities: expected.clone(),
        };
        for effect in [EffectClass::ReadOnly, EffectClass::Idempotent] {
            assert_eq!(
                reconcile_after_restart(
                    &[record(effect, EventOutcome::Succeeded)],
                    &expected,
                    &observed
                ),
                (RecoveryDecision::Resume, RecoveryReason::RecoverablePhase)
            );
        }
        assert_eq!(
            reconcile_after_restart(
                &[record(EffectClass::StateChanging, EventOutcome::Succeeded)],
                &expected,
                &observed
            ),
            (RecoveryDecision::Reobserve, RecoveryReason::UncertainEffect)
        );
    }

    #[test]
    fn repository_and_each_external_identity_mismatch_block() {
        let expected = identities();
        let mut observed = LiveObservation {
            repository_matches: false,
            identities: expected.clone(),
        };
        assert_eq!(
            reconcile_after_restart(&[], &expected, &observed).0,
            RecoveryDecision::Block
        );
        observed.repository_matches = true;
        for field in 0..8 {
            let mut changed = expected.clone();
            match field {
                0 => changed.worktree = Some("other".to_owned()),
                1 => changed.branch = Some("other".to_owned()),
                2 => changed.commit = Some("other".to_owned()),
                3 => changed.process = Some("other".to_owned()),
                4 => changed.agent_session = Some("other".to_owned()),
                5 => changed.gate = Some("other".to_owned()),
                6 => changed.model = Some("other".to_owned()),
                _ => changed.evidence = Some("other".to_owned()),
            }
            observed.identities = changed;
            assert_eq!(
                reconcile_after_restart(&[], &expected, &observed).0,
                RecoveryDecision::Block
            );
        }
    }

    #[test]
    fn recovery_expectations_are_derived_from_durable_identity_fields() {
        let record = record(EffectClass::StateChanging, EventOutcome::Uncertain);
        let derived = IdentitySet::from_record(&record);
        assert_eq!(derived.worktree.as_deref(), Some("worktree-1"));
        assert_eq!(derived.branch.as_deref(), Some("codingmage/task-1"));
        assert_eq!(
            derived.commit.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(derived.process.as_deref(), Some("pid-10-start-20"));
        assert_eq!(derived.agent_session.as_deref(), Some("session-1"));
        assert_eq!(derived.model.as_deref(), Some("model-1"));
        assert_eq!(derived.gate.as_deref(), Some("gate-1"));
        assert_eq!(derived.evidence.as_deref(), Some("evidence-1"));
    }
}
