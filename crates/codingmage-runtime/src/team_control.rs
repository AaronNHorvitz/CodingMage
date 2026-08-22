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
    CampaignControlOutcome, CampaignStatus, CampaignStatusDeferral, CampaignStatusOutcomes,
    CampaignStatusTaskReason, CampaignStatusUtilization, RuntimeError, canonical_file,
    generated_run_id,
    team_campaign::{MANIFEST_NAME, TeamCampaignManifest},
};

const CONTROL_NAME: &str = "team-control.json";
const STATE_NAME: &str = "team-campaign.json";
const CONTROL_VERSION: u16 = 1;
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
    Ok(CampaignStatus {
        schema_version: 2,
        campaign_id: snapshot.campaign_id,
        state: state.to_owned(),
        actor: actor.to_owned(),
        model,
        branch: manifest.branch,
        head: snapshot.campaign_head,
        current_task_id: active.map(|record| record.task_id.clone()),
        current_round: active
            .map(|record| u16::try_from(record.correction_sessions.len()).unwrap_or(u16::MAX)),
        last_task_id: snapshot
            .tasks
            .values()
            .filter(|record| record.state == CampaignTaskState::Merged)
            .max_by_key(|record| record.next_transition)
            .map(|record| record.task_id.clone()),
        completed_units: completed,
        attempt_count: utilization.provider_attempts,
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
