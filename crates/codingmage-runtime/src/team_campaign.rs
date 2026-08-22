//! Durable production loop for parallel multi-agent campaigns.

use std::{collections::BTreeMap, fs, path::Path, sync::Arc, time::SystemTime};

use codingmage_campaign::{
    CampaignExecutionMode, CampaignSpec, CampaignTaskState, TaskIntegrationPolicy,
    TaskPublicationMode, TeamLeadOutcome,
};
use codingmage_codex::{CodexLeadAdapter, team_lead_schema};
use codingmage_contracts::{RunId, TaskId, WorktreeId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_git::{OwnedWorktree, create_owned_worktree, inventory_repository};
use codingmage_plan::TaskPlan;
use codingmage_process::{CancellationToken, ProcessExecutor};
use codingmage_service::CoordinatorLock;
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};

use crate::{
    CampaignOutcome, CampaignState, CampaignStopReason, ProductionTeamIntegrationVerifier,
    ProductionTeamUnitRunner, ProgressActor, ProgressStage, RunProgress, RuntimeError,
    TeamIntegrationVerifier, TeamPlanningOutcome, TeamPublicationOutcome, TeamStateStore,
    admit_team_lead_report, build_team_lead_binding, enqueue_team_integration, execute_team_batch,
    generated_run_id, initialize_team_campaign, integrate_team_queue_head_with_strategy,
    login_discovery_environment, private_directory, refresh_team_readiness,
    synchronize_campaign_branch, synchronize_task_completion, synchronize_task_publication,
    team_control::{
        TeamCancellationWatcher, observe_team_control, observe_team_integration_approval,
    },
    write_private_idempotent,
};

pub(crate) const MANIFEST_NAME: &str = "team-campaign-manifest.json";
const MANIFEST_VERSION: u16 = 1;
const REPORT_NAME: &str = "team-campaign-report.json";
const REPORT_VERSION: u16 = 1;

/// Immutable completion identity for one task in a final team-campaign report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamTaskCompletionReport {
    /// Exact candidate accepted by independent task review.
    pub reviewed_commit: String,
    /// Coordinator-created commit accepted by final integration verification.
    pub integration_commit: String,
    /// Mechanical canonical-plan completion commit.
    pub completion_commit: String,
}

/// Content-minimized final report produced only after full gates and cumulative review pass.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamCampaignReport {
    /// Closed report schema version.
    pub version: u16,
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact repository identity.
    pub repository_id: String,
    /// Coordinator-owned campaign branch.
    pub branch: String,
    /// Immutable campaign base commit.
    pub initial_commit: String,
    /// Fully verified final campaign commit.
    pub final_commit: String,
    /// Canonical completed task-source digest.
    pub task_source_sha256: String,
    /// Exact task completion identities in canonical task order.
    pub tasks: BTreeMap<String, TeamTaskCompletionReport>,
    /// Full-suite deterministic gate evidence.
    pub final_gate_evidence_sha256: String,
    /// Fresh cumulative Codex review evidence.
    pub final_review_evidence_sha256: String,
    /// Trusted local completion timestamp.
    pub completed_at_ms: u64,
}

impl TeamCampaignReport {
    fn verify(
        &self,
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    ) -> bool {
        self.version == REPORT_VERSION
            && self.campaign_id == spec.campaign_id
            && self.repository_id == spec.repository_id
            && snapshot.campaign_id == spec.campaign_id
            && self.branch == manifest.branch
            && self.initial_commit == spec.initial_commit
            && self.final_commit == snapshot.campaign_head
            && self.task_source_sha256 == snapshot.task_source_sha256
            && self.tasks.len() == snapshot.tasks.len()
            && valid_sha256(&self.final_gate_evidence_sha256)
            && valid_sha256(&self.final_review_evidence_sha256)
            && self.completed_at_ms > 0
            && snapshot.tasks.iter().all(|(task_id, record)| {
                record.state == CampaignTaskState::Merged
                    && self.tasks.get(task_id).is_some_and(|report| {
                        record.reviewed_commit.as_ref() == Some(&report.reviewed_commit)
                            && record.integration_commit.as_ref()
                                == Some(&report.integration_commit)
                            && record.completion_commit.as_ref() == Some(&report.completion_commit)
                    })
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamCampaignManifest {
    pub(crate) version: u16,
    pub(crate) campaign_id: String,
    pub(crate) repository_id: String,
    pub(crate) authority_sha256: String,
    pub(crate) initial_commit: String,
    pub(crate) campaign_run_id: String,
    pub(crate) worktree_id: String,
    pub(crate) branch: String,
}

impl TeamCampaignManifest {
    pub(crate) fn verify(&self, spec: &CampaignSpec, authority_sha256: &str) -> bool {
        self.version == MANIFEST_VERSION
            && self.campaign_id == spec.campaign_id
            && self.repository_id == spec.repository_id
            && self.authority_sha256 == authority_sha256
            && self.initial_commit == spec.initial_commit
            && RunId::new(self.campaign_run_id.clone()).is_ok()
            && WorktreeId::new(self.worktree_id.clone()).is_ok()
            && !self.branch.is_empty()
            && self.branch.starts_with(&spec.campaign_branch)
    }
}

/// Runs or resumes one parallel campaign until completion or a truthful terminal blocker.
///
/// Serial policy continues through the existing serial engine. Parallel policy uses one durable
/// scheduler for the campaign, admits only deterministically validated lead proposals, runs each
/// bounded batch concurrently, and serializes every accepted integration.
///
/// # Errors
///
/// Returns [`RuntimeError`] for stale authority, malformed durable state, provider failure, or an
/// uncertain local effect. The campaign worktree and state are retained for exact resumption.
#[allow(clippy::too_many_lines)]
pub fn run_team_campaign_with_progress(
    config: &Config,
    spec: CampaignSpec,
    codingmage_binary: &Path,
    mut observer: impl FnMut(RunProgress),
) -> Result<CampaignOutcome, RuntimeError> {
    if spec
        .multi_agent
        .as_ref()
        .is_none_or(|policy| policy.execution_mode == CampaignExecutionMode::Serial)
    {
        return crate::run_serial_campaign_with_progress(config, spec, codingmage_binary, observer);
    }
    observer(RunProgress::new(
        ProgressActor::Coordinator,
        ProgressStage::Preparing,
    ));
    spec.verify().map_err(RuntimeError::Campaign)?;
    let policy = spec.multi_agent.clone().ok_or(RuntimeError::Authority)?;
    let binary = fs::canonicalize(codingmage_binary).map_err(|_| RuntimeError::Authority)?;
    let source_root = binary.parent().ok_or(RuntimeError::Authority)?;
    let authorization = RepositoryAuthorization::authorize(config, source_root)
        .map_err(|_| RuntimeError::Authority)?;
    let inventory = inventory_repository(&authorization).map_err(|_| RuntimeError::Repository)?;
    if !inventory.condition.is_clean()
        || inventory.head != spec.initial_commit
        || authorization.identity().repository_id.as_str() != spec.repository_id
        || fs::canonicalize(&config.target_path).map_err(|_| RuntimeError::Authority)?
            != spec.repository_path
    {
        return Err(RuntimeError::Authority);
    }
    let source =
        fs::read(config.target_path.join(&config.task_source)).map_err(|_| RuntimeError::Plan)?;
    let initial_plan = TaskPlan::parse(&source).map_err(|_| RuntimeError::Plan)?;
    if initial_plan.source_sha256 != spec.task_source_sha256 {
        return Err(RuntimeError::Plan);
    }
    let authority_sha256 = spec.authority_sha256().map_err(RuntimeError::Campaign)?;
    let invocation_id = generated_run_id()?;
    let _campaign_lock = CoordinatorLock::acquire(
        &config.state_root.join("team-campaign-locks"),
        &authorization.identity().repository_id,
        invocation_id.as_str(),
    )
    .map_err(|_| RuntimeError::Orchestration)?;
    let campaign_root = config
        .state_root
        .join("team-campaigns")
        .join(&spec.campaign_id);
    private_directory(&campaign_root)?;
    let mut campaign_config = config.clone();
    campaign_config
        .integration_branch
        .clone_from(&spec.campaign_branch);
    let (campaign, manifest, mut snapshot) = load_or_initialize(
        &campaign_config,
        &spec,
        &authorization,
        &initial_plan,
        &campaign_root,
        &authority_sha256,
    )?;
    let state_root = campaign_root.join("state");
    private_directory(&state_root)?;
    let mut state_store = TeamStateStore::open(
        &state_root,
        &spec.campaign_id,
        authorization.identity().repository_id.clone(),
        RunId::new(manifest.campaign_run_id.clone()).map_err(|_| RuntimeError::State)?,
    )
    .map_err(|_| RuntimeError::State)?;
    if let Some(durable) = state_store
        .load_reconciled(now_ms())
        .map_err(|_| RuntimeError::State)?
    {
        snapshot = durable;
    } else {
        state_store
            .persist(&snapshot, now_ms())
            .map_err(|_| RuntimeError::State)?;
    }
    if campaign.manifest().branch != manifest.branch
        || campaign.observe_head(&authorization).is_err()
    {
        return Err(RuntimeError::Authority);
    }

    let process_root = campaign_root.join("lead-processes");
    let lead_executor = ProcessExecutor::new_with_guard_arguments(
        &binary,
        vec!["__process-guard".to_owned()],
        &process_root,
    )
    .map_err(|_| RuntimeError::Process)?;
    let lead_schema_path = campaign_root.join("team-lead.schema.json");
    write_private_idempotent(&lead_schema_path, team_lead_schema().as_bytes())?;
    let login_environment = login_discovery_environment()?;
    let lead = CodexLeadAdapter::new(
        spec.team_lead.executable.clone(),
        &spec.team_lead.model,
        &spec.team_lead.effort,
        lead_schema_path,
    )
    .and_then(|adapter| adapter.with_login_environment(login_environment))
    .map_err(RuntimeError::Reviewer)?;
    let cancellation = CancellationToken::default();
    let _cancellation_watcher = TeamCancellationWatcher::start(
        &campaign_root,
        &spec,
        &manifest,
        &authority_sha256,
        cancellation.child(),
    );
    let runner = Arc::new(ProductionTeamUnitRunner::new(
        config.clone(),
        spec.clone(),
        campaign.manifest().path.clone(),
        binary.clone(),
        campaign_root.join("execution"),
    )?);
    let mut integration_verifier = ProductionTeamIntegrationVerifier::new(
        config.clone(),
        &binary,
        &campaign_root.join("integration-verification"),
        crate::provider_spec(&spec.reviewer),
        cancellation.child(),
    )?;
    let mut integrated_this_invocation = 0_u32;
    let mut last_task_id = None;
    let mut publication_port = None;

    loop {
        let control = observe_team_control(&campaign_root, &spec, &manifest, &authority_sha256)?;
        if control.cancelled {
            cancellation.cancel();
            return Ok(controlled_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                CampaignState::Cancelled,
                CampaignStopReason::OperatorCancellation,
                "codingmage.team.cancelled",
            ));
        }
        if control.paused || control.stop_after_unit {
            let (reason, code) = if control.paused {
                (
                    CampaignStopReason::OperatorPause,
                    "codingmage.team.operator_paused",
                )
            } else {
                (
                    CampaignStopReason::StopAfterUnit,
                    "codingmage.team.stop_after_unit",
                )
            };
            return Ok(controlled_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                CampaignState::Paused,
                reason,
                code,
            ));
        }
        let plan_source = fs::read(campaign.manifest().path.join(&config.task_source))
            .map_err(|_| RuntimeError::Plan)?;
        let plan = TaskPlan::parse(&plan_source).map_err(|_| RuntimeError::Plan)?;
        if plan.source_sha256 != snapshot.task_source_sha256 {
            return Err(RuntimeError::Plan);
        }
        refresh_team_readiness(&plan, &mut snapshot)?;
        persist(&mut state_store, &snapshot)?;

        if policy.publication_mode != TaskPublicationMode::LocalOnly {
            if publication_port.is_none() {
                publication_port = match crate::GhCliPublicationPort::new(
                    config,
                    &spec,
                    &binary,
                    &campaign_root.join("publication-processes"),
                    cancellation.child(),
                ) {
                    Ok(port) => Some(port),
                    Err(error) => {
                        return Ok(blocked_outcome(
                            &spec,
                            &campaign,
                            &snapshot,
                            integrated_this_invocation,
                            last_task_id,
                            error.code(),
                        ));
                    }
                };
            }
            let Some(port) = publication_port.as_mut() else {
                return Err(RuntimeError::State);
            };
            match synchronize_campaign_branch(&spec, &manifest.branch, &snapshot, port) {
                Ok(_) => {}
                Err(error) => {
                    return Ok(blocked_outcome(
                        &spec,
                        &campaign,
                        &snapshot,
                        integrated_this_invocation,
                        last_task_id,
                        error.code(),
                    ));
                }
            };
            if policy.publication_mode == TaskPublicationMode::PerTaskDraftPullRequest {
                for (task_id, record) in &snapshot.tasks {
                    if record.state != CampaignTaskState::Merged
                        || record.issue_number.is_none()
                        || record.pull_request_number.is_none()
                    {
                        continue;
                    }
                    match synchronize_task_completion(
                        &spec,
                        &manifest.branch,
                        &initial_plan,
                        &snapshot,
                        task_id,
                        port,
                    ) {
                        Ok(_) => {}
                        Err(error) => {
                            return Ok(blocked_outcome(
                                &spec,
                                &campaign,
                                &snapshot,
                                integrated_this_invocation,
                                last_task_id,
                                error.code(),
                            ));
                        }
                    }
                }
            }
        }

        let publication_candidates = snapshot
            .tasks
            .iter()
            .filter_map(|(task_id, record)| {
                matches!(
                    record.state,
                    CampaignTaskState::PublicationReady
                        | CampaignTaskState::PullRequestOpen
                        | CampaignTaskState::CiWaiting
                )
                .then_some(task_id.clone())
            })
            .collect::<Vec<_>>();
        let mut waiting_for_ci = false;
        let mut correction_required = false;
        match policy.publication_mode {
            TaskPublicationMode::LocalOnly => {
                for task_id in publication_candidates {
                    let approved = task_integration_approved(
                        &campaign_root,
                        &spec,
                        &manifest,
                        &authority_sha256,
                        &snapshot,
                        &task_id,
                        policy.task_integration_policy,
                    )?;
                    if let Some(blocker_code) =
                        task_integration_blocker(policy.task_integration_policy, approved)
                    {
                        return Ok(blocked_outcome(
                            &spec,
                            &campaign,
                            &snapshot,
                            integrated_this_invocation,
                            last_task_id,
                            blocker_code,
                        ));
                    }
                    enqueue_team_integration(&mut snapshot, &task_id, |value| {
                        persist(&mut state_store, value)
                    })?;
                }
            }
            TaskPublicationMode::PerTaskDraftPullRequest => {
                for task_id in publication_candidates {
                    let Some(port) = publication_port.as_mut() else {
                        return Err(RuntimeError::State);
                    };
                    let outcome = match synchronize_task_publication(
                        &spec,
                        &manifest.branch,
                        &plan,
                        &mut snapshot,
                        &task_id,
                        port,
                        |value| persist(&mut state_store, value),
                    ) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            return Ok(blocked_outcome(
                                &spec,
                                &campaign,
                                &snapshot,
                                integrated_this_invocation,
                                last_task_id,
                                error.code(),
                            ));
                        }
                    };
                    match outcome {
                        TeamPublicationOutcome::WaitingForCi => waiting_for_ci = true,
                        TeamPublicationOutcome::CorrectionRequired => correction_required = true,
                        TeamPublicationOutcome::ReadyForIntegration => {
                            let approved = task_integration_approved(
                                &campaign_root,
                                &spec,
                                &manifest,
                                &authority_sha256,
                                &snapshot,
                                &task_id,
                                policy.task_integration_policy,
                            )?;
                            if let Some(blocker_code) =
                                task_integration_blocker(policy.task_integration_policy, approved)
                            {
                                return Ok(blocked_outcome(
                                    &spec,
                                    &campaign,
                                    &snapshot,
                                    integrated_this_invocation,
                                    last_task_id,
                                    blocker_code,
                                ));
                            }
                            enqueue_team_integration(&mut snapshot, &task_id, |value| {
                                persist(&mut state_store, value)
                            })?;
                        }
                    }
                }
            }
            TaskPublicationMode::CampaignDraftPullRequest => {
                for task_id in publication_candidates {
                    let approved = task_integration_approved(
                        &campaign_root,
                        &spec,
                        &manifest,
                        &authority_sha256,
                        &snapshot,
                        &task_id,
                        policy.task_integration_policy,
                    )?;
                    if let Some(blocker_code) =
                        task_integration_blocker(policy.task_integration_policy, approved)
                    {
                        return Ok(blocked_outcome(
                            &spec,
                            &campaign,
                            &snapshot,
                            integrated_this_invocation,
                            last_task_id,
                            blocker_code,
                        ));
                    }
                    enqueue_team_integration(&mut snapshot, &task_id, |value| {
                        persist(&mut state_store, value)
                    })?;
                }
            }
        }
        if !snapshot.integration_queue.is_empty() {
            observer(RunProgress::new(
                ProgressActor::IntegrationLead,
                ProgressStage::Integrating,
            ));
            let outcome = integrate_team_queue_head_with_strategy(
                config,
                &authorization,
                &campaign,
                &mut snapshot,
                &mut integration_verifier,
                policy.task_merge_strategy,
                |value| persist(&mut state_store, value),
            )?;
            integrated_this_invocation = integrated_this_invocation.saturating_add(1);
            last_task_id = Some(outcome.task_id);
            continue;
        }
        if snapshot
            .tasks
            .values()
            .all(|record| record.state == CampaignTaskState::Merged)
        {
            observer(RunProgress::new(
                ProgressActor::LocalGates,
                ProgressStage::VerifyingFinal,
            ));
            let _ = finalize_team_campaign(
                &campaign_root,
                &spec,
                &manifest,
                &snapshot,
                &authorization,
                &campaign,
                &mut integration_verifier,
            )?;
            observer(RunProgress::new(
                ProgressActor::Coordinator,
                ProgressStage::Finished,
            ));
            return Ok(CampaignOutcome {
                campaign_id: spec.campaign_id,
                state: CampaignState::Complete,
                branch: campaign.manifest().branch.clone(),
                head: snapshot.campaign_head,
                completed_units: integrated_this_invocation,
                stop_reason: CampaignStopReason::Completion,
                last_task_id,
                blocker_code: None,
            });
        }
        if snapshot.tasks.values().any(|record| {
            matches!(
                record.state,
                CampaignTaskState::Blocked
                    | CampaignTaskState::Disputed
                    | CampaignTaskState::Failed
                    | CampaignTaskState::Cancelled
            )
        }) && !snapshot
            .tasks
            .values()
            .any(|record| record.state == CampaignTaskState::Ready)
        {
            return Ok(blocked_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                "codingmage.team.no_dependency_ready_work",
            ));
        }
        if !snapshot
            .tasks
            .values()
            .any(|record| record.state == CampaignTaskState::Ready)
        {
            let blocker_code = if correction_required {
                Some("codingmage.team.ci_correction_required")
            } else if waiting_for_ci {
                Some("codingmage.team.ci_pending")
            } else {
                None
            };
            if let Some(blocker_code) = blocker_code {
                return Ok(blocked_outcome(
                    &spec,
                    &campaign,
                    &snapshot,
                    integrated_this_invocation,
                    last_task_id,
                    blocker_code,
                ));
            }
        }
        observer(RunProgress::new(
            ProgressActor::CampaignLead,
            ProgressStage::PlanningCampaign,
        ));
        let Ok(binding) =
            build_team_lead_binding(&spec, &plan, &snapshot, &campaign.manifest().path)
        else {
            return Ok(blocked_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                "codingmage.team.no_dependency_ready_work",
            ));
        };
        let lead_plan = lead.plan(&binding).map_err(RuntimeError::Reviewer)?;
        let (lead_result, _) = lead
            .execute(&lead_executor, &lead_plan, &binding, &cancellation)
            .map_err(RuntimeError::Reviewer)?;
        let planning = admit_team_lead_report(
            &spec,
            &plan,
            &mut snapshot,
            lead_result.report,
            now_ms(),
            |value| persist(&mut state_store, value),
        )?;
        let TeamPlanningOutcome::Admitted(jobs) = planning else {
            let TeamPlanningOutcome::NoExecution(disposition) = planning else {
                unreachable!();
            };
            let code = match disposition {
                TeamLeadOutcome::Blocked(_) => "codingmage.team.lead_blocked",
                TeamLeadOutcome::Deferred(_) => "codingmage.team.lead_deferred",
                TeamLeadOutcome::HumanDecision(_) => "codingmage.team.human_decision_required",
                TeamLeadOutcome::Proposals(_) => return Err(RuntimeError::State),
            };
            return Ok(blocked_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                code,
            ));
        };
        if jobs.is_empty() {
            return Ok(blocked_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                "codingmage.team.all_proposals_deferred",
            ));
        }
        let batch = execute_team_batch(
            &spec,
            &mut snapshot,
            &jobs,
            &runner,
            &cancellation,
            |value| persist(&mut state_store, value),
            |observation| {
                if let crate::TeamBatchObservation::TaskProgress { progress, .. } = observation {
                    observer(progress);
                }
            },
        )?;
        snapshot = batch.snapshot;
    }
}

/// Loads an integrity-checked final report for a completed parallel campaign.
///
/// # Errors
///
/// Returns [`RuntimeError`] for serial policy, stale authority, repository mismatch, malformed
/// state, or report integrity failure.
pub fn team_campaign_report(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
) -> Result<Option<TeamCampaignReport>, RuntimeError> {
    if spec
        .multi_agent
        .as_ref()
        .is_none_or(|policy| policy.execution_mode != CampaignExecutionMode::Parallel)
    {
        return Err(RuntimeError::Spec);
    }
    spec.verify().map_err(RuntimeError::Campaign)?;
    let binary = fs::canonicalize(codingmage_binary).map_err(|_| RuntimeError::Authority)?;
    let source_root = binary.parent().ok_or(RuntimeError::Authority)?;
    let authorization = RepositoryAuthorization::authorize(config, source_root)
        .map_err(|_| RuntimeError::Authority)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id
        || fs::canonicalize(&config.target_path).map_err(|_| RuntimeError::Authority)?
            != spec.repository_path
    {
        return Err(RuntimeError::Authority);
    }
    let authority_sha256 = spec.authority_sha256().map_err(RuntimeError::Campaign)?;
    let campaign_root = config
        .state_root
        .join("team-campaigns")
        .join(&spec.campaign_id);
    if fs::symlink_metadata(campaign_root.join(MANIFEST_NAME)).is_err() {
        return Ok(None);
    }
    let manifest =
        IntegrityDocument::<TeamCampaignManifest>::load(&campaign_root, MANIFEST_NAME, |value| {
            value.verify(spec, &authority_sha256)
        })
        .map_err(|_| RuntimeError::State)?
        .payload;
    let snapshot = IntegrityDocument::<codingmage_campaign::TeamCampaignSnapshot>::load(
        &campaign_root.join("state"),
        "team-campaign.json",
        |value| value.verify().is_ok(),
    )
    .map_err(|_| RuntimeError::State)?
    .payload;
    if fs::symlink_metadata(campaign_root.join(REPORT_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<TeamCampaignReport>::load(&campaign_root, REPORT_NAME, |value| {
        value.verify(spec, &manifest, &snapshot)
    })
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

fn finalize_team_campaign(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    verifier: &mut ProductionTeamIntegrationVerifier,
) -> Result<TeamCampaignReport, RuntimeError> {
    if fs::symlink_metadata(campaign_root.join(REPORT_NAME)).is_ok() {
        return IntegrityDocument::<TeamCampaignReport>::load(
            campaign_root,
            REPORT_NAME,
            |value| value.verify(spec, manifest, snapshot),
        )
        .map(|document| document.payload)
        .map_err(|_| RuntimeError::State);
    }
    let observed_head = campaign
        .observe_head(authorization)
        .map_err(|_| RuntimeError::Repository)?;
    if observed_head != snapshot.campaign_head {
        return Err(RuntimeError::State);
    }
    let verification = verifier.verify("campaign-final", campaign, &snapshot.campaign_head)?;
    verification.verify()?;
    let tasks = snapshot
        .tasks
        .iter()
        .map(|(task_id, record)| {
            Ok((
                task_id.clone(),
                TeamTaskCompletionReport {
                    reviewed_commit: record.reviewed_commit.clone().ok_or(RuntimeError::State)?,
                    integration_commit: record
                        .integration_commit
                        .clone()
                        .ok_or(RuntimeError::State)?,
                    completion_commit: record
                        .completion_commit
                        .clone()
                        .ok_or(RuntimeError::State)?,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, RuntimeError>>()?;
    let report = TeamCampaignReport {
        version: REPORT_VERSION,
        campaign_id: spec.campaign_id.clone(),
        repository_id: spec.repository_id.clone(),
        branch: manifest.branch.clone(),
        initial_commit: spec.initial_commit.clone(),
        final_commit: snapshot.campaign_head.clone(),
        task_source_sha256: snapshot.task_source_sha256.clone(),
        tasks,
        final_gate_evidence_sha256: verification.gate_evidence_sha256,
        final_review_evidence_sha256: verification.review_evidence_sha256,
        completed_at_ms: now_ms(),
    };
    if !report.verify(spec, manifest, snapshot) {
        return Err(RuntimeError::State);
    }
    IntegrityDocument::write_atomic(campaign_root, REPORT_NAME, report.clone(), |value| {
        value.verify(spec, manifest, snapshot)
    })
    .map_err(|_| RuntimeError::State)?;
    Ok(report)
}

fn load_or_initialize(
    config: &Config,
    spec: &CampaignSpec,
    authorization: &RepositoryAuthorization,
    initial_plan: &TaskPlan,
    campaign_root: &Path,
    authority_sha256: &str,
) -> Result<
    (
        OwnedWorktree,
        TeamCampaignManifest,
        codingmage_campaign::TeamCampaignSnapshot,
    ),
    RuntimeError,
> {
    let manifest_path = campaign_root.join(MANIFEST_NAME);
    if manifest_path.exists() {
        let manifest = IntegrityDocument::<TeamCampaignManifest>::load(
            campaign_root,
            MANIFEST_NAME,
            |value| value.verify(spec, authority_sha256),
        )
        .map_err(|_| RuntimeError::State)?
        .payload;
        let worktree_id =
            WorktreeId::new(manifest.worktree_id.clone()).map_err(|_| RuntimeError::State)?;
        let campaign =
            OwnedWorktree::load(config, &worktree_id).map_err(|_| RuntimeError::Repository)?;
        let snapshot = initialize_team_campaign(spec, initial_plan)?;
        return Ok((campaign, manifest, snapshot));
    }
    let run_id = generated_run_id()?;
    let campaign = create_owned_worktree(
        authorization,
        config,
        run_id.clone(),
        TaskId::new("team-campaign-root").map_err(|_| RuntimeError::State)?,
        &spec.initial_commit,
    )
    .map_err(|_| RuntimeError::Repository)?;
    let manifest = TeamCampaignManifest {
        version: MANIFEST_VERSION,
        campaign_id: spec.campaign_id.clone(),
        repository_id: spec.repository_id.clone(),
        authority_sha256: authority_sha256.to_owned(),
        initial_commit: spec.initial_commit.clone(),
        campaign_run_id: run_id.as_str().to_owned(),
        worktree_id: campaign.manifest().worktree_id.as_str().to_owned(),
        branch: campaign.manifest().branch.clone(),
    };
    IntegrityDocument::write_atomic(campaign_root, MANIFEST_NAME, manifest.clone(), |value| {
        value.verify(spec, authority_sha256)
    })
    .map_err(|_| RuntimeError::State)?;
    let snapshot = initialize_team_campaign(spec, initial_plan)?;
    Ok((campaign, manifest, snapshot))
}

fn persist(
    store: &mut TeamStateStore,
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
) -> Result<(), RuntimeError> {
    store
        .persist(snapshot, now_ms())
        .map(|_| ())
        .map_err(|_| RuntimeError::State)
}

fn blocked_outcome(
    spec: &CampaignSpec,
    campaign: &OwnedWorktree,
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    completed_units: u32,
    last_task_id: Option<String>,
    blocker_code: &str,
) -> CampaignOutcome {
    CampaignOutcome {
        campaign_id: spec.campaign_id.clone(),
        state: CampaignState::Blocked,
        branch: campaign.manifest().branch.clone(),
        head: snapshot.campaign_head.clone(),
        completed_units,
        stop_reason: CampaignStopReason::NoIndependentReadyWork,
        last_task_id,
        blocker_code: Some(blocker_code.to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
fn controlled_outcome(
    spec: &CampaignSpec,
    campaign: &OwnedWorktree,
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    completed_units: u32,
    last_task_id: Option<String>,
    state: CampaignState,
    stop_reason: CampaignStopReason,
    blocker_code: &str,
) -> CampaignOutcome {
    CampaignOutcome {
        campaign_id: spec.campaign_id.clone(),
        state,
        branch: campaign.manifest().branch.clone(),
        head: snapshot.campaign_head.clone(),
        completed_units,
        stop_reason,
        last_task_id,
        blocker_code: Some(blocker_code.to_owned()),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn task_integration_approved(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    task_id: &str,
    policy: TaskIntegrationPolicy,
) -> Result<bool, RuntimeError> {
    if policy != TaskIntegrationPolicy::HumanRequired {
        return Ok(false);
    }
    let reviewed_commit = snapshot
        .tasks
        .get(task_id)
        .and_then(|record| record.reviewed_commit.as_deref())
        .ok_or(RuntimeError::State)?;
    observe_team_integration_approval(
        campaign_root,
        spec,
        manifest,
        authority_sha256,
        task_id,
        &snapshot.campaign_head,
        reviewed_commit,
    )
}

const fn task_integration_blocker(
    policy: TaskIntegrationPolicy,
    approved: bool,
) -> Option<&'static str> {
    match policy {
        TaskIntegrationPolicy::Never => Some("codingmage.team.integration_denied"),
        TaskIntegrationPolicy::HumanRequired if !approved => {
            Some("codingmage.team.integration_approval_required")
        }
        TaskIntegrationPolicy::HumanRequired | TaskIntegrationPolicy::AutoToCampaignBranch => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_integration_policy_is_closed_and_deny_first() {
        assert_eq!(
            task_integration_blocker(TaskIntegrationPolicy::Never, false),
            Some("codingmage.team.integration_denied")
        );
        assert_eq!(
            task_integration_blocker(TaskIntegrationPolicy::HumanRequired, false),
            Some("codingmage.team.integration_approval_required")
        );
        assert_eq!(
            task_integration_blocker(TaskIntegrationPolicy::HumanRequired, true),
            None
        );
        assert_eq!(
            task_integration_blocker(TaskIntegrationPolicy::AutoToCampaignBranch, false),
            None
        );
    }
}
