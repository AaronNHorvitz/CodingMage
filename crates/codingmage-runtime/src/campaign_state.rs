use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_campaign::CampaignLimits;
use codingmage_contracts::{
    EvidenceId, LeadBlockedReason, LeadDeferredReason, LeadHumanDecisionReason,
    LeadReconsiderationTrigger, RepositoryId, RunId, TaskId, WorktreeId,
};
use codingmage_state::{
    CampaignCheckpointProjection, DurableIdentities, EventKind, EventOutcome, Journal,
    JournalEvent, RedactedField,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CampaignLimitKind, RunUtilization, RuntimeError};

const SCHEMA_VERSION: u16 = 7;
const MAX_CHECKPOINT_BYTES: usize = 1024 * 1024;
const CLEARANCE_SCHEMA_VERSION: u16 = 1;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CampaignPhase {
    Ready,
    Planning,
    RunningUnit,
    Integrating,
    Paused,
    Blocked,
    Complete,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActiveUnit {
    pub task_id: String,
    pub source_head: String,
    pub task_source_sha256: String,
    pub owned_paths: Vec<PathBuf>,
    #[serde(default)]
    pub run_id: Option<RunId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingIntegration {
    pub task_id: String,
    pub expected_head: String,
    pub target_head: String,
    pub owned_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeferredTaskProjection {
    pub reason: LeadDeferredReason,
    pub trigger: LeadReconsiderationTrigger,
    pub source_head: String,
    pub task_source_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HumanDecisionProjectionReason {
    Lead(LeadHumanDecisionReason),
    RepeatedSatisfiedDeferral,
}

impl HumanDecisionProjectionReason {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Lead(reason) => reason.code(),
            Self::RepeatedSatisfiedDeferral => "repeated_satisfied_deferral",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HumanDecisionProjection {
    pub reason: HumanDecisionProjectionReason,
    pub source_head: String,
    pub task_source_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LeadRejectionReason {
    MalformedOutput,
    InvalidProposal,
}

impl LeadRejectionReason {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::MalformedOutput => "malformed_output",
            Self::InvalidProposal => "invalid_proposal",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RejectedProposalProjection {
    pub sequence: u32,
    pub reason: LeadRejectionReason,
    pub source_head: String,
    pub task_source_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CampaignOutcomeProjection {
    pub completed: u32,
    pub blocked: u32,
    pub deferred: u32,
    pub pending_human_decision: u32,
    pub rejected_proposals: u32,
    pub accepted: u32,
    pub max_accepted: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CampaignUtilization {
    pub provider_attempts: u32,
    pub malformed_report_repairs: u32,
    pub correction_rounds: u32,
    pub process_invocations: u32,
    pub output_bytes: u64,
    pub retained_state_bytes: u64,
    pub execution_elapsed_ms: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct CampaignUnitBudget {
    baseline: CampaignUtilization,
    limits: CampaignLimits,
    campaign_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CampaignReservation {
    pub provider_attempts: u32,
    pub malformed_report_repairs: u32,
    pub correction_rounds: u32,
    pub process_invocations: u32,
}

impl CampaignUnitBudget {
    pub(crate) fn authorize(
        &self,
        unit: &RunUtilization,
        correction_rounds: u16,
        reservation: CampaignReservation,
    ) -> Result<(), RuntimeError> {
        let exhausted = self.exhausted_with_reservation(unit, correction_rounds, reservation)?;
        exhausted.map_or(Ok(()), |limit| Err(RuntimeError::CampaignLimit(limit)))
    }

    fn exhausted_with_reservation(
        &self,
        unit: &RunUtilization,
        correction_rounds: u16,
        reservation: CampaignReservation,
    ) -> Result<Option<CampaignLimitKind>, RuntimeError> {
        let retained = retained_tree_bytes(&self.campaign_root)?;
        self.exhausted_with_observations(unit, correction_rounds, reservation, retained)
    }

    fn exhausted_with_observations(
        &self,
        unit: &RunUtilization,
        correction_rounds: u16,
        reservation: CampaignReservation,
        retained: u64,
    ) -> Result<Option<CampaignLimitKind>, RuntimeError> {
        let provider_attempts = self
            .baseline
            .provider_attempts
            .checked_add(unit.provider_attempts)
            .and_then(|value| value.checked_add(reservation.provider_attempts))
            .ok_or(RuntimeError::State)?;
        let malformed_report_repairs = self
            .baseline
            .malformed_report_repairs
            .checked_add(unit.malformed_report_repairs)
            .and_then(|value| value.checked_add(reservation.malformed_report_repairs))
            .ok_or(RuntimeError::State)?;
        let corrections = self
            .baseline
            .correction_rounds
            .checked_add(u32::from(correction_rounds))
            .and_then(|value| value.checked_add(reservation.correction_rounds))
            .ok_or(RuntimeError::State)?;
        let processes = self
            .baseline
            .process_invocations
            .checked_add(unit.process_invocations)
            .and_then(|value| value.checked_add(reservation.process_invocations))
            .ok_or(RuntimeError::State)?;
        let output = self
            .baseline
            .output_bytes
            .checked_add(unit.output_bytes)
            .ok_or(RuntimeError::State)?;
        let elapsed = self
            .baseline
            .execution_elapsed_ms
            .checked_add(unit.execution_elapsed_ms)
            .ok_or(RuntimeError::State)?;
        Ok([
            (
                reservation.provider_attempts > 0
                    && provider_attempts > self.limits.provider_attempts,
                CampaignLimitKind::ProviderAttempts,
            ),
            (
                reservation.malformed_report_repairs > 0
                    && malformed_report_repairs > self.limits.malformed_report_repairs,
                CampaignLimitKind::MalformedReportRepairs,
            ),
            (
                reservation.correction_rounds > 0 && corrections > self.limits.correction_rounds,
                CampaignLimitKind::CorrectionRounds,
            ),
            (
                reservation.process_invocations > 0 && processes > self.limits.process_invocations,
                CampaignLimitKind::ProcessInvocations,
            ),
            (
                output >= self.limits.output_bytes,
                CampaignLimitKind::OutputBytes,
            ),
            (
                retained >= self.limits.retained_state_bytes,
                CampaignLimitKind::RetainedStateBytes,
            ),
            (
                elapsed >= self.limits.execution_elapsed_ms,
                CampaignLimitKind::ExecutionElapsed,
            ),
        ]
        .into_iter()
        .find_map(|(exhausted, limit)| exhausted.then_some(limit)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CampaignCheckpoint {
    pub schema_version: u16,
    pub authority_sha256: String,
    pub campaign_id: String,
    pub repository_id: String,
    pub campaign_run_id: RunId,
    pub worktree_id: WorktreeId,
    pub branch: String,
    pub initial_head: String,
    pub head: String,
    pub completed_units: u32,
    pub last_task_id: Option<String>,
    pub phase: CampaignPhase,
    pub blocker_code: Option<String>,
    #[serde(default)]
    pub blocked_task_ids: BTreeSet<String>,
    #[serde(default)]
    pub blocked_reasons: BTreeMap<String, LeadBlockedReason>,
    #[serde(default)]
    pub deferred_tasks: BTreeMap<String, DeferredTaskProjection>,
    #[serde(default)]
    pub satisfied_deferrals: BTreeMap<String, DeferredTaskProjection>,
    #[serde(default)]
    pub human_decisions: BTreeMap<String, HumanDecisionProjection>,
    #[serde(default)]
    pub rejected_proposals: Vec<RejectedProposalProjection>,
    pub outcomes: CampaignOutcomeProjection,
    pub utilization: CampaignUtilization,
    pub limits: CampaignLimits,
    pub operator_paused: bool,
    pub stop_after_unit: bool,
    pub cancelled: bool,
    pub resume_validation: ResumeValidationState,
    pub applied_control_requests: BTreeSet<String>,
    pub active_unit: Option<ActiveUnit>,
    pub pending_integration: Option<PendingIntegration>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointEnvelope {
    checkpoint: CampaignCheckpoint,
    checkpoint_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlockerClearanceIntent {
    pub schema_version: u16,
    pub request_id: String,
    pub campaign_id: String,
    pub repository_id: String,
    pub task_id: String,
    pub blocked_reason: LeadBlockedReason,
    pub campaign_head: String,
    pub task_source_sha256: String,
    pub prerequisite_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeferralTriggerIntent {
    pub schema_version: u16,
    pub request_id: String,
    pub campaign_id: String,
    pub repository_id: String,
    pub task_id: String,
    pub reason: LeadDeferredReason,
    pub trigger: LeadReconsiderationTrigger,
    pub campaign_head: String,
    pub task_source_sha256: String,
    pub evidence_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BlockerClearanceEnvelope {
    intent: BlockerClearanceIntent,
    intent_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeferralTriggerEnvelope {
    intent: DeferralTriggerIntent,
    intent_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CampaignControlAction {
    Pause,
    Resume,
    StopAfterUnit,
    Cancel,
}

impl CampaignControlAction {
    pub(crate) const fn parse(value: &str) -> Option<Self> {
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResumeValidationState {
    NotRequired,
    Pending,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CampaignControlIntent {
    schema_version: u16,
    pub request_id: String,
    pub authority_sha256: String,
    pub campaign_id: String,
    pub repository_id: String,
    pub campaign_run_id: RunId,
    pub action: CampaignControlAction,
    pub observed_head: String,
    pub observed_updated_at_ms: u64,
    pub created_at_ms: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CampaignControlEnvelope {
    intent: CampaignControlIntent,
    intent_sha256: String,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCampaignCheckpointV1 {
    schema_version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: RunId,
    worktree_id: WorktreeId,
    branch: String,
    initial_head: String,
    head: String,
    completed_units: u32,
    last_task_id: Option<String>,
    phase: CampaignPhase,
    blocker_code: Option<String>,
    active_unit: Option<ActiveUnit>,
    pending_integration: Option<PendingIntegration>,
    started_at_ms: u64,
    updated_at_ms: u64,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCampaignCheckpointWithBlockedIds {
    schema_version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: RunId,
    worktree_id: WorktreeId,
    branch: String,
    initial_head: String,
    head: String,
    completed_units: u32,
    last_task_id: Option<String>,
    phase: CampaignPhase,
    blocker_code: Option<String>,
    blocked_task_ids: BTreeSet<String>,
    active_unit: Option<ActiveUnit>,
    pending_integration: Option<PendingIntegration>,
    started_at_ms: u64,
    updated_at_ms: u64,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCampaignCheckpointWithBlockedReasons {
    schema_version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: RunId,
    worktree_id: WorktreeId,
    branch: String,
    initial_head: String,
    head: String,
    completed_units: u32,
    last_task_id: Option<String>,
    phase: CampaignPhase,
    blocker_code: Option<String>,
    blocked_task_ids: BTreeSet<String>,
    blocked_reasons: BTreeMap<String, LeadBlockedReason>,
    active_unit: Option<ActiveUnit>,
    pending_integration: Option<PendingIntegration>,
    started_at_ms: u64,
    updated_at_ms: u64,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCampaignCheckpointWithDeferrals {
    schema_version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: RunId,
    worktree_id: WorktreeId,
    branch: String,
    initial_head: String,
    head: String,
    completed_units: u32,
    last_task_id: Option<String>,
    phase: CampaignPhase,
    blocker_code: Option<String>,
    blocked_task_ids: BTreeSet<String>,
    blocked_reasons: BTreeMap<String, LeadBlockedReason>,
    deferred_tasks: BTreeMap<String, DeferredTaskProjection>,
    satisfied_deferrals: BTreeMap<String, DeferredTaskProjection>,
    active_unit: Option<ActiveUnit>,
    pending_integration: Option<PendingIntegration>,
    started_at_ms: u64,
    updated_at_ms: u64,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCampaignCheckpointWithHumanDecisions {
    schema_version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_run_id: RunId,
    worktree_id: WorktreeId,
    branch: String,
    initial_head: String,
    head: String,
    completed_units: u32,
    last_task_id: Option<String>,
    phase: CampaignPhase,
    blocker_code: Option<String>,
    blocked_task_ids: BTreeSet<String>,
    blocked_reasons: BTreeMap<String, LeadBlockedReason>,
    deferred_tasks: BTreeMap<String, DeferredTaskProjection>,
    satisfied_deferrals: BTreeMap<String, DeferredTaskProjection>,
    human_decisions: BTreeMap<String, HumanDecisionProjection>,
    active_unit: Option<ActiveUnit>,
    pending_integration: Option<PendingIntegration>,
    started_at_ms: u64,
    updated_at_ms: u64,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeWithHumanDecisions {
    checkpoint: LegacyCampaignCheckpointWithHumanDecisions,
    checkpoint_sha256: String,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeWithDeferrals {
    checkpoint: LegacyCampaignCheckpointWithDeferrals,
    checkpoint_sha256: String,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeWithBlockedReasons {
    checkpoint: LegacyCampaignCheckpointWithBlockedReasons,
    checkpoint_sha256: String,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeWithBlockedIds {
    checkpoint: LegacyCampaignCheckpointWithBlockedIds,
    checkpoint_sha256: String,
}

#[cfg(test)]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeV1 {
    checkpoint: LegacyCampaignCheckpointV1,
    checkpoint_sha256: String,
}

impl CampaignCheckpoint {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        authority_sha256: String,
        campaign_id: String,
        repository_id: String,
        campaign_run_id: RunId,
        worktree_id: WorktreeId,
        branch: String,
        initial_head: String,
        max_accepted_outcomes: u32,
        limits: CampaignLimits,
    ) -> Result<Self, RuntimeError> {
        if max_accepted_outcomes == 0 {
            return Err(RuntimeError::State);
        }
        let now = timestamp_ms()?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            authority_sha256,
            campaign_id,
            repository_id,
            campaign_run_id,
            worktree_id,
            branch,
            initial_head: initial_head.clone(),
            head: initial_head,
            completed_units: 0,
            last_task_id: None,
            phase: CampaignPhase::Ready,
            blocker_code: None,
            blocked_task_ids: BTreeSet::new(),
            blocked_reasons: BTreeMap::new(),
            deferred_tasks: BTreeMap::new(),
            satisfied_deferrals: BTreeMap::new(),
            human_decisions: BTreeMap::new(),
            rejected_proposals: Vec::new(),
            outcomes: CampaignOutcomeProjection {
                completed: 0,
                blocked: 0,
                deferred: 0,
                pending_human_decision: 0,
                rejected_proposals: 0,
                accepted: 0,
                max_accepted: max_accepted_outcomes,
            },
            utilization: CampaignUtilization::default(),
            limits,
            operator_paused: false,
            stop_after_unit: false,
            cancelled: false,
            resume_validation: ResumeValidationState::NotRequired,
            applied_control_requests: BTreeSet::new(),
            active_unit: None,
            pending_integration: None,
            started_at_ms: now,
            updated_at_ms: now,
        })
    }

    pub(crate) fn load(root: &Path) -> Result<Option<Self>, RuntimeError> {
        let path = root.join("checkpoint.json");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeError::State),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_CHECKPOINT_BYTES as u64
        {
            return Err(RuntimeError::State);
        }
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|_| RuntimeError::State)?;
        let envelope: CheckpointEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
        let canonical =
            serde_json::to_vec(&envelope.checkpoint).map_err(|_| RuntimeError::State)?;
        if envelope.checkpoint.schema_version != SCHEMA_VERSION {
            return Err(RuntimeError::State);
        }
        if sha256_hex(&canonical) != envelope.checkpoint_sha256 {
            return Err(RuntimeError::State);
        }
        envelope.checkpoint.validate_outcomes()?;
        Ok(Some(envelope.checkpoint))
    }

    pub(crate) fn persist(&mut self, root: &Path) -> Result<(), RuntimeError> {
        private_directory(root)?;
        self.refresh_outcomes()?;
        self.utilization.retained_state_bytes = retained_tree_bytes(root)?;
        self.updated_at_ms = timestamp_ms()?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = CheckpointEnvelope {
            checkpoint: self.clone(),
            checkpoint_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let temporary = root.join(format!(
            ".checkpoint.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        let current = root.join("checkpoint.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| RuntimeError::State)?;
        set_file_private(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RuntimeError::State)?;
        fs::rename(&temporary, &current).map_err(|_| RuntimeError::State)?;
        File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)?;
        self.append_journal_projection(root, &canonical)
    }

    fn append_journal_projection(&self, root: &Path, canonical: &[u8]) -> Result<(), RuntimeError> {
        let projection = self.journal_projection()?;
        let task_id = self
            .active_unit
            .as_ref()
            .map(|active| active.task_id.as_str())
            .or_else(|| {
                self.pending_integration
                    .as_ref()
                    .map(|pending| pending.task_id.as_str())
            })
            .or(self.last_task_id.as_deref())
            .unwrap_or("campaign-root");
        let evidence = EvidenceId::new(format!("checkpoint-{}", sha256_hex(canonical)))
            .map_err(|_| RuntimeError::State)?;
        let redactions = [
            "provider_output",
            "source_text",
            "command_output",
            "environment_values",
            "credentials",
        ]
        .into_iter()
        .map(RedactedField::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| RuntimeError::State)?;
        let event = JournalEvent {
            timestamp_ms: self.updated_at_ms,
            run_id: self.campaign_run_id.clone(),
            task_id: TaskId::new(task_id).map_err(|_| RuntimeError::State)?,
            repository_id: RepositoryId::new(self.repository_id.clone())
                .map_err(|_| RuntimeError::State)?,
            identities: DurableIdentities {
                worktree: Some(self.worktree_id.clone()),
                branch: Some(self.branch.clone()),
                commit: Some(self.head.clone()),
                ..DurableIdentities::default()
            },
            kind: EventKind::CampaignCheckpointed {
                projection: Box::new(projection),
            },
            outcome: EventOutcome::Succeeded,
            evidence: vec![evidence],
            redactions,
        };
        let mut journal = Journal::open(
            root,
            format!("{}-checkpoint", self.campaign_run_id.as_str()),
        )
        .map_err(|_| RuntimeError::State)?;
        journal.append(event).map_err(|_| RuntimeError::State)?;
        Ok(())
    }

    fn journal_projection(&self) -> Result<CampaignCheckpointProjection, RuntimeError> {
        let queue_sha256 = projection_sha256(&(
            &self.head,
            self.completed_units,
            &self.blocked_task_ids,
            self.deferred_tasks.keys().collect::<Vec<_>>(),
            self.human_decisions.keys().collect::<Vec<_>>(),
        ))?;
        Ok(CampaignCheckpointProjection {
            phase: self.phase.label().to_owned(),
            queue_sha256,
            completed_units: self.outcomes.completed,
            blocked_tasks: self.outcomes.blocked,
            deferred_tasks: self.outcomes.deferred,
            satisfied_deferrals: u32::try_from(self.satisfied_deferrals.len())
                .map_err(|_| RuntimeError::State)?,
            human_decisions: self.outcomes.pending_human_decision,
            rejected_proposals: self.outcomes.rejected_proposals,
            accepted_outcomes: self.outcomes.accepted,
            max_outcomes: Some(self.outcomes.max_accepted),
            active_unit: self.active_unit.is_some(),
            active_pod_run_id: self
                .active_unit
                .as_ref()
                .and_then(|active| active.run_id.as_ref())
                .map(|run_id| run_id.as_str().to_owned()),
            active_pod_sha256: self
                .active_unit
                .as_ref()
                .map(projection_sha256)
                .transpose()?,
            pending_integration_sha256: self
                .pending_integration
                .as_ref()
                .map(projection_sha256)
                .transpose()?,
            blocker_projection_sha256: projection_sha256(&(
                &self.blocker_code,
                &self.blocked_task_ids,
                &self.blocked_reasons,
            ))?,
            deferral_projection_sha256: projection_sha256(&self.deferred_tasks)?,
            trigger_projection_sha256: projection_sha256(&self.satisfied_deferrals)?,
            control_projection_sha256: projection_sha256(&(
                self.operator_paused,
                self.stop_after_unit,
                self.cancelled,
                self.resume_validation,
                &self.applied_control_requests,
            ))?,
            completion_projection_sha256: projection_sha256(&(
                self.completed_units,
                &self.last_task_id,
                &self.head,
                &self.pending_integration,
            ))?,
            provider_attempts: Some(self.utilization.provider_attempts),
            max_provider_attempts: Some(self.limits.provider_attempts),
            malformed_report_repairs: Some(self.utilization.malformed_report_repairs),
            max_malformed_report_repairs: Some(self.limits.malformed_report_repairs),
            correction_round: Some(self.utilization.correction_rounds),
            max_correction_rounds: Some(self.limits.correction_rounds),
            process_invocations: Some(self.utilization.process_invocations),
            max_process_invocations: Some(self.limits.process_invocations),
            output_bytes: Some(self.utilization.output_bytes),
            max_output_bytes: Some(self.limits.output_bytes),
            retained_state_bytes: Some(self.utilization.retained_state_bytes),
            max_retained_state_bytes: Some(self.limits.retained_state_bytes),
            execution_elapsed_ms: Some(self.utilization.execution_elapsed_ms),
            max_execution_elapsed_ms: Some(self.limits.execution_elapsed_ms),
            operator_paused: Some(self.operator_paused),
            stop_after_unit: Some(self.stop_after_unit),
            cancelled: Some(self.cancelled),
            resume_validation: Some(match self.resume_validation {
                ResumeValidationState::NotRequired => "not_required".to_owned(),
                ResumeValidationState::Pending => "pending".to_owned(),
            }),
        })
    }

    pub(crate) fn validate_authority(
        &self,
        authority_sha256: &str,
        campaign_id: &str,
        repository_id: &str,
        initial_head: &str,
        max_accepted_outcomes: u32,
        limits: &CampaignLimits,
    ) -> Result<(), RuntimeError> {
        if self.authority_sha256 != authority_sha256
            || self.campaign_id != campaign_id
            || self.repository_id != repository_id
            || self.initial_head != initial_head
            || self.outcomes.max_accepted != max_accepted_outcomes
            || self.limits != *limits
        {
            return Err(RuntimeError::Authority);
        }
        Ok(())
    }

    pub(crate) fn elapsed_ms(&self) -> Result<u64, RuntimeError> {
        Ok(timestamp_ms()?.saturating_sub(self.started_at_ms))
    }

    pub(crate) fn exhausted_limit(&self) -> Option<CampaignLimitKind> {
        [
            (
                self.utilization.provider_attempts >= self.limits.provider_attempts,
                CampaignLimitKind::ProviderAttempts,
            ),
            (
                self.utilization.malformed_report_repairs >= self.limits.malformed_report_repairs,
                CampaignLimitKind::MalformedReportRepairs,
            ),
            (
                self.utilization.correction_rounds >= self.limits.correction_rounds,
                CampaignLimitKind::CorrectionRounds,
            ),
            (
                self.utilization.process_invocations >= self.limits.process_invocations,
                CampaignLimitKind::ProcessInvocations,
            ),
            (
                self.utilization.output_bytes >= self.limits.output_bytes,
                CampaignLimitKind::OutputBytes,
            ),
            (
                self.utilization.retained_state_bytes >= self.limits.retained_state_bytes,
                CampaignLimitKind::RetainedStateBytes,
            ),
            (
                self.utilization.execution_elapsed_ms >= self.limits.execution_elapsed_ms,
                CampaignLimitKind::ExecutionElapsed,
            ),
        ]
        .into_iter()
        .find_map(|(exhausted, limit)| exhausted.then_some(limit))
    }

    pub(crate) fn unit_budget(&self, campaign_root: &Path) -> CampaignUnitBudget {
        CampaignUnitBudget {
            baseline: self.utilization.clone(),
            limits: self.limits.clone(),
            campaign_root: campaign_root.to_path_buf(),
        }
    }

    pub(crate) fn apply_control(
        &mut self,
        intent: &CampaignControlIntent,
    ) -> Result<bool, RuntimeError> {
        if self.applied_control_requests.contains(&intent.request_id) {
            return Ok(false);
        }
        if intent.authority_sha256 != self.authority_sha256
            || intent.campaign_id != self.campaign_id
            || intent.repository_id != self.repository_id
            || intent.campaign_run_id != self.campaign_run_id
            || intent.observed_updated_at_ms > self.updated_at_ms
            || self.applied_control_requests.len() >= 10_000
        {
            return Err(RuntimeError::Authority);
        }
        match intent.action {
            CampaignControlAction::Pause if !self.cancelled && !self.operator_paused => {
                self.operator_paused = true;
            }
            CampaignControlAction::Resume
                if !self.cancelled && (self.operator_paused || self.stop_after_unit) =>
            {
                self.operator_paused = false;
                self.stop_after_unit = false;
                self.resume_validation = ResumeValidationState::Pending;
            }
            CampaignControlAction::StopAfterUnit if !self.cancelled && !self.stop_after_unit => {
                self.stop_after_unit = true;
            }
            CampaignControlAction::Cancel if !self.cancelled => {
                self.cancelled = true;
                self.operator_paused = false;
                self.stop_after_unit = false;
                self.resume_validation = ResumeValidationState::NotRequired;
            }
            _ => return Err(RuntimeError::Authority),
        }
        self.applied_control_requests
            .insert(intent.request_id.clone());
        Ok(true)
    }

    pub(crate) fn reconcile_control_journal(&self, root: &Path) -> Result<(), RuntimeError> {
        let intents = CampaignControlIntent::pending(root)?;
        let by_id = intents
            .iter()
            .map(|intent| (intent.request_id.as_str(), intent))
            .collect::<BTreeMap<_, _>>();
        for intent in &intents {
            if intent.authority_sha256 != self.authority_sha256
                || intent.campaign_id != self.campaign_id
                || intent.repository_id != self.repository_id
                || intent.campaign_run_id != self.campaign_run_id
                || intent.observed_updated_at_ms > self.updated_at_ms
            {
                return Err(RuntimeError::Authority);
            }
        }

        let mut journal = Journal::open(root, format!("{}-control", self.campaign_run_id.as_str()))
            .map_err(|_| RuntimeError::State)?;
        let mut requested = BTreeSet::new();
        let mut applied = BTreeSet::new();
        for record in journal.records() {
            let (request_id, action, set) = match &record.event.kind {
                EventKind::ControlRequested { request_id, action } => {
                    (request_id, action, &mut requested)
                }
                EventKind::ControlApplied { request_id, action } => {
                    (request_id, action, &mut applied)
                }
                _ => continue,
            };
            if record.event.task_id.as_str() != "campaign-control" {
                continue;
            }
            let intent = by_id.get(request_id.as_str()).ok_or(RuntimeError::State)?;
            if record.event.run_id != self.campaign_run_id
                || record.event.repository_id.as_str() != self.repository_id
                || action != intent.action.code()
                || !set.insert(request_id.clone())
            {
                return Err(RuntimeError::State);
            }
        }
        if !applied.is_subset(&requested)
            || applied
                .iter()
                .any(|request_id| !self.applied_control_requests.contains(request_id))
        {
            return Err(RuntimeError::State);
        }

        for intent in &intents {
            if !requested.contains(&intent.request_id) {
                journal
                    .append(self.control_event(intent, false)?)
                    .map_err(|_| RuntimeError::State)?;
            }
        }
        for request_id in &self.applied_control_requests {
            let intent = by_id.get(request_id.as_str()).ok_or(RuntimeError::State)?;
            if !applied.contains(request_id) {
                journal
                    .append(self.control_event(intent, true)?)
                    .map_err(|_| RuntimeError::State)?;
            }
        }
        Ok(())
    }

    fn control_event(
        &self,
        intent: &CampaignControlIntent,
        applied: bool,
    ) -> Result<JournalEvent, RuntimeError> {
        Ok(JournalEvent {
            timestamp_ms: if applied {
                self.updated_at_ms
            } else {
                intent.created_at_ms
            },
            run_id: self.campaign_run_id.clone(),
            task_id: TaskId::new("campaign-control").map_err(|_| RuntimeError::State)?,
            repository_id: RepositoryId::new(self.repository_id.clone())
                .map_err(|_| RuntimeError::State)?,
            identities: DurableIdentities {
                worktree: Some(self.worktree_id.clone()),
                branch: Some(self.branch.clone()),
                commit: Some(if applied {
                    self.head.clone()
                } else {
                    intent.observed_head.clone()
                }),
                ..DurableIdentities::default()
            },
            kind: if applied {
                EventKind::ControlApplied {
                    request_id: intent.request_id.clone(),
                    action: intent.action.code().to_owned(),
                }
            } else {
                EventKind::ControlRequested {
                    request_id: intent.request_id.clone(),
                    action: intent.action.code().to_owned(),
                }
            },
            outcome: EventOutcome::Succeeded,
            evidence: vec![
                EvidenceId::new(format!("control-{}", intent.request_id))
                    .map_err(|_| RuntimeError::State)?,
            ],
            redactions: Vec::new(),
        })
    }

    fn refresh_outcomes(&mut self) -> Result<(), RuntimeError> {
        let projection = CampaignOutcomeProjection {
            completed: self.completed_units,
            blocked: u32::try_from(self.blocked_task_ids.len()).map_err(|_| RuntimeError::State)?,
            deferred: u32::try_from(self.deferred_tasks.len()).map_err(|_| RuntimeError::State)?,
            pending_human_decision: u32::try_from(self.human_decisions.len())
                .map_err(|_| RuntimeError::State)?,
            rejected_proposals: u32::try_from(self.rejected_proposals.len())
                .map_err(|_| RuntimeError::State)?,
            accepted: 0,
            max_accepted: self.outcomes.max_accepted,
        };
        let accepted = projection
            .completed
            .checked_add(projection.blocked)
            .and_then(|value| value.checked_add(projection.deferred))
            .and_then(|value| value.checked_add(projection.pending_human_decision))
            .ok_or(RuntimeError::State)?;
        self.outcomes = CampaignOutcomeProjection {
            accepted,
            ..projection
        };
        self.validate_outcomes()
    }

    fn validate_outcomes(&self) -> Result<(), RuntimeError> {
        let blocked = self.blocked_task_ids.iter().collect::<BTreeSet<_>>();
        let blocked_reasons = self.blocked_reasons.keys().collect::<BTreeSet<_>>();
        let deferred = self.deferred_tasks.keys().collect::<BTreeSet<_>>();
        let satisfied = self.satisfied_deferrals.keys().collect::<BTreeSet<_>>();
        let human_decisions = self.human_decisions.keys().collect::<BTreeSet<_>>();
        let expected = self
            .outcomes
            .completed
            .checked_add(self.outcomes.blocked)
            .and_then(|value| value.checked_add(self.outcomes.deferred))
            .and_then(|value| value.checked_add(self.outcomes.pending_human_decision))
            .ok_or(RuntimeError::State)?;
        if self.outcomes.completed != self.completed_units
            || self.outcomes.blocked
                != u32::try_from(self.blocked_task_ids.len()).map_err(|_| RuntimeError::State)?
            || self.outcomes.deferred
                != u32::try_from(self.deferred_tasks.len()).map_err(|_| RuntimeError::State)?
            || self.outcomes.pending_human_decision
                != u32::try_from(self.human_decisions.len()).map_err(|_| RuntimeError::State)?
            || self.outcomes.rejected_proposals
                != u32::try_from(self.rejected_proposals.len()).map_err(|_| RuntimeError::State)?
            || self.outcomes.accepted != expected
            || self.outcomes.max_accepted == 0
            || self.outcomes.accepted > self.outcomes.max_accepted
            || self.applied_control_requests.len() > 10_000
            || self.cancelled && (self.operator_paused || self.stop_after_unit)
            || self.cancelled && self.resume_validation == ResumeValidationState::Pending
            || blocked != blocked_reasons
            || !blocked.is_disjoint(&deferred)
            || !blocked.is_disjoint(&human_decisions)
            || !deferred.is_disjoint(&satisfied)
            || !deferred.is_disjoint(&human_decisions)
            || blocked
                .iter()
                .chain(deferred.iter())
                .chain(satisfied.iter())
                .chain(human_decisions.iter())
                .any(|task_id| TaskId::new((*task_id).clone()).is_err())
            || self
                .deferred_tasks
                .values()
                .chain(self.satisfied_deferrals.values())
                .any(|projection| projection.reason.required_trigger() != projection.trigger)
        {
            return Err(RuntimeError::State);
        }
        Ok(())
    }

    pub(crate) fn record_provider_attempt(&mut self) -> Result<(), RuntimeError> {
        self.utilization.provider_attempts = self
            .utilization
            .provider_attempts
            .checked_add(1)
            .ok_or(RuntimeError::State)?;
        self.utilization.process_invocations = self
            .utilization
            .process_invocations
            .checked_add(1)
            .ok_or(RuntimeError::State)?;
        Ok(())
    }

    pub(crate) fn record_process_result(
        &mut self,
        result: &codingmage_process::ProcessResult,
    ) -> Result<(), RuntimeError> {
        let output = result
            .stdout
            .total_bytes
            .checked_add(result.stderr.total_bytes)
            .ok_or(RuntimeError::State)?;
        self.utilization.output_bytes = self
            .utilization
            .output_bytes
            .checked_add(output)
            .ok_or(RuntimeError::State)?;
        let elapsed = u64::try_from(result.elapsed.as_millis()).map_err(|_| RuntimeError::State)?;
        self.utilization.execution_elapsed_ms = self
            .utilization
            .execution_elapsed_ms
            .checked_add(elapsed)
            .ok_or(RuntimeError::State)?;
        Ok(())
    }

    pub(crate) fn record_unit_utilization(
        &mut self,
        run: &RunUtilization,
        correction_rounds: u16,
    ) -> Result<(), RuntimeError> {
        self.utilization.provider_attempts = self
            .utilization
            .provider_attempts
            .checked_add(run.provider_attempts)
            .ok_or(RuntimeError::State)?;
        self.utilization.malformed_report_repairs = self
            .utilization
            .malformed_report_repairs
            .checked_add(run.malformed_report_repairs)
            .ok_or(RuntimeError::State)?;
        self.utilization.correction_rounds = self
            .utilization
            .correction_rounds
            .checked_add(u32::from(correction_rounds))
            .ok_or(RuntimeError::State)?;
        self.utilization.process_invocations = self
            .utilization
            .process_invocations
            .checked_add(run.process_invocations)
            .ok_or(RuntimeError::State)?;
        self.utilization.output_bytes = self
            .utilization
            .output_bytes
            .checked_add(run.output_bytes)
            .ok_or(RuntimeError::State)?;
        self.utilization.execution_elapsed_ms = self
            .utilization
            .execution_elapsed_ms
            .checked_add(run.execution_elapsed_ms)
            .ok_or(RuntimeError::State)?;
        Ok(())
    }
}

fn projection_sha256<T: Serialize>(projection: &T) -> Result<String, RuntimeError> {
    serde_json::to_vec(projection)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|_| RuntimeError::State)
}

impl BlockerClearanceIntent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request_id: String,
        campaign_id: String,
        repository_id: String,
        task_id: String,
        blocked_reason: LeadBlockedReason,
        campaign_head: String,
        task_source_sha256: String,
        prerequisite_sha256: String,
    ) -> Self {
        Self {
            schema_version: CLEARANCE_SCHEMA_VERSION,
            request_id,
            campaign_id,
            repository_id,
            task_id,
            blocked_reason,
            campaign_head,
            task_source_sha256,
            prerequisite_sha256,
        }
    }

    pub(crate) fn load(root: &Path, request_id: &str) -> Result<Option<Self>, RuntimeError> {
        let path = clearance_path(root, request_id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeError::State),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_CHECKPOINT_BYTES as u64
        {
            return Err(RuntimeError::State);
        }
        let bytes = fs::read(path).map_err(|_| RuntimeError::State)?;
        let envelope: BlockerClearanceEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
        let canonical = serde_json::to_vec(&envelope.intent).map_err(|_| RuntimeError::State)?;
        if envelope.intent.schema_version != CLEARANCE_SCHEMA_VERSION
            || envelope.intent.request_id != request_id
            || sha256_hex(&canonical) != envelope.intent_sha256
        {
            return Err(RuntimeError::State);
        }
        Ok(Some(envelope.intent))
    }

    pub(crate) fn persist_new(&self, root: &Path) -> Result<(), RuntimeError> {
        let clearances = root.join("blocker-clearances");
        private_directory(&clearances)?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = BlockerClearanceEnvelope {
            intent: self.clone(),
            intent_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(clearance_path(root, &self.request_id))
            .map_err(|_| RuntimeError::State)?;
        set_file_private(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RuntimeError::State)?;
        File::open(&clearances)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)
    }
}

impl DeferralTriggerIntent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request_id: String,
        campaign_id: String,
        repository_id: String,
        task_id: String,
        projection: &DeferredTaskProjection,
        evidence_sha256: String,
    ) -> Self {
        Self {
            schema_version: CLEARANCE_SCHEMA_VERSION,
            request_id,
            campaign_id,
            repository_id,
            task_id,
            reason: projection.reason,
            trigger: projection.trigger,
            campaign_head: projection.source_head.clone(),
            task_source_sha256: projection.task_source_sha256.clone(),
            evidence_sha256,
        }
    }

    pub(crate) fn load(root: &Path, request_id: &str) -> Result<Option<Self>, RuntimeError> {
        let path = trigger_path(root, request_id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeError::State),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_CHECKPOINT_BYTES as u64
        {
            return Err(RuntimeError::State);
        }
        let bytes = fs::read(path).map_err(|_| RuntimeError::State)?;
        let envelope: DeferralTriggerEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
        let canonical = serde_json::to_vec(&envelope.intent).map_err(|_| RuntimeError::State)?;
        if envelope.intent.schema_version != CLEARANCE_SCHEMA_VERSION
            || envelope.intent.request_id != request_id
            || sha256_hex(&canonical) != envelope.intent_sha256
        {
            return Err(RuntimeError::State);
        }
        Ok(Some(envelope.intent))
    }

    pub(crate) fn persist_new(&self, root: &Path) -> Result<(), RuntimeError> {
        let observations = root.join("deferral-trigger-observations");
        private_directory(&observations)?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = DeferralTriggerEnvelope {
            intent: self.clone(),
            intent_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(trigger_path(root, &self.request_id))
            .map_err(|_| RuntimeError::State)?;
        set_file_private(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RuntimeError::State)?;
        File::open(&observations)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)
    }
}

impl CampaignControlIntent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request_id: String,
        authority_sha256: String,
        campaign_id: String,
        repository_id: String,
        campaign_run_id: RunId,
        action: CampaignControlAction,
        observed_head: String,
        observed_updated_at_ms: u64,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            schema_version: 1,
            request_id,
            authority_sha256,
            campaign_id,
            repository_id,
            campaign_run_id,
            action,
            observed_head,
            observed_updated_at_ms,
            created_at_ms: timestamp_ms()?,
        })
    }

    pub(crate) fn load(root: &Path, request_id: &str) -> Result<Option<Self>, RuntimeError> {
        load_control_path(&control_path(root, request_id), Some(request_id))
    }

    pub(crate) fn pending(root: &Path) -> Result<Vec<Self>, RuntimeError> {
        let directory = root.join("control-requests");
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => validate_private_control_entry(&metadata, true)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(RuntimeError::State),
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(RuntimeError::State),
        };
        let mut intents = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| RuntimeError::State)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return Err(RuntimeError::State);
            }
            intents.push(load_control_path(&path, None)?.ok_or(RuntimeError::State)?);
            if intents.len() > 10_000 {
                return Err(RuntimeError::State);
            }
        }
        intents.sort_by(|left, right| {
            (left.created_at_ms, left.request_id.as_str())
                .cmp(&(right.created_at_ms, right.request_id.as_str()))
        });
        Ok(intents)
    }

    pub(crate) fn persist_new(&self, root: &Path) -> Result<(), RuntimeError> {
        let directory = root.join("control-requests");
        private_directory(&directory)?;
        validate_private_control_entry(
            &fs::symlink_metadata(&directory).map_err(|_| RuntimeError::State)?,
            true,
        )?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = CampaignControlEnvelope {
            intent: self.clone(),
            intent_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let temporary = directory.join(format!(".{}.tmp", self.request_id));
        let destination = control_path(root, &self.request_id);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| RuntimeError::State)?;
        set_file_private(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RuntimeError::State)?;
        if fs::hard_link(&temporary, &destination).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(RuntimeError::State);
        }
        fs::remove_file(&temporary).map_err(|_| RuntimeError::State)?;
        File::open(&directory)
            .and_then(|handle| handle.sync_all())
            .map_err(|_| RuntimeError::State)
    }
}

fn load_control_path(
    path: &Path,
    expected_request_id: Option<&str>,
) -> Result<Option<CampaignControlIntent>, RuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(RuntimeError::State),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_CHECKPOINT_BYTES as u64
    {
        return Err(RuntimeError::State);
    }
    validate_private_control_entry(&metadata, false)?;
    let bytes = fs::read(path).map_err(|_| RuntimeError::State)?;
    let envelope: CampaignControlEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
    let canonical = serde_json::to_vec(&envelope.intent).map_err(|_| RuntimeError::State)?;
    if envelope.intent.schema_version != 1
        || expected_request_id.is_some_and(|value| envelope.intent.request_id != value)
        || sha256_hex(&canonical) != envelope.intent_sha256
    {
        return Err(RuntimeError::State);
    }
    Ok(Some(envelope.intent))
}

fn clearance_path(root: &Path, request_id: &str) -> PathBuf {
    root.join("blocker-clearances")
        .join(format!("{request_id}.json"))
}

fn trigger_path(root: &Path, request_id: &str) -> PathBuf {
    root.join("deferral-trigger-observations")
        .join(format!("{request_id}.json"))
}

fn control_path(root: &Path, request_id: &str) -> PathBuf {
    root.join("control-requests")
        .join(format!("{request_id}.json"))
}

pub(crate) fn validate_private_campaign_state(root: &Path) -> Result<(), RuntimeError> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let effective = fs::read_to_string("/proc/self/status")
            .map_err(|_| RuntimeError::Authority)?
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or(RuntimeError::Authority)?;
        for path in [root, &root.join("checkpoint.json")] {
            let metadata = fs::symlink_metadata(path).map_err(|_| RuntimeError::State)?;
            if metadata.file_type().is_symlink()
                || metadata.uid() != effective
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(RuntimeError::Authority);
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        Err(RuntimeError::Authority)
    }
}

impl CampaignPhase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Planning => "planning",
            Self::RunningUnit => "running_unit",
            Self::Integrating => "integrating",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) const fn actor(self) -> &'static str {
        match self {
            Self::Planning => "codex-lead",
            Self::RunningUnit => "pod",
            Self::Integrating => "integration",
            Self::Ready | Self::Paused | Self::Blocked | Self::Complete | Self::Cancelled => {
                "coordinator"
            }
        }
    }
}

fn timestamp_ms() -> Result<u64, RuntimeError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeError::State)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| RuntimeError::State)
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn private_directory(path: &Path) -> Result<(), RuntimeError> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (!metadata.is_dir() || metadata.file_type().is_symlink())
    {
        return Err(RuntimeError::Authority);
    }
    fs::create_dir_all(path).map_err(|_| RuntimeError::State)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| RuntimeError::State)?;
    }
    Ok(())
}

fn validate_private_control_entry(
    metadata: &fs::Metadata,
    directory: bool,
) -> Result<(), RuntimeError> {
    if metadata.file_type().is_symlink()
        || directory != metadata.is_dir()
        || !directory && !metadata.is_file()
    {
        return Err(RuntimeError::Authority);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let effective = fs::read_to_string("/proc/self/status")
            .map_err(|_| RuntimeError::Authority)?
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or(RuntimeError::Authority)?;
        if metadata.uid() != effective || metadata.permissions().mode() & 0o077 != 0 {
            return Err(RuntimeError::Authority);
        }
    }
    Ok(())
}

fn retained_tree_bytes(root: &Path) -> Result<u64, RuntimeError> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|_| RuntimeError::State)? {
            let entry = entry.map_err(|_| RuntimeError::State)?;
            let file_type = entry.file_type().map_err(|_| RuntimeError::State)?;
            if file_type.is_symlink() {
                return Err(RuntimeError::State);
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = checked_retained_total(
                    total,
                    entry.metadata().map_err(|_| RuntimeError::State)?.len(),
                )?;
            } else {
                return Err(RuntimeError::State);
            }
        }
    }
    Ok(total)
}

fn checked_retained_total(total: u64, next: u64) -> Result<u64, RuntimeError> {
    total.checked_add(next).ok_or(RuntimeError::State)
}

#[cfg(unix)]
fn set_file_private(file: &File) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| RuntimeError::State)
}

#[cfg(not(unix))]
fn set_file_private(_file: &File) -> Result<(), RuntimeError> {
    Err(RuntimeError::State)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_campaign::{
        CampaignAuthentication, CampaignGateTier, CampaignProvider, CampaignPublication,
        CampaignSpec,
    };

    fn campaign_limits() -> CampaignLimits {
        CampaignLimits {
            provider_attempts: 1_000,
            malformed_report_repairs: 100,
            correction_rounds: 100,
            process_invocations: 10_000,
            output_bytes: 1_073_741_824,
            retained_state_bytes: 1_073_741_824,
            execution_elapsed_ms: 86_400_000,
        }
    }

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codingmage-campaign-checkpoint-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn checkpoint() -> CampaignCheckpoint {
        CampaignCheckpoint::new(
            "a".repeat(64),
            "campaign-1".to_owned(),
            "repo-1".to_owned(),
            RunId::new("run-1").unwrap(),
            WorktreeId::new("wt-1").unwrap(),
            "codingmage/campaign-1/campaign-root".to_owned(),
            "b".repeat(40),
            10,
            campaign_limits(),
        )
        .unwrap()
    }

    fn campaign_policy_spec() -> CampaignSpec {
        let provider = |name: &str, effort: &str| CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: name.to_owned(),
            effort: effort.to_owned(),
        };
        CampaignSpec {
            version: 3,
            campaign_id: "campaign-1".to_owned(),
            repository_id: "repo-1".to_owned(),
            repository_path: PathBuf::from("/tmp/codingmage-policy-target"),
            initial_commit: "b".repeat(40),
            task_source_sha256: "c".repeat(64),
            operator_authorization_sha256: "d".repeat(64),
            max_parallel_pods: 1,
            max_units: 10,
            limits: campaign_limits(),
            team_lead: provider("lead-model", "high"),
            implementer: provider("implementer-model", "high"),
            implementer_authentication: CampaignAuthentication::ExistingLogin,
            reviewer: provider("reviewer-model", "xhigh"),
            gate_tiers: vec![CampaignGateTier {
                name: "required".to_owned(),
                profiles: vec!["test".to_owned(), "clippy".to_owned()],
            }],
            campaign_branch: "codingmage/campaign-1".to_owned(),
            allowed_paths: vec![PathBuf::from("crates/public")],
            denied_paths: vec![PathBuf::from("crates/private")],
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
    enum ControlCrashBoundary {
        IntentPersisted,
        RequestObserved,
        StatePersisted,
        AppliedObserved,
    }

    fn assert_control_effect(
        checkpoint: &CampaignCheckpoint,
        action: CampaignControlAction,
        request_id: &str,
    ) {
        assert_eq!(checkpoint.applied_control_requests.len(), 1);
        assert!(checkpoint.applied_control_requests.contains(request_id));
        match action {
            CampaignControlAction::Pause => {
                assert!(checkpoint.operator_paused);
                assert!(!checkpoint.stop_after_unit);
                assert!(!checkpoint.cancelled);
                assert_eq!(
                    checkpoint.resume_validation,
                    ResumeValidationState::NotRequired
                );
            }
            CampaignControlAction::Resume => {
                assert!(!checkpoint.operator_paused);
                assert!(!checkpoint.stop_after_unit);
                assert!(!checkpoint.cancelled);
                assert_eq!(checkpoint.resume_validation, ResumeValidationState::Pending);
            }
            CampaignControlAction::StopAfterUnit => {
                assert!(!checkpoint.operator_paused);
                assert!(checkpoint.stop_after_unit);
                assert!(!checkpoint.cancelled);
                assert_eq!(
                    checkpoint.resume_validation,
                    ResumeValidationState::NotRequired
                );
            }
            CampaignControlAction::Cancel => {
                assert!(!checkpoint.operator_paused);
                assert!(!checkpoint.stop_after_unit);
                assert!(checkpoint.cancelled);
                assert_eq!(
                    checkpoint.resume_validation,
                    ResumeValidationState::NotRequired
                );
            }
        }
    }

    fn weakened_policy_mutations(baseline: &CampaignSpec) -> Vec<CampaignSpec> {
        let mut mutations = Vec::new();
        for provider in ["team_lead", "implementer", "reviewer"] {
            let mut value = baseline.clone();
            match provider {
                "team_lead" => value.team_lead.effort = "low".to_owned(),
                "implementer" => value.implementer.effort = "low".to_owned(),
                "reviewer" => value.reviewer.effort = "low".to_owned(),
                _ => unreachable!(),
            }
            mutations.push(value);
        }
        let mut value = baseline.clone();
        value.gate_tiers[0].profiles = vec!["test".to_owned()];
        mutations.push(value);
        let mut value = baseline.clone();
        value.allowed_paths = vec![PathBuf::from("src")];
        value.denied_paths = vec![PathBuf::from("private")];
        mutations.push(value);
        let mut value = baseline.clone();
        value.publication = CampaignPublication::DraftStoryPullRequests;
        mutations.push(value);
        mutations
    }

    fn assert_checkpoint_refuses_weakened_policy(
        checkpoint: &CampaignCheckpoint,
        baseline: &CampaignSpec,
    ) {
        let authority_sha256 = baseline.authority_sha256().unwrap();
        checkpoint
            .validate_authority(
                &authority_sha256,
                &baseline.campaign_id,
                &baseline.repository_id,
                &baseline.initial_commit,
                baseline.max_units,
                &baseline.limits,
            )
            .unwrap();
        for mutation in weakened_policy_mutations(baseline) {
            mutation.verify().unwrap();
            let changed_authority = mutation.authority_sha256().unwrap();
            assert_ne!(changed_authority, authority_sha256);
            assert_eq!(
                checkpoint.validate_authority(
                    &changed_authority,
                    &mutation.campaign_id,
                    &mutation.repository_id,
                    &mutation.initial_commit,
                    mutation.max_units,
                    &mutation.limits,
                ),
                Err(RuntimeError::Authority)
            );
        }
    }

    fn matrix_limits(exhausted: CampaignLimitKind) -> CampaignLimits {
        let mut limits = CampaignLimits {
            provider_attempts: 10,
            malformed_report_repairs: 10,
            correction_rounds: 10,
            process_invocations: 10,
            output_bytes: 10,
            retained_state_bytes: 10,
            execution_elapsed_ms: 10,
        };
        match exhausted {
            CampaignLimitKind::ProviderAttempts => limits.provider_attempts = 3,
            CampaignLimitKind::MalformedReportRepairs => {
                limits.malformed_report_repairs = 3;
            }
            CampaignLimitKind::CorrectionRounds => limits.correction_rounds = 3,
            CampaignLimitKind::ProcessInvocations => limits.process_invocations = 3,
            CampaignLimitKind::OutputBytes => limits.output_bytes = 3,
            CampaignLimitKind::RetainedStateBytes => limits.retained_state_bytes = 3,
            CampaignLimitKind::ExecutionElapsed => limits.execution_elapsed_ms = 3,
        }
        limits
    }

    fn limit_usage(exhausted: CampaignLimitKind, value: u32) -> RunUtilization {
        let mut usage = RunUtilization::default();
        match exhausted {
            CampaignLimitKind::ProviderAttempts => {
                usage.provider_attempts = value;
                usage.process_invocations = value;
            }
            CampaignLimitKind::MalformedReportRepairs => {
                usage.provider_attempts = value;
                usage.malformed_report_repairs = value;
                usage.process_invocations = value;
            }
            CampaignLimitKind::ProcessInvocations => usage.process_invocations = value,
            CampaignLimitKind::OutputBytes => usage.output_bytes = u64::from(value),
            CampaignLimitKind::ExecutionElapsed => {
                usage.execution_elapsed_ms = u64::from(value);
            }
            CampaignLimitKind::CorrectionRounds | CampaignLimitKind::RetainedStateBytes => {}
        }
        usage
    }

    fn limit_baseline(exhausted: CampaignLimitKind, value: u32) -> CampaignUtilization {
        let usage = limit_usage(exhausted, value);
        CampaignUtilization {
            provider_attempts: usage.provider_attempts,
            malformed_report_repairs: usage.malformed_report_repairs,
            correction_rounds: if exhausted == CampaignLimitKind::CorrectionRounds {
                value
            } else {
                0
            },
            process_invocations: usage.process_invocations,
            output_bytes: usage.output_bytes,
            retained_state_bytes: 0,
            execution_elapsed_ms: usage.execution_elapsed_ms,
        }
    }

    fn limit_reservation(exhausted: CampaignLimitKind, value: u32) -> CampaignReservation {
        let mut reservation = CampaignReservation::default();
        match exhausted {
            CampaignLimitKind::ProviderAttempts => {
                reservation.provider_attempts = value;
                reservation.process_invocations = value;
            }
            CampaignLimitKind::MalformedReportRepairs => {
                reservation.provider_attempts = value;
                reservation.malformed_report_repairs = value;
                reservation.process_invocations = value;
            }
            CampaignLimitKind::CorrectionRounds => reservation.correction_rounds = value,
            CampaignLimitKind::ProcessInvocations => reservation.process_invocations = value,
            CampaignLimitKind::OutputBytes
            | CampaignLimitKind::RetainedStateBytes
            | CampaignLimitKind::ExecutionElapsed => {}
        }
        reservation
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn checkpoint_round_trips_and_rejects_tampering() {
        let root = root("round-trip");
        let prohibited_log_content = [
            "prompt_secret_marker",
            "source_text_secret_marker",
            "filename_secret_marker",
            "provider_prose_secret_marker",
            "command_output_secret_marker",
            "environment_value_secret_marker",
            "credential_secret_marker",
            "hidden_reasoning_secret_marker",
        ];
        let mut checkpoint = checkpoint();
        checkpoint.phase = CampaignPhase::Integrating;
        checkpoint.blocked_task_ids.insert("1.1.1.2".to_owned());
        checkpoint.blocked_reasons.insert(
            "1.1.1.2".to_owned(),
            LeadBlockedReason::UnavailableExternalDependency,
        );
        checkpoint.deferred_tasks.insert(
            "1.1.1.3".to_owned(),
            DeferredTaskProjection {
                reason: LeadDeferredReason::DeterministicDependencyOrder,
                trigger: LeadReconsiderationTrigger::CampaignHeadAdvancement,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            },
        );
        checkpoint.satisfied_deferrals.insert(
            "1.1.1.5".to_owned(),
            DeferredTaskProjection {
                reason: LeadDeferredReason::DeterministicDependencyOrder,
                trigger: LeadReconsiderationTrigger::CampaignHeadAdvancement,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            },
        );
        checkpoint.human_decisions.insert(
            "1.1.1.4".to_owned(),
            HumanDecisionProjection {
                reason: HumanDecisionProjectionReason::RepeatedSatisfiedDeferral,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            },
        );
        checkpoint
            .rejected_proposals
            .push(RejectedProposalProjection {
                sequence: 1,
                reason: LeadRejectionReason::InvalidProposal,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            });
        checkpoint
            .record_unit_utilization(
                &RunUtilization {
                    provider_attempts: 2,
                    malformed_report_repairs: 1,
                    process_invocations: 4,
                    output_bytes: 12,
                    execution_elapsed_ms: 8,
                    retained_state_bytes: 6,
                },
                2,
            )
            .unwrap();
        checkpoint.active_unit = Some(ActiveUnit {
            task_id: "1.1.1.1".to_owned(),
            source_head: "b".repeat(40),
            task_source_sha256: "c".repeat(64),
            owned_paths: prohibited_log_content.iter().map(PathBuf::from).collect(),
            run_id: Some(RunId::new("run-pod-1").unwrap()),
        });
        checkpoint.pending_integration = Some(PendingIntegration {
            task_id: "1.1.1.1".to_owned(),
            expected_head: "b".repeat(40),
            target_head: "c".repeat(40),
            owned_paths: prohibited_log_content.iter().map(PathBuf::from).collect(),
        });
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            CampaignCheckpoint::load(&root).unwrap(),
            Some(checkpoint.clone())
        );

        let journal = Journal::open(&root, "checkpoint-reader").unwrap();
        assert_eq!(journal.records().len(), 1);
        let record = &journal.records()[0];
        assert_eq!(record.event.run_id.as_str(), "run-1");
        assert_eq!(record.event.task_id.as_str(), "1.1.1.1");
        assert_eq!(record.event.repository_id.as_str(), "repo-1");
        assert_eq!(
            record.event.identities.worktree.as_ref().unwrap().as_str(),
            "wt-1"
        );
        assert_eq!(record.event.identities.commit, Some("b".repeat(40)));
        let EventKind::CampaignCheckpointed { projection } = &record.event.kind else {
            panic!("expected a campaign checkpoint event");
        };
        assert_eq!(
            projection.as_ref(),
            &checkpoint.journal_projection().unwrap()
        );
        assert!(matches!(
            projection.as_ref(),
            CampaignCheckpointProjection {
                    phase,
                    queue_sha256,
                    completed_units: 0,
                    blocked_tasks: 1,
                    deferred_tasks: 1,
                    satisfied_deferrals: 1,
                    human_decisions: 1,
                    rejected_proposals: 1,
                    accepted_outcomes: 3,
                    max_outcomes: Some(10),
                    active_unit: true,
                    active_pod_run_id: Some(active_run),
                    active_pod_sha256: Some(active_pod_sha256),
                    pending_integration_sha256: Some(pending_integration_sha256),
                    blocker_projection_sha256,
                    deferral_projection_sha256,
                    trigger_projection_sha256,
                    control_projection_sha256,
                    completion_projection_sha256,
                    provider_attempts: Some(2),
                    max_provider_attempts: Some(1_000),
                    malformed_report_repairs: Some(1),
                    max_malformed_report_repairs: Some(100),
                    correction_round: Some(2),
                    max_correction_rounds: Some(100),
                    process_invocations: Some(4),
                    max_process_invocations: Some(10_000),
                    output_bytes: Some(12),
                    max_output_bytes: Some(1_073_741_824),
                    retained_state_bytes: Some(0),
                    max_retained_state_bytes: Some(1_073_741_824),
                    execution_elapsed_ms: Some(8),
                    max_execution_elapsed_ms: Some(86_400_000),
                    operator_paused: Some(false),
                    stop_after_unit: Some(false),
                    cancelled: Some(false),
                    resume_validation: Some(resume),
            } if phase == "integrating"
                && resume == "not_required"
                && active_run == "run-pod-1"
                && [
                    queue_sha256,
                    active_pod_sha256,
                    pending_integration_sha256,
                    blocker_projection_sha256,
                    deferral_projection_sha256,
                    trigger_projection_sha256,
                    control_projection_sha256,
                    completion_projection_sha256,
                ]
                .iter()
                .all(|digest| digest.len() == 64)
        ));
        assert_eq!(record.event.evidence.len(), 1);
        assert_eq!(record.event.redactions.len(), 5);
        let journal_bytes = fs::read(root.join("events.jsonl")).unwrap();
        for marker in prohibited_log_content {
            assert!(
                !journal_bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
            );
        }
        drop(journal);

        let path = root.join("checkpoint.json");
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'c').unwrap();
        bytes[index] = b'd';
        fs::write(path, bytes).unwrap();
        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_refuses_self_consistent_outcome_projection_mutations() {
        let root = root("outcome-mutations");
        let mut checkpoint = checkpoint();
        checkpoint.completed_units = 1;
        checkpoint.blocked_task_ids.insert("1.1.1.2".to_owned());
        checkpoint.blocked_reasons.insert(
            "1.1.1.2".to_owned(),
            LeadBlockedReason::UnavailableExternalDependency,
        );
        checkpoint.deferred_tasks.insert(
            "1.1.1.3".to_owned(),
            DeferredTaskProjection {
                reason: LeadDeferredReason::OperatorPause,
                trigger: LeadReconsiderationTrigger::OperatorResume,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            },
        );
        checkpoint.human_decisions.insert(
            "1.1.1.4".to_owned(),
            HumanDecisionProjection {
                reason: HumanDecisionProjectionReason::RepeatedSatisfiedDeferral,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            },
        );
        checkpoint
            .rejected_proposals
            .push(RejectedProposalProjection {
                sequence: 1,
                reason: LeadRejectionReason::MalformedOutput,
                source_head: "b".repeat(40),
                task_source_sha256: "c".repeat(64),
            });
        checkpoint.persist(&root).unwrap();
        let bytes = fs::read(root.join("checkpoint.json")).unwrap();
        let envelope: CheckpointEnvelope = serde_json::from_slice(&bytes).unwrap();

        let mut mutations = Vec::new();
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.completed += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.blocked += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.deferred += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.pending_human_decision += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.rejected_proposals += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.accepted += 1;
        mutations.push(changed);
        let mut changed = envelope.checkpoint.clone();
        changed.outcomes.max_accepted = 0;
        mutations.push(changed);
        let mut changed = envelope.checkpoint;
        changed.outcomes.max_accepted = 3;
        mutations.push(changed);

        for changed in mutations {
            let canonical = serde_json::to_vec(&changed).unwrap();
            let changed = CheckpointEnvelope {
                checkpoint: changed,
                checkpoint_sha256: sha256_hex(&canonical),
            };
            fs::write(
                root.join("checkpoint.json"),
                serde_json::to_vec_pretty(&changed).unwrap(),
            )
            .unwrap();
            assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_binds_original_authority() {
        let checkpoint = checkpoint();
        checkpoint
            .validate_authority(
                &"a".repeat(64),
                "campaign-1",
                "repo-1",
                &"b".repeat(40),
                10,
                &campaign_limits(),
            )
            .unwrap();
        assert_eq!(
            checkpoint.validate_authority(
                &"c".repeat(64),
                "campaign-1",
                "repo-1",
                &"b".repeat(40),
                10,
                &campaign_limits(),
            ),
            Err(RuntimeError::Authority)
        );
        assert_eq!(
            checkpoint.validate_authority(
                &"a".repeat(64),
                "campaign-1",
                "repo-1",
                &"b".repeat(40),
                9,
                &campaign_limits(),
            ),
            Err(RuntimeError::Authority)
        );
        let mut changed_limits = campaign_limits();
        changed_limits.output_bytes -= 1;
        assert_eq!(
            checkpoint.validate_authority(
                &"a".repeat(64),
                "campaign-1",
                "repo-1",
                &"b".repeat(40),
                10,
                &changed_limits,
            ),
            Err(RuntimeError::Authority)
        );
    }

    #[test]
    fn checkpoint_reports_each_exhausted_limit_in_stable_order() {
        let mut checkpoint = checkpoint();
        assert_eq!(checkpoint.exhausted_limit(), None);

        checkpoint.utilization.execution_elapsed_ms = checkpoint.limits.execution_elapsed_ms;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::ExecutionElapsed)
        );
        checkpoint.utilization.retained_state_bytes = checkpoint.limits.retained_state_bytes;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::RetainedStateBytes)
        );
        checkpoint.utilization.output_bytes = checkpoint.limits.output_bytes;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::OutputBytes)
        );
        checkpoint.utilization.process_invocations = checkpoint.limits.process_invocations;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::ProcessInvocations)
        );
        checkpoint.utilization.correction_rounds = checkpoint.limits.correction_rounds;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::CorrectionRounds)
        );
        checkpoint.utilization.malformed_report_repairs =
            checkpoint.limits.malformed_report_repairs;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::MalformedReportRepairs)
        );
        checkpoint.utilization.provider_attempts = checkpoint.limits.provider_attempts;
        assert_eq!(
            checkpoint.exhausted_limit(),
            Some(CampaignLimitKind::ProviderAttempts)
        );
    }

    #[test]
    fn accepted_outcome_limit_covers_boundary_overflow_restart_and_concurrent_observation() {
        use std::sync::Arc;
        use std::thread;

        for (name, completed, accepted) in [("one-below", 9, 9), ("exact", 10, 10)] {
            let root = root(&format!("accepted-outcomes-{name}"));
            let mut checkpoint = checkpoint();
            checkpoint.completed_units = completed;
            checkpoint.persist(&root).unwrap();
            let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
            assert_eq!(loaded.outcomes.accepted, accepted);
            assert_eq!(loaded.outcomes.max_accepted, 10);

            let loaded = Arc::new(loaded);
            let handles = (0..4)
                .map(|_| {
                    let loaded = Arc::clone(&loaded);
                    thread::spawn(move || (loaded.outcomes.accepted, loaded.outcomes.max_accepted))
                })
                .collect::<Vec<_>>();
            for handle in handles {
                assert_eq!(handle.join().unwrap(), (accepted, 10));
            }
            fs::remove_dir_all(root).unwrap();
        }

        let above_root = root("accepted-outcomes-one-above");
        let mut above = checkpoint();
        above.completed_units = 11;
        assert_eq!(above.persist(&above_root), Err(RuntimeError::State));
        fs::remove_dir_all(above_root).unwrap();

        let overflow_root = root("accepted-outcomes-overflow");
        let mut overflow = CampaignCheckpoint::new(
            "a".repeat(64),
            "campaign-1".to_owned(),
            "repo-1".to_owned(),
            RunId::new("outcome-overflow-run").unwrap(),
            WorktreeId::new("outcome-overflow-worktree").unwrap(),
            "codingmage/campaign-1/campaign-root".to_owned(),
            "b".repeat(40),
            u32::MAX,
            campaign_limits(),
        )
        .unwrap();
        overflow.completed_units = u32::MAX;
        overflow.blocked_task_ids.insert("1.1.1.1".to_owned());
        overflow.blocked_reasons.insert(
            "1.1.1.1".to_owned(),
            LeadBlockedReason::UnavailableExternalDependency,
        );
        assert_eq!(overflow.persist(&overflow_root), Err(RuntimeError::State));
        fs::remove_dir_all(overflow_root).unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn every_exhausted_campaign_limit_preserves_exact_policy_authority() {
        let limits = [
            CampaignLimitKind::ProviderAttempts,
            CampaignLimitKind::MalformedReportRepairs,
            CampaignLimitKind::CorrectionRounds,
            CampaignLimitKind::ProcessInvocations,
            CampaignLimitKind::OutputBytes,
            CampaignLimitKind::RetainedStateBytes,
            CampaignLimitKind::ExecutionElapsed,
        ];

        for exhausted in limits {
            let root = root(&format!("policy-invariance-{exhausted:?}"));
            let mut spec = campaign_policy_spec();
            if exhausted == CampaignLimitKind::RetainedStateBytes {
                spec.limits.retained_state_bytes = 1;
                private_directory(&root).unwrap();
                fs::write(root.join("retained-evidence"), b"x").unwrap();
            }
            spec.verify().unwrap();
            let authority_sha256 = spec.authority_sha256().unwrap();
            let mut checkpoint = CampaignCheckpoint::new(
                authority_sha256.clone(),
                spec.campaign_id.clone(),
                spec.repository_id.clone(),
                RunId::new("policy-run").unwrap(),
                WorktreeId::new("policy-worktree").unwrap(),
                spec.campaign_branch.clone(),
                spec.initial_commit.clone(),
                spec.max_units,
                spec.limits.clone(),
            )
            .unwrap();
            match exhausted {
                CampaignLimitKind::ProviderAttempts => {
                    checkpoint.utilization.provider_attempts = checkpoint.limits.provider_attempts;
                    checkpoint.utilization.process_invocations =
                        checkpoint.limits.provider_attempts;
                }
                CampaignLimitKind::MalformedReportRepairs => {
                    checkpoint.utilization.malformed_report_repairs =
                        checkpoint.limits.malformed_report_repairs;
                    checkpoint.utilization.provider_attempts =
                        checkpoint.limits.malformed_report_repairs;
                    checkpoint.utilization.process_invocations =
                        checkpoint.limits.malformed_report_repairs;
                }
                CampaignLimitKind::CorrectionRounds => {
                    checkpoint.utilization.correction_rounds = checkpoint.limits.correction_rounds;
                }
                CampaignLimitKind::ProcessInvocations => {
                    checkpoint.utilization.process_invocations =
                        checkpoint.limits.process_invocations;
                }
                CampaignLimitKind::OutputBytes => {
                    checkpoint.utilization.output_bytes = checkpoint.limits.output_bytes;
                }
                CampaignLimitKind::RetainedStateBytes => {}
                CampaignLimitKind::ExecutionElapsed => {
                    checkpoint.utilization.execution_elapsed_ms =
                        checkpoint.limits.execution_elapsed_ms;
                }
            }
            checkpoint
                .persist(&root)
                .unwrap_or_else(|error| panic!("{exhausted:?}: {error:?}"));

            let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
            assert_eq!(loaded.exhausted_limit(), Some(exhausted));
            assert_eq!(loaded.authority_sha256, authority_sha256);
            assert_checkpoint_refuses_weakened_policy(&loaded, &spec);
            fs::remove_dir_all(root).unwrap();
        }

        let root = root("policy-invariance-accepted-outcomes");
        let spec = campaign_policy_spec();
        let mut checkpoint = CampaignCheckpoint::new(
            spec.authority_sha256().unwrap(),
            spec.campaign_id.clone(),
            spec.repository_id.clone(),
            RunId::new("policy-outcomes-run").unwrap(),
            WorktreeId::new("policy-outcomes-worktree").unwrap(),
            spec.campaign_branch.clone(),
            spec.initial_commit.clone(),
            spec.max_units,
            spec.limits.clone(),
        )
        .unwrap();
        checkpoint.completed_units = spec.max_units;
        checkpoint.persist(&root).unwrap();
        let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
        assert_eq!(loaded.outcomes.accepted, loaded.outcomes.max_accepted);
        assert_checkpoint_refuses_weakened_policy(&loaded, &spec);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_preserves_atomic_observation_overages_for_next_admission_stop() {
        let root = root("observed-overage");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("retained-evidence"), b"retained").unwrap();
        let mut checkpoint = checkpoint();
        checkpoint.limits.output_bytes = 1;
        checkpoint.limits.retained_state_bytes = 1;
        checkpoint.limits.execution_elapsed_ms = 1;
        checkpoint.utilization.output_bytes = 2;
        checkpoint.utilization.execution_elapsed_ms = 2;
        checkpoint.persist(&root).unwrap();

        let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
        assert!(loaded.utilization.output_bytes > loaded.limits.output_bytes);
        assert!(loaded.utilization.retained_state_bytes > loaded.limits.retained_state_bytes);
        assert!(loaded.utilization.execution_elapsed_ms > loaded.limits.execution_elapsed_ms);
        assert_eq!(
            loaded.exhausted_limit(),
            Some(CampaignLimitKind::OutputBytes)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn unit_budget_allows_exact_boundary_and_refuses_every_additional_effect() {
        let root = root("unit-budget");
        fs::create_dir_all(&root).unwrap();
        let budget = CampaignUnitBudget {
            baseline: CampaignUtilization::default(),
            limits: CampaignLimits {
                provider_attempts: 2,
                malformed_report_repairs: 1,
                correction_rounds: 1,
                process_invocations: 4,
                output_bytes: 10,
                retained_state_bytes: 10,
                execution_elapsed_ms: 10,
            },
            campaign_root: root.clone(),
        };
        let mut usage = RunUtilization::default();

        assert_eq!(
            budget.authorize(
                &usage,
                0,
                CampaignReservation {
                    provider_attempts: 2,
                    process_invocations: 2,
                    ..CampaignReservation::default()
                },
            ),
            Ok(())
        );
        usage.provider_attempts = 2;
        usage.process_invocations = 2;
        assert_eq!(
            budget.authorize(
                &usage,
                0,
                CampaignReservation {
                    provider_attempts: 1,
                    process_invocations: 1,
                    ..CampaignReservation::default()
                },
            ),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::ProviderAttempts
            ))
        );

        usage.provider_attempts = 0;
        usage.malformed_report_repairs = 1;
        assert_eq!(
            budget.authorize(
                &usage,
                0,
                CampaignReservation {
                    malformed_report_repairs: 1,
                    ..CampaignReservation::default()
                },
            ),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::MalformedReportRepairs
            ))
        );
        usage.malformed_report_repairs = 0;
        assert_eq!(
            budget.authorize(
                &usage,
                1,
                CampaignReservation {
                    correction_rounds: 1,
                    ..CampaignReservation::default()
                },
            ),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::CorrectionRounds
            ))
        );
        usage.process_invocations = 4;
        assert_eq!(
            budget.authorize(
                &usage,
                0,
                CampaignReservation {
                    process_invocations: 1,
                    ..CampaignReservation::default()
                },
            ),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::ProcessInvocations
            ))
        );
        usage.process_invocations = 0;
        usage.output_bytes = 10;
        assert_eq!(
            budget.authorize(&usage, 0, CampaignReservation::default()),
            Err(RuntimeError::CampaignLimit(CampaignLimitKind::OutputBytes))
        );
        usage.output_bytes = 0;
        usage.execution_elapsed_ms = 10;
        assert_eq!(
            budget.authorize(&usage, 0, CampaignReservation::default()),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::ExecutionElapsed
            ))
        );
        usage.execution_elapsed_ms = 0;
        fs::write(root.join("ten-bytes"), b"0123456789").unwrap();
        assert_eq!(
            budget.authorize(&usage, 0, CampaignReservation::default()),
            Err(RuntimeError::CampaignLimit(
                CampaignLimitKind::RetainedStateBytes
            ))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn unit_budget_matrix_covers_boundaries_overflow_and_concurrent_observation() {
        use std::sync::Arc;
        use std::thread;

        let kinds = [
            CampaignLimitKind::ProviderAttempts,
            CampaignLimitKind::MalformedReportRepairs,
            CampaignLimitKind::CorrectionRounds,
            CampaignLimitKind::ProcessInvocations,
            CampaignLimitKind::OutputBytes,
            CampaignLimitKind::RetainedStateBytes,
            CampaignLimitKind::ExecutionElapsed,
        ];

        for kind in kinds {
            let root = root(&format!("limit-matrix-{kind:?}"));
            let budget = CampaignUnitBudget {
                baseline: limit_baseline(kind, 1),
                limits: matrix_limits(kind),
                campaign_root: root,
            };
            let unit_one = limit_usage(kind, 1);
            let correction_one = u16::from(kind == CampaignLimitKind::CorrectionRounds);
            let retained_one_below = if kind == CampaignLimitKind::RetainedStateBytes {
                2
            } else {
                0
            };
            assert_eq!(
                budget
                    .exhausted_with_observations(
                        &unit_one,
                        correction_one,
                        CampaignReservation::default(),
                        retained_one_below,
                    )
                    .unwrap(),
                None,
                "one below {kind:?}"
            );

            let reservable = matches!(
                kind,
                CampaignLimitKind::ProviderAttempts
                    | CampaignLimitKind::MalformedReportRepairs
                    | CampaignLimitKind::CorrectionRounds
                    | CampaignLimitKind::ProcessInvocations
            );
            let (exact_unit, exact_corrections, exact_reservation, exact_retained, exact_result) =
                if reservable {
                    (
                        unit_one.clone(),
                        correction_one,
                        limit_reservation(kind, 1),
                        retained_one_below,
                        None,
                    )
                } else {
                    (
                        limit_usage(kind, 2),
                        0,
                        CampaignReservation::default(),
                        if kind == CampaignLimitKind::RetainedStateBytes {
                            3
                        } else {
                            0
                        },
                        Some(kind),
                    )
                };
            assert_eq!(
                budget
                    .exhausted_with_observations(
                        &exact_unit,
                        exact_corrections,
                        exact_reservation,
                        exact_retained,
                    )
                    .unwrap(),
                exact_result,
                "exact boundary {kind:?}"
            );

            let (above_unit, above_corrections, above_reservation, above_retained) = if reservable {
                (
                    unit_one.clone(),
                    correction_one,
                    limit_reservation(kind, 2),
                    retained_one_below,
                )
            } else {
                (
                    limit_usage(kind, 3),
                    0,
                    CampaignReservation::default(),
                    if kind == CampaignLimitKind::RetainedStateBytes {
                        4
                    } else {
                        0
                    },
                )
            };
            assert_eq!(
                budget
                    .exhausted_with_observations(
                        &above_unit,
                        above_corrections,
                        above_reservation,
                        above_retained,
                    )
                    .unwrap(),
                Some(kind),
                "one above {kind:?}"
            );

            let budget = Arc::new(budget);
            let handles = (0..4)
                .map(|_| {
                    let budget = Arc::clone(&budget);
                    let unit = above_unit.clone();
                    thread::spawn(move || {
                        budget.exhausted_with_observations(
                            &unit,
                            above_corrections,
                            above_reservation,
                            above_retained,
                        )
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                assert_eq!(handle.join().unwrap().unwrap(), Some(kind));
            }

            let mut overflow = CampaignUtilization::default();
            let overflow_unit = match kind {
                CampaignLimitKind::ProviderAttempts => {
                    overflow.provider_attempts = u32::MAX;
                    limit_usage(kind, 1)
                }
                CampaignLimitKind::MalformedReportRepairs => {
                    overflow.malformed_report_repairs = u32::MAX;
                    limit_usage(kind, 1)
                }
                CampaignLimitKind::CorrectionRounds => {
                    overflow.correction_rounds = u32::MAX;
                    RunUtilization::default()
                }
                CampaignLimitKind::ProcessInvocations => {
                    overflow.process_invocations = u32::MAX;
                    limit_usage(kind, 1)
                }
                CampaignLimitKind::OutputBytes => {
                    overflow.output_bytes = u64::MAX;
                    limit_usage(kind, 1)
                }
                CampaignLimitKind::RetainedStateBytes => RunUtilization::default(),
                CampaignLimitKind::ExecutionElapsed => {
                    overflow.execution_elapsed_ms = u64::MAX;
                    limit_usage(kind, 1)
                }
            };
            if kind == CampaignLimitKind::RetainedStateBytes {
                assert_eq!(
                    checked_retained_total(u64::MAX, 1),
                    Err(RuntimeError::State)
                );
            } else {
                let overflow_budget = CampaignUnitBudget {
                    baseline: overflow,
                    limits: matrix_limits(kind),
                    campaign_root: PathBuf::new(),
                };
                assert_eq!(
                    overflow_budget.exhausted_with_observations(
                        &overflow_unit,
                        u16::from(kind == CampaignLimitKind::CorrectionRounds),
                        CampaignReservation::default(),
                        0,
                    ),
                    Err(RuntimeError::State),
                    "overflow {kind:?}"
                );
            }
        }
    }

    #[test]
    fn checkpoint_refuses_integrity_verified_legacy_v1_without_blocked_tasks() {
        let root = root("legacy-v1");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let legacy = LegacyCampaignCheckpointV1 {
            schema_version: 1,
            authority_sha256: current.authority_sha256.clone(),
            campaign_id: current.campaign_id.clone(),
            repository_id: current.repository_id.clone(),
            campaign_run_id: current.campaign_run_id.clone(),
            worktree_id: current.worktree_id.clone(),
            branch: current.branch.clone(),
            initial_head: current.initial_head.clone(),
            head: current.head.clone(),
            completed_units: current.completed_units,
            last_task_id: current.last_task_id.clone(),
            phase: current.phase,
            blocker_code: current.blocker_code.clone(),
            active_unit: current.active_unit.clone(),
            pending_integration: current.pending_integration.clone(),
            started_at_ms: current.started_at_ms,
            updated_at_ms: current.updated_at_ms,
        };
        let canonical = serde_json::to_vec(&legacy).unwrap();
        let envelope = LegacyCheckpointEnvelopeV1 {
            checkpoint: legacy,
            checkpoint_sha256: sha256_hex(&canonical),
        };
        fs::write(
            root.join("checkpoint.json"),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();

        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_refuses_integrity_verified_blocked_id_legacy_shape() {
        let root = root("legacy-blocked-ids");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let legacy = LegacyCampaignCheckpointWithBlockedIds {
            schema_version: 1,
            authority_sha256: current.authority_sha256.clone(),
            campaign_id: current.campaign_id.clone(),
            repository_id: current.repository_id.clone(),
            campaign_run_id: current.campaign_run_id.clone(),
            worktree_id: current.worktree_id.clone(),
            branch: current.branch.clone(),
            initial_head: current.initial_head.clone(),
            head: current.head.clone(),
            completed_units: current.completed_units,
            last_task_id: current.last_task_id.clone(),
            phase: current.phase,
            blocker_code: current.blocker_code.clone(),
            blocked_task_ids: BTreeSet::from(["1.1.1.2".to_owned()]),
            active_unit: current.active_unit.clone(),
            pending_integration: current.pending_integration.clone(),
            started_at_ms: current.started_at_ms,
            updated_at_ms: current.updated_at_ms,
        };
        let canonical = serde_json::to_vec(&legacy).unwrap();
        let envelope = LegacyCheckpointEnvelopeWithBlockedIds {
            checkpoint: legacy,
            checkpoint_sha256: sha256_hex(&canonical),
        };
        fs::write(
            root.join("checkpoint.json"),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();

        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_refuses_integrity_verified_blocked_reason_legacy_shape() {
        let root = root("legacy-blocked-reasons");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let legacy = LegacyCampaignCheckpointWithBlockedReasons {
            schema_version: 1,
            authority_sha256: current.authority_sha256.clone(),
            campaign_id: current.campaign_id.clone(),
            repository_id: current.repository_id.clone(),
            campaign_run_id: current.campaign_run_id.clone(),
            worktree_id: current.worktree_id.clone(),
            branch: current.branch.clone(),
            initial_head: current.initial_head.clone(),
            head: current.head.clone(),
            completed_units: current.completed_units,
            last_task_id: current.last_task_id.clone(),
            phase: current.phase,
            blocker_code: current.blocker_code.clone(),
            blocked_task_ids: BTreeSet::from(["1.1.1.2".to_owned()]),
            blocked_reasons: BTreeMap::from([(
                "1.1.1.2".to_owned(),
                LeadBlockedReason::UnavailableExternalDependency,
            )]),
            active_unit: current.active_unit.clone(),
            pending_integration: current.pending_integration.clone(),
            started_at_ms: current.started_at_ms,
            updated_at_ms: current.updated_at_ms,
        };
        let canonical = serde_json::to_vec(&legacy).unwrap();
        let envelope = LegacyCheckpointEnvelopeWithBlockedReasons {
            checkpoint: legacy,
            checkpoint_sha256: sha256_hex(&canonical),
        };
        fs::write(
            root.join("checkpoint.json"),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();

        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_refuses_integrity_verified_deferral_legacy_shape() {
        let root = root("legacy-deferrals");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let projection = DeferredTaskProjection {
            reason: LeadDeferredReason::OperatorPause,
            trigger: LeadReconsiderationTrigger::OperatorResume,
            source_head: current.head.clone(),
            task_source_sha256: "c".repeat(64),
        };
        let legacy = LegacyCampaignCheckpointWithDeferrals {
            schema_version: 1,
            authority_sha256: current.authority_sha256.clone(),
            campaign_id: current.campaign_id.clone(),
            repository_id: current.repository_id.clone(),
            campaign_run_id: current.campaign_run_id.clone(),
            worktree_id: current.worktree_id.clone(),
            branch: current.branch.clone(),
            initial_head: current.initial_head.clone(),
            head: current.head.clone(),
            completed_units: current.completed_units,
            last_task_id: current.last_task_id.clone(),
            phase: current.phase,
            blocker_code: current.blocker_code.clone(),
            blocked_task_ids: BTreeSet::new(),
            blocked_reasons: BTreeMap::new(),
            deferred_tasks: BTreeMap::from([("1.1.1.2".to_owned(), projection.clone())]),
            satisfied_deferrals: BTreeMap::new(),
            active_unit: current.active_unit.clone(),
            pending_integration: current.pending_integration.clone(),
            started_at_ms: current.started_at_ms,
            updated_at_ms: current.updated_at_ms,
        };
        let canonical = serde_json::to_vec(&legacy).unwrap();
        let envelope = LegacyCheckpointEnvelopeWithDeferrals {
            checkpoint: legacy,
            checkpoint_sha256: sha256_hex(&canonical),
        };
        fs::write(
            root.join("checkpoint.json"),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();

        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_refuses_integrity_verified_human_decision_legacy_shape() {
        let root = root("legacy-human-decisions");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let decision = HumanDecisionProjection {
            reason: HumanDecisionProjectionReason::RepeatedSatisfiedDeferral,
            source_head: current.head.clone(),
            task_source_sha256: "c".repeat(64),
        };
        let legacy = LegacyCampaignCheckpointWithHumanDecisions {
            schema_version: 1,
            authority_sha256: current.authority_sha256.clone(),
            campaign_id: current.campaign_id.clone(),
            repository_id: current.repository_id.clone(),
            campaign_run_id: current.campaign_run_id.clone(),
            worktree_id: current.worktree_id.clone(),
            branch: current.branch.clone(),
            initial_head: current.initial_head.clone(),
            head: current.head.clone(),
            completed_units: current.completed_units,
            last_task_id: current.last_task_id.clone(),
            phase: current.phase,
            blocker_code: current.blocker_code.clone(),
            blocked_task_ids: BTreeSet::new(),
            blocked_reasons: BTreeMap::new(),
            deferred_tasks: BTreeMap::new(),
            satisfied_deferrals: BTreeMap::new(),
            human_decisions: BTreeMap::from([("1.1.1.4".to_owned(), decision.clone())]),
            active_unit: current.active_unit.clone(),
            pending_integration: current.pending_integration.clone(),
            started_at_ms: current.started_at_ms,
            updated_at_ms: current.updated_at_ms,
        };
        let canonical = serde_json::to_vec(&legacy).unwrap();
        let envelope = LegacyCheckpointEnvelopeWithHumanDecisions {
            checkpoint: legacy,
            checkpoint_sha256: sha256_hex(&canonical),
        };
        fs::write(
            root.join("checkpoint.json"),
            serde_json::to_vec_pretty(&envelope).unwrap(),
        )
        .unwrap();

        assert_eq!(CampaignCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blocker_clearance_intent_is_create_once_and_integrity_bound() {
        let root = root("clearance-intent");
        let intent = BlockerClearanceIntent::new(
            "clear-1".to_owned(),
            "campaign-1".to_owned(),
            "repo-1".to_owned(),
            "1.1.1.2".to_owned(),
            LeadBlockedReason::UnavailableExternalDependency,
            "b".repeat(40),
            "c".repeat(64),
            "d".repeat(64),
        );
        intent.persist_new(&root).unwrap();
        assert_eq!(
            BlockerClearanceIntent::load(&root, "clear-1").unwrap(),
            Some(intent.clone())
        );
        assert_eq!(intent.persist_new(&root), Err(RuntimeError::State));

        let path = clearance_path(&root, "clear-1");
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'd').unwrap();
        bytes[index] = b'e';
        fs::write(path, bytes).unwrap();
        assert_eq!(
            BlockerClearanceIntent::load(&root, "clear-1"),
            Err(RuntimeError::State)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deferral_trigger_intent_is_create_once_and_integrity_bound() {
        let root = root("trigger-intent");
        let projection = DeferredTaskProjection {
            reason: LeadDeferredReason::OperatorPause,
            trigger: LeadReconsiderationTrigger::OperatorResume,
            source_head: "b".repeat(40),
            task_source_sha256: "c".repeat(64),
        };
        let intent = DeferralTriggerIntent::new(
            "resume-1".to_owned(),
            "campaign-1".to_owned(),
            "repo-1".to_owned(),
            "1.1.1.2".to_owned(),
            &projection,
            "d".repeat(64),
        );
        intent.persist_new(&root).unwrap();
        assert_eq!(
            DeferralTriggerIntent::load(&root, "resume-1").unwrap(),
            Some(intent.clone())
        );
        assert_eq!(intent.persist_new(&root), Err(RuntimeError::State));

        let path = trigger_path(&root, "resume-1");
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'd').unwrap();
        bytes[index] = b'e';
        fs::write(path, bytes).unwrap();
        assert_eq!(
            DeferralTriggerIntent::load(&root, "resume-1"),
            Err(RuntimeError::State)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn control_requests_are_ordered_integrity_bound_and_idempotently_applied() {
        let root = root("controls");
        fs::create_dir_all(&root).unwrap();
        let checkpoint = checkpoint();
        let mut pause = CampaignControlIntent::new(
            "control-2".to_owned(),
            checkpoint.authority_sha256.clone(),
            checkpoint.campaign_id.clone(),
            checkpoint.repository_id.clone(),
            checkpoint.campaign_run_id.clone(),
            CampaignControlAction::Pause,
            checkpoint.head.clone(),
            checkpoint.updated_at_ms,
        )
        .unwrap();
        pause.created_at_ms = 2;
        let mut first = pause.clone();
        first.request_id = "control-1".to_owned();
        first.created_at_ms = 1;
        pause.persist_new(&root).unwrap();
        first.persist_new(&root).unwrap();

        let pending = CampaignControlIntent::pending(&root).unwrap();
        assert_eq!(
            pending
                .iter()
                .map(|intent| intent.request_id.as_str())
                .collect::<Vec<_>>(),
            ["control-1", "control-2"]
        );
        let mut applied = checkpoint;
        assert!(applied.apply_control(&pending[0]).unwrap());
        assert!(!applied.apply_control(&pending[0]).unwrap());
        assert_eq!(
            applied.apply_control(&pending[1]),
            Err(RuntimeError::Authority),
            "a differently identified duplicate effect must fail closed"
        );

        let path = control_path(&root, "control-1");
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope["intent"]["action"] = serde_json::Value::String("cancel".to_owned());
        fs::write(&path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();
        assert_eq!(
            CampaignControlIntent::load(&root, "control-1"),
            Err(RuntimeError::State)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn control_state_machine_has_closed_pause_resume_stop_and_cancel_semantics() {
        let mut checkpoint = checkpoint();
        let authority_sha256 = checkpoint.authority_sha256.clone();
        let campaign_id = checkpoint.campaign_id.clone();
        let repository_id = checkpoint.repository_id.clone();
        let campaign_run_id = checkpoint.campaign_run_id.clone();
        let head = checkpoint.head.clone();
        let updated_at_ms = checkpoint.updated_at_ms;
        let control = |request: &str, action| {
            CampaignControlIntent::new(
                request.to_owned(),
                authority_sha256.clone(),
                campaign_id.clone(),
                repository_id.clone(),
                campaign_run_id.clone(),
                action,
                head.clone(),
                updated_at_ms,
            )
            .unwrap()
        };

        assert!(
            checkpoint
                .apply_control(&control("pause-1", CampaignControlAction::Pause))
                .unwrap()
        );
        assert!(checkpoint.operator_paused);
        assert!(
            checkpoint
                .apply_control(&control("resume-1", CampaignControlAction::Resume))
                .unwrap()
        );
        assert!(!checkpoint.operator_paused);
        assert_eq!(checkpoint.resume_validation, ResumeValidationState::Pending);
        assert!(
            checkpoint
                .apply_control(&control("stop-1", CampaignControlAction::StopAfterUnit))
                .unwrap()
        );
        assert!(checkpoint.stop_after_unit);
        assert!(
            checkpoint
                .apply_control(&control("resume-2", CampaignControlAction::Resume))
                .unwrap()
        );
        assert!(!checkpoint.stop_after_unit);
        assert_eq!(checkpoint.resume_validation, ResumeValidationState::Pending);
        assert!(
            checkpoint
                .apply_control(&control("cancel-1", CampaignControlAction::Cancel))
                .unwrap()
        );
        assert!(checkpoint.cancelled);
        assert_eq!(
            checkpoint.resume_validation,
            ResumeValidationState::NotRequired
        );
        assert_eq!(
            checkpoint.apply_control(&control("resume-3", CampaignControlAction::Resume)),
            Err(RuntimeError::Authority)
        );
        assert_eq!(CampaignControlAction::parse("unknown"), None);
    }

    #[test]
    fn every_control_crash_boundary_replays_to_one_recoverable_effect() {
        let actions = [
            CampaignControlAction::Pause,
            CampaignControlAction::Resume,
            CampaignControlAction::StopAfterUnit,
            CampaignControlAction::Cancel,
        ];
        let boundaries = [
            ControlCrashBoundary::IntentPersisted,
            ControlCrashBoundary::RequestObserved,
            ControlCrashBoundary::StatePersisted,
            ControlCrashBoundary::AppliedObserved,
        ];

        for action in actions {
            for boundary in boundaries {
                let name = format!("control-crash-{}-{boundary:?}", action.code());
                let root = root(&name);
                let mut checkpoint = checkpoint();
                if action == CampaignControlAction::Resume {
                    checkpoint.operator_paused = true;
                }
                checkpoint.persist(&root).unwrap();
                let request_id = format!("{}-{boundary:?}", action.code());
                let intent = CampaignControlIntent::new(
                    request_id.clone(),
                    checkpoint.authority_sha256.clone(),
                    checkpoint.campaign_id.clone(),
                    checkpoint.repository_id.clone(),
                    checkpoint.campaign_run_id.clone(),
                    action,
                    checkpoint.head.clone(),
                    checkpoint.updated_at_ms,
                )
                .unwrap();
                intent.persist_new(&root).unwrap();

                if boundary >= ControlCrashBoundary::RequestObserved {
                    let mut journal = Journal::open(&root, format!("{name}-requested")).unwrap();
                    journal
                        .append(checkpoint.control_event(&intent, false).unwrap())
                        .unwrap();
                }
                if boundary >= ControlCrashBoundary::StatePersisted {
                    assert!(checkpoint.apply_control(&intent).unwrap());
                    checkpoint.persist(&root).unwrap();
                }
                if boundary == ControlCrashBoundary::AppliedObserved {
                    checkpoint.reconcile_control_journal(&root).unwrap();
                }

                let mut recovered = CampaignCheckpoint::load(&root).unwrap().unwrap();
                let first = crate::apply_pending_campaign_controls(&mut recovered, &root).unwrap();
                assert_eq!(
                    first.resumed,
                    action == CampaignControlAction::Resume
                        && boundary < ControlCrashBoundary::StatePersisted
                );
                assert_control_effect(&recovered, action, &request_id);
                let stable = recovered.clone();

                for _ in 0..2 {
                    let replay =
                        crate::apply_pending_campaign_controls(&mut recovered, &root).unwrap();
                    assert!(!replay.resumed);
                    assert_eq!(recovered, stable);
                    assert_control_effect(&recovered, action, &request_id);
                }

                let journal = Journal::open(&root, format!("{name}-reader")).unwrap();
                let requested = journal
                    .records()
                    .iter()
                    .filter(|record| {
                        matches!(
                            &record.event.kind,
                            EventKind::ControlRequested {
                                request_id: observed,
                                action: observed_action,
                            } if observed == &request_id && observed_action == action.code()
                        )
                    })
                    .count();
                let applied = journal
                    .records()
                    .iter()
                    .filter(|record| {
                        matches!(
                            &record.event.kind,
                            EventKind::ControlApplied {
                                request_id: observed,
                                action: observed_action,
                            } if observed == &request_id && observed_action == action.code()
                        )
                    })
                    .count();
                assert_eq!((requested, applied), (1, 1));
                drop(journal);

                let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
                assert_eq!(loaded, recovered);
                assert_control_effect(&loaded, action, &request_id);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn control_journal_reconciles_intent_and_applied_observation_exactly_once() {
        let root = root("control-journal-recovery");
        let mut checkpoint = checkpoint();
        checkpoint.persist(&root).unwrap();
        let intent = CampaignControlIntent::new(
            "pause-recovery-1".to_owned(),
            checkpoint.authority_sha256.clone(),
            checkpoint.campaign_id.clone(),
            checkpoint.repository_id.clone(),
            checkpoint.campaign_run_id.clone(),
            CampaignControlAction::Pause,
            checkpoint.head.clone(),
            checkpoint.updated_at_ms,
        )
        .unwrap();
        intent.persist_new(&root).unwrap();

        checkpoint.reconcile_control_journal(&root).unwrap();
        assert!(checkpoint.apply_control(&intent).unwrap());
        checkpoint.persist(&root).unwrap();
        checkpoint.reconcile_control_journal(&root).unwrap();
        checkpoint.reconcile_control_journal(&root).unwrap();

        let journal = Journal::open(&root, "control-recovery-reader").unwrap();
        let requested = journal
            .records()
            .iter()
            .filter(|record| {
                matches!(
                    &record.event.kind,
                    EventKind::ControlRequested { request_id, .. }
                        if request_id == "pause-recovery-1"
                )
            })
            .count();
        let applied = journal
            .records()
            .iter()
            .filter(|record| {
                matches!(
                    &record.event.kind,
                    EventKind::ControlApplied { request_id, .. }
                        if request_id == "pause-recovery-1"
                )
            })
            .count();
        assert_eq!((requested, applied), (1, 1));
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn control_journal_refuses_duplicate_or_cross_run_observations() {
        for (name, cross_run) in [("duplicate-request", false), ("cross-run-request", true)] {
            let root = root(name);
            let mut checkpoint = checkpoint();
            checkpoint.persist(&root).unwrap();
            let intent = CampaignControlIntent::new(
                format!("{name}-1"),
                checkpoint.authority_sha256.clone(),
                checkpoint.campaign_id.clone(),
                checkpoint.repository_id.clone(),
                checkpoint.campaign_run_id.clone(),
                CampaignControlAction::Pause,
                checkpoint.head.clone(),
                checkpoint.updated_at_ms,
            )
            .unwrap();
            intent.persist_new(&root).unwrap();
            checkpoint.reconcile_control_journal(&root).unwrap();

            let mut event = checkpoint.control_event(&intent, false).unwrap();
            if cross_run {
                event.run_id = RunId::new("wrong-run").unwrap();
            }
            let mut journal = Journal::open(&root, format!("{name}-writer")).unwrap();
            journal.append(event).unwrap();
            drop(journal);
            assert_eq!(
                checkpoint.reconcile_control_journal(&root),
                Err(RuntimeError::State)
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn stop_and_cancel_controls_map_to_exact_campaign_termination() {
        let stop_root = root("stop-termination");
        let mut stopped = checkpoint();
        stopped.persist(&stop_root).unwrap();
        stopped.stop_after_unit = true;
        let termination = crate::campaign_control_termination(&mut stopped, &stop_root)
            .unwrap()
            .unwrap();
        assert_eq!(termination.state, crate::CampaignState::Paused);
        assert_eq!(termination.reason, crate::CampaignStopReason::StopAfterUnit);
        assert_eq!(stopped.phase, CampaignPhase::Paused);

        let cancel_root = root("cancel-termination");
        let mut cancelled = checkpoint();
        cancelled.persist(&cancel_root).unwrap();
        cancelled.cancelled = true;
        let termination = crate::campaign_control_termination(&mut cancelled, &cancel_root)
            .unwrap()
            .unwrap();
        assert_eq!(termination.state, crate::CampaignState::Cancelled);
        assert_eq!(
            termination.reason,
            crate::CampaignStopReason::OperatorCancellation
        );
        assert_eq!(cancelled.phase, CampaignPhase::Cancelled);
        fs::remove_dir_all(stop_root).unwrap();
        fs::remove_dir_all(cancel_root).unwrap();
    }
}
