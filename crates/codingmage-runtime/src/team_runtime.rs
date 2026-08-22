//! Bounded concurrent execution and durable lifecycle projection for campaign pods.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codingmage_campaign::{
    ActorClass, CampaignAuthentication, CampaignSpec, CampaignTaskRecord, CampaignTaskState,
    CampaignTaskTransition, DurablePodLease, DurablePodScheduler, TaskResourceReservation,
    TaskTerminalReason, TaskUtilization, TeamCampaignSnapshot, TeamResourceController,
};
use codingmage_contracts::{RunId, WorktreeId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_git::{OwnedWorktree, integrate_reviewed_descendant};
use codingmage_orchestrator::TaskState;
use codingmage_process::CancellationToken;
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AuthenticationMode, CompletionPolicy, ImplementerSpec, LifecycleObserver, RunOutcome,
    RunProgress, RunSpec, RuntimeError, UnitLifecycleEvent, private_directory, provider_spec,
    run_one_with_progress_id_budget,
};

const MESSAGE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MIN_CHANNEL_CAPACITY: usize = 32;
const CI_CORRECTION_RESULT_NAME: &str = "ci-correction-result.json";
const CI_CORRECTION_RESULT_VERSION: u16 = 1;

/// One admitted pod and its exact coordinator-owned resource reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamBatchJob {
    /// Stable dispatch order used for deterministic result projection.
    pub sequence: u64,
    /// Exact one-unit runtime identity, persisted before worker execution.
    pub run_id: RunId,
    /// Exact durable scheduler lease.
    pub lease: DurablePodLease,
    /// Exact physical and logical resource reservation.
    pub reservation: TaskResourceReservation,
}

impl TeamBatchJob {
    fn verify(&self, snapshot: &TeamCampaignSnapshot) -> Result<(), RuntimeError> {
        let record = snapshot
            .tasks
            .get(&self.lease.task_id)
            .ok_or(RuntimeError::State)?;
        let active = snapshot
            .scheduler
            .active
            .get(&self.lease.lease_id)
            .ok_or(RuntimeError::State)?;
        if active != &self.lease
            || record.state != CampaignTaskState::Leased
            || record.lease_id.as_ref() != Some(&self.lease.lease_id)
            || record.pod_id.as_ref() != Some(&self.lease.pod_id)
            || record
                .run_id
                .as_ref()
                .is_some_and(|run_id| run_id != self.run_id.as_str())
            || self.reservation.task_id != self.lease.task_id
            || self.reservation.lease_id != self.lease.lease_id
            || self.reservation.actor != ActorClass::Implementer
            || self.reservation.cpu_units != snapshot.resources.policy.implementation_cpu_units
            || self.reservation.memory_bytes
                != snapshot.resources.policy.implementation_memory_bytes
            || self.reservation.disk_bytes != snapshot.resources.policy.implementation_disk_bytes
            || self.reservation.process_slots != snapshot.resources.policy.implementation_processes
            || self.reservation.exclusive_resources != self.lease.test_resources
            || self.reservation.deadline_ms
                != self
                    .reservation
                    .started_at_ms
                    .checked_add(snapshot.resources.policy.pod_timeout_ms)
                    .ok_or(RuntimeError::State)?
        {
            return Err(RuntimeError::State);
        }
        Ok(())
    }
}

/// Content-minimized event sender scoped to exactly one admitted pod.
#[derive(Clone)]
pub struct TeamEventSink {
    sequence: u64,
    task_id: String,
    sender: SyncSender<WorkerMessage>,
}

impl TeamEventSink {
    /// Reports one typed progress transition without provider text.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the coordinator is no longer receiving this pod.
    pub fn progress(&self, progress: RunProgress) -> Result<(), RuntimeError> {
        self.send(WorkerEvent::Progress(progress))
    }

    /// Reports one durable content-minimized lifecycle observation.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the coordinator is no longer receiving this pod.
    pub fn lifecycle(&self, event: UnitLifecycleEvent) -> Result<(), RuntimeError> {
        self.send(WorkerEvent::Lifecycle(event))
    }

    /// Reports monotonic observed utilization and liveness for this exact pod.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the coordinator is no longer receiving this pod.
    pub fn heartbeat(&self, utilization: TaskUtilization) -> Result<(), RuntimeError> {
        self.send(WorkerEvent::Heartbeat(utilization))
    }

    /// Reports liveness without changing the latest observed utilization.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the coordinator is no longer receiving this pod.
    pub fn liveness(&self) -> Result<(), RuntimeError> {
        self.send(WorkerEvent::Liveness)
    }

    fn send(&self, event: WorkerEvent) -> Result<(), RuntimeError> {
        self.sender
            .send(WorkerMessage {
                sequence: self.sequence,
                task_id: self.task_id.clone(),
                event,
            })
            .map_err(|_| RuntimeError::State)
    }
}

/// Injectable pod implementation used by the bounded team executor.
pub trait TeamUnitRunner: Send + Sync + 'static {
    /// Executes one exact admitted job and reports content-minimized observations through `events`.
    ///
    /// # Errors
    ///
    /// Returns a stable content-free runtime failure for the exact pod. Implementations must
    /// observe `cancellation` and release every owned process before returning.
    fn run(
        &self,
        job: &TeamBatchJob,
        cancellation: CancellationToken,
        events: TeamEventSink,
    ) -> Result<RunOutcome, RuntimeError>;
}

/// Production pod adapter that reuses the proven one-unit implementation and review workflow.
#[derive(Clone, Debug)]
pub struct ProductionTeamUnitRunner {
    base_config: Config,
    campaign_spec: CampaignSpec,
    campaign_repository: PathBuf,
    codingmage_binary: PathBuf,
    private_root: PathBuf,
    actor_permits: ActorPermitPool,
}

impl ProductionTeamUnitRunner {
    /// Creates a production runner for one isolated campaign repository.
    ///
    /// The constructor performs no filesystem writes. Per-pod private roots are created only after
    /// the batch executor has durably persisted the matching run and resource reservation.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::Authority`] for malformed campaign authority or relative roots.
    pub fn new(
        base_config: Config,
        campaign_spec: CampaignSpec,
        campaign_repository: PathBuf,
        codingmage_binary: PathBuf,
        private_root: PathBuf,
    ) -> Result<Self, RuntimeError> {
        campaign_spec.verify().map_err(RuntimeError::Campaign)?;
        if !campaign_repository.is_absolute()
            || !codingmage_binary.is_absolute()
            || !private_root.is_absolute()
            || campaign_repository == private_root
            || campaign_repository.starts_with(&private_root)
            || private_root.starts_with(&campaign_repository)
        {
            return Err(RuntimeError::Authority);
        }
        let policy = campaign_spec
            .multi_agent
            .as_ref()
            .ok_or(RuntimeError::Authority)?;
        let actor_permits = ActorPermitPool::new(
            usize::from(policy.concurrency.test_workers),
            usize::from(policy.concurrency.codex_reviewers),
        )?;
        Ok(Self {
            base_config,
            campaign_spec,
            campaign_repository,
            codingmage_binary,
            private_root,
            actor_permits,
        })
    }

    fn config_for(&self, job: &TeamBatchJob) -> Result<Config, RuntimeError> {
        let pod_root = self.private_root.join("pods").join(&job.lease.lease_id);
        let scratch_root = pod_root.join("scratch");
        let state_root = pod_root.join("state");
        private_directory(&scratch_root)?;
        private_directory(&state_root)?;
        let mut config = self.base_config.clone();
        config.target_path.clone_from(&self.campaign_repository);
        config
            .default_branch
            .clone_from(&self.campaign_spec.campaign_branch);
        config.integration_branch = format!(
            "{}/pods/{}",
            self.campaign_spec.campaign_branch, job.lease.pod_id
        );
        config.scratch_root = scratch_root;
        config.state_root = state_root;
        Ok(config)
    }

    fn run_spec_for(&self, job: &TeamBatchJob) -> RunSpec {
        RunSpec {
            version: 2,
            task_id: job.lease.task_id.clone(),
            owned_paths: job.lease.owned_paths.clone(),
            completion_policy: CompletionPolicy::CandidateOnly,
            implementer: ImplementerSpec {
                provider: provider_spec(&self.campaign_spec.implementer),
                authentication: match self.campaign_spec.implementer_authentication {
                    CampaignAuthentication::Bare => AuthenticationMode::Bare,
                    CampaignAuthentication::ExistingLogin => AuthenticationMode::ExistingLogin,
                },
            },
            reviewer: provider_spec(&self.campaign_spec.reviewer),
        }
    }

    fn run_ci_correction(
        &self,
        record: &CampaignTaskRecord,
        cancellation: CancellationToken,
        progress: impl FnMut(RunProgress),
    ) -> Result<(CiCorrectionContext, CiCorrectionExecution), RuntimeError> {
        let context = self.ci_correction_context(record)?;
        let execution = self.load_or_run_ci_correction(record, &context, cancellation, progress)?;
        Ok((context, execution))
    }

    fn ci_correction_context(
        &self,
        record: &CampaignTaskRecord,
    ) -> Result<CiCorrectionContext, RuntimeError> {
        if record.state != CampaignTaskState::Correcting {
            return Err(RuntimeError::State);
        }
        let lease_id = record.lease_id.as_deref().ok_or(RuntimeError::State)?;
        let pod_id = record.pod_id.as_deref().ok_or(RuntimeError::State)?;
        let worktree_id = WorktreeId::new(record.worktree_id.clone().ok_or(RuntimeError::State)?)
            .map_err(|_| RuntimeError::State)?;
        let prior_commit = record.candidate_commit.clone().ok_or(RuntimeError::State)?;
        let ci_evidence = record
            .ci_evidence_sha256
            .last()
            .cloned()
            .ok_or(RuntimeError::State)?;
        let run_material = format!(
            "{}\0{}\0{}\0{}",
            record.campaign_id, record.task_id, prior_commit, ci_evidence
        );
        let run_digest = hex(&Sha256::digest(run_material.as_bytes()));
        let run_id = RunId::new(format!("ci-correction-{}", &run_digest[..32]))
            .map_err(|_| RuntimeError::State)?;

        let pod_root = self.private_root.join("pods").join(lease_id);
        let mut original_config = self.base_config.clone();
        original_config
            .target_path
            .clone_from(&self.campaign_repository);
        original_config
            .default_branch
            .clone_from(&self.campaign_spec.campaign_branch);
        original_config.integration_branch =
            format!("{}/pods/{pod_id}", self.campaign_spec.campaign_branch);
        original_config.scratch_root = pod_root.join("scratch");
        original_config.state_root = pod_root.join("state");
        let original_worktree = OwnedWorktree::load(&original_config, &worktree_id)
            .map_err(|_| RuntimeError::Repository)?;
        if original_worktree.manifest().branch != record.branch.as_deref().unwrap_or_default() {
            return Err(RuntimeError::Authority);
        }

        let correction_root = self
            .private_root
            .join("ci-corrections")
            .join(&record.task_id)
            .join(&run_digest[..32]);
        let mut correction_config = self.base_config.clone();
        correction_config
            .target_path
            .clone_from(&original_worktree.manifest().path);
        correction_config
            .default_branch
            .clone_from(&original_worktree.manifest().branch);
        correction_config.integration_branch = format!(
            "{}/ci-corrections/{}",
            original_worktree.manifest().branch,
            &run_digest[..16]
        );
        correction_config.scratch_root = correction_root.join("scratch");
        correction_config.state_root = correction_root.join("state");
        let policy = self
            .campaign_spec
            .multi_agent
            .as_ref()
            .ok_or(RuntimeError::Authority)?;
        let consumed = u16::try_from(record.correction_sessions.len()).unwrap_or(u16::MAX);
        let remaining = policy.max_task_correction_cycles.saturating_sub(consumed);
        if remaining == 0 {
            return Err(RuntimeError::CampaignLimit(
                crate::CampaignLimitKind::CorrectionRounds,
            ));
        }
        correction_config.correction_limit = remaining;
        private_directory(&correction_config.scratch_root)?;
        private_directory(&correction_config.state_root)?;
        let spec = RunSpec {
            version: 2,
            task_id: record.task_id.clone(),
            owned_paths: record.owned_paths.clone(),
            completion_policy: CompletionPolicy::CandidateOnly,
            implementer: ImplementerSpec {
                provider: provider_spec(&self.campaign_spec.implementer),
                authentication: match self.campaign_spec.implementer_authentication {
                    CampaignAuthentication::Bare => AuthenticationMode::Bare,
                    CampaignAuthentication::ExistingLogin => AuthenticationMode::ExistingLogin,
                },
            },
            reviewer: provider_spec(&self.campaign_spec.reviewer),
        };
        let external_context = format!(
            "REMOTE CI CORRECTION\nThe configured remote checks failed for reviewed commit {prior_commit}. The failure observation is bound by SHA-256 evidence {ci_evidence}. Re-run every authorized local gate, inspect task-scoped platform or integration assumptions, make only necessary changes within the declared owned paths, and return a complete candidate for fresh independent review. Do not access the network or alter publication policy."
        );
        Ok(CiCorrectionContext {
            run_id,
            correction_root,
            correction_config,
            spec,
            original_config,
            original_worktree,
            prior_commit,
            ci_evidence,
            external_context,
        })
    }

    fn load_or_run_ci_correction(
        &self,
        record: &CampaignTaskRecord,
        context: &CiCorrectionContext,
        cancellation: CancellationToken,
        progress: impl FnMut(RunProgress),
    ) -> Result<CiCorrectionExecution, RuntimeError> {
        let verify = |value: &CiCorrectionExecution| {
            value.verify(
                &record.campaign_id,
                &record.task_id,
                context.run_id.as_str(),
                &context.prior_commit,
                &context.ci_evidence,
            )
        };
        if fs::symlink_metadata(context.correction_root.join(CI_CORRECTION_RESULT_NAME)).is_ok() {
            return IntegrityDocument::<CiCorrectionExecution>::load(
                &context.correction_root,
                CI_CORRECTION_RESULT_NAME,
                verify,
            )
            .map(|document| document.payload)
            .map_err(|_| RuntimeError::State);
        }
        let lifecycle_events = Arc::new(Mutex::new(Vec::new()));
        let lifecycle_capture = Arc::clone(&lifecycle_events);
        let lifecycle: LifecycleObserver = Arc::new(move |event| {
            lifecycle_capture
                .lock()
                .map_err(|_| RuntimeError::State)?
                .push(event);
            Ok(())
        });
        let mut limited_progress =
            limited_progress_observer(self.actor_permits.clone(), cancellation.clone(), progress);
        let outcome = run_one_with_progress_id_budget(
            &context.correction_config,
            context.spec.clone(),
            &self.codingmage_binary,
            context.run_id.clone(),
            &mut limited_progress,
            None,
            None,
            Some(lifecycle),
            cancellation,
            Some(context.external_context.clone()),
        )?;
        let candidate_commit = outcome
            .candidate_commit
            .clone()
            .ok_or(RuntimeError::State)?;
        if outcome.state != TaskState::Checkpointed
            || outcome.review_verdict.as_deref() != Some("pass")
            || candidate_commit == context.prior_commit
        {
            return Err(RuntimeError::Verification);
        }
        let execution = CiCorrectionExecution {
            version: CI_CORRECTION_RESULT_VERSION,
            campaign_id: record.campaign_id.clone(),
            task_id: record.task_id.clone(),
            run_id: context.run_id.as_str().to_owned(),
            prior_commit: context.prior_commit.clone(),
            ci_evidence_sha256: context.ci_evidence.clone(),
            candidate_commit,
            utilization: task_utilization(&outcome.utilization),
            events: lifecycle_events
                .lock()
                .map_err(|_| RuntimeError::State)?
                .clone(),
        };
        if !verify(&execution) {
            return Err(RuntimeError::State);
        }
        IntegrityDocument::write_atomic(
            &context.correction_root,
            CI_CORRECTION_RESULT_NAME,
            execution.clone(),
            verify,
        )
        .map_err(|_| RuntimeError::State)?;
        Ok(execution)
    }

    fn reconcile_ci_correction(
        &self,
        record: &CampaignTaskRecord,
        context: &CiCorrectionContext,
        execution: &CiCorrectionExecution,
    ) -> Result<(), RuntimeError> {
        let source_root = self
            .codingmage_binary
            .parent()
            .ok_or(RuntimeError::Authority)?;
        let authorization =
            RepositoryAuthorization::authorize(&context.original_config, source_root)
                .map_err(|_| RuntimeError::Authority)?;
        let observed = context
            .original_worktree
            .observe_head(&authorization)
            .map_err(|_| RuntimeError::Repository)?;
        if observed == context.prior_commit {
            integrate_reviewed_descendant(
                &authorization,
                &context.original_worktree,
                &context.prior_commit,
                &execution.candidate_commit,
                &record.owned_paths,
            )
            .map_err(|_| RuntimeError::Integration)?;
        } else if observed != execution.candidate_commit {
            return Err(RuntimeError::State);
        }
        Ok(())
    }
}

struct CiCorrectionContext {
    run_id: RunId,
    correction_root: PathBuf,
    correction_config: Config,
    spec: RunSpec,
    original_config: Config,
    original_worktree: OwnedWorktree,
    prior_commit: String,
    ci_evidence: String,
    external_context: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CiCorrectionExecution {
    version: u16,
    campaign_id: String,
    task_id: String,
    run_id: String,
    prior_commit: String,
    ci_evidence_sha256: String,
    candidate_commit: String,
    utilization: TaskUtilization,
    events: Vec<UnitLifecycleEvent>,
}

impl CiCorrectionExecution {
    fn verify(
        &self,
        campaign_id: &str,
        task_id: &str,
        run_id: &str,
        prior_commit: &str,
        ci_evidence_sha256: &str,
    ) -> bool {
        self.version == CI_CORRECTION_RESULT_VERSION
            && self.campaign_id == campaign_id
            && self.task_id == task_id
            && self.run_id == run_id
            && self.prior_commit == prior_commit
            && self.ci_evidence_sha256 == ci_evidence_sha256
            && valid_commit(&self.prior_commit)
            && valid_commit(&self.candidate_commit)
            && self.candidate_commit != self.prior_commit
            && valid_sha256(&self.ci_evidence_sha256)
            && !self.events.is_empty()
            && self.events.len() <= 1_000
            && self.lifecycle_evidence().is_ok()
    }

    fn lifecycle_evidence(&self) -> Result<CiCorrectionLifecycle, RuntimeError> {
        let mut evidence = CiCorrectionLifecycle::default();
        let mut candidate_observed = false;
        let mut passing_review_observed = false;
        for event in &self.events {
            match event {
                UnitLifecycleEvent::ImplementationSessionBound { session_id, .. } => {
                    if session_id.is_empty() {
                        return Err(RuntimeError::State);
                    }
                    evidence.implementation_sessions.push(session_id.clone());
                }
                UnitLifecycleEvent::CandidateCommitted { commit, .. }
                    if commit == &self.candidate_commit =>
                {
                    candidate_observed = true;
                }
                UnitLifecycleEvent::GatesObserved {
                    commit,
                    evidence: gate_evidence,
                    passed,
                } if commit == &self.candidate_commit && *passed => {
                    if gate_evidence.is_empty()
                        || gate_evidence.iter().any(|value| !valid_sha256(value))
                    {
                        return Err(RuntimeError::State);
                    }
                    evidence.gate_evidence.extend(gate_evidence.clone());
                }
                UnitLifecycleEvent::ReviewObserved {
                    session_id,
                    commit,
                    verdict,
                    evidence: review_evidence,
                } if commit == &self.candidate_commit && verdict == "pass" => {
                    if session_id.is_empty() || !valid_sha256(review_evidence) {
                        return Err(RuntimeError::State);
                    }
                    passing_review_observed = true;
                    evidence.review_sessions.push(session_id.clone());
                    evidence.review_evidence.push(review_evidence.clone());
                }
                UnitLifecycleEvent::WorktreeCreated { .. }
                | UnitLifecycleEvent::CandidateCommitted { .. }
                | UnitLifecycleEvent::GatesObserved { .. }
                | UnitLifecycleEvent::ReviewObserved { .. }
                | UnitLifecycleEvent::CompletionCommitted { .. }
                | UnitLifecycleEvent::WorktreeReleased { .. } => {}
            }
        }
        if evidence.implementation_sessions.is_empty()
            || evidence.gate_evidence.is_empty()
            || evidence.review_sessions.is_empty()
            || !candidate_observed
            || !passing_review_observed
            || evidence
                .implementation_sessions
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != evidence.implementation_sessions.len()
            || evidence
                .review_sessions
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != evidence.review_sessions.len()
        {
            return Err(RuntimeError::State);
        }
        Ok(evidence)
    }
}

#[derive(Default)]
struct CiCorrectionLifecycle {
    implementation_sessions: Vec<String>,
    gate_evidence: Vec<String>,
    review_sessions: Vec<String>,
    review_evidence: Vec<String>,
}

impl TeamUnitRunner for ProductionTeamUnitRunner {
    fn run(
        &self,
        job: &TeamBatchJob,
        cancellation: CancellationToken,
        events: TeamEventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        let config = self.config_for(job)?;
        let spec = self.run_spec_for(job);
        let progress_sink = events.clone();
        let sink_cancellation = cancellation.clone();
        let sink_observer = move |event| {
            if progress_sink.progress(event).is_err() {
                sink_cancellation.cancel();
            }
        };
        let mut progress = limited_progress_observer(
            self.actor_permits.clone(),
            cancellation.clone(),
            sink_observer,
        );
        let lifecycle_sink = events.clone();
        let lifecycle: LifecycleObserver = Arc::new(move |event| lifecycle_sink.lifecycle(event));
        let outcome = run_one_with_progress_id_budget(
            &config,
            spec,
            &self.codingmage_binary,
            job.run_id.clone(),
            &mut progress,
            None,
            None,
            Some(lifecycle),
            cancellation,
            None,
        )?;
        events.heartbeat(task_utilization(&outcome.utilization))?;
        Ok(outcome)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermitClass {
    Gate,
    Reviewer,
}

fn permit_class(progress: RunProgress) -> Option<PermitClass> {
    match progress.actor {
        crate::ProgressActor::LocalGates => Some(PermitClass::Gate),
        crate::ProgressActor::Codex => Some(PermitClass::Reviewer),
        crate::ProgressActor::Coordinator
        | crate::ProgressActor::Claude
        | crate::ProgressActor::CampaignLead
        | crate::ProgressActor::IntegrationLead => None,
    }
}

fn limited_progress_observer<F>(
    actor_permits: ActorPermitPool,
    cancellation: CancellationToken,
    mut observer: F,
) -> impl FnMut(RunProgress)
where
    F: FnMut(RunProgress),
{
    let mut actor_permit: Option<(PermitClass, ActorPermit)> = None;
    move |event| {
        let desired = permit_class(event);
        if actor_permit.as_ref().map(|value| value.0) != desired {
            actor_permit = None;
            if let Some(class) = desired {
                let Ok(permit) = actor_permits.acquire(class, &cancellation) else {
                    cancellation.cancel();
                    return;
                };
                actor_permit = Some((class, permit));
            }
        }
        observer(event);
    }
}

#[derive(Clone, Debug)]
struct ActorPermitPool {
    gates: Arc<BoundedSemaphore>,
    reviewers: Arc<BoundedSemaphore>,
}

impl ActorPermitPool {
    fn new(gates: usize, reviewers: usize) -> Result<Self, RuntimeError> {
        if gates == 0 || reviewers == 0 {
            return Err(RuntimeError::Authority);
        }
        Ok(Self {
            gates: Arc::new(BoundedSemaphore::new(gates)),
            reviewers: Arc::new(BoundedSemaphore::new(reviewers)),
        })
    }

    fn acquire(
        &self,
        class: PermitClass,
        cancellation: &CancellationToken,
    ) -> Result<ActorPermit, ()> {
        match class {
            PermitClass::Gate => BoundedSemaphore::acquire(&self.gates, cancellation),
            PermitClass::Reviewer => BoundedSemaphore::acquire(&self.reviewers, cancellation),
        }
    }
}

#[derive(Debug)]
struct BoundedSemaphore {
    limit: usize,
    active: Mutex<usize>,
    changed: Condvar,
}

impl BoundedSemaphore {
    const fn new(limit: usize) -> Self {
        Self {
            limit,
            active: Mutex::new(0),
            changed: Condvar::new(),
        }
    }

    fn acquire(semaphore: &Arc<Self>, cancellation: &CancellationToken) -> Result<ActorPermit, ()> {
        let mut active = semaphore.active.lock().map_err(|_| ())?;
        while *active >= semaphore.limit {
            if cancellation.is_cancelled() {
                return Err(());
            }
            let (observed, _) = semaphore
                .changed
                .wait_timeout(active, MESSAGE_POLL_INTERVAL)
                .map_err(|_| ())?;
            active = observed;
        }
        if cancellation.is_cancelled() {
            return Err(());
        }
        *active = active.checked_add(1).ok_or(())?;
        drop(active);
        Ok(ActorPermit {
            semaphore: Arc::clone(semaphore),
        })
    }
}

#[derive(Debug)]
struct ActorPermit {
    semaphore: Arc<BoundedSemaphore>,
}

impl Drop for ActorPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.semaphore.active.lock() {
            *active = active.saturating_sub(1);
            self.semaphore.changed.notify_one();
        }
    }
}

/// One operator-facing, content-minimized team observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeamBatchObservation {
    /// One pod reported typed progress.
    TaskProgress {
        /// Stable dispatch sequence.
        sequence: u64,
        /// Canonical task identity.
        task_id: String,
        /// Typed progress observation.
        progress: RunProgress,
    },
    /// One pod reached a terminal one-unit result.
    TaskFinished {
        /// Stable dispatch sequence.
        sequence: u64,
        /// Canonical task identity.
        task_id: String,
        /// Success or stable content-free failure code.
        result: Result<TaskState, &'static str>,
    },
}

/// One terminal pod result retained in stable dispatch order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamTaskOutcome {
    /// Stable dispatch sequence.
    pub sequence: u64,
    /// Canonical task identity.
    pub task_id: String,
    /// One-unit outcome or stable fail-closed runtime error.
    pub result: Result<RunOutcome, RuntimeError>,
}

/// Complete bounded batch result and its final durable projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamBatchOutcome {
    /// Results sorted by dispatch sequence, independent of completion timing.
    pub tasks: Vec<TeamTaskOutcome>,
    /// Task identities in observed completion order.
    pub completion_order: Vec<String>,
    /// Final integrity-valid campaign projection.
    pub snapshot: TeamCampaignSnapshot,
}

#[derive(Debug)]
enum WorkerEvent {
    Progress(RunProgress),
    Lifecycle(UnitLifecycleEvent),
    Heartbeat(TaskUtilization),
    Liveness,
    Finished(Result<RunOutcome, RuntimeError>),
}

#[derive(Debug)]
struct WorkerMessage {
    sequence: u64,
    task_id: String,
    event: WorkerEvent,
}

struct WorkerSet {
    receiver: Receiver<WorkerMessage>,
    controls: BTreeMap<String, CancellationToken>,
    reservations: BTreeMap<String, String>,
    deadlines: BTreeMap<String, u64>,
    expected_tasks: BTreeMap<u64, String>,
    handles: Vec<thread::JoinHandle<()>>,
}

struct BatchDriver<'a, P, O> {
    snapshot: &'a mut TeamCampaignSnapshot,
    resources: TeamResourceController,
    scheduler: DurablePodScheduler,
    reservations: &'a BTreeMap<String, String>,
    heartbeat_sequences: BTreeMap<String, u64>,
    persist: &'a mut P,
    observe: &'a mut O,
    results: Vec<TeamTaskOutcome>,
    completion_order: Vec<String>,
}

impl<P, O> BatchDriver<'_, P, O>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
    O: FnMut(TeamBatchObservation),
{
    fn handle(
        &mut self,
        message: WorkerMessage,
        timed_out: bool,
        campaign_cancelled: bool,
    ) -> Result<(), RuntimeError> {
        match message.event {
            WorkerEvent::Progress(progress) => {
                self.touch(&message.task_id, None)?;
                apply_progress(self.snapshot, &message.task_id, progress)?;
                self.persist()?;
                (self.observe)(TeamBatchObservation::TaskProgress {
                    sequence: message.sequence,
                    task_id: message.task_id,
                    progress,
                });
            }
            WorkerEvent::Lifecycle(event) => {
                self.touch(&message.task_id, None)?;
                apply_lifecycle(self.snapshot, &message.task_id, event)?;
                self.persist()?;
            }
            WorkerEvent::Heartbeat(utilization) => {
                self.touch(&message.task_id, Some(utilization))?;
                self.persist()?;
            }
            WorkerEvent::Liveness => {
                self.touch(&message.task_id, None)?;
                self.persist()?;
            }
            WorkerEvent::Finished(mut result) => {
                if timed_out {
                    result = Err(RuntimeError::Process);
                }
                let task_state = result
                    .as_ref()
                    .map(|value| value.state)
                    .map_err(|error| error.code());
                finish_task(
                    self.snapshot,
                    &mut self.resources,
                    &mut self.scheduler,
                    self.reservations,
                    &message.task_id,
                    &result,
                    TerminalContext {
                        timed_out,
                        campaign_cancelled,
                    },
                )?;
                self.persist()?;
                self.completion_order.push(message.task_id.clone());
                (self.observe)(TeamBatchObservation::TaskFinished {
                    sequence: message.sequence,
                    task_id: message.task_id.clone(),
                    result: task_state,
                });
                self.results.push(TeamTaskOutcome {
                    sequence: message.sequence,
                    task_id: message.task_id,
                    result,
                });
            }
        }
        Ok(())
    }

    fn touch(
        &mut self,
        task_id: &str,
        utilization: Option<TaskUtilization>,
    ) -> Result<(), RuntimeError> {
        touch_task(
            self.snapshot,
            &mut self.resources,
            &mut self.scheduler,
            self.reservations,
            &mut self.heartbeat_sequences,
            task_id,
            utilization,
        )
    }

    fn persist(&mut self) -> Result<(), RuntimeError> {
        persist_projection(
            self.snapshot,
            &self.resources,
            &self.scheduler,
            self.persist,
        )
    }
}

#[derive(Clone, Copy)]
struct TerminalContext {
    timed_out: bool,
    campaign_cancelled: bool,
}

/// Executes an already-admitted set of independent pods concurrently.
///
/// All reservations are validated and durably persisted before any worker starts. Worker panics,
/// deadlines, cancellation, and ordinary failures are isolated to their exact task. Results are
/// returned in stable dispatch order while completion order is retained separately.
///
/// # Errors
///
/// Returns a stable fail-closed error before execution for contradictory authority, resources, or
/// identity. A persistence failure cancels every exact child and returns without claiming success.
pub fn execute_team_batch<R, P, O>(
    spec: &CampaignSpec,
    snapshot: &mut TeamCampaignSnapshot,
    jobs: &[TeamBatchJob],
    runner: &Arc<R>,
    campaign_cancellation: &CancellationToken,
    mut persist: P,
    mut observe: O,
) -> Result<TeamBatchOutcome, RuntimeError>
where
    R: TeamUnitRunner + ?Sized,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
    O: FnMut(TeamBatchObservation),
{
    let (resources, scheduler) = prepare_batch(spec, snapshot, jobs, &mut persist)?;
    let WorkerSet {
        receiver,
        controls,
        reservations,
        deadlines,
        expected_tasks,
        handles,
    } = spawn_workers(
        jobs,
        runner,
        campaign_cancellation,
        Duration::from_millis(snapshot.resources.policy.heartbeat_interval_ms),
    );
    let mut timed_out = BTreeSet::new();
    let mut fatal_error = None;
    let mut driver = BatchDriver {
        snapshot,
        resources,
        scheduler,
        reservations: &reservations,
        heartbeat_sequences: controls
            .keys()
            .map(|task_id| (task_id.clone(), 0_u64))
            .collect(),
        persist: &mut persist,
        observe: &mut observe,
        results: Vec::with_capacity(jobs.len()),
        completion_order: Vec::with_capacity(jobs.len()),
    };
    while driver.results.len() < jobs.len() {
        cancel_expired(
            &controls,
            &deadlines,
            &mut timed_out,
            campaign_cancellation.is_cancelled(),
        );
        match receiver.recv_timeout(MESSAGE_POLL_INTERVAL) {
            Ok(message) => {
                if !controls.contains_key(&message.task_id)
                    || expected_tasks.get(&message.sequence) != Some(&message.task_id)
                {
                    fatal_error = Some(RuntimeError::State);
                    break;
                }
                if deadlines
                    .get(&message.task_id)
                    .is_some_and(|deadline| now_ms() >= *deadline)
                {
                    timed_out.insert(message.task_id.clone());
                    if let Some(control) = controls.get(&message.task_id) {
                        control.cancel();
                    }
                }
                if timed_out.contains(&message.task_id)
                    && !matches!(&message.event, WorkerEvent::Finished(_))
                {
                    continue;
                }
                let task_timed_out = timed_out.contains(&message.task_id);
                if let Err(error) = driver.handle(
                    message,
                    task_timed_out,
                    campaign_cancellation.is_cancelled(),
                ) {
                    fatal_error = Some(error);
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                fatal_error = Some(RuntimeError::State);
                break;
            }
        }
    }
    if let Some(error) = fatal_error {
        drop(driver);
        cancel_all(&controls);
        drop(receiver);
        join_all(handles);
        return Err(error);
    }
    driver.results.sort_by_key(|outcome| outcome.sequence);
    let result = TeamBatchOutcome {
        tasks: std::mem::take(&mut driver.results),
        completion_order: std::mem::take(&mut driver.completion_order),
        snapshot: driver.snapshot.clone(),
    };
    drop(driver);
    drop(receiver);
    join_all(handles);
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    Ok(result)
}

/// Reviewed remote-CI correction accepted back onto the original task branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamCiCorrectionOutcome {
    /// Exact corrected task.
    pub task_id: String,
    /// Fresh locally gated and independently reviewed candidate.
    pub reviewed_commit: String,
}

/// Resumes one exact CI-failed task through bounded implementation, gates, and fresh review.
///
/// The retained task worktree is the immutable starting point. A deterministic child run captures
/// an integrity-protected correction result before the original task branch is fast-forwarded, so
/// interruption after either local effect is reconciled without duplicate provider work.
///
/// # Errors
///
/// Returns a content-free authority, resource, provider, gate, review, repository, or persistence
/// failure. No unrelated task state is changed.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn execute_team_ci_correction<P, O>(
    spec: &CampaignSpec,
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    runner: &ProductionTeamUnitRunner,
    cancellation: CancellationToken,
    mut persist: P,
    mut observe: O,
) -> Result<TeamCiCorrectionOutcome, RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
    O: FnMut(RunProgress),
{
    spec.verify().map_err(RuntimeError::Campaign)?;
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    let record = snapshot
        .tasks
        .get(task_id)
        .cloned()
        .ok_or(RuntimeError::State)?;
    if record.state != CampaignTaskState::Correcting
        || record.reviewed_commit.is_some()
        || record.candidate_commit.is_none()
        || record.ci_evidence_sha256.is_empty()
    {
        return Err(RuntimeError::State);
    }
    let lease_id = record.lease_id.as_deref().ok_or(RuntimeError::State)?;
    let lease = snapshot
        .scheduler
        .active
        .get(lease_id)
        .cloned()
        .ok_or(RuntimeError::State)?;
    if lease.task_id != task_id {
        return Err(RuntimeError::State);
    }
    let (mut resources, reservation) =
        ensure_ci_correction_reservation(spec, snapshot, &lease, now_ms(), &mut persist)?;

    let result = runner.run_ci_correction(&record, cancellation, &mut observe);
    let (context, execution) = match result {
        Ok(execution) => execution,
        Err(error) => {
            resources
                .release(&reservation.reservation_id)
                .map_err(|_| RuntimeError::State)?;
            snapshot.resources = resources.snapshot().clone();
            persist(snapshot)?;
            return Err(error);
        }
    };
    let observed_at = now_ms();
    if observed_at > reservation.deadline_ms {
        resources
            .release(&reservation.reservation_id)
            .map_err(|_| RuntimeError::State)?;
        snapshot.resources = resources.snapshot().clone();
        persist(snapshot)?;
        return Err(RuntimeError::Process);
    }
    if resources
        .observe(
            &reservation.reservation_id,
            reservation.heartbeat_sequence.saturating_add(1),
            observed_at,
            execution.utilization.clone(),
        )
        .is_err()
    {
        resources
            .release(&reservation.reservation_id)
            .map_err(|_| RuntimeError::State)?;
        snapshot.resources = resources.snapshot().clone();
        persist(snapshot)?;
        return Err(RuntimeError::State);
    }
    let released = resources
        .release(&reservation.reservation_id)
        .map_err(|_| RuntimeError::State)?;

    let lifecycle = match execution.lifecycle_evidence() {
        Ok(lifecycle) => lifecycle,
        Err(error) => {
            snapshot.resources = resources.snapshot().clone();
            persist(snapshot)?;
            return Err(error);
        }
    };
    if lifecycle.implementation_sessions.iter().any(|session| {
        record.implementation_session.as_ref() == Some(session)
            || record
                .correction_sessions
                .iter()
                .any(|value| value == session)
    }) {
        snapshot.resources = resources.snapshot().clone();
        persist(snapshot)?;
        return Err(RuntimeError::State);
    }
    let combined_utilization = match add_utilization(&record.utilization, &released.observed) {
        Ok(utilization) => utilization,
        Err(error) => {
            snapshot.resources = resources.snapshot().clone();
            persist(snapshot)?;
            return Err(error);
        }
    };
    if let Err(error) = runner.reconcile_ci_correction(&record, &context, &execution) {
        snapshot.resources = resources.snapshot().clone();
        persist(snapshot)?;
        return Err(error);
    }

    let mut corrected = record;
    corrected
        .correction_sessions
        .extend(lifecycle.implementation_sessions);
    corrected
        .gate_evidence_sha256
        .extend(lifecycle.gate_evidence);
    corrected.review_sessions.extend(lifecycle.review_sessions);
    corrected
        .review_evidence_sha256
        .extend(lifecycle.review_evidence);
    corrected.candidate_commit = Some(execution.candidate_commit.clone());
    corrected.reviewed_commit = Some(execution.candidate_commit.clone());
    corrected.integration_commit = None;
    corrected.completion_commit = None;
    corrected.utilization = combined_utilization;
    transition(
        &mut corrected,
        CampaignTaskState::LocalGates,
        "ci-correction-gates",
    )?;
    transition(
        &mut corrected,
        CampaignTaskState::Reviewing,
        "ci-correction-review",
    )?;
    transition(
        &mut corrected,
        CampaignTaskState::PublicationReady,
        "ci-correction-ready",
    )?;
    snapshot.tasks.insert(task_id.to_owned(), corrected);
    snapshot.resources = resources.snapshot().clone();
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    persist(snapshot)?;
    Ok(TeamCiCorrectionOutcome {
        task_id: task_id.to_owned(),
        reviewed_commit: execution.candidate_commit,
    })
}

fn ensure_ci_correction_reservation<P>(
    spec: &CampaignSpec,
    snapshot: &mut TeamCampaignSnapshot,
    lease: &DurablePodLease,
    started_at_ms: u64,
    persist: &mut P,
) -> Result<(TeamResourceController, TaskResourceReservation), RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    let mut resources = TeamResourceController::from_snapshot(spec, snapshot.resources.clone())
        .map_err(|_| RuntimeError::State)?;
    let existing = resources
        .snapshot()
        .active
        .values()
        .find(|reservation| {
            reservation.lease_id == lease.lease_id && reservation.actor == ActorClass::Implementer
        })
        .cloned();
    let existing = if existing
        .as_ref()
        .is_some_and(|reservation| started_at_ms > reservation.deadline_ms)
    {
        let reservation_id = existing
            .as_ref()
            .map(|reservation| reservation.reservation_id.clone())
            .ok_or(RuntimeError::State)?;
        resources
            .release(&reservation_id)
            .map_err(|_| RuntimeError::State)?;
        snapshot.resources = resources.snapshot().clone();
        persist(snapshot)?;
        None
    } else {
        existing
    };
    if let Some(reservation) = existing {
        if reservation.task_id != lease.task_id {
            return Err(RuntimeError::State);
        }
        return Ok((resources, reservation));
    }
    let reservation = resources
        .implementation_request(lease, started_at_ms)
        .map_err(|_| RuntimeError::State)?;
    resources
        .reserve(reservation.clone())
        .map_err(|_| RuntimeError::State)?;
    snapshot.resources = resources.snapshot().clone();
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    persist(snapshot)?;
    Ok((resources, reservation))
}

fn prepare_batch<P>(
    spec: &CampaignSpec,
    snapshot: &mut TeamCampaignSnapshot,
    jobs: &[TeamBatchJob],
    persist: &mut P,
) -> Result<(TeamResourceController, DurablePodScheduler), RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    spec.verify().map_err(RuntimeError::Campaign)?;
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    if jobs.is_empty()
        || jobs.len() > usize::from(snapshot.scheduler.max_parallel_pods)
        || jobs
            .iter()
            .map(|job| job.sequence)
            .collect::<BTreeSet<_>>()
            .len()
            != jobs.len()
        || jobs
            .iter()
            .map(|job| &job.lease.task_id)
            .collect::<BTreeSet<_>>()
            .len()
            != jobs.len()
    {
        return Err(RuntimeError::State);
    }
    for job in jobs {
        job.verify(snapshot)?;
    }
    let mut candidate = snapshot.clone();
    let mut resources = TeamResourceController::from_snapshot(spec, candidate.resources.clone())
        .map_err(|_| RuntimeError::State)?;
    let scheduler = DurablePodScheduler::from_snapshot(candidate.scheduler.clone())
        .map_err(|_| RuntimeError::State)?;
    for job in jobs {
        let record = candidate
            .tasks
            .get_mut(&job.lease.task_id)
            .ok_or(RuntimeError::State)?;
        record
            .bind_run(
                job.run_id.as_str().to_owned(),
                event_evidence(record, "run"),
            )
            .map_err(|_| RuntimeError::State)?;
        resources
            .reserve(job.reservation.clone())
            .map_err(|_| RuntimeError::State)?;
    }
    candidate.resources = resources.snapshot().clone();
    candidate.verify().map_err(|_| RuntimeError::State)?;
    persist(&candidate)?;
    *snapshot = candidate;
    Ok((resources, scheduler))
}

fn spawn_workers<R>(
    jobs: &[TeamBatchJob],
    runner: &Arc<R>,
    campaign_cancellation: &CancellationToken,
    heartbeat_interval: Duration,
) -> WorkerSet
where
    R: TeamUnitRunner + ?Sized,
{
    let channel_capacity = jobs.len().saturating_mul(16).max(MIN_CHANNEL_CAPACITY);
    let (sender, receiver) = sync_channel(channel_capacity);
    let mut controls = BTreeMap::new();
    let mut reservations = BTreeMap::new();
    let mut deadlines = BTreeMap::new();
    let mut expected_tasks = BTreeMap::new();
    let mut handles = Vec::new();
    for job in jobs.iter().cloned() {
        let task_id = job.lease.task_id.clone();
        let token = campaign_cancellation.child();
        controls.insert(task_id.clone(), token.clone());
        reservations.insert(task_id.clone(), job.reservation.reservation_id.clone());
        deadlines.insert(task_id.clone(), job.reservation.deadline_ms);
        expected_tasks.insert(job.sequence, task_id.clone());
        let worker_sender = sender.clone();
        let heartbeat_sender = sender.clone();
        let heartbeat_task_id = task_id.clone();
        let heartbeat_sequence = job.sequence;
        let worker_runner = Arc::clone(runner);
        handles.push(thread::spawn(move || {
            let (heartbeat_stop, heartbeat_control) = std::sync::mpsc::channel::<()>();
            let heartbeat = thread::spawn(move || {
                loop {
                    match heartbeat_control.recv_timeout(heartbeat_interval) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if heartbeat_sender
                                .send(WorkerMessage {
                                    sequence: heartbeat_sequence,
                                    task_id: heartbeat_task_id.clone(),
                                    event: WorkerEvent::Liveness,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            });
            let events = TeamEventSink {
                sequence: job.sequence,
                task_id: task_id.clone(),
                sender: worker_sender.clone(),
            };
            let result = catch_unwind(AssertUnwindSafe(|| worker_runner.run(&job, token, events)))
                .unwrap_or(Err(RuntimeError::Process));
            let _ = heartbeat_stop.send(());
            let _ = heartbeat.join();
            let _ = worker_sender.send(WorkerMessage {
                sequence: job.sequence,
                task_id,
                event: WorkerEvent::Finished(result),
            });
        }));
    }
    drop(sender);
    WorkerSet {
        receiver,
        controls,
        reservations,
        deadlines,
        expected_tasks,
        handles,
    }
}

fn apply_progress(
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    progress: RunProgress,
) -> Result<(), RuntimeError> {
    use crate::ProgressStage;

    let record = snapshot.tasks.get_mut(task_id).ok_or(RuntimeError::State)?;
    let target = match progress.stage {
        ProgressStage::VerifyingCandidate if record.state == CampaignTaskState::Implementing => {
            Some(CampaignTaskState::LocalGates)
        }
        ProgressStage::Reviewing if record.state == CampaignTaskState::LocalGates => {
            Some(CampaignTaskState::Reviewing)
        }
        ProgressStage::CandidateBlocked | ProgressStage::Correcting
            if matches!(
                record.state,
                CampaignTaskState::LocalGates | CampaignTaskState::Reviewing
            ) =>
        {
            Some(CampaignTaskState::Correcting)
        }
        _ => None,
    };
    if let Some(target) = target {
        transition(record, target, "progress")?;
    }
    Ok(())
}

fn apply_lifecycle(
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    event: UnitLifecycleEvent,
) -> Result<(), RuntimeError> {
    let record = snapshot.tasks.get_mut(task_id).ok_or(RuntimeError::State)?;
    match event {
        UnitLifecycleEvent::WorktreeCreated {
            worktree_id,
            branch,
        } => record
            .bind_worktree(worktree_id, branch, event_evidence(record, "worktree"))
            .map_err(|_| RuntimeError::State),
        UnitLifecycleEvent::ImplementationSessionBound {
            session_id,
            correction_round,
        } => {
            let mut candidate = record.clone();
            if correction_round == 0 {
                if candidate.implementation_session.is_some() {
                    return Err(RuntimeError::State);
                }
                candidate.implementation_session = Some(session_id);
            } else {
                candidate.correction_sessions.push(session_id);
            }
            candidate.verify().map_err(|_| RuntimeError::State)?;
            *record = candidate;
            Ok(())
        }
        UnitLifecycleEvent::CandidateCommitted {
            commit,
            correction_round: _,
        } => {
            let mut candidate = record.clone();
            candidate.candidate_commit = Some(commit);
            candidate.reviewed_commit = None;
            candidate.integration_commit = None;
            candidate.completion_commit = None;
            candidate.verify().map_err(|_| RuntimeError::State)?;
            *record = candidate;
            Ok(())
        }
        UnitLifecycleEvent::GatesObserved {
            commit,
            evidence,
            passed: _,
        } => {
            if record.candidate_commit.as_deref() != Some(commit.as_str()) {
                return Err(RuntimeError::State);
            }
            if matches!(
                record.state,
                CampaignTaskState::Implementing | CampaignTaskState::Correcting
            ) {
                transition(record, CampaignTaskState::LocalGates, "gates")?;
            }
            let mut candidate = record.clone();
            candidate.gate_evidence_sha256.extend(evidence);
            candidate.verify().map_err(|_| RuntimeError::State)?;
            *record = candidate;
            Ok(())
        }
        UnitLifecycleEvent::ReviewObserved {
            session_id,
            commit,
            verdict,
            evidence,
        } => {
            if record.candidate_commit.as_deref() != Some(commit.as_str()) {
                return Err(RuntimeError::State);
            }
            if record.state == CampaignTaskState::LocalGates {
                transition(record, CampaignTaskState::Reviewing, "review")?;
            }
            let mut candidate = record.clone();
            candidate.review_sessions.push(session_id);
            candidate.review_evidence_sha256.push(evidence);
            if verdict == "pass" {
                candidate.reviewed_commit = Some(commit);
            }
            candidate.verify().map_err(|_| RuntimeError::State)?;
            *record = candidate;
            Ok(())
        }
        UnitLifecycleEvent::CompletionCommitted { commit } => {
            let mut candidate = record.clone();
            candidate.completion_commit = Some(commit);
            candidate.verify().map_err(|_| RuntimeError::State)?;
            *record = candidate;
            if record.state == CampaignTaskState::Reviewing {
                transition(record, CampaignTaskState::PublicationReady, "completion")?;
            }
            Ok(())
        }
        UnitLifecycleEvent::WorktreeReleased { worktree_id } => {
            if record.worktree_id.as_deref() != Some(worktree_id.as_str()) {
                return Err(RuntimeError::State);
            }
            Ok(())
        }
    }
}

fn touch_task(
    snapshot: &mut TeamCampaignSnapshot,
    resources: &mut TeamResourceController,
    scheduler: &mut DurablePodScheduler,
    reservations: &BTreeMap<String, String>,
    sequences: &mut BTreeMap<String, u64>,
    task_id: &str,
    utilization: Option<TaskUtilization>,
) -> Result<(), RuntimeError> {
    let sequence = sequences.get_mut(task_id).ok_or(RuntimeError::State)?;
    *sequence = sequence.saturating_add(1);
    let timestamp = now_ms();
    let record = snapshot.tasks.get_mut(task_id).ok_or(RuntimeError::State)?;
    let lease_id = record.lease_id.clone().ok_or(RuntimeError::State)?;
    let reservation_id = reservations.get(task_id).ok_or(RuntimeError::State)?;
    let current = resources
        .snapshot()
        .active
        .get(reservation_id)
        .ok_or(RuntimeError::State)?
        .observed
        .clone();
    scheduler
        .heartbeat(&lease_id, *sequence, timestamp)
        .map_err(|_| RuntimeError::State)?;
    record
        .heartbeat(*sequence, timestamp)
        .map_err(|_| RuntimeError::State)?;
    resources
        .observe(
            reservation_id,
            *sequence,
            timestamp,
            utilization.unwrap_or(current),
        )
        .map_err(|_| RuntimeError::State)
}

fn finish_task(
    snapshot: &mut TeamCampaignSnapshot,
    resources: &mut TeamResourceController,
    scheduler: &mut DurablePodScheduler,
    reservations: &BTreeMap<String, String>,
    task_id: &str,
    result: &Result<RunOutcome, RuntimeError>,
    terminal: TerminalContext,
) -> Result<(), RuntimeError> {
    let record = snapshot.tasks.get(task_id).ok_or(RuntimeError::State)?;
    if result.as_ref().is_ok_and(|outcome| {
        outcome.task_id.as_str() != task_id
            || record.run_id.as_deref() != Some(outcome.run_id.as_str())
    }) {
        return Err(RuntimeError::State);
    }
    let reservation = reservations.get(task_id).ok_or(RuntimeError::State)?;
    let released = resources
        .release(reservation)
        .map_err(|_| RuntimeError::State)?;
    let record = snapshot.tasks.get_mut(task_id).ok_or(RuntimeError::State)?;
    record.utilization = released.observed;
    if matches!(result, Ok(outcome) if outcome.state == TaskState::Checkpointed)
        && record.state == CampaignTaskState::Reviewing
        && record.reviewed_commit.is_some()
        && record.completion_commit.is_none()
    {
        transition(record, CampaignTaskState::PublicationReady, "checkpoint")?;
        return Ok(());
    }
    if matches!(result, Ok(outcome) if outcome.state == TaskState::Complete)
        && record.state == CampaignTaskState::PublicationReady
    {
        return Ok(());
    }

    let (state, reason) = if terminal.campaign_cancelled {
        (
            CampaignTaskState::Cancelled,
            TaskTerminalReason::OperatorCancelled,
        )
    } else if terminal.timed_out {
        (CampaignTaskState::Failed, TaskTerminalReason::TimedOut)
    } else {
        classify_terminal(result)
    };
    transition_terminal(record, state, reason)?;
    let lease_id = record.lease_id.clone().ok_or(RuntimeError::State)?;
    scheduler
        .release(&lease_id)
        .map_err(|_| RuntimeError::State)?;
    Ok(())
}

fn classify_terminal(
    result: &Result<RunOutcome, RuntimeError>,
) -> (CampaignTaskState, TaskTerminalReason) {
    match result {
        Ok(outcome) => match outcome.state {
            TaskState::Blocked => (
                CampaignTaskState::Blocked,
                TaskTerminalReason::PrerequisiteBlocked,
            ),
            TaskState::Paused => (
                CampaignTaskState::Blocked,
                TaskTerminalReason::ProviderBlocked,
            ),
            TaskState::Cancelled => (
                CampaignTaskState::Cancelled,
                TaskTerminalReason::OperatorCancelled,
            ),
            _ => (
                CampaignTaskState::Failed,
                TaskTerminalReason::ProviderFailed,
            ),
        },
        Err(RuntimeError::Process) => (
            CampaignTaskState::Failed,
            TaskTerminalReason::ProcessCrashed,
        ),
        Err(RuntimeError::Verification) => {
            (CampaignTaskState::Failed, TaskTerminalReason::GateFailed)
        }
        Err(RuntimeError::Integration) => (
            CampaignTaskState::Blocked,
            TaskTerminalReason::IntegrationConflict,
        ),
        Err(RuntimeError::Repository | RuntimeError::State) => (
            CampaignTaskState::Blocked,
            TaskTerminalReason::StaleIdentity,
        ),
        Err(RuntimeError::CampaignLimit(_)) => {
            (CampaignTaskState::Failed, TaskTerminalReason::LimitExceeded)
        }
        Err(RuntimeError::Implementer(_)) => (
            CampaignTaskState::Failed,
            TaskTerminalReason::ProviderFailed,
        ),
        Err(RuntimeError::Reviewer(_)) => (
            CampaignTaskState::Blocked,
            TaskTerminalReason::ReviewBlocked,
        ),
        Err(
            RuntimeError::Spec
            | RuntimeError::Authority
            | RuntimeError::Plan
            | RuntimeError::Orchestration
            | RuntimeError::Campaign(_),
        ) => (CampaignTaskState::Blocked, TaskTerminalReason::PolicyDenied),
    }
}

fn task_utilization(observed: &crate::RunUtilization) -> TaskUtilization {
    TaskUtilization {
        provider_attempts: observed.provider_attempts,
        // Current provider adapters do not expose a trustworthy token count.
        provider_tokens: 0,
        process_invocations: observed.process_invocations,
        output_bytes: observed.output_bytes,
        retained_state_bytes: observed.retained_state_bytes,
        execution_elapsed_ms: observed.execution_elapsed_ms,
    }
}

fn transition(
    record: &mut CampaignTaskRecord,
    to: CampaignTaskState,
    label: &str,
) -> Result<(), RuntimeError> {
    let request = transition_request(record, to, label);
    record.transition(&request).map_err(|_| RuntimeError::State)
}

fn transition_terminal(
    record: &mut CampaignTaskRecord,
    to: CampaignTaskState,
    reason: TaskTerminalReason,
) -> Result<(), RuntimeError> {
    let request = transition_request(record, to, reason.code());
    record
        .transition_terminal(&request, reason)
        .map_err(|_| RuntimeError::State)
}

fn transition_request(
    record: &CampaignTaskRecord,
    to: CampaignTaskState,
    label: &str,
) -> CampaignTaskTransition {
    CampaignTaskTransition {
        sequence: record.next_transition,
        campaign_id: record.campaign_id.clone(),
        task_id: record.task_id.clone(),
        generation: record.generation,
        from: record.state,
        to,
        evidence_sha256: event_evidence(record, label),
    }
}

fn event_evidence(record: &CampaignTaskRecord, label: &str) -> String {
    let material = format!(
        "{}\0{}\0{}\0{}\0{label}",
        record.campaign_id, record.task_id, record.generation, record.next_transition
    );
    hex(&Sha256::digest(material.as_bytes()))
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn add_utilization(
    left: &TaskUtilization,
    right: &TaskUtilization,
) -> Result<TaskUtilization, RuntimeError> {
    Ok(TaskUtilization {
        provider_attempts: left
            .provider_attempts
            .checked_add(right.provider_attempts)
            .ok_or(RuntimeError::State)?,
        provider_tokens: left
            .provider_tokens
            .checked_add(right.provider_tokens)
            .ok_or(RuntimeError::State)?,
        process_invocations: left
            .process_invocations
            .checked_add(right.process_invocations)
            .ok_or(RuntimeError::State)?,
        output_bytes: left
            .output_bytes
            .checked_add(right.output_bytes)
            .ok_or(RuntimeError::State)?,
        retained_state_bytes: left
            .retained_state_bytes
            .checked_add(right.retained_state_bytes)
            .ok_or(RuntimeError::State)?,
        execution_elapsed_ms: left
            .execution_elapsed_ms
            .checked_add(right.execution_elapsed_ms)
            .ok_or(RuntimeError::State)?,
    })
}

fn persist_projection<P>(
    snapshot: &mut TeamCampaignSnapshot,
    resources: &TeamResourceController,
    scheduler: &DurablePodScheduler,
    persist: &mut P,
) -> Result<(), RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    snapshot.resources = resources.snapshot().clone();
    snapshot.scheduler = scheduler.snapshot().clone();
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    persist(snapshot)
}

fn cancel_expired(
    controls: &BTreeMap<String, CancellationToken>,
    deadlines: &BTreeMap<String, u64>,
    timed_out: &mut BTreeSet<String>,
    campaign_cancelled: bool,
) {
    let now = now_ms();
    for (task_id, token) in controls {
        if campaign_cancelled
            || deadlines
                .get(task_id)
                .is_some_and(|deadline| now >= *deadline)
        {
            token.cancel();
            if !campaign_cancelled {
                timed_out.insert(task_id.clone());
            }
        }
    }
}

fn cancel_all(controls: &BTreeMap<String, CancellationToken>) {
    for token in controls.values() {
        token.cancel();
    }
}

fn join_all(handles: Vec<thread::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.join();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        path::PathBuf,
        sync::{
            Barrier, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use codingmage_campaign::{
        AdmissionDecision, CampaignAuthentication, CampaignConcurrency, CampaignExecutionMode,
        CampaignGateTier, CampaignLimits, CampaignProvider, CampaignPublication,
        DestinationPromotionPolicy, MultiAgentPolicy, PodProposal, PodRisk,
        TEAM_STATE_SCHEMA_VERSION, TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode,
        TeamResourcePolicy,
    };
    use codingmage_contracts::TaskId;

    use super::*;
    use crate::RunUtilization;

    struct ActiveGuard<'a>(&'a AtomicUsize);

    impl Drop for ActiveGuard<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct FakeRunner {
        barrier: Option<Arc<Barrier>>,
        active: AtomicUsize,
        peak: AtomicUsize,
        delays_ms: BTreeMap<u64, u64>,
        panic_sequence: Option<u64>,
        wait_for_cancellation: Option<u64>,
    }

    struct CountingRunner(Arc<AtomicUsize>);

    impl TeamUnitRunner for CountingRunner {
        fn run(
            &self,
            _: &TeamBatchJob,
            _: CancellationToken,
            _: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Process)
        }
    }

    struct CancellationRunner(Arc<AtomicUsize>);

    impl TeamUnitRunner for CancellationRunner {
        fn run(
            &self,
            job: &TeamBatchJob,
            cancellation: CancellationToken,
            events: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            events.lifecycle(UnitLifecycleEvent::WorktreeCreated {
                worktree_id: format!("worktree-{}", job.sequence),
                branch: format!("codingmage/task-{}", job.sequence),
            })?;
            while !cancellation.is_cancelled() {
                thread::sleep(Duration::from_millis(2));
            }
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Process)
        }
    }

    struct CampaignCancellationRunner {
        barrier: Arc<Barrier>,
        finished: Arc<AtomicUsize>,
    }

    impl TeamUnitRunner for CampaignCancellationRunner {
        fn run(
            &self,
            job: &TeamBatchJob,
            cancellation: CancellationToken,
            events: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            events.lifecycle(UnitLifecycleEvent::WorktreeCreated {
                worktree_id: format!("worktree-{}", job.sequence),
                branch: format!("codingmage/task-{}", job.sequence),
            })?;
            self.barrier.wait();
            while !cancellation.is_cancelled() {
                thread::sleep(Duration::from_millis(2));
            }
            self.finished.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Process)
        }
    }

    struct CheckpointRunner;

    impl TeamUnitRunner for CheckpointRunner {
        fn run(
            &self,
            job: &TeamBatchJob,
            _: CancellationToken,
            events: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            emit_checkpoint(job, &events)
        }
    }

    struct WrongRunRunner;

    impl TeamUnitRunner for WrongRunRunner {
        fn run(
            &self,
            job: &TeamBatchJob,
            _: CancellationToken,
            events: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            let mut outcome = emit_success(job, &events)?;
            outcome.run_id = RunId::new("run-wrong-identity".to_owned()).expect("valid wrong run");
            Ok(outcome)
        }
    }

    impl FakeRunner {
        fn successful() -> Self {
            Self {
                barrier: None,
                active: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                delays_ms: BTreeMap::new(),
                panic_sequence: None,
                wait_for_cancellation: None,
            }
        }
    }

    impl TeamUnitRunner for FakeRunner {
        fn run(
            &self,
            job: &TeamBatchJob,
            cancellation: CancellationToken,
            events: TeamEventSink,
        ) -> Result<RunOutcome, RuntimeError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            self.peak.fetch_max(active, Ordering::SeqCst);
            let _guard = ActiveGuard(&self.active);
            if let Some(barrier) = &self.barrier {
                barrier.wait();
            }
            if self.wait_for_cancellation == Some(job.sequence) {
                while !cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(2));
                }
                return Err(RuntimeError::Process);
            }
            assert_ne!(
                self.panic_sequence,
                Some(job.sequence),
                "bounded fake worker panic"
            );
            if let Some(delay) = self.delays_ms.get(&job.sequence) {
                thread::sleep(Duration::from_millis(*delay));
            }
            emit_success(job, &events)
        }
    }

    fn emit_success(
        job: &TeamBatchJob,
        events: &TeamEventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        let branch = format!("codingmage/task-{}", job.sequence);
        let candidate = format!("{:040x}", job.sequence.saturating_add(1));
        let completion = format!("{:040x}", job.sequence.saturating_add(101));
        events.lifecycle(UnitLifecycleEvent::WorktreeCreated {
            worktree_id: format!("worktree-{}", job.sequence),
            branch: branch.clone(),
        })?;
        events.lifecycle(UnitLifecycleEvent::ImplementationSessionBound {
            session_id: format!("implementation-{}", job.sequence),
            correction_round: 0,
        })?;
        events.lifecycle(UnitLifecycleEvent::CandidateCommitted {
            commit: candidate.clone(),
            correction_round: 0,
        })?;
        events.lifecycle(UnitLifecycleEvent::GatesObserved {
            commit: candidate.clone(),
            evidence: vec![format!("{:064x}", job.sequence.saturating_add(201))],
            passed: true,
        })?;
        events.lifecycle(UnitLifecycleEvent::ReviewObserved {
            session_id: format!("review-{}", job.sequence),
            commit: candidate.clone(),
            verdict: "pass".to_owned(),
            evidence: format!("{:064x}", job.sequence.saturating_add(301)),
        })?;
        events.lifecycle(UnitLifecycleEvent::GatesObserved {
            commit: candidate.clone(),
            evidence: vec![format!("{:064x}", job.sequence.saturating_add(401))],
            passed: true,
        })?;
        events.lifecycle(UnitLifecycleEvent::CompletionCommitted {
            commit: completion.clone(),
        })?;
        events.lifecycle(UnitLifecycleEvent::WorktreeReleased {
            worktree_id: format!("worktree-{}", job.sequence),
        })?;
        events.heartbeat(TaskUtilization {
            provider_attempts: 2,
            provider_tokens: 100,
            process_invocations: 3,
            output_bytes: 50,
            retained_state_bytes: 25,
            execution_elapsed_ms: 10,
        })?;
        Ok(RunOutcome {
            run_id: job.run_id.clone(),
            task_id: TaskId::new(job.lease.task_id.clone()).expect("valid task"),
            state: TaskState::Complete,
            branch: Some(branch),
            candidate_commit: Some(candidate),
            completion_commit: Some(completion),
            review_verdict: Some("pass".to_owned()),
            correction_rounds: 0,
            utilization: RunUtilization::default(),
        })
    }

    fn emit_checkpoint(
        job: &TeamBatchJob,
        events: &TeamEventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        let branch = format!("codingmage/task-{}", job.sequence);
        let candidate = format!("{:040x}", job.sequence.saturating_add(1));
        events.lifecycle(UnitLifecycleEvent::WorktreeCreated {
            worktree_id: format!("worktree-{}", job.sequence),
            branch: branch.clone(),
        })?;
        events.lifecycle(UnitLifecycleEvent::ImplementationSessionBound {
            session_id: format!("implementation-{}", job.sequence),
            correction_round: 0,
        })?;
        events.lifecycle(UnitLifecycleEvent::CandidateCommitted {
            commit: candidate.clone(),
            correction_round: 0,
        })?;
        events.lifecycle(UnitLifecycleEvent::GatesObserved {
            commit: candidate.clone(),
            evidence: vec![format!("{:064x}", job.sequence.saturating_add(201))],
            passed: true,
        })?;
        events.lifecycle(UnitLifecycleEvent::ReviewObserved {
            session_id: format!("review-{}", job.sequence),
            commit: candidate.clone(),
            verdict: "pass".to_owned(),
            evidence: format!("{:064x}", job.sequence.saturating_add(301)),
        })?;
        events.lifecycle(UnitLifecycleEvent::WorktreeReleased {
            worktree_id: format!("worktree-{}", job.sequence),
        })?;
        Ok(RunOutcome {
            run_id: job.run_id.clone(),
            task_id: TaskId::new(job.lease.task_id.clone()).expect("valid task"),
            state: TaskState::Checkpointed,
            branch: Some(branch),
            candidate_commit: Some(candidate),
            completion_commit: None,
            review_verdict: Some("pass".to_owned()),
            correction_rounds: 0,
            utilization: RunUtilization::default(),
        })
    }

    fn fixture(
        capacity: u16,
        count: u16,
        timeout_ms: u64,
    ) -> (CampaignSpec, TeamCampaignSnapshot, Vec<TeamBatchJob>) {
        let mut spec = spec(capacity);
        let policy = &mut spec.multi_agent.as_mut().expect("team policy").resources;
        policy.pod_timeout_ms = timeout_ms;
        policy.heartbeat_interval_ms = 10;
        policy.stale_after_ms = timeout_ms.clamp(20, 100);
        if policy.stale_after_ms >= policy.pod_timeout_ms {
            policy.stale_after_ms = policy.pod_timeout_ms.saturating_sub(1);
        }
        let tasks = (0..count)
            .map(|index| format!("23.3.2.{}", index.saturating_add(1)))
            .collect::<Vec<_>>();
        let mut scheduler = DurablePodScheduler::new(&spec).expect("scheduler");
        let generation = scheduler.begin_generation(&tasks).expect("generation");
        let resources = TeamResourceController::new(&spec).expect("resources");
        let started_at = now_ms();
        let mut records = BTreeMap::new();
        let mut jobs = Vec::new();
        for (sequence, task_id) in tasks.into_iter().enumerate() {
            let proposal = proposal(&spec, &task_id, sequence);
            let AdmissionDecision::Admitted(lease) = scheduler
                .admit(&spec, generation, &spec.initial_commit, &proposal)
                .expect("admission")
            else {
                panic!("independent task must be admitted");
            };
            let mut record = CampaignTaskRecord::planned(
                spec.campaign_id.clone(),
                task_id.clone(),
                spec.initial_commit.clone(),
            )
            .expect("planned record");
            record
                .transition(&CampaignTaskTransition {
                    sequence: 0,
                    campaign_id: spec.campaign_id.clone(),
                    task_id: task_id.clone(),
                    generation: 0,
                    from: CampaignTaskState::Planned,
                    to: CampaignTaskState::Ready,
                    evidence_sha256: format!("{:064x}", sequence.saturating_add(1)),
                })
                .expect("ready");
            record
                .propose(generation, format!("{:064x}", sequence.saturating_add(101)))
                .expect("proposed");
            record
                .bind_lease(&lease, format!("{:064x}", sequence.saturating_add(201)))
                .expect("leased");
            let reservation = resources
                .implementation_request(&lease, started_at)
                .expect("reservation");
            records.insert(task_id, record);
            jobs.push(TeamBatchJob {
                sequence: u64::try_from(sequence).expect("sequence"),
                run_id: RunId::new(format!("run-team-{sequence}")).expect("valid run identity"),
                lease,
                reservation,
            });
        }
        let snapshot = TeamCampaignSnapshot {
            version: TEAM_STATE_SCHEMA_VERSION,
            campaign_id: spec.campaign_id.clone(),
            generation,
            campaign_head: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            scheduler: scheduler.snapshot().clone(),
            resources: resources.snapshot().clone(),
            tasks: records,
            integration_queue: Vec::new(),
        };
        snapshot.verify().expect("valid fixture");
        (spec, snapshot, jobs)
    }

    fn spec(capacity: u16) -> CampaignSpec {
        CampaignSpec {
            version: 3,
            campaign_id: "campaign-runtime-team".to_owned(),
            repository_id: "repo-runtime-team".to_owned(),
            repository_path: PathBuf::from("/tmp/repository"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
            max_parallel_pods: capacity,
            max_units: 100,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 20,
                process_invocations: 1_000,
                output_bytes: 1_000_000,
                retained_state_bytes: 1_000_000,
                execution_elapsed_ms: 1_000_000,
            },
            team_lead: provider("codex"),
            implementer: provider("claude"),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider("codex"),
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["workspace".to_owned()],
            }],
            campaign_branch: "codingmage/campaign-runtime-team".to_owned(),
            allowed_paths: vec![PathBuf::from("crates"), PathBuf::from("docs")],
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: CampaignExecutionMode::Parallel,
                publication_mode: TaskPublicationMode::LocalOnly,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
                task_merge_strategy: TaskMergeStrategy::Squash,
                github: None,
                concurrency: CampaignConcurrency {
                    claude_implementers: capacity,
                    codex_team_leads: 1,
                    codex_reviewers: capacity,
                    test_workers: capacity,
                    github_writers: 1,
                    integration_workers: 1,
                },
                resources: TeamResourcePolicy {
                    total_cpu_units: capacity.saturating_mul(2),
                    implementation_cpu_units: 1,
                    total_memory_bytes: u64::from(capacity) * 8 * 1024 * 1024,
                    implementation_memory_bytes: 4 * 1024 * 1024,
                    total_disk_bytes: u64::from(capacity) * 8 * 1024 * 1024,
                    implementation_disk_bytes: 4 * 1024 * 1024,
                    total_processes: u32::from(capacity) * 128,
                    implementation_processes: 64,
                    ..TeamResourcePolicy::default()
                },
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 10,
            }),
        }
    }

    fn provider(name: &str) -> CampaignProvider {
        CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "model".to_owned(),
            effort: "high".to_owned(),
        }
    }

    fn proposal(spec: &CampaignSpec, task_id: &str, sequence: usize) -> PodProposal {
        let path = PathBuf::from(format!("crates/unit-{sequence}"));
        PodProposal::seal(
            PodProposal {
                version: 3,
                task_id: task_id.to_owned(),
                task_source_sha256: spec.task_source_sha256.clone(),
                owned_paths: vec![path.clone()],
                dependencies: Vec::new(),
                gate_tiers: vec!["focused".to_owned()],
                test_resources: vec![format!("resource-{sequence}")],
                expected_artifacts: vec![path],
                risk: PodRisk::Routine,
                rationale_summary: "bounded fixture".to_owned(),
                proposal_sha256: "0".repeat(64),
            },
            spec,
        )
        .expect("sealed proposal")
    }

    #[test]
    fn five_workers_overlap_and_preserve_stable_result_order() {
        let (spec, mut snapshot, jobs) = fixture(5, 5, 2_000);
        let runner = Arc::new(FakeRunner {
            barrier: Some(Arc::new(Barrier::new(5))),
            ..FakeRunner::successful()
        });
        let persisted = Arc::new(Mutex::new(Vec::new()));
        let persisted_for_run = Arc::clone(&persisted);
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &CancellationToken::default(),
            move |state| {
                persisted_for_run.lock().expect("lock").push(state.clone());
                Ok(())
            },
            |_| {},
        )
        .expect("batch succeeds");
        assert_eq!(runner.peak.load(Ordering::SeqCst), 5);
        assert_eq!(
            outcome
                .tasks
                .iter()
                .map(|task| task.sequence)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert!(outcome.tasks.iter().all(|task| task.result.is_ok()));
        assert!(outcome.snapshot.resources.active.is_empty());
        assert_eq!(outcome.snapshot.scheduler.active.len(), 5);
        assert!(outcome.snapshot.tasks.values().all(|record| {
            record.state == CampaignTaskState::PublicationReady
                && record.implementation_session.is_some()
                && record.reviewed_commit.is_some()
                && record.completion_commit.is_some()
        }));
        assert!(persisted.lock().expect("lock").len() > 5);
    }

    #[test]
    fn one_panicking_worker_does_not_cancel_successful_siblings() {
        let (spec, mut snapshot, jobs) = fixture(5, 5, 2_000);
        let runner = Arc::new(FakeRunner {
            panic_sequence: Some(1),
            ..FakeRunner::successful()
        });
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &CancellationToken::default(),
            |_| Ok(()),
            |_| {},
        )
        .expect("batch remains coherent");
        assert!(outcome.tasks[0].result.is_ok());
        assert_eq!(outcome.tasks[1].result, Err(RuntimeError::Process));
        assert!(
            outcome
                .tasks
                .iter()
                .enumerate()
                .all(|(index, task)| index == 1 || task.result.is_ok())
        );
        let failed = &outcome.snapshot.tasks["23.3.2.2"];
        assert_eq!(failed.state, CampaignTaskState::Failed);
        assert_eq!(failed.terminal_reason.as_deref(), Some("process_crashed"));
        assert_eq!(outcome.snapshot.scheduler.active.len(), 4);
        assert!(outcome.snapshot.resources.active.is_empty());
    }

    #[test]
    fn deadline_cancels_only_the_expired_worker() {
        let (spec, mut snapshot, jobs) = fixture(2, 2, 120);
        let runner = Arc::new(FakeRunner {
            wait_for_cancellation: Some(0),
            ..FakeRunner::successful()
        });
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &CancellationToken::default(),
            |_| Ok(()),
            |_| {},
        )
        .expect("bounded timeout remains coherent");
        assert_eq!(outcome.tasks[0].result, Err(RuntimeError::Process));
        assert!(outcome.tasks[1].result.is_ok());
        assert_eq!(
            outcome.snapshot.tasks["23.3.2.1"]
                .terminal_reason
                .as_deref(),
            Some("timed_out")
        );
        assert_eq!(
            outcome.snapshot.tasks["23.3.2.2"].state,
            CampaignTaskState::PublicationReady
        );
    }

    #[test]
    fn campaign_cancellation_reconciles_all_five_active_pods() {
        let (spec, mut snapshot, jobs) = fixture(5, 5, 2_000);
        let barrier = Arc::new(Barrier::new(6));
        let finished = Arc::new(AtomicUsize::new(0));
        let runner = Arc::new(CampaignCancellationRunner {
            barrier: Arc::clone(&barrier),
            finished: Arc::clone(&finished),
        });
        let cancellation = CancellationToken::default();
        let cancellation_for_thread = cancellation.clone();
        let cancel_barrier = Arc::clone(&barrier);
        let canceller = thread::spawn(move || {
            cancel_barrier.wait();
            cancellation_for_thread.cancel();
        });
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &cancellation,
            |_| Ok(()),
            |_| {},
        )
        .expect("campaign cancellation remains coherent");
        canceller.join().unwrap();
        assert_eq!(finished.load(Ordering::SeqCst), 5);
        assert!(outcome.tasks.iter().all(|task| task.result.is_err()));
        assert!(outcome.snapshot.tasks.values().all(|record| {
            record.state == CampaignTaskState::Cancelled
                && record.terminal_reason.as_deref() == Some("operator_cancelled")
        }));
        assert!(outcome.snapshot.resources.active.is_empty());
        assert!(outcome.snapshot.scheduler.active.is_empty());
    }

    #[test]
    fn completion_permutations_do_not_change_result_order() {
        let (spec, mut snapshot, jobs) = fixture(3, 3, 2_000);
        let runner = Arc::new(FakeRunner {
            delays_ms: BTreeMap::from([(0, 60), (1, 30), (2, 0)]),
            ..FakeRunner::successful()
        });
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &CancellationToken::default(),
            |_| Ok(()),
            |_| {},
        )
        .expect("permuted batch");
        assert_eq!(
            outcome.completion_order,
            vec!["23.3.2.3", "23.3.2.2", "23.3.2.1"]
        );
        assert_eq!(
            outcome
                .tasks
                .iter()
                .map(|task| task.sequence)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn accelerated_parallel_soak_covers_every_capacity_and_completion_rotation() {
        for capacity in 1..=5 {
            for cycle in 0_u64..10 {
                let (spec, mut snapshot, jobs) = fixture(capacity, capacity, 2_000);
                let delays_ms = jobs
                    .iter()
                    .map(|job| {
                        let rank = (job.sequence.saturating_add(cycle)) % u64::from(capacity);
                        (job.sequence, rank.saturating_mul(2))
                    })
                    .collect();
                let runner = Arc::new(FakeRunner {
                    delays_ms,
                    ..FakeRunner::successful()
                });
                let outcome = execute_team_batch(
                    &spec,
                    &mut snapshot,
                    &jobs,
                    &runner,
                    &CancellationToken::default(),
                    |_| Ok(()),
                    |_| {},
                )
                .expect("accelerated soak cycle");
                assert_eq!(outcome.tasks.len(), usize::from(capacity));
                assert!(outcome.tasks.iter().all(|task| task.result.is_ok()));
                assert_eq!(
                    outcome
                        .tasks
                        .iter()
                        .map(|task| task.sequence)
                        .collect::<Vec<_>>(),
                    (0..u64::from(capacity)).collect::<Vec<_>>()
                );
                assert_eq!(outcome.completion_order.len(), usize::from(capacity));
                assert_eq!(
                    outcome
                        .completion_order
                        .iter()
                        .collect::<BTreeSet<_>>()
                        .len(),
                    usize::from(capacity)
                );
                assert!(outcome.snapshot.resources.active.is_empty());
                assert_eq!(
                    outcome.snapshot.scheduler.active.len(),
                    usize::from(capacity)
                );
                outcome.snapshot.verify().expect("verified soak snapshot");
            }
        }
    }

    #[test]
    fn conflicting_reservations_fail_before_persistence_or_execution() {
        let (spec, mut snapshot, mut jobs) = fixture(2, 2, 2_000);
        jobs[1].reservation.reservation_id = jobs[0].reservation.reservation_id.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let runner = Arc::new(CountingRunner(Arc::clone(&calls)));
        let persists = AtomicUsize::new(0);
        assert_eq!(
            execute_team_batch(
                &spec,
                &mut snapshot,
                &jobs,
                &runner,
                &CancellationToken::default(),
                |_| {
                    persists.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
                |_| {},
            ),
            Err(RuntimeError::State)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(persists.load(Ordering::SeqCst), 0);
        assert!(snapshot.resources.active.is_empty());
    }

    #[test]
    fn persistence_failure_cancels_and_joins_owned_workers() {
        let (spec, mut snapshot, jobs) = fixture(1, 1, 2_000);
        let finished = Arc::new(AtomicUsize::new(0));
        let runner = Arc::new(CancellationRunner(Arc::clone(&finished)));
        let persists = AtomicUsize::new(0);
        assert_eq!(
            execute_team_batch(
                &spec,
                &mut snapshot,
                &jobs,
                &runner,
                &CancellationToken::default(),
                |_| {
                    if persists.fetch_add(1, Ordering::SeqCst) == 0 {
                        Ok(())
                    } else {
                        Err(RuntimeError::State)
                    }
                },
                |_| {},
            ),
            Err(RuntimeError::State)
        );
        assert_eq!(finished.load(Ordering::SeqCst), 1);
        assert_eq!(persists.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn candidate_only_checkpoint_becomes_publication_ready_without_task_completion() {
        let (spec, mut snapshot, jobs) = fixture(1, 1, 2_000);
        let runner = Arc::new(CheckpointRunner);
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &CancellationToken::default(),
            |_| Ok(()),
            |_| {},
        )
        .expect("candidate checkpoint");
        let record = &outcome.snapshot.tasks["23.3.2.1"];
        assert_eq!(record.state, CampaignTaskState::PublicationReady);
        assert!(record.reviewed_commit.is_some());
        assert!(record.completion_commit.is_none());
        assert!(
            outcome
                .snapshot
                .scheduler
                .active
                .contains_key(record.lease_id.as_ref().expect("active lease identity"))
        );
    }

    #[test]
    fn cross_run_terminal_outcome_fails_closed() {
        let (spec, mut snapshot, jobs) = fixture(1, 1, 2_000);
        let runner = Arc::new(WrongRunRunner);
        assert_eq!(
            execute_team_batch(
                &spec,
                &mut snapshot,
                &jobs,
                &runner,
                &CancellationToken::default(),
                |_| Ok(()),
                |_| {},
            ),
            Err(RuntimeError::State)
        );
    }

    fn correction_execution() -> CiCorrectionExecution {
        let candidate = "d".repeat(40);
        CiCorrectionExecution {
            version: CI_CORRECTION_RESULT_VERSION,
            campaign_id: "campaign-runtime-team".to_owned(),
            task_id: "23.3.2.1".to_owned(),
            run_id: "ci-correction-fixture".to_owned(),
            prior_commit: "a".repeat(40),
            ci_evidence_sha256: "b".repeat(64),
            candidate_commit: candidate.clone(),
            utilization: TaskUtilization::default(),
            events: vec![
                UnitLifecycleEvent::ImplementationSessionBound {
                    session_id: "implementation-correction".to_owned(),
                    correction_round: 0,
                },
                UnitLifecycleEvent::CandidateCommitted {
                    commit: candidate.clone(),
                    correction_round: 0,
                },
                UnitLifecycleEvent::GatesObserved {
                    commit: candidate.clone(),
                    evidence: vec!["e".repeat(64)],
                    passed: true,
                },
                UnitLifecycleEvent::ReviewObserved {
                    session_id: "review-correction".to_owned(),
                    commit: candidate,
                    verdict: "pass".to_owned(),
                    evidence: "f".repeat(64),
                },
            ],
        }
    }

    fn correction_execution_is_valid(execution: &CiCorrectionExecution) -> bool {
        execution.verify(
            "campaign-runtime-team",
            "23.3.2.1",
            "ci-correction-fixture",
            &"a".repeat(40),
            &"b".repeat(64),
        )
    }

    #[test]
    fn ci_correction_requires_complete_exact_fresh_evidence() {
        let execution = correction_execution();
        assert!(correction_execution_is_valid(&execution));
        let evidence = execution.lifecycle_evidence().expect("exact evidence");
        assert_eq!(
            evidence.implementation_sessions,
            vec!["implementation-correction"]
        );
        assert_eq!(evidence.gate_evidence, vec!["e".repeat(64)]);
        assert_eq!(evidence.review_sessions, vec!["review-correction"]);
        assert_eq!(evidence.review_evidence, vec!["f".repeat(64)]);

        for event_index in 0..execution.events.len() {
            let mut missing = execution.clone();
            missing.events.remove(event_index);
            assert!(!correction_execution_is_valid(&missing));
        }
    }

    #[test]
    fn ci_correction_rejects_failed_mismatched_or_malformed_evidence() {
        let mut failed_gate = correction_execution();
        let UnitLifecycleEvent::GatesObserved { passed, .. } = &mut failed_gate.events[2] else {
            panic!("gate event");
        };
        *passed = false;
        assert!(!correction_execution_is_valid(&failed_gate));

        let mut mismatched_review = correction_execution();
        let UnitLifecycleEvent::ReviewObserved { commit, .. } = &mut mismatched_review.events[3]
        else {
            panic!("review event");
        };
        *commit = "c".repeat(40);
        assert!(!correction_execution_is_valid(&mismatched_review));

        let mut malformed_digest = correction_execution();
        let UnitLifecycleEvent::GatesObserved { evidence, .. } = &mut malformed_digest.events[2]
        else {
            panic!("gate event");
        };
        evidence[0] = "not-a-digest".to_owned();
        assert!(!correction_execution_is_valid(&malformed_digest));

        let mut duplicate_session = correction_execution();
        duplicate_session
            .events
            .push(duplicate_session.events[0].clone());
        assert!(!correction_execution_is_valid(&duplicate_session));
    }

    #[test]
    fn ci_correction_reservation_is_reused_and_stale_ownership_is_reconciled() {
        let (spec, mut snapshot, jobs) = fixture(1, 1, 2_000);
        let lease = &jobs[0].lease;
        let persists = Cell::<usize>::new(0);
        let mut persist = |_: &TeamCampaignSnapshot| {
            persists.set(persists.get().saturating_add(1));
            Ok(())
        };

        let (_, first) =
            ensure_ci_correction_reservation(&spec, &mut snapshot, lease, 100, &mut persist)
                .expect("first reservation");
        assert_eq!(persists.get(), 1);
        let (_, recovered) =
            ensure_ci_correction_reservation(&spec, &mut snapshot, lease, 200, &mut persist)
                .expect("recover existing reservation");
        assert_eq!(recovered.reservation_id, first.reservation_id);
        assert_eq!(persists.get(), 1);

        let (_, replacement) =
            ensure_ci_correction_reservation(&spec, &mut snapshot, lease, 5_000, &mut persist)
                .expect("replace stale reservation");
        assert_ne!(replacement.reservation_id, first.reservation_id);
        assert!(snapshot.resources.released.contains(&first.reservation_id));
        assert_eq!(snapshot.resources.active.len(), 1);
        assert_eq!(persists.get(), 3);
    }

    #[test]
    fn actor_permits_enforce_capacity_and_release_exactly() {
        let pool = ActorPermitPool::new(1, 1).unwrap();
        let cancellation = CancellationToken::default();
        let first = pool.acquire(PermitClass::Gate, &cancellation).unwrap();
        let waiting_pool = pool.clone();
        let waiting_cancellation = cancellation.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let acquired = waiting_pool
                .acquire(PermitClass::Gate, &waiting_cancellation)
                .is_ok();
            sender.send(acquired).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
        drop(first);
        assert!(receiver.recv_timeout(Duration::from_secs(1)).unwrap());
        handle.join().unwrap();
    }

    #[test]
    fn actor_permit_wait_observes_task_cancellation() {
        let pool = ActorPermitPool::new(1, 1).unwrap();
        let cancellation = CancellationToken::default();
        let _first = pool.acquire(PermitClass::Reviewer, &cancellation).unwrap();
        let waiting_pool = pool.clone();
        let waiting_cancellation = cancellation.clone();
        let handle = thread::spawn(move || {
            waiting_pool.acquire(PermitClass::Reviewer, &waiting_cancellation)
        });
        thread::sleep(Duration::from_millis(30));
        cancellation.cancel();
        assert!(handle.join().unwrap().is_err());
    }

    #[test]
    fn active_worker_liveness_advances_durable_heartbeats() {
        let (spec, mut snapshot, jobs) = fixture(1, 1, 500);
        let mut runner = FakeRunner::successful();
        runner.delays_ms.insert(0, 45);
        let outcome = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &Arc::new(runner),
            &CancellationToken::default(),
            |_| Ok(()),
            |_| {},
        )
        .unwrap();
        assert!(outcome.snapshot.tasks["23.3.2.1"].heartbeat_sequence > 9);
    }
}
