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
    /// Exact deterministic gate identity.
    pub gate: Option<String>,
    /// Exact immutable evidence identity.
    pub evidence: Option<String>,
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
        | EventKind::RetryScheduled { .. } => {
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

    fn record(effect: EffectClass, outcome: EventOutcome) -> JournalRecord {
        let event = JournalEvent {
            timestamp_ms: 1,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            repository_id: RepositoryId::new("repo-1").unwrap(),
            kind: EventKind::Transition {
                phase: "implementation".to_owned(),
                effect,
            },
            outcome,
            evidence: vec![EvidenceId::new("evidence-1").unwrap()],
            redactions: vec![RedactedField::new("provider_output").unwrap()],
        };
        let root = std::env::temp_dir().join(format!("codingmage-recovery-{}", std::process::id()));
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
        for field in 0..7 {
            let mut changed = expected.clone();
            match field {
                0 => changed.worktree = Some("other".to_owned()),
                1 => changed.branch = Some("other".to_owned()),
                2 => changed.commit = Some("other".to_owned()),
                3 => changed.process = Some("other".to_owned()),
                4 => changed.agent_session = Some("other".to_owned()),
                5 => changed.gate = Some("other".to_owned()),
                _ => changed.evidence = Some("other".to_owned()),
            }
            observed.identities = changed;
            assert_eq!(
                reconcile_after_restart(&[], &expected, &observed).0,
                RecoveryDecision::Block
            );
        }
    }
}
