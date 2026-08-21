//! Fail-closed task lifecycle and port-driven one-unit orchestration.

use std::{
    collections::BTreeSet,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{EvidenceId, RepositoryId, RunId, TaskId};
use codingmage_plan::{CheckState, PlanError, SelectedWork, TaskPlan};
use codingmage_state::{
    DurableIdentities, EffectClass, EventKind, EventOutcome, Journal, JournalEvent, RedactedField,
};
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

    fn recovered(run_id: RunId, task_id: TaskId, state: TaskState) -> Self {
        Self {
            run_id,
            task_id,
            state,
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

/// Structured implementation or correction disposition consumed by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplementationOutcome {
    /// The bounded edits are ready for coordinator-owned commit and verification.
    Ready,
    /// The implementer reported a precise blocker and produced no accepted candidate.
    Blocked,
}

/// Narrow effect ports composed by the one-unit coordinator.
#[allow(clippy::missing_errors_doc)]
pub trait WorkflowPort {
    /// Creates and records the exact claim.
    fn claim(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Creates one owned worktree and implementation session.
    fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Waits for one structured implementation result.
    fn finish_implementation(
        &mut self,
    ) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError>;
    /// Runs deterministic local gates.
    fn verify_local(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError>;
    /// Runs read-only senior review.
    fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError>;
    /// Persists exact correction identities before the provider or Git effect can begin.
    fn prepare_correction(&mut self) -> Result<(), OrchestrationError> {
        Ok(())
    }
    /// Applies one bounded correction packet.
    fn correct(&mut self) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError>;
    /// Runs final verification after pass or correction.
    fn verify_final(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError>;
    /// Writes a durable success checkpoint.
    fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Reconciles the canonical completion claim.
    fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError>;
    /// Releases all exact owned resources. This is called on every return path after claim.
    fn release(&mut self) -> Result<EvidenceId, OrchestrationError>;
}

/// Stable workflow operations persisted before and after delegated effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowOperation {
    /// Acquire task ownership.
    Claim,
    /// Create implementation resources.
    StartImplementation,
    /// Await an implementation result.
    FinishImplementation,
    /// Execute deterministic local verification.
    VerifyLocal,
    /// Execute read-only senior review.
    Review,
    /// Apply an accepted correction packet.
    Correct,
    /// Execute final verification.
    VerifyFinal,
    /// Persist successful checkpoint state.
    Checkpoint,
    /// Reconcile canonical completion.
    ReconcileCompletion,
    /// Release exact owned resources.
    Release,
}

impl WorkflowOperation {
    const fn phase(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::StartImplementation => "start_implementation",
            Self::FinishImplementation => "finish_implementation",
            Self::VerifyLocal => "verify_local",
            Self::Review => "review",
            Self::Correct => "correct",
            Self::VerifyFinal => "verify_final",
            Self::Checkpoint => "checkpoint",
            Self::ReconcileCompletion => "reconcile_completion",
            Self::Release => "release",
        }
    }

    const fn effect(self) -> EffectClass {
        match self {
            Self::VerifyLocal | Self::Review | Self::VerifyFinal => EffectClass::ReadOnly,
            Self::Checkpoint | Self::Release => EffectClass::Idempotent,
            Self::Claim
            | Self::StartImplementation
            | Self::FinishImplementation
            | Self::Correct
            | Self::ReconcileCompletion => EffectClass::StateChanging,
        }
    }
}

/// Journaling adapter that records intent before delegating every workflow operation.
pub struct DurableWorkflowPort<'a, P> {
    inner: &'a mut P,
    journal: &'a mut Journal,
    repository_id: RepositoryId,
    run_id: RunId,
    task_id: TaskId,
    identities: DurableIdentities,
}

impl<'a, P> DurableWorkflowPort<'a, P> {
    /// Wraps an existing port with durable, content-minimized intent and observation records.
    #[must_use]
    pub fn new(
        inner: &'a mut P,
        journal: &'a mut Journal,
        repository_id: RepositoryId,
        run_id: RunId,
        task_id: TaskId,
    ) -> Self {
        Self {
            inner,
            journal,
            repository_id,
            run_id,
            task_id,
            identities: DurableIdentities::default(),
        }
    }

    /// Adds exact external identities that will be copied into every subsequent durable event.
    #[must_use]
    pub fn with_identities(mut self, identities: DurableIdentities) -> Self {
        self.identities = identities;
        self
    }

    fn record_intent(&mut self, operation: WorkflowOperation) -> Result<(), OrchestrationError> {
        self.append_event(
            EventKind::Transition {
                phase: operation.phase().to_owned(),
                effect: operation.effect(),
            },
            EventOutcome::Uncertain,
            Vec::new(),
        )
    }

    fn record_observation(
        &mut self,
        operation: WorkflowOperation,
        evidence: EvidenceId,
    ) -> Result<EvidenceId, OrchestrationError> {
        self.append_event(
            EventKind::EffectObserved {
                phase: operation.phase().to_owned(),
            },
            EventOutcome::Succeeded,
            vec![evidence.clone()],
        )?;
        Ok(evidence)
    }

    fn append_event(
        &mut self,
        kind: EventKind,
        outcome: EventOutcome,
        evidence: Vec<EvidenceId>,
    ) -> Result<(), OrchestrationError> {
        self.journal
            .append(JournalEvent {
                timestamp_ms: timestamp_ms()?,
                run_id: self.run_id.clone(),
                task_id: self.task_id.clone(),
                repository_id: self.repository_id.clone(),
                identities: self.identities.clone(),
                kind,
                outcome,
                evidence,
                redactions: vec![
                    RedactedField::new("provider_output")
                        .map_err(|_| OrchestrationError::DurableState)?,
                    RedactedField::new("source_content")
                        .map_err(|_| OrchestrationError::DurableState)?,
                ],
            })
            .map_err(|_| OrchestrationError::DurableState)?;
        self.journal
            .write_snapshot()
            .map(|_| ())
            .map_err(|_| OrchestrationError::DurableState)
    }
}

impl<P: WorkflowPort> WorkflowPort for DurableWorkflowPort<'_, P> {
    fn claim(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.record_intent(WorkflowOperation::Claim)?;
        let value = self.inner.claim()?;
        self.record_observation(WorkflowOperation::Claim, value)
    }

    fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.record_intent(WorkflowOperation::StartImplementation)?;
        let value = self.inner.start_implementation()?;
        self.record_observation(WorkflowOperation::StartImplementation, value)
    }

    fn finish_implementation(
        &mut self,
    ) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError> {
        self.record_intent(WorkflowOperation::FinishImplementation)?;
        let (outcome, evidence) = self.inner.finish_implementation()?;
        let evidence =
            self.record_observation(WorkflowOperation::FinishImplementation, evidence)?;
        Ok((outcome, evidence))
    }

    fn verify_local(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        self.record_intent(WorkflowOperation::VerifyLocal)?;
        let (outcome, evidence) = self.inner.verify_local()?;
        let evidence = self.record_observation(WorkflowOperation::VerifyLocal, evidence)?;
        Ok((outcome, evidence))
    }

    fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError> {
        self.record_intent(WorkflowOperation::Review)?;
        let (outcome, evidence) = self.inner.review()?;
        let evidence = self.record_observation(WorkflowOperation::Review, evidence)?;
        Ok((outcome, evidence))
    }

    fn correct(&mut self) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError> {
        self.inner.prepare_correction()?;
        self.record_intent(WorkflowOperation::Correct)?;
        let (outcome, evidence) = self.inner.correct()?;
        let evidence = self.record_observation(WorkflowOperation::Correct, evidence)?;
        Ok((outcome, evidence))
    }

    fn verify_final(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        self.record_intent(WorkflowOperation::VerifyFinal)?;
        let (outcome, evidence) = self.inner.verify_final()?;
        let evidence = self.record_observation(WorkflowOperation::VerifyFinal, evidence)?;
        Ok((outcome, evidence))
    }

    fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.record_intent(WorkflowOperation::Checkpoint)?;
        let value = self.inner.checkpoint()?;
        self.record_observation(WorkflowOperation::Checkpoint, value)
    }

    fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.record_intent(WorkflowOperation::ReconcileCompletion)?;
        let value = self.inner.reconcile_completion()?;
        self.record_observation(WorkflowOperation::ReconcileCompletion, value)
    }

    fn release(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.record_intent(WorkflowOperation::Release)?;
        let value = self.inner.release()?;
        self.record_observation(WorkflowOperation::Release, value)
    }
}

impl<P: WorkflowPort> DurableWorkflowPort<'_, P> {
    /// Reobserves an interrupted correction under its original durable intent.
    ///
    /// This deliberately records no second transition intent. The delegated port must resume or
    /// reobserve the exact correction identity it persisted before the interrupted provider or Git
    /// effect. A successful return closes the prior uncertain intent with one observation.
    ///
    /// # Errors
    ///
    /// Returns the delegated correction or durable-state error without starting another intent.
    pub fn reobserve_correction(
        &mut self,
    ) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError> {
        let has_open_intent = self
            .journal
            .records()
            .iter()
            .rev()
            .find_map(|record| match &record.event.kind {
                EventKind::EffectObserved { phase }
                    if phase == WorkflowOperation::Correct.phase() =>
                {
                    Some(false)
                }
                EventKind::Transition { phase, .. }
                    if phase == WorkflowOperation::Correct.phase() =>
                {
                    Some(true)
                }
                _ => None,
            });
        if has_open_intent != Some(true) {
            self.record_intent(WorkflowOperation::Correct)?;
        }
        let (outcome, evidence) = self.inner.correct()?;
        let evidence = self.record_observation(WorkflowOperation::Correct, evidence)?;
        Ok((outcome, evidence))
    }
}

fn timestamp_ms() -> Result<u64, OrchestrationError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OrchestrationError::DurableState)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| OrchestrationError::DurableState)
}

/// One-unit coordinator that alone may advance task state.
#[derive(Clone, Debug)]
pub struct OneUnitCoordinator {
    machine: TaskMachine,
    correction_limit: u16,
    correction_count: u16,
}

impl OneUnitCoordinator {
    /// Creates a coordinator for one discovered unit.
    #[must_use]
    pub fn new(run_id: RunId, task_id: TaskId) -> Self {
        Self {
            machine: TaskMachine::new(run_id, task_id),
            correction_limit: 3,
            correction_count: 0,
        }
    }

    /// Sets the total gate-and-review correction limit for this unit.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestrationError::Transition`] for a zero or excessive limit.
    pub fn with_correction_limit(mut self, limit: u16) -> Result<Self, OrchestrationError> {
        if limit == 0 || limit > 100 {
            return Err(OrchestrationError::Transition);
        }
        self.correction_limit = limit;
        Ok(self)
    }

    /// Restores only the coordinator projection needed to reobserve one durably interrupted
    /// correction. Callers must establish the exact run, task, correction count, and external
    /// identities from integrity-checked state before constructing this value.
    ///
    /// # Errors
    ///
    /// Returns [`OrchestrationError::Transition`] for an invalid limit or correction count.
    pub fn recover_interrupted_correction(
        run_id: RunId,
        task_id: TaskId,
        correction_count: u16,
        correction_limit: u16,
    ) -> Result<Self, OrchestrationError> {
        if correction_limit == 0 || correction_limit > 100 || correction_count >= correction_limit {
            return Err(OrchestrationError::Transition);
        }
        Ok(Self {
            machine: TaskMachine::recovered(run_id, task_id, TaskState::Correcting),
            correction_limit,
            correction_count,
        })
    }

    /// Reobserves one exact interrupted correction and continues verification without replaying
    /// the correction intent, implementation, claim, or worktree creation.
    ///
    /// # Errors
    ///
    /// Returns a durable-port, transition, verification, checkpoint, reconciliation, or release
    /// failure. Exact resource release is attempted on every path.
    pub fn resume_interrupted_correction<P: WorkflowPort>(
        &mut self,
        port: &mut DurableWorkflowPort<'_, P>,
        reconcile: bool,
    ) -> Result<TaskState, OrchestrationError> {
        if self.state() != TaskState::Correcting {
            return Err(OrchestrationError::Transition);
        }
        let result = (|| {
            let (outcome, correction) = port.reobserve_correction()?;
            self.correction_count = self.correction_count.saturating_add(1);
            if outcome == ImplementationOutcome::Blocked {
                self.transition(
                    TaskState::Blocked,
                    SideEffectIntent::ReleaseOwnedResources,
                    correction,
                )?;
                return Ok(self.state());
            }
            self.transition(
                TaskState::LocalVerification,
                SideEffectIntent::RunLocalGates,
                correction,
            )?;
            self.run_verification(port, reconcile)
        })();
        let release = port.release();
        match release {
            Ok(_) => result,
            Err(error) => Err(error),
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
        self.run_with_completion(port, true)
    }

    /// Executes one bounded unit through checkpoint creation without claiming canonical task
    /// completion.
    ///
    /// This path is used for truthful partial progress when one task includes an unavailable
    /// external prerequisite. The reviewed candidate remains checkpointed and its canonical
    /// checkbox remains open.
    ///
    /// # Errors
    ///
    /// Returns the precise port or transition error after attempting exact resource release.
    pub fn run_to_checkpoint(
        &mut self,
        port: &mut impl WorkflowPort,
    ) -> Result<TaskState, OrchestrationError> {
        self.run_with_completion(port, false)
    }

    fn run_with_completion(
        &mut self,
        port: &mut impl WorkflowPort,
        reconcile: bool,
    ) -> Result<TaskState, OrchestrationError> {
        self.transition(TaskState::Ready, SideEffectIntent::None, evidence("ready"))?;
        let claim = port.claim()?;
        self.transition(TaskState::Claimed, SideEffectIntent::AcquireClaim, claim)?;
        let result = self.run_claimed(port, reconcile);
        let release = port.release();
        match release {
            Ok(_) => result,
            Err(error) => Err(error),
        }
    }

    fn run_claimed(
        &mut self,
        port: &mut impl WorkflowPort,
        reconcile: bool,
    ) -> Result<TaskState, OrchestrationError> {
        let started = port.start_implementation()?;
        self.transition(
            TaskState::Implementing,
            SideEffectIntent::CreateImplementation,
            started,
        )?;
        let (implementation, implemented) = port.finish_implementation()?;
        if implementation == ImplementationOutcome::Blocked {
            self.transition(
                TaskState::Blocked,
                SideEffectIntent::ReleaseOwnedResources,
                implemented,
            )?;
            return Ok(self.state());
        }
        self.transition(
            TaskState::LocalVerification,
            SideEffectIntent::RunLocalGates,
            implemented,
        )?;
        self.run_verification(port, reconcile)
    }

    fn run_verification(
        &mut self,
        port: &mut impl WorkflowPort,
        reconcile: bool,
    ) -> Result<TaskState, OrchestrationError> {
        loop {
            let (local, local_evidence) = port.verify_local()?;
            if local != VerificationOutcome::Pass {
                if local == VerificationOutcome::TerminalFailure || !self.correction_available() {
                    return self.finish_failure(local, local_evidence);
                }
                if self.apply_correction(port, local_evidence)? {
                    return Ok(self.state());
                }
                continue;
            }
            self.transition(
                TaskState::SeniorReview,
                SideEffectIntent::StartReview,
                local_evidence,
            )?;
            let (review, review_evidence) = port.review()?;
            match review {
                ReviewOutcome::Blocked => {
                    self.transition(
                        TaskState::Blocked,
                        SideEffectIntent::ReleaseOwnedResources,
                        review_evidence,
                    )?;
                    return Ok(self.state());
                }
                ReviewOutcome::ChangesRequired => {
                    if !self.correction_available() {
                        self.transition(
                            TaskState::RecoverableFailure,
                            SideEffectIntent::ReleaseOwnedResources,
                            review_evidence,
                        )?;
                        return Ok(self.state());
                    }
                    if self.apply_correction(port, review_evidence)? {
                        return Ok(self.state());
                    }
                    continue;
                }
                ReviewOutcome::Pass => {}
            }
            self.transition(
                TaskState::FinalVerification,
                SideEffectIntent::RunFinalVerification,
                review_evidence,
            )?;
            let (final_outcome, final_evidence) = port.verify_final()?;
            if final_outcome == VerificationOutcome::Pass {
                self.transition(
                    TaskState::Checkpointed,
                    SideEffectIntent::WriteCheckpoint,
                    final_evidence,
                )?;
                break;
            }
            if final_outcome == VerificationOutcome::TerminalFailure || !self.correction_available()
            {
                return self.finish_failure(final_outcome, final_evidence);
            }
            if self.apply_correction(port, final_evidence)? {
                return Ok(self.state());
            }
        }
        let checkpoint = port.checkpoint()?;
        if !reconcile {
            return Ok(self.state());
        }
        let completion = port.reconcile_completion()?;
        self.transition_many(
            TaskState::Complete,
            SideEffectIntent::ReconcileCompletion,
            vec![checkpoint, completion],
        )?;
        Ok(self.state())
    }

    fn correction_available(&self) -> bool {
        self.correction_count < self.correction_limit
    }

    fn apply_correction(
        &mut self,
        port: &mut impl WorkflowPort,
        evidence: EvidenceId,
    ) -> Result<bool, OrchestrationError> {
        self.transition(
            TaskState::Correcting,
            SideEffectIntent::StartCorrection,
            evidence,
        )?;
        let (outcome, correction) = port.correct()?;
        self.correction_count = self.correction_count.saturating_add(1);
        if outcome == ImplementationOutcome::Blocked {
            self.transition(
                TaskState::Blocked,
                SideEffectIntent::ReleaseOwnedResources,
                correction,
            )?;
            return Ok(true);
        }
        self.transition(
            TaskState::LocalVerification,
            SideEffectIntent::RunLocalGates,
            correction,
        )?;
        Ok(false)
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

/// Reconciles one evidenced canonical checkbox transition and selects subsequent ready work.
///
/// The function reparses both exact source versions. Exactly one checklist item may change, and
/// that change must be the named open-to-checked item with a new line hash. All titles,
/// dependencies, hierarchy, and other states must remain byte-derived equivalents.
///
/// # Errors
///
/// Returns [`OrchestrationError`] when parsing fails, completion evidence is absent, the named item
/// does not make the exact transition, or any unrelated plan structure changes.
pub fn reconcile_and_select_next(
    before_source: &[u8],
    after_source: &[u8],
    completed_item_id: &str,
    completion_evidence: &EvidenceId,
    blockers: &BTreeSet<String>,
) -> Result<Option<SelectedWork>, OrchestrationError> {
    if completion_evidence.as_str().is_empty() {
        return Err(OrchestrationError::Evidence);
    }
    let before = TaskPlan::parse(before_source).map_err(map_plan_error)?;
    let after = TaskPlan::parse(after_source).map_err(map_plan_error)?;
    if before.version != after.version
        || before.sprints != after.sprints
        || before.stories != after.stories
        || before.items.len() != after.items.len()
    {
        return Err(OrchestrationError::PlanDrift);
    }
    let mut changed = 0_usize;
    for (prior, current) in before.items.iter().zip(&after.items) {
        if prior.id != current.id
            || prior.kind != current.kind
            || prior.title != current.title
            || prior.parent_id != current.parent_id
            || prior.dependencies != current.dependencies
            || prior.anchor.line != current.anchor.line
        {
            return Err(OrchestrationError::PlanDrift);
        }
        if prior != current {
            changed = changed.saturating_add(1);
            if prior.id != completed_item_id
                || prior.state != CheckState::Open
                || current.state != CheckState::Checked
                || prior.anchor.line_sha256 == current.anchor.line_sha256
            {
                return Err(OrchestrationError::PlanDrift);
            }
        }
    }
    if changed != 1 {
        return Err(OrchestrationError::Evidence);
    }
    match after.select_next(blockers) {
        Ok(selected) => Ok(Some(selected)),
        Err(PlanError::NoReadyWork) => Ok(None),
        Err(error) => Err(map_plan_error(error)),
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
    /// Canonical plan changed outside the exact evidenced completion.
    PlanDrift,
    /// Durable intent or observation could not be recorded.
    DurableState,
}

impl fmt::Display for OrchestrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.orchestration.identity",
            Self::Sequence => "codingmage.orchestration.sequence",
            Self::Transition => "codingmage.orchestration.transition",
            Self::Evidence => "codingmage.orchestration.evidence",
            Self::Port => "codingmage.orchestration.port",
            Self::PlanDrift => "codingmage.orchestration.plan_drift",
            Self::DurableState => "codingmage.orchestration.durable_state",
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
            TaskState::LocalVerification | TaskState::FinalVerification,
            TaskState::Correcting,
            SideEffectIntent::StartCorrection
        ) | (
            TaskState::Correcting,
            TaskState::LocalVerification,
            SideEffectIntent::RunLocalGates
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

const fn map_plan_error(_error: PlanError) -> OrchestrationError {
    OrchestrationError::PlanDrift
}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_state::{
        IdentitySet, LiveObservation, RecoveryDecision, SnapshotEnvelope, reconcile_after_restart,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct FakePort {
        calls: Vec<&'static str>,
        implementation: Option<ImplementationOutcome>,
        correction: Option<ImplementationOutcome>,
        review: Option<ReviewOutcome>,
        review_calls: usize,
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
                Ok(evidence(&format!("{name}-{}", self.calls.len())))
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
        fn finish_implementation(
            &mut self,
        ) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("implemented")?;
            Ok((
                self.implementation.unwrap_or(ImplementationOutcome::Ready),
                evidence,
            ))
        }
        fn verify_local(
            &mut self,
        ) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("local")?;
            Ok((self.local.unwrap_or(VerificationOutcome::Pass), evidence))
        }
        fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("review")?;
            self.review_calls = self.review_calls.saturating_add(1);
            let outcome = match (self.review, self.review_calls) {
                (Some(ReviewOutcome::ChangesRequired), 2..) | (None, _) => ReviewOutcome::Pass,
                (Some(value), _) => value,
            };
            Ok((outcome, evidence))
        }
        fn correct(&mut self) -> Result<(ImplementationOutcome, EvidenceId), OrchestrationError> {
            let evidence = self.call("correct")?;
            Ok((
                self.correction.unwrap_or(ImplementationOutcome::Ready),
                evidence,
            ))
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

    fn state_root(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-orchestrator-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn interrupted_correction_resumes_without_replaying_prior_effects() {
        let root = state_root("resume-correction");
        let run_id = RunId::new("run-resume").unwrap();
        let task_id = TaskId::new("task-resume").unwrap();
        let repository_id = RepositoryId::new("repo-resume").unwrap();
        let mut journal = Journal::open(&root, "resume-owner").unwrap();
        journal
            .append(JournalEvent {
                timestamp_ms: 1,
                run_id: run_id.clone(),
                task_id: task_id.clone(),
                repository_id: repository_id.clone(),
                identities: DurableIdentities::default(),
                kind: EventKind::Transition {
                    phase: WorkflowOperation::Correct.phase().to_owned(),
                    effect: EffectClass::StateChanging,
                },
                outcome: EventOutcome::Uncertain,
                evidence: Vec::new(),
                redactions: vec![RedactedField::new("provider_output").unwrap()],
            })
            .unwrap();
        let mut fake = FakePort::default();
        let mut coordinator = OneUnitCoordinator::recover_interrupted_correction(
            run_id.clone(),
            task_id.clone(),
            0,
            3,
        )
        .unwrap();
        {
            let mut durable =
                DurableWorkflowPort::new(&mut fake, &mut journal, repository_id, run_id, task_id);
            assert_eq!(
                coordinator
                    .resume_interrupted_correction(&mut durable, true)
                    .unwrap(),
                TaskState::Complete
            );
        }
        assert_eq!(
            fake.calls,
            [
                "correct",
                "local",
                "review",
                "final",
                "checkpoint",
                "complete",
                "release"
            ]
        );
        let records = journal.records();
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(
                    &record.event.kind,
                    EventKind::Transition { phase, .. } if phase == "correct"
                ))
                .count(),
            1
        );
        assert!(records.iter().any(|record| matches!(
            &record.event.kind,
            EventKind::EffectObserved { phase } if phase == "correct"
        )));
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
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
    fn durable_vertical_slice_records_each_intent_and_observation() {
        let root = state_root("complete");
        let mut journal = Journal::open(&root, "owner").unwrap();
        let run = RunId::new("run-1").unwrap();
        let task = TaskId::new("task-1").unwrap();
        let mut inner = FakePort::default();
        let mut coordinator = OneUnitCoordinator::new(run.clone(), task.clone());
        {
            let mut durable = DurableWorkflowPort::new(
                &mut inner,
                &mut journal,
                RepositoryId::new("repo-1").unwrap(),
                run,
                task,
            );
            assert_eq!(coordinator.run(&mut durable), Ok(TaskState::Complete));
        }
        assert_eq!(journal.records().len(), 18);
        for pair in journal.records().chunks_exact(2) {
            assert!(matches!(pair[0].event.kind, EventKind::Transition { .. }));
            assert_eq!(pair[0].event.outcome, EventOutcome::Uncertain);
            assert!(matches!(
                pair[1].event.kind,
                EventKind::EffectObserved { .. }
            ));
            assert_eq!(pair[1].event.outcome, EventOutcome::Succeeded);
        }
        SnapshotEnvelope::load(&root, journal.records()).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crash_after_every_durable_intent_never_replays_state_change() {
        let operations = [
            WorkflowOperation::Claim,
            WorkflowOperation::StartImplementation,
            WorkflowOperation::FinishImplementation,
            WorkflowOperation::VerifyLocal,
            WorkflowOperation::Review,
            WorkflowOperation::Correct,
            WorkflowOperation::VerifyFinal,
            WorkflowOperation::Checkpoint,
            WorkflowOperation::ReconcileCompletion,
            WorkflowOperation::Release,
        ];
        for operation in operations {
            let root = state_root(operation.phase());
            let mut journal = Journal::open(&root, "owner").unwrap();
            let mut inner = FakePort::default();
            {
                let mut durable = DurableWorkflowPort::new(
                    &mut inner,
                    &mut journal,
                    RepositoryId::new("repo-1").unwrap(),
                    RunId::new("run-1").unwrap(),
                    TaskId::new("task-1").unwrap(),
                );
                durable.record_intent(operation).unwrap();
            }
            let expected = IdentitySet::default();
            let observed = LiveObservation {
                repository_matches: true,
                identities: IdentitySet::default(),
            };
            let decision = reconcile_after_restart(journal.records(), &expected, &observed).0;
            let expected_decision = if operation.effect() == EffectClass::StateChanging {
                RecoveryDecision::Reobserve
            } else {
                RecoveryDecision::Resume
            };
            assert_eq!(decision, expected_decision, "operation={operation:?}");
            SnapshotEnvelope::load(&root, journal.records()).unwrap();
            drop(journal);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn correction_and_block_paths_are_bounded() {
        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            implementation: Some(ImplementationOutcome::Blocked),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Blocked));
        assert_eq!(port.calls, ["claim", "start", "implemented", "release"]);

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
        assert_eq!(
            port.calls.iter().filter(|call| **call == "review").count(),
            2
        );
        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            review: Some(ReviewOutcome::Blocked),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Blocked));
        assert_eq!(port.calls.last(), Some(&"release"));

        let mut coordinator = new_coordinator();
        let mut port = FakePort {
            review: Some(ReviewOutcome::ChangesRequired),
            correction: Some(ImplementationOutcome::Blocked),
            ..FakePort::default()
        };
        assert_eq!(coordinator.run(&mut port), Ok(TaskState::Blocked));
        assert_eq!(port.calls.last(), Some(&"release"));
    }

    #[test]
    fn candidate_only_progress_stops_at_checkpoint_without_reconciliation() {
        let mut coordinator = new_coordinator();
        let mut port = FakePort::default();
        assert_eq!(
            coordinator.run_to_checkpoint(&mut port),
            Ok(TaskState::Checkpointed)
        );
        assert!(port.calls.contains(&"checkpoint"));
        assert!(!port.calls.contains(&"reconcile"));
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
    fn repeated_gate_failure_consumes_the_exact_correction_limit() {
        let mut coordinator = new_coordinator().with_correction_limit(2).unwrap();
        let mut port = FakePort {
            local: Some(VerificationOutcome::RecoverableFailure),
            ..FakePort::default()
        };
        assert_eq!(
            coordinator.run(&mut port),
            Ok(TaskState::RecoverableFailure)
        );
        assert_eq!(
            port.calls.iter().filter(|call| **call == "correct").count(),
            2
        );
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

    #[test]
    fn canonical_progression_accepts_one_completion_and_selects_next() {
        const BEFORE: &str = "# Plan\n\n## Sprint 0 - Work\n\n**Sprint goal:** Finish work.\n\n### Story 0.1 - Units\n\n- [ ] **Task 0.1.1 - Complete units**\n  - [ ] **Sub-task 0.1.1.1:** Finish first unit.\n  - [ ] **Sub-task 0.1.1.2:** Finish second unit.\n\n- [ ] **AC 0.1.1:** Units pass.\n\n### Sprint 0 Gate\n\n- [ ] **Gate 0.1:** Work passes.\n";
        let after = BEFORE.replacen("[ ] **Sub-task 0.1.1.1", "[x] **Sub-task 0.1.1.1", 1);
        let next = reconcile_and_select_next(
            BEFORE.as_bytes(),
            after.as_bytes(),
            "0.1.1.1",
            &evidence("completion"),
            &BTreeSet::new(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(next.item.id, "0.1.1.2");
        let blocked = BTreeSet::from(["0.1.1.2".to_owned()]);
        assert_eq!(
            reconcile_and_select_next(
                BEFORE.as_bytes(),
                after.as_bytes(),
                "0.1.1.1",
                &evidence("completion"),
                &blocked,
            ),
            Ok(None)
        );
    }

    #[test]
    fn canonical_progression_rejects_false_completion_and_unrelated_drift() {
        const BEFORE: &str = "# Plan\n\n## Sprint 0 - Work\n\n**Sprint goal:** Finish work.\n\n### Story 0.1 - Units\n\n- [ ] **Task 0.1.1 - Complete units**\n  - [ ] **Sub-task 0.1.1.1:** Finish first unit.\n  - [ ] **Sub-task 0.1.1.2:** Finish second unit.\n\n- [ ] **AC 0.1.1:** Units pass.\n\n### Sprint 0 Gate\n\n- [ ] **Gate 0.1:** Work passes.\n";
        assert_eq!(
            reconcile_and_select_next(
                BEFORE.as_bytes(),
                BEFORE.as_bytes(),
                "0.1.1.1",
                &evidence("completion"),
                &BTreeSet::new(),
            ),
            Err(OrchestrationError::Evidence)
        );
        let drift = BEFORE
            .replacen("[ ] **Sub-task 0.1.1.1", "[x] **Sub-task 0.1.1.1", 1)
            .replace("Finish second unit", "Silently expand second unit");
        assert_eq!(
            reconcile_and_select_next(
                BEFORE.as_bytes(),
                drift.as_bytes(),
                "0.1.1.1",
                &evidence("completion"),
                &BTreeSet::new(),
            ),
            Err(OrchestrationError::PlanDrift)
        );
    }
}
