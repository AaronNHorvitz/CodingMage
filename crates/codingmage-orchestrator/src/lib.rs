//! Fail-closed task lifecycle and port-driven one-unit orchestration.

use std::{collections::BTreeSet, fmt};

use codingmage_contracts::{EvidenceId, RunId, TaskId};
use serde::{Deserialize, Serialize};

/// Durable lifecycle state for one bounded task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Task was found in the canonical source.
    Discovered,
    /// Dependencies and policy permit claiming.
    Ready,
    /// Exact run owns the task claim.
    Claimed,
    /// Implementation agent is active.
    Implementing,
    /// Deterministic local gates are active.
    LocalVerification,
    /// Independent senior review is active.
    SeniorReview,
    /// Accepted findings are being corrected.
    Correcting,
    /// Corrected commit receives final verification.
    FinalVerification,
    /// Successful unit is durably checkpointed.
    Checkpointed,
    /// Canonical completion was reconciled.
    Complete,
    /// Precise prerequisite is unavailable.
    Blocked,
    /// Operator or capacity policy paused work.
    Paused,
    /// Failure may be retried from durable state.
    RecoverableFailure,
    /// Work cannot continue automatically.
    TerminalFailure,
    /// Exact run was cancelled.
    Cancelled,
}

/// Coordinator-owned side-effect intent associated with one transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectIntent {
    /// No external effect.
    None,
    /// Acquire exact task ownership.
    AcquireClaim,
    /// Create an owned worktree and provider session.
    CreateImplementation,
    /// Execute deterministic local gates.
    RunLocalGates,
    /// Start read-only senior review.
    StartReview,
    /// Resume bounded correction.
    StartCorrection,
    /// Execute final deterministic and senior verification.
    RunFinalVerification,
    /// Write a durable successful checkpoint.
    WriteCheckpoint,
    /// Reconcile canonical task completion.
    ReconcileCompletion,
    /// Release exact locks, worktrees, and processes.
    ReleaseOwnedResources,
}

/// One exact transition request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    /// Contiguous run-local sequence.
    pub sequence: u64,
    /// Exact run.
    pub run_id: RunId,
    /// Exact task.
    pub task_id: TaskId,
    /// Required current state.
    pub from: TaskState,
    /// Requested resulting state.
    pub to: TaskState,
    /// Deterministic evidence authorizing the transition.
    pub evidence: Vec<EvidenceId>,
    /// Coordinator-owned effect intent.
    pub intent: SideEffectIntent,
}

/// In-memory projection that accepts only legal ordered transitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskMachine {
    run_id: RunId,
    task_id: TaskId,
    state: TaskState,
    next_sequence: u64,
    accepted_evidence: BTreeSet<EvidenceId>,
}

impl TaskMachine {
    /// Creates a discovered task projection.
    #[must_use]
    pub fn new(run_id: RunId, task_id: TaskId) -> Self {
        Self {
            run_id,
            task_id,
            state: TaskState::Discovered,
            next_sequence: 0,
            accepted_evidence: BTreeSet::new(),
        }
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.state
    }

    /// Returns the next required sequence.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Applies one legal, evidence-bearing transition.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestrationError`] for duplicate, stale, reordered, skipped, contradictory,
    /// cross-run, cross-task, or evidence-free transitions.
    pub fn apply(&mut self, transition: &Transition) -> Result<(), OrchestrationError> {
        if transition.run_id != self.run_id || transition.task_id != self.task_id {
            return Err(OrchestrationError::Identity);
        }
        if transition.sequence != self.next_sequence {
            return Err(OrchestrationError::Sequence);
        }
        if transition.from != self.state
            || !legal_transition(transition.from, transition.to, transition.intent)
        {
            return Err(OrchestrationError::Transition);
        }
        if transition.evidence.is_empty()
            || transition
                .evidence
                .iter()
                .any(|evidence| self.accepted_evidence.contains(evidence))
        {
            return Err(OrchestrationError::Evidence);
        }
        self.accepted_evidence
            .extend(transition.evidence.iter().cloned());
        self.state = transition.to;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(())
    }
}

/// Senior-review outcome consumed by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewOutcome {
    /// Review found no blocking defect.
    Pass,
    /// Validated findings require correction.
    ChangesRequired,
    /// Precise external blocker prevents completion.
    Blocked,
}

/// Deterministic gate outcome consumed by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationOutcome {
    /// Required gates passed.
    Pass,
    /// Failure is recoverable in a later attempt.
    RecoverableFailure,
    /// Failure is terminal for this run.
    TerminalFailure,
}

/// Narrow effect ports composed by the one-unit coordinator.
#[allow(clippy::missing_errors_doc)]
pub trait WorkflowPort {
    /// Creates and records the exact claim.
    fn claim(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Creates one owned worktree and implementation session.
    fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Waits for one structured implementation result.
    fn finish_implementation(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Runs deterministic local gates.
    fn verify_local(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError>;
    /// Runs read-only senior review.
    fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError>;
    /// Applies one bounded correction packet.
    fn correct(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Runs final verification after pass or correction.
    fn verify_final(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError>;
    /// Writes a durable success checkpoint.
    fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Reconciles the canonical completion claim.
    fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Releases all exact owned resources. This is called on every return path after claim.
    fn release(&mut self) -> Result<EvidenceId, OrchestrationError>;
}

/// One-unit coordinator that alone may advance task state.
#[derive(Clone, Debug)]
pub struct OneUnitCoordinator {
    machine: TaskMachine,
}

impl OneUnitCoordinator {
    /// Creates a coordinator for one discovered unit.
    #[must_use]
    pub fn new(run_id: RunId, task_id: TaskId) -> Self {
        Self {
            machine: TaskMachine::new(run_id, task_id),
        }
    }

    /// Returns the current state.
    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.machine.state()
    }

    /// Executes one complete bounded unit through supplied authority ports.
    ///
    /// # Errors
    ///
    /// Returns the precise port or transition error after attempting exact resource release.
    pub fn run(&mut self, port: &mut impl WorkflowPort) -> Result<TaskState, OrchestrationError> {
        self.transition(TaskState::Ready, SideEffectIntent::None, evidence("ready"))?;
        let claim = port.claim()?;
        self.transition(TaskState::Claimed, SideEffectIntent::AcquireClaim, claim)?;
        let result = self.run_claimed(port);
        let release = port.release();
        match release {
            Ok(_) => result,
            Err(error) => Err(error),
        }
    }

    fn run_claimed(
        &mut self,
        port: &mut impl WorkflowPort,
    ) -> Result<TaskState, OrchestrationError> {
        let started = port.start_implementation()?;
        self.transition(
            TaskState::Implementing,
            SideEffectIntent::CreateImplementation,
            started,
        )?;
        let implemented = port.finish_implementation()?;
        self.transition(
            TaskState::LocalVerification,
            SideEffectIntent::RunLocalGates,
            implemented,
        )?;
        let (local, local_evidence) = port.verify_local()?;
        if local != VerificationOutcome::Pass {
            return self.finish_failure(local, local_evidence);
        }
        self.transition(
            TaskState::SeniorReview,
            SideEffectIntent::StartReview,
            local_evidence,
        )?;
        let (review, review_evidence) = port.review()?;
        match review {
            ReviewOutcome::Pass => {}
            ReviewOutcome::Blocked => {
                self.transition(
                    TaskState::Blocked,
                    SideEffectIntent::ReleaseOwnedResources,
                    review_evidence,
                )?;
                return Ok(self.state());
            }
            ReviewOutcome::ChangesRequired => {
                self.transition(
                    TaskState::Correcting,
                    SideEffectIntent::StartCorrection,
                    review_evidence.clone(),
                )?;
                let correction = port.correct()?;
                self.transition(
                    TaskState::FinalVerification,
                    SideEffectIntent::RunFinalVerification,
                    correction,
                )?;
            }
        }
        if review == ReviewOutcome::Pass {
            self.transition(
                TaskState::FinalVerification,
                SideEffectIntent::RunFinalVerification,
                review_evidence,
            )?;
        }
        let (final_outcome, final_evidence) = port.verify_final()?;
        if final_outcome != VerificationOutcome::Pass {
            return self.finish_failure(final_outcome, final_evidence);
        }
        self.transition(
            TaskState::Checkpointed,
            SideEffectIntent::WriteCheckpoint,
            final_evidence,
        )?;
        let checkpoint = port.checkpoint()?;
        let completion = port.reconcile_completion()?;
        self.transition_many(
            TaskState::Complete,
            SideEffectIntent::ReconcileCompletion,
            vec![checkpoint, completion],
        )?;
        Ok(self.state())
    }

    fn finish_failure(
        &mut self,
        outcome: VerificationOutcome,
        evidence: EvidenceId,
    ) -> Result<TaskState, OrchestrationError> {
        let state = match outcome {
            VerificationOutcome::Pass => return Err(OrchestrationError::Transition),
            VerificationOutcome::RecoverableFailure => TaskState::RecoverableFailure,
            VerificationOutcome::TerminalFailure => TaskState::TerminalFailure,
        };
        self.transition(state, SideEffectIntent::ReleaseOwnedResources, evidence)?;
        Ok(self.state())
    }

    fn transition(
        &mut self,
        to: TaskState,
        intent: SideEffectIntent,
        evidence: EvidenceId,
    ) -> Result<(), OrchestrationError> {
        self.transition_many(to, intent, vec![evidence])
    }

    fn transition_many(
        &mut self,
        to: TaskState,
        intent: SideEffectIntent,
        evidence: Vec<EvidenceId>,
    ) -> Result<(), OrchestrationError> {
        self.machine.apply(&Transition {
            sequence: self.machine.next_sequence(),
            run_id: self.machine.run_id.clone(),
            task_id: self.machine.task_id.clone(),
            from: self.machine.state(),
            to,
            evidence,
            intent,
        })
    }
}

/// Content-free orchestration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationError {
    /// Run or task identity changed.
    Identity,
    /// Event sequence was duplicate, stale, skipped, or reordered.
    Sequence,
    /// State transition or side-effect intent is illegal.
    Transition,
    /// Evidence is absent, duplicated, or stale.
    Evidence,
    /// A workflow port failed.
    Port,
}

impl fmt::Display for OrchestrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.orchestration.identity",
            Self::Sequence => "codingmage.orchestration.sequence",
            Self::Transition => "codingmage.orchestration.transition",
            Self::Evidence => "codingmage.orchestration.evidence",
            Self::Port => "codingmage.orchestration.port",
        })
    }
}

impl std::error::Error for OrchestrationError {}

#[allow(clippy::unnested_or_patterns)]
fn legal_transition(from: TaskState, to: TaskState, intent: SideEffectIntent) -> bool {
    matches!(
        (from, to, intent),
        (
            TaskState::Discovered,
            TaskState::Ready,
            SideEffectIntent::None
        ) | (
            TaskState::Ready,
            TaskState::Claimed,
            SideEffectIntent::AcquireClaim
        ) | (
            TaskState::Claimed,
            TaskState::Implementing,
            SideEffectIntent::CreateImplementation
        ) | (
            TaskState::Implementing,
            TaskState::LocalVerification,
            SideEffectIntent::RunLocalGates
        ) | (
            TaskState::LocalVerification,
            TaskState::SeniorReview,
            SideEffectIntent::StartReview
        ) | (
            TaskState::SeniorReview,
            TaskState::Correcting,
            SideEffectIntent::StartCorrection
        ) | (
            TaskState::SeniorReview,
            TaskState::FinalVerification,
            SideEffectIntent::RunFinalVerification
        ) | (
            TaskState::Correcting,
            TaskState::FinalVerification,
            SideEffectIntent::RunFinalVerification
        ) | (
            TaskState::FinalVerification,
            TaskState::Checkpointed,
            SideEffectIntent::WriteCheckpoint
        ) | (
            TaskState::Checkpointed,
            TaskState::Complete,
            SideEffectIntent::ReconcileCompletion
        ) | (
            TaskState::Claimed
                | TaskState::Implementing
                | TaskState::LocalVerification
                | TaskState::SeniorReview
                | TaskState::Correcting
                | TaskState::FinalVerification,
            TaskState::Blocked
                | TaskState::Paused
                | TaskState::RecoverableFailure
                | TaskState::TerminalFailure
                | TaskState::Cancelled,
            SideEffectIntent::ReleaseOwnedResources
        )
    )
}

fn evidence(value: &str) -> EvidenceId {
    EvidenceId::new(value).expect("static evidence IDs are valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakePort {
        calls: Vec<&'static str>,
        review: Option<ReviewOutcome>,
        local: Option<VerificationOutcome>,
        final_outcome: Option<VerificationOutcome>,
        fail_at: Option<&'static str>,
    }

    impl FakePort {
        fn call(&mut self, name: &'static str) -> Result<EvidenceId, OrchestrationError> {
            self.calls.push(name);
            if self.fail_at == Some(name) {
                Err(OrchestrationError::Port)
            } else {
                Ok(evidence(name))
            }
        }
    }

    impl WorkflowPort for FakePort {
        fn claim(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("claim")
        }
        fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("start")
        }
        fn finish_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("implemented")
        }
        fn verify_local(
            &mut self,
        ) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("local")?;
            Ok((self.local.unwrap_or(VerificationOutcome::Pass), evidence))
        }
        fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("review")?;
            Ok((self.review.unwrap_or(ReviewOutcome::Pass), evidence))
        }
        fn correct(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("correct")
        }
        fn verify_final(
            &mut self,
        ) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("final")?;
            Ok((
                self.final_outcome.unwrap_or(VerificationOutcome::Pass),
                evidence,
            ))
        }
        fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("checkpoint")
        }
        fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("complete")
        }
        fn release(&mut self) -> Result<EvidenceId, OrchestrationError> {
            self.call("release")
        }
    }

    fn new_coordinator() -> OneUnitCoordinator {
        OneUnitCoordinator::new(RunId::new("run-1").unwrap(), TaskId::new("task-1").unwrap())
    }

    #[test]
    fn fake_vertical_slice_completes_exactly_one_unit() {
        let mut coordinator = new_coordinator();
        let mut port = FakePort::default();
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Complete));
        assert_eq!(
            port.calls,
            [
                "claim",
                "start",
                "implemented",
                "local",
                "review",
                "final",
                "checkpoint",
                "complete",
                "release"
            ]
        );
    }

    #[test]
    fn correction_and_block_paths_are_bounded() {
        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            review: Some(ReviewOutcome::ChangesRequired),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Complete));
        assert_eq!(
            port.calls.iter().filter(|call| **call == "correct").count(),
            1
        );
        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            review: Some(ReviewOutcome::Blocked),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Blocked));
        assert_eq!(port.calls.last(), Some(&"release"));
    }

    #[test]
    fn recoverable_terminal_and_port_failures_release() {
        for outcome in [
            VerificationOutcome::RecoverableFailure,
            VerificationOutcome::TerminalFailure,
        ] {
            let mut coordinator = new_coordinator();
            let mut port = FakePort {
                local: Some(outcome),
                ..FakePort::default()
            };
            assert!(matches!(
                coordinator.run(&mut port),
                Ok(TaskState::RecoverableFailure | TaskState::TerminalFailure)
            ));
            assert_eq!(port.calls.last(), Some(&"release"));
        }
        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            fail_at: Some("review"),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Err(OrchestrationError::Port));
        assert_eq!(port.calls.last(), Some(&"release"));
    }

    #[test]
    fn duplicate_reordered_skipped_and_cross_identity_events_fail() {
        let run = RunId::new("run-1").unwrap();
        let task = TaskId::new("task-1").unwrap();
        let mut machine = TaskMachine::new(run.clone(), task.clone());
        let ready = Transition {
            sequence: 0,
            run_id: run.clone(),
            task_id: task.clone(),
            from: TaskState::Discovered,
            to: TaskState::Ready,
            evidence: vec![evidence("ready")],
            intent: SideEffectIntent::None,
        };
        machine.apply(&ready).unwrap();
        assert_eq!(machine.apply(&ready), Err(OrchestrationError::Sequence));
        let mut skipped = ready.clone();
        skipped.sequence = 2;
        skipped.from = TaskState::Ready;
        assert_eq!(machine.apply(&skipped), Err(OrchestrationError::Sequence));
        let mut cross = ready;
        cross.sequence = 1;
        cross.from = TaskState::Ready;
        cross.run_id = RunId::new("run-2").unwrap();
        assert_eq!(machine.apply(&cross), Err(OrchestrationError::Identity));
    }

    #[test]
    fn illegal_state_and_reused_evidence_fail_without_mutation() {
        let run = RunId::new("run-1").unwrap();
        let task = TaskId::new("task-1").unwrap();
        let mut machine = TaskMachine::new(run.clone(), task.clone());
        let illegal = Transition {
            sequence: 0,
            run_id: run.clone(),
            task_id: task.clone(),
            from: TaskState::Discovered,
            to: TaskState::Complete,
            evidence: vec![evidence("x")],
            intent: SideEffectIntent::ReconcileCompletion,
        };
        assert_eq!(machine.apply(&illegal), Err(OrchestrationError::Transition));
        assert_eq!(machine.state(), TaskState::Discovered);
        let ready = Transition {
            sequence: 0,
            run_id: run,
            task_id: task,
            from: TaskState::Discovered,
            to: TaskState::Ready,
            evidence: vec![evidence("x")],
            intent: SideEffectIntent::None,
        };
        machine.apply(&ready).unwrap();
        let reused = Transition {
            sequence: 1,
            run_id: machine.run_id.clone(),
            task_id: machine.task_id.clone(),
            from: TaskState::Ready,
            to: TaskState::Claimed,
            evidence: vec![evidence("x")],
            intent: SideEffectIntent::AcquireClaim,
        };
        assert_eq!(machine.apply(&reused), Err(OrchestrationError::Evidence));
    }
}
