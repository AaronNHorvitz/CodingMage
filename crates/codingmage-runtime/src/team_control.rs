//! Integrity-protected status and lifecycle controls for durable team campaigns.

use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use codingmage_campaign::{CampaignSpec, CampaignTaskState, TaskUtilization};
use codingmage_contracts::{RunId, WorktreeId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_git::inventory_repository;
use codingmage_process::CancellationToken;
use codingmage_service::CoordinatorLock;
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};

use crate::{
    CampaignActiveTaskStatus, CampaignControlOutcome, CampaignPromotionApprovalBinding,
    CampaignStatus, CampaignStatusDeferral, CampaignStatusOutcomes, CampaignStatusTaskReason,
    CampaignStatusUtilization, RuntimeError, canonical_file, generated_run_id,
    team_campaign::{MANIFEST_NAME, TeamCampaignManifest},
    team_promotion::campaign_promotion_approval_binding,
};

const CONTROL_NAME: &str = "team-control.json";
const APPROVAL_NAME: &str = "team-integration-approvals.json";
const DESTINATION_APPROVAL_NAME: &str = "team-destination-approvals.json";
const STATE_NAME: &str = "team-campaign.json";
const CONTROL_VERSION: u16 = 1;
const APPROVAL_VERSION: u16 = 1;
const MAX_CONTROL_REQUESTS: usize = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TeamControlAction {
    Pause,
    Resume,
    StopAfterUnit,
    Cancel,
}

impl TeamControlAction {
    const fn parse(value: &str) -> Option<Self> {
        match value.as_bytes() {
            b"pause" => Some(Self::Pause),
            b"resume" => Some(Self::Resume),
            b"stop_after_unit" => Some(Self::StopAfterUnit),
            b"cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::StopAfterUnit => "stop_after_unit",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamControlRequest {
    request_id: String,
    action: TeamControlAction,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamControlState {
    version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: String,
    worktree_id: String,
    paused: bool,
    stop_after_unit: bool,
    cancelled: bool,
    created_at_ms: u64,
    updated_at_ms: u64,
    requests: Vec<TeamControlRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamIntegrationApproval {
    request_id: String,
    task_id: String,
    campaign_head: String,
    reviewed_commit: String,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamApprovalState {
    version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: String,
    worktree_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    approvals: Vec<TeamIntegrationApproval>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamDestinationApproval {
    request_id: String,
    pull_request_number: u64,
    destination_branch: String,
    destination_commit: String,
    final_commit: String,
    report_sha256: String,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TeamDestinationApprovalState {
    version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: String,
    worktree_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    approvals: Vec<TeamDestinationApproval>,
}

impl TeamDestinationApprovalState {
    fn initial(
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            version: APPROVAL_VERSION,
            authority_sha256: authority_sha256.to_owned(),
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            campaign_run_id: manifest.campaign_run_id.clone(),
            worktree_id: manifest.worktree_id.clone(),
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
            approvals: Vec::new(),
        }
    }

    fn verify(
        &self,
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
    ) -> bool {
        if self.version != APPROVAL_VERSION
            || self.authority_sha256 != authority_sha256
            || self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || self.campaign_run_id != manifest.campaign_run_id
            || self.worktree_id != manifest.worktree_id
            || self.created_at_ms > self.updated_at_ms
            || self.approvals.len() > MAX_CONTROL_REQUESTS
        {
            return false;
        }
        let mut requests = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        let mut prior_timestamp = self.created_at_ms;
        for approval in &self.approvals {
            if RunId::new(approval.request_id.clone()).is_err()
                || approval.pull_request_number == 0
                || !valid_branch(&approval.destination_branch)
                || !valid_commit(&approval.destination_commit)
                || !valid_commit(&approval.final_commit)
                || !valid_sha256(&approval.report_sha256)
                || approval.timestamp_ms < prior_timestamp
                || !requests.insert(approval.request_id.as_str())
                || !bindings.insert((
                    approval.pull_request_number,
                    approval.destination_branch.as_str(),
                    approval.destination_commit.as_str(),
                    approval.final_commit.as_str(),
                    approval.report_sha256.as_str(),
                ))
            {
                return false;
            }
            prior_timestamp = approval.timestamp_ms;
        }
        prior_timestamp == self.updated_at_ms
    }

    fn apply(&mut self, approval: TeamDestinationApproval) -> Result<bool, RuntimeError> {
        if let Some(existing) = self
            .approvals
            .iter()
            .find(|value| value.request_id == approval.request_id)
        {
            return if destination_approval_matches(existing, &approval) {
                Ok(false)
            } else {
                Err(RuntimeError::Authority)
            };
        }
        if self.approvals.len() >= MAX_CONTROL_REQUESTS
            || approval.timestamp_ms < self.updated_at_ms
            || self
                .approvals
                .iter()
                .any(|value| destination_approval_matches(value, &approval))
        {
            return Err(RuntimeError::Authority);
        }
        self.updated_at_ms = approval.timestamp_ms;
        self.approvals.push(approval);
        Ok(true)
    }

    fn contains(&self, binding: &CampaignPromotionApprovalBinding) -> bool {
        self.approvals.iter().any(|approval| {
            approval.pull_request_number == binding.pull_request_number
                && approval.destination_branch == binding.destination_branch
                && approval.destination_commit == binding.destination_commit
                && approval.final_commit == binding.final_commit
                && approval.report_sha256 == binding.report_sha256
        })
    }
}

impl TeamApprovalState {
    fn initial(
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            version: APPROVAL_VERSION,
            authority_sha256: authority_sha256.to_owned(),
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            campaign_run_id: manifest.campaign_run_id.clone(),
            worktree_id: manifest.worktree_id.clone(),
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
            approvals: Vec::new(),
        }
    }

    fn verify(
        &self,
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
    ) -> bool {
        if self.version != APPROVAL_VERSION
            || self.authority_sha256 != authority_sha256
            || self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || self.campaign_run_id != manifest.campaign_run_id
            || self.worktree_id != manifest.worktree_id
            || RunId::new(self.campaign_run_id.clone()).is_err()
            || WorktreeId::new(self.worktree_id.clone()).is_err()
            || self.created_at_ms > self.updated_at_ms
            || self.approvals.len() > MAX_CONTROL_REQUESTS
        {
            return false;
        }
        let mut requests = BTreeSet::new();
        let mut task_bindings = BTreeSet::new();
        let mut prior_timestamp = self.created_at_ms;
        for approval in &self.approvals {
            if RunId::new(approval.request_id.clone()).is_err()
                || codingmage_contracts::TaskId::new(approval.task_id.clone()).is_err()
                || !valid_commit(&approval.campaign_head)
                || !valid_commit(&approval.reviewed_commit)
                || approval.timestamp_ms < prior_timestamp
                || !requests.insert(approval.request_id.as_str())
                || !task_bindings.insert((
                    approval.task_id.as_str(),
                    approval.campaign_head.as_str(),
                    approval.reviewed_commit.as_str(),
                ))
            {
                return false;
            }
            prior_timestamp = approval.timestamp_ms;
        }
        prior_timestamp == self.updated_at_ms
    }

    fn apply(&mut self, approval: TeamIntegrationApproval) -> Result<bool, RuntimeError> {
        if let Some(existing) = self
            .approvals
            .iter()
            .find(|value| value.request_id == approval.request_id)
        {
            return if existing.task_id == approval.task_id
                && existing.campaign_head == approval.campaign_head
                && existing.reviewed_commit == approval.reviewed_commit
            {
                Ok(false)
            } else {
                Err(RuntimeError::Authority)
            };
        }
        if self.approvals.len() >= MAX_CONTROL_REQUESTS
            || approval.timestamp_ms < self.updated_at_ms
            || self.approvals.iter().any(|value| {
                value.task_id == approval.task_id
                    && value.campaign_head == approval.campaign_head
                    && value.reviewed_commit == approval.reviewed_commit
            })
        {
            return Err(RuntimeError::Authority);
        }
        self.updated_at_ms = approval.timestamp_ms;
        self.approvals.push(approval);
        Ok(true)
    }

    fn contains(&self, task_id: &str, campaign_head: &str, reviewed_commit: &str) -> bool {
        self.approvals.iter().any(|approval| {
            approval.task_id == task_id
                && approval.campaign_head == campaign_head
                && approval.reviewed_commit == reviewed_commit
        })
    }
}

impl TeamControlState {
    fn initial(
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            version: CONTROL_VERSION,
            authority_sha256: authority_sha256.to_owned(),
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            campaign_run_id: manifest.campaign_run_id.clone(),
            worktree_id: manifest.worktree_id.clone(),
            paused: false,
            stop_after_unit: false,
            cancelled: false,
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
            requests: Vec::new(),
        }
    }

    fn verify(
        &self,
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
    ) -> bool {
        if self.version != CONTROL_VERSION
            || self.authority_sha256 != authority_sha256
            || self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || self.campaign_run_id != manifest.campaign_run_id
            || self.worktree_id != manifest.worktree_id
            || RunId::new(self.campaign_run_id.clone()).is_err()
            || WorktreeId::new(self.worktree_id.clone()).is_err()
            || self.created_at_ms > self.updated_at_ms
            || self.requests.len() > MAX_CONTROL_REQUESTS
        {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut prior_timestamp = self.created_at_ms;
        let mut projection = (false, false, false);
        for request in &self.requests {
            if RunId::new(request.request_id.clone()).is_err()
                || !seen.insert(request.request_id.as_str())
                || request.timestamp_ms < prior_timestamp
                || apply_action(&mut projection, request.action).is_err()
            {
                return false;
            }
            prior_timestamp = request.timestamp_ms;
        }
        projection == (self.paused, self.stop_after_unit, self.cancelled)
            && prior_timestamp == self.updated_at_ms
    }

    fn apply(
        &mut self,
        request_id: &str,
        action: TeamControlAction,
        timestamp_ms: u64,
    ) -> Result<bool, RuntimeError> {
        if let Some(existing) = self
            .requests
            .iter()
            .find(|request| request.request_id == request_id)
        {
            return if existing.action == action {
                Ok(false)
            } else {
                Err(RuntimeError::Authority)
            };
        }
        if self.requests.len() >= MAX_CONTROL_REQUESTS || timestamp_ms < self.updated_at_ms {
            return Err(RuntimeError::State);
        }
        let mut projection = (self.paused, self.stop_after_unit, self.cancelled);
        apply_action(&mut projection, action)?;
        (self.paused, self.stop_after_unit, self.cancelled) = projection;
        self.updated_at_ms = timestamp_ms;
        self.requests.push(TeamControlRequest {
            request_id: request_id.to_owned(),
            action,
            timestamp_ms,
        });
        Ok(true)
    }
}

fn apply_action(
    projection: &mut (bool, bool, bool),
    action: TeamControlAction,
) -> Result<(), RuntimeError> {
    let (paused, stop_after_unit, cancelled) = projection;
    match action {
        TeamControlAction::Pause if !*cancelled && !*paused => *paused = true,
        TeamControlAction::Resume if !*cancelled && (*paused || *stop_after_unit) => {
            *paused = false;
            *stop_after_unit = false;
        }
        TeamControlAction::StopAfterUnit if !*cancelled && !*stop_after_unit => {
            *stop_after_unit = true;
        }
        TeamControlAction::Cancel if !*cancelled => {
            *cancelled = true;
            *paused = false;
            *stop_after_unit = false;
        }
        _ => return Err(RuntimeError::Authority),
    }
    Ok(())
}

/// Control projection observed by the active campaign at a deterministic boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TeamControlObservation {
    pub(crate) paused: bool,
    pub(crate) stop_after_unit: bool,
    pub(crate) cancelled: bool,
}

/// Watches only the integrity-protected cancellation flag and cancels the exact owned token.
pub(crate) struct TeamCancellationWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TeamCancellationWatcher {
    pub(crate) fn start(
        campaign_root: &Path,
        spec: &CampaignSpec,
        manifest: &TeamCampaignManifest,
        authority_sha256: &str,
        cancellation: CancellationToken,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let watcher_stop = Arc::clone(&stop);
        let root = campaign_root.to_path_buf();
        let spec = spec.clone();
        let manifest = manifest.clone();
        let authority_sha256 = authority_sha256.to_owned();
        let handle = thread::spawn(move || {
            while !watcher_stop.load(Ordering::Acquire) {
                match observe_team_control(&root, &spec, &manifest, &authority_sha256) {
                    Ok(observation) if observation.cancelled => {
                        cancellation.cancel();
                        break;
                    }
                    Err(_) => {
                        cancellation.cancel();
                        break;
                    }
                    _ => thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for TeamCancellationWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn observe_team_control(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
) -> Result<TeamControlObservation, RuntimeError> {
    let Some(state) = load_control(campaign_root, spec, manifest, authority_sha256)? else {
        return Ok(TeamControlObservation::default());
    };
    Ok(TeamControlObservation {
        paused: state.paused,
        stop_after_unit: state.stop_after_unit,
        cancelled: state.cancelled,
    })
}

pub(crate) fn observe_team_integration_approval(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
    task_id: &str,
    campaign_head: &str,
    reviewed_commit: &str,
) -> Result<bool, RuntimeError> {
    Ok(
        load_approvals(campaign_root, spec, manifest, authority_sha256)?
            .is_some_and(|state| state.contains(task_id, campaign_head, reviewed_commit)),
    )
}

pub(crate) fn observe_team_destination_approval(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
    binding: &CampaignPromotionApprovalBinding,
) -> Result<bool, RuntimeError> {
    Ok(
        load_destination_approvals(campaign_root, spec, manifest, authority_sha256)?
            .is_some_and(|state| state.contains(binding)),
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn request_team_integration_approval(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    task_id: &str,
    campaign_head: &str,
    reviewed_commit: &str,
    request_id: &str,
) -> Result<CampaignControlOutcome, RuntimeError> {
    let task_id =
        codingmage_contracts::TaskId::new(task_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    let request_id = RunId::new(request_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    if !valid_commit(campaign_head) || !valid_commit(reviewed_commit) {
        return Err(RuntimeError::Spec);
    }
    let (authorization, authority_sha256, campaign_root) =
        validate_team_authority(config, spec, codingmage_binary, true)?;
    let manifest =
        load_manifest(&campaign_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    let snapshot = IntegrityDocument::<codingmage_campaign::TeamCampaignSnapshot>::load(
        &campaign_root.join("state"),
        STATE_NAME,
        |value| value.verify().is_ok(),
    )
    .map_err(|_| RuntimeError::State)?
    .payload;
    let record = snapshot
        .tasks
        .get(task_id.as_str())
        .ok_or(RuntimeError::State)?;
    if authorization.identity().repository_id.as_str() != manifest.repository_id
        || snapshot.campaign_head != campaign_head
        || record.reviewed_commit.as_deref() != Some(reviewed_commit)
        || !matches!(
            record.state,
            CampaignTaskState::PublicationReady
                | CampaignTaskState::PullRequestOpen
                | CampaignTaskState::CiWaiting
        )
    {
        return Err(RuntimeError::Authority);
    }
    let lock_id = generated_run_id()?;
    let _lock = CoordinatorLock::acquire(
        &config
            .state_root
            .join("team-campaign-approval-locks")
            .join(&spec.campaign_id),
        &authorization.identity().repository_id,
        lock_id.as_str(),
    )
    .map_err(|_| RuntimeError::Orchestration)?;
    let timestamp_ms = now_ms()?;
    let mut state = load_approvals(&campaign_root, spec, &manifest, &authority_sha256)?
        .unwrap_or_else(|| {
            TeamApprovalState::initial(spec, &manifest, &authority_sha256, timestamp_ms)
        });
    let created = state.apply(TeamIntegrationApproval {
        request_id: request_id.as_str().to_owned(),
        task_id: task_id.as_str().to_owned(),
        campaign_head: campaign_head.to_owned(),
        reviewed_commit: reviewed_commit.to_owned(),
        timestamp_ms,
    })?;
    if created {
        IntegrityDocument::write_atomic(&campaign_root, APPROVAL_NAME, state, |value| {
            value.verify(spec, &manifest, &authority_sha256)
        })
        .map_err(|_| RuntimeError::State)?;
    }
    Ok(CampaignControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        request_id: request_id.as_str().to_owned(),
        action: "approve_task_integration".to_owned(),
        created,
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn request_team_destination_approval(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    pull_request_number: u64,
    destination_branch: &str,
    destination_commit: &str,
    final_commit: &str,
    report_sha256: &str,
    request_id: &str,
) -> Result<CampaignControlOutcome, RuntimeError> {
    let request_id = RunId::new(request_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    if pull_request_number == 0
        || !valid_branch(destination_branch)
        || !valid_commit(destination_commit)
        || !valid_commit(final_commit)
        || !valid_sha256(report_sha256)
    {
        return Err(RuntimeError::Spec);
    }
    let (authorization, authority_sha256, campaign_root) =
        validate_team_authority(config, spec, codingmage_binary, true)?;
    let manifest =
        load_manifest(&campaign_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    let report =
        crate::team_campaign_report(config, spec, codingmage_binary)?.ok_or(RuntimeError::State)?;
    let expected = campaign_promotion_approval_binding(
        &campaign_root,
        spec,
        &manifest.branch,
        &report,
        &authority_sha256,
    )
    .map_err(|_| RuntimeError::State)?
    .ok_or(RuntimeError::State)?;
    let supplied = CampaignPromotionApprovalBinding {
        campaign_id: spec.campaign_id.clone(),
        pull_request_number,
        destination_branch: destination_branch.to_owned(),
        destination_commit: destination_commit.to_owned(),
        final_commit: final_commit.to_owned(),
        report_sha256: report_sha256.to_owned(),
    };
    if authorization.identity().repository_id.as_str() != manifest.repository_id
        || supplied != expected
    {
        return Err(RuntimeError::Authority);
    }
    let lock_id = generated_run_id()?;
    let _lock = CoordinatorLock::acquire(
        &config
            .state_root
            .join("team-campaign-destination-approval-locks")
            .join(&spec.campaign_id),
        &authorization.identity().repository_id,
        lock_id.as_str(),
    )
    .map_err(|_| RuntimeError::Orchestration)?;
    let timestamp_ms = now_ms()?;
    let mut state = load_destination_approvals(&campaign_root, spec, &manifest, &authority_sha256)?
        .unwrap_or_else(|| {
            TeamDestinationApprovalState::initial(spec, &manifest, &authority_sha256, timestamp_ms)
        });
    let created = state.apply(TeamDestinationApproval {
        request_id: request_id.as_str().to_owned(),
        pull_request_number,
        destination_branch: destination_branch.to_owned(),
        destination_commit: destination_commit.to_owned(),
        final_commit: final_commit.to_owned(),
        report_sha256: report_sha256.to_owned(),
        timestamp_ms,
    })?;
    if created {
        IntegrityDocument::write_atomic(
            &campaign_root,
            DESTINATION_APPROVAL_NAME,
            state,
            |value| value.verify(spec, &manifest, &authority_sha256),
        )
        .map_err(|_| RuntimeError::State)?;
    }
    Ok(CampaignControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        request_id: request_id.as_str().to_owned(),
        action: "approve_destination_promotion".to_owned(),
        created,
    })
}

pub(crate) fn team_campaign_status(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
) -> Result<Option<CampaignStatus>, RuntimeError> {
    let (authorization, authority_sha256, campaign_root) =
        validate_team_authority(config, spec, codingmage_binary, false)?;
    let Some(manifest) = load_manifest(&campaign_root, spec, &authority_sha256)? else {
        return Ok(None);
    };
    if authorization.identity().repository_id.as_str() != manifest.repository_id {
        return Err(RuntimeError::Authority);
    }
    let state_root = campaign_root.join("state");
    let snapshot = IntegrityDocument::<codingmage_campaign::TeamCampaignSnapshot>::load(
        &state_root,
        STATE_NAME,
        |value| value.verify().is_ok(),
    )
    .map_err(|_| RuntimeError::State)?
    .payload;
    let control = load_control(&campaign_root, spec, &manifest, &authority_sha256)?;
    let updated_at_ms = control.as_ref().map_or_else(
        || modified_ms(&state_root.join(STATE_NAME)),
        |value| value.updated_at_ms,
    );
    let started_at_ms = control.as_ref().map_or_else(
        || modified_ms(&campaign_root.join(MANIFEST_NAME)),
        |value| value.created_at_ms,
    );
    Ok(Some(project_status(
        spec,
        manifest,
        snapshot,
        control.as_ref(),
        started_at_ms,
        updated_at_ms,
    )?))
}

#[allow(clippy::too_many_lines)]
pub(crate) fn request_team_campaign_control(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    action: &str,
    request_id: &str,
) -> Result<CampaignControlOutcome, RuntimeError> {
    let action = TeamControlAction::parse(action).ok_or(RuntimeError::Spec)?;
    let request_id = RunId::new(request_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    let (authorization, authority_sha256, campaign_root) =
        validate_team_authority(config, spec, codingmage_binary, true)?;
    let manifest =
        load_manifest(&campaign_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    let state_root = campaign_root.join("state");
    let snapshot = IntegrityDocument::<codingmage_campaign::TeamCampaignSnapshot>::load(
        &state_root,
        STATE_NAME,
        |value| value.verify().is_ok(),
    )
    .map_err(|_| RuntimeError::State)?
    .payload;
    if snapshot
        .tasks
        .values()
        .all(|record| record.state == CampaignTaskState::Merged)
    {
        return Err(RuntimeError::State);
    }
    let lock_id = generated_run_id()?;
    let _lock = CoordinatorLock::acquire(
        &config
            .state_root
            .join("team-campaign-control-locks")
            .join(&spec.campaign_id),
        &authorization.identity().repository_id,
        lock_id.as_str(),
    )
    .map_err(|_| RuntimeError::Orchestration)?;
    let timestamp_ms = now_ms()?;
    let mut state = load_control(&campaign_root, spec, &manifest, &authority_sha256)?
        .unwrap_or_else(|| {
            TeamControlState::initial(spec, &manifest, &authority_sha256, timestamp_ms)
        });
    let created = state.apply(request_id.as_str(), action, timestamp_ms)?;
    if created {
        IntegrityDocument::write_atomic(&campaign_root, CONTROL_NAME, state, |value| {
            value.verify(spec, &manifest, &authority_sha256)
        })
        .map_err(|_| RuntimeError::State)?;
    }
    Ok(CampaignControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        request_id: request_id.as_str().to_owned(),
        action: action.code().to_owned(),
        created,
    })
}

fn validate_team_authority(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    require_initial_checkout: bool,
) -> Result<(RepositoryAuthorization, String, std::path::PathBuf), RuntimeError> {
    spec.verify().map_err(RuntimeError::Campaign)?;
    let authority_sha256 = spec.authority_sha256().map_err(RuntimeError::Campaign)?;
    let binary = canonical_file(codingmage_binary)?;
    let source_root = binary.parent().ok_or(RuntimeError::Authority)?;
    let authorization = RepositoryAuthorization::authorize(config, source_root)
        .map_err(|_| RuntimeError::Authority)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id
        || fs::canonicalize(&config.target_path).map_err(|_| RuntimeError::Authority)?
            != spec.repository_path
    {
        return Err(RuntimeError::Authority);
    }
    if require_initial_checkout {
        let inventory =
            inventory_repository(&authorization).map_err(|_| RuntimeError::Repository)?;
        if !inventory.condition.is_clean() || inventory.head != spec.initial_commit {
            return Err(RuntimeError::Authority);
        }
    }
    Ok((
        authorization,
        authority_sha256,
        config
            .state_root
            .join("team-campaigns")
            .join(&spec.campaign_id),
    ))
}

fn load_manifest(
    campaign_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
) -> Result<Option<TeamCampaignManifest>, RuntimeError> {
    if fs::symlink_metadata(campaign_root.join(MANIFEST_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<TeamCampaignManifest>::load(campaign_root, MANIFEST_NAME, |value| {
        value.verify(spec, authority_sha256)
    })
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

fn load_control(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
) -> Result<Option<TeamControlState>, RuntimeError> {
    if fs::symlink_metadata(campaign_root.join(CONTROL_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<TeamControlState>::load(campaign_root, CONTROL_NAME, |value| {
        value.verify(spec, manifest, authority_sha256)
    })
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

fn load_approvals(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
) -> Result<Option<TeamApprovalState>, RuntimeError> {
    if fs::symlink_metadata(campaign_root.join(APPROVAL_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<TeamApprovalState>::load(campaign_root, APPROVAL_NAME, |value| {
        value.verify(spec, manifest, authority_sha256)
    })
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

fn load_destination_approvals(
    campaign_root: &Path,
    spec: &CampaignSpec,
    manifest: &TeamCampaignManifest,
    authority_sha256: &str,
) -> Result<Option<TeamDestinationApprovalState>, RuntimeError> {
    if fs::symlink_metadata(campaign_root.join(DESTINATION_APPROVAL_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<TeamDestinationApprovalState>::load(
        campaign_root,
        DESTINATION_APPROVAL_NAME,
        |value| value.verify(spec, manifest, authority_sha256),
    )
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

#[allow(clippy::too_many_lines)]
fn project_status(
    spec: &CampaignSpec,
    manifest: TeamCampaignManifest,
    snapshot: codingmage_campaign::TeamCampaignSnapshot,
    control: Option<&TeamControlState>,
    started_at_ms: u64,
    updated_at_ms: u64,
) -> Result<CampaignStatus, RuntimeError> {
    let active = snapshot.tasks.values().find(|record| {
        matches!(
            record.state,
            CampaignTaskState::Leased
                | CampaignTaskState::Implementing
                | CampaignTaskState::LocalGates
                | CampaignTaskState::Reviewing
                | CampaignTaskState::Correcting
                | CampaignTaskState::Integrating
        )
    });
    let cancelled = control.as_ref().is_some_and(|value| value.cancelled);
    let paused = control
        .as_ref()
        .is_some_and(|value| value.paused || value.stop_after_unit);
    let complete = snapshot
        .tasks
        .values()
        .all(|record| record.state == CampaignTaskState::Merged);
    let blocked_records = snapshot.tasks.values().filter(|record| {
        matches!(
            record.state,
            CampaignTaskState::Blocked | CampaignTaskState::Failed | CampaignTaskState::Cancelled
        )
    });
    let blockers = blocked_records
        .clone()
        .map(|record| CampaignStatusTaskReason {
            task_id: record.task_id.clone(),
            reason_code: record
                .terminal_reason
                .clone()
                .unwrap_or_else(|| "terminal_without_reason".to_owned()),
        })
        .collect::<Vec<_>>();
    let human_decisions = snapshot
        .tasks
        .values()
        .filter(|record| record.state == CampaignTaskState::Disputed)
        .map(|record| CampaignStatusTaskReason {
            task_id: record.task_id.clone(),
            reason_code: record
                .terminal_reason
                .clone()
                .unwrap_or_else(|| "review_disputed".to_owned()),
        })
        .collect::<Vec<_>>();
    let completed = count_tasks(&snapshot, |state| state == CampaignTaskState::Merged)?;
    let blocked = u32::try_from(blockers.len()).map_err(|_| RuntimeError::State)?;
    let disputed = u32::try_from(human_decisions.len()).map_err(|_| RuntimeError::State)?;
    let accepted = count_tasks(&snapshot, |state| {
        !matches!(state, CampaignTaskState::Planned | CampaignTaskState::Ready)
    })?;
    let utilization = total_utilization(&snapshot);
    let correction_rounds = snapshot
        .tasks
        .values()
        .try_fold(0_u32, |total, record| {
            u32::try_from(record.correction_sessions.len())
                .ok()
                .and_then(|count| total.checked_add(count))
        })
        .ok_or(RuntimeError::State)?;
    let condition = if cancelled {
        TeamCampaignCondition::Cancelled
    } else if paused {
        TeamCampaignCondition::Paused
    } else if complete {
        TeamCampaignCondition::Complete
    } else if blocked > 0 {
        TeamCampaignCondition::Blocked
    } else {
        TeamCampaignCondition::Active
    };
    let (state, actor, model) = status_identity(spec, active, condition);
    let active_tasks = snapshot
        .tasks
        .values()
        .filter(|record| active_task_state(record.state))
        .map(|record| {
            let (_, actor, model) =
                status_identity(spec, Some(record), TeamCampaignCondition::Active);
            CampaignActiveTaskStatus {
                task_id: record.task_id.clone(),
                pod_id: record.pod_id.clone(),
                state: task_state_code(record.state).to_owned(),
                actor: actor.to_owned(),
                model,
                correction_round: u16::try_from(record.correction_sessions.len())
                    .unwrap_or(u16::MAX),
                heartbeat_sequence: record.heartbeat_sequence,
            }
        })
        .collect();
    Ok(CampaignStatus {
        schema_version: 4,
        campaign_id: snapshot.campaign_id,
        state: state.to_owned(),
        actor: actor.to_owned(),
        model,
        branch: manifest.branch,
        head: snapshot.campaign_head,
        current_task_id: active.map(|record| record.task_id.clone()),
        current_round: active
            .map(|record| u16::try_from(record.correction_sessions.len()).unwrap_or(u16::MAX)),
        active_tasks,
        last_task_id: snapshot
            .tasks
            .values()
            .filter(|record| record.state == CampaignTaskState::Merged)
            .max_by_key(|record| record.next_transition)
            .map(|record| record.task_id.clone()),
        completed_units: completed,
        attempt_count: utilization.provider_attempts,
        planning_generation: u32::try_from(snapshot.generation).map_err(|_| RuntimeError::State)?,
        identical_planning_generations: 0,
        pending_planning_triggers: Vec::new(),
        outcomes: CampaignStatusOutcomes {
            completed,
            blocked,
            deferred: 0,
            pending_human_decision: disputed,
            rejected_proposals: 0,
            accepted,
            max_accepted: spec.max_units,
        },
        utilization: CampaignStatusUtilization {
            provider_attempts: utilization.provider_attempts,
            malformed_report_repairs: 0,
            correction_rounds,
            process_invocations: utilization.process_invocations,
            output_bytes: utilization.output_bytes,
            retained_state_bytes: utilization.retained_state_bytes,
            execution_elapsed_ms: utilization.execution_elapsed_ms,
        },
        limits: spec.limits.clone(),
        blocker_count: blocked.saturating_add(disputed),
        blocker_code: if cancelled {
            Some("codingmage.team.cancelled".to_owned())
        } else if paused {
            Some("codingmage.team.operator_paused".to_owned())
        } else if blocked > 0 || disputed > 0 {
            Some("codingmage.team.no_dependency_ready_work".to_owned())
        } else {
            None
        },
        blockers,
        deferrals: Vec::<CampaignStatusDeferral>::new(),
        human_decisions,
        elapsed_ms: updated_at_ms.saturating_sub(started_at_ms),
        updated_at_ms,
    })
}

const fn active_task_state(state: CampaignTaskState) -> bool {
    matches!(
        state,
        CampaignTaskState::Leased
            | CampaignTaskState::Implementing
            | CampaignTaskState::LocalGates
            | CampaignTaskState::Reviewing
            | CampaignTaskState::Correcting
            | CampaignTaskState::Integrating
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TeamCampaignCondition {
    Active,
    Blocked,
    Paused,
    Complete,
    Cancelled,
}

fn status_identity(
    spec: &CampaignSpec,
    active: Option<&codingmage_campaign::CampaignTaskRecord>,
    condition: TeamCampaignCondition,
) -> (&'static str, &'static str, Option<String>) {
    match condition {
        TeamCampaignCondition::Cancelled => return ("cancelled", "coordinator", None),
        TeamCampaignCondition::Paused => return ("paused", "coordinator", None),
        TeamCampaignCondition::Complete => return ("complete", "coordinator", None),
        TeamCampaignCondition::Blocked => return ("blocked", "coordinator", None),
        TeamCampaignCondition::Active => {}
    }
    let Some(record) = active else {
        return ("ready", "coordinator", None);
    };
    match record.state {
        CampaignTaskState::Implementing | CampaignTaskState::Correcting => (
            task_state_code(record.state),
            "claude",
            Some(spec.implementer.model.clone()),
        ),
        CampaignTaskState::LocalGates => (task_state_code(record.state), "local-gates", None),
        CampaignTaskState::Reviewing => (
            task_state_code(record.state),
            "codex",
            Some(spec.reviewer.model.clone()),
        ),
        CampaignTaskState::Integrating => (task_state_code(record.state), "integration", None),
        _ => (task_state_code(record.state), "coordinator", None),
    }
}

const fn task_state_code(state: CampaignTaskState) -> &'static str {
    match state {
        CampaignTaskState::Planned => "planned",
        CampaignTaskState::Ready => "ready",
        CampaignTaskState::Proposed => "proposed",
        CampaignTaskState::Leased => "leased",
        CampaignTaskState::Implementing => "implementing",
        CampaignTaskState::LocalGates => "local_gates",
        CampaignTaskState::Reviewing => "reviewing",
        CampaignTaskState::Correcting => "correcting",
        CampaignTaskState::PublicationReady => "publication_ready",
        CampaignTaskState::PullRequestOpen => "pr_open",
        CampaignTaskState::CiWaiting => "ci_waiting",
        CampaignTaskState::IntegrationQueued => "integration_queued",
        CampaignTaskState::MergeReady => "merge_ready",
        CampaignTaskState::Integrating => "integrating",
        CampaignTaskState::Merged => "merged",
        CampaignTaskState::Blocked => "blocked",
        CampaignTaskState::Disputed => "disputed",
        CampaignTaskState::Failed => "failed",
        CampaignTaskState::Cancelled => "cancelled",
    }
}

fn count_tasks(
    snapshot: &codingmage_campaign::TeamCampaignSnapshot,
    predicate: impl Fn(CampaignTaskState) -> bool,
) -> Result<u32, RuntimeError> {
    u32::try_from(
        snapshot
            .tasks
            .values()
            .filter(|record| predicate(record.state))
            .count(),
    )
    .map_err(|_| RuntimeError::State)
}

fn total_utilization(snapshot: &codingmage_campaign::TeamCampaignSnapshot) -> TaskUtilization {
    snapshot.resources.active.values().fold(
        snapshot.resources.consumed.clone(),
        |mut total, reservation| {
            total.provider_attempts = total
                .provider_attempts
                .saturating_add(reservation.observed.provider_attempts);
            total.provider_tokens = total
                .provider_tokens
                .saturating_add(reservation.observed.provider_tokens);
            total.process_invocations = total
                .process_invocations
                .saturating_add(reservation.observed.process_invocations);
            total.output_bytes = total
                .output_bytes
                .saturating_add(reservation.observed.output_bytes);
            total.retained_state_bytes = total
                .retained_state_bytes
                .saturating_add(reservation.observed.retained_state_bytes);
            total.execution_elapsed_ms = total
                .execution_elapsed_ms
                .saturating_add(reservation.observed.execution_elapsed_ms);
            total
        },
    )
}

fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn now_ms() -> Result<u64, RuntimeError> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or(RuntimeError::State)
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}

fn destination_approval_matches(
    left: &TeamDestinationApproval,
    right: &TeamDestinationApproval,
) -> bool {
    left.pull_request_number == right.pull_request_number
        && left.destination_branch == right.destination_branch
        && left.destination_commit == right.destination_commit
        && left.final_commit == right.final_commit
        && left.report_sha256 == right.report_sha256
}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_campaign::{
        CampaignAuthentication, CampaignConcurrency, CampaignExecutionMode, CampaignGateTier,
        CampaignLimits, CampaignProvider, CampaignPublication, DestinationPromotionPolicy,
        MultiAgentPolicy, TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode,
        TeamResourcePolicy,
    };
    use std::path::PathBuf;

    #[test]
    fn control_history_is_idempotent_ordered_and_irreversibly_cancelled() {
        let spec = spec();
        let authority = spec.authority_sha256().unwrap();
        let manifest = manifest(&spec, &authority);
        let mut state = TeamControlState::initial(&spec, &manifest, &authority, 10);
        assert!(
            state
                .apply("pause-1", TeamControlAction::Pause, 11)
                .unwrap()
        );
        assert!(
            !state
                .apply("pause-1", TeamControlAction::Pause, 12)
                .unwrap()
        );
        assert!(
            state
                .apply("resume-1", TeamControlAction::Resume, 13)
                .unwrap()
        );
        assert!(
            state
                .apply("stop-1", TeamControlAction::StopAfterUnit, 14)
                .unwrap()
        );
        assert!(
            state
                .apply("cancel-1", TeamControlAction::Cancel, 15)
                .unwrap()
        );
        assert!(state.verify(&spec, &manifest, &authority));
        assert_eq!(
            state.apply("resume-2", TeamControlAction::Resume, 16),
            Err(RuntimeError::Authority)
        );
        let mut tampered = state;
        tampered.requests.swap(0, 1);
        assert!(!tampered.verify(&spec, &manifest, &authority));
    }

    #[test]
    fn task_integration_approval_is_exact_idempotent_and_stale_head_bound() {
        let spec = spec();
        let authority = spec.authority_sha256().unwrap();
        let manifest = manifest(&spec, &authority);
        let mut state = TeamApprovalState::initial(&spec, &manifest, &authority, 10);
        let approval = TeamIntegrationApproval {
            request_id: "approval-1".to_owned(),
            task_id: "24.1.1.1".to_owned(),
            campaign_head: "d".repeat(40),
            reviewed_commit: "e".repeat(40),
            timestamp_ms: 11,
        };
        assert!(state.apply(approval.clone()).unwrap());
        let mut replay = approval.clone();
        replay.timestamp_ms = 12;
        assert!(!state.apply(replay).unwrap());
        assert!(state.verify(&spec, &manifest, &authority));
        assert!(state.contains(
            &approval.task_id,
            &approval.campaign_head,
            &approval.reviewed_commit
        ));
        assert!(!state.contains(
            &approval.task_id,
            &"f".repeat(40),
            &approval.reviewed_commit
        ));

        let mut conflicting = approval;
        conflicting.reviewed_commit = "0".repeat(40);
        conflicting.timestamp_ms = 13;
        assert_eq!(state.apply(conflicting), Err(RuntimeError::Authority));
    }

    #[test]
    fn destination_approval_is_exact_idempotent_and_report_bound() {
        let spec = spec();
        let authority = spec.authority_sha256().unwrap();
        let manifest = manifest(&spec, &authority);
        let mut state = TeamDestinationApprovalState::initial(&spec, &manifest, &authority, 20);
        let approval = TeamDestinationApproval {
            request_id: "destination-approval-1".to_owned(),
            pull_request_number: 81,
            destination_branch: "main".to_owned(),
            destination_commit: "d".repeat(40),
            final_commit: "e".repeat(40),
            report_sha256: "f".repeat(64),
            timestamp_ms: 21,
        };
        assert!(state.apply(approval.clone()).unwrap());
        let mut replay = approval.clone();
        replay.timestamp_ms = 22;
        assert!(!state.apply(replay).unwrap());
        assert!(state.verify(&spec, &manifest, &authority));
        let binding = CampaignPromotionApprovalBinding {
            campaign_id: spec.campaign_id.clone(),
            pull_request_number: approval.pull_request_number,
            destination_branch: approval.destination_branch.clone(),
            destination_commit: approval.destination_commit.clone(),
            final_commit: approval.final_commit.clone(),
            report_sha256: approval.report_sha256.clone(),
        };
        assert!(state.contains(&binding));
        let mut changed_binding = binding;
        changed_binding.report_sha256 = "0".repeat(64);
        assert!(!state.contains(&changed_binding));

        let mut conflicting = approval;
        conflicting.destination_commit = "1".repeat(40);
        conflicting.timestamp_ms = 23;
        assert_eq!(state.apply(conflicting), Err(RuntimeError::Authority));
    }

    fn spec() -> CampaignSpec {
        let provider = CampaignProvider {
            executable: PathBuf::from("/bin/true"),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        CampaignSpec {
            version: 3,
            campaign_id: "fixture-team".to_owned(),
            repository_id: "repo-fixture".to_owned(),
            repository_path: PathBuf::from("/tmp/fixture-team"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
            max_parallel_pods: 2,
            max_units: 2,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 10,
                process_invocations: 1_000,
                output_bytes: 1_000_000,
                retained_state_bytes: 1_000_000,
                execution_elapsed_ms: 100_000,
            },
            team_lead: provider.clone(),
            implementer: provider.clone(),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider,
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["fixture".to_owned()],
            }],
            campaign_branch: "codingmage/fixture-team".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: vec![],
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
                    claude_implementers: 2,
                    codex_team_leads: 1,
                    codex_reviewers: 2,
                    test_workers: 2,
                    github_writers: 1,
                    integration_workers: 1,
                },
                resources: TeamResourcePolicy::default(),
                max_campaign_tokens: 100_000,
                max_task_tokens: 50_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 0,
                integration_validation_interval: 1,
                provider_routing: None,
            }),
        }
    }

    fn manifest(spec: &CampaignSpec, authority: &str) -> TeamCampaignManifest {
        TeamCampaignManifest {
            version: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            authority_sha256: authority.to_owned(),
            initial_commit: spec.initial_commit.clone(),
            campaign_run_id: "run-team-control".to_owned(),
            worktree_id: "wt-team-control".to_owned(),
            branch: "codingmage/fixture-team/root".to_owned(),
        }
    }
}
