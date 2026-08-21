//! Durable production loop for parallel multi-agent campaigns.

use std::{fs, path::Path, sync::Arc, time::SystemTime};

use codingmage_campaign::{
    CampaignExecutionMode, CampaignSpec, CampaignTaskState, TaskPublicationMode, TeamLeadOutcome,
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
    TeamPlanningOutcome, TeamStateStore, admit_team_lead_report, build_team_lead_binding,
    enqueue_team_integration, execute_team_batch, generated_run_id, initialize_team_campaign,
    integrate_team_queue_head, login_discovery_environment, private_directory,
    refresh_team_readiness, write_private_idempotent,
};

const MANIFEST_NAME: &str = "team-campaign-manifest.json";
const MANIFEST_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamCampaignManifest {
    version: u16,
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
    initial_commit: String,
    campaign_run_id: String,
    worktree_id: String,
    branch: String,
}

impl TeamCampaignManifest {
    fn verify(&self, spec: &CampaignSpec, authority_sha256: &str) -> bool {
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

    loop {
        let plan_source = fs::read(campaign.manifest().path.join(&config.task_source))
            .map_err(|_| RuntimeError::Plan)?;
        let plan = TaskPlan::parse(&plan_source).map_err(|_| RuntimeError::Plan)?;
        if plan.source_sha256 != snapshot.task_source_sha256 {
            return Err(RuntimeError::Plan);
        }
        refresh_team_readiness(&plan, &mut snapshot)?;
        persist(&mut state_store, &snapshot)?;

        let publication_ready = snapshot
            .tasks
            .iter()
            .filter_map(|(task_id, record)| {
                (record.state == CampaignTaskState::PublicationReady).then_some(task_id.clone())
            })
            .collect::<Vec<_>>();
        if !publication_ready.is_empty()
            && policy.publication_mode != TaskPublicationMode::LocalOnly
        {
            return Ok(blocked_outcome(
                &spec,
                &campaign,
                &snapshot,
                integrated_this_invocation,
                last_task_id,
                "codingmage.team.remote_publication_not_configured",
            ));
        }
        for task_id in publication_ready {
            enqueue_team_integration(&mut snapshot, &task_id, |value| {
                persist(&mut state_store, value)
            })?;
        }
        if !snapshot.integration_queue.is_empty() {
            observer(RunProgress::new(
                ProgressActor::IntegrationLead,
                ProgressStage::Integrating,
            ));
            let outcome = integrate_team_queue_head(
                config,
                &authorization,
                &campaign,
                &mut snapshot,
                &mut integration_verifier,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
