use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{
    EvidenceId, LeadBlockedReason, LeadDeferredReason, LeadHumanDecisionReason,
    LeadReconsiderationTrigger, RepositoryId, RunId, TaskId, WorktreeId,
};
use codingmage_state::{
    DurableIdentities, EventKind, EventOutcome, Journal, JournalEvent, RedactedField,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RuntimeError;

const SCHEMA_VERSION: u16 = 2;
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HumanDecisionProjectionReason {
    Lead(LeadHumanDecisionReason),
    RepeatedSatisfiedDeferral,
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
    pub active_unit: Option<ActiveUnit>,
    pub pending_integration: Option<PendingIntegration>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Deserialize, Serialize)]
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
    ) -> Result<Self, RuntimeError> {
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
        Ok(Some(envelope.checkpoint))
    }

    pub(crate) fn persist(&mut self, root: &Path) -> Result<(), RuntimeError> {
        private_directory(root)?;
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
        let completed_units = self.completed_units;
        let blocked_tasks =
            u32::try_from(self.blocked_task_ids.len()).map_err(|_| RuntimeError::State)?;
        let deferred_tasks =
            u32::try_from(self.deferred_tasks.len()).map_err(|_| RuntimeError::State)?;
        let satisfied_deferrals =
            u32::try_from(self.satisfied_deferrals.len()).map_err(|_| RuntimeError::State)?;
        let human_decisions =
            u32::try_from(self.human_decisions.len()).map_err(|_| RuntimeError::State)?;
        let rejected_proposals =
            u32::try_from(self.rejected_proposals.len()).map_err(|_| RuntimeError::State)?;
        let accepted_outcomes = completed_units
            .checked_add(blocked_tasks)
            .and_then(|value| value.checked_add(deferred_tasks))
            .and_then(|value| value.checked_add(human_decisions))
            .ok_or(RuntimeError::State)?;
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
                phase: self.phase.label().to_owned(),
                completed_units,
                blocked_tasks,
                deferred_tasks,
                satisfied_deferrals,
                human_decisions,
                rejected_proposals,
                accepted_outcomes,
                max_outcomes: None,
                active_unit: self.active_unit.is_some(),
                provider_attempts: None,
                correction_round: None,
                operator_paused: None,
                stop_after_unit: None,
                cancelled: None,
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

    pub(crate) fn validate_authority(
        &self,
        authority_sha256: &str,
        campaign_id: &str,
        repository_id: &str,
        initial_head: &str,
    ) -> Result<(), RuntimeError> {
        if self.authority_sha256 != authority_sha256
            || self.campaign_id != campaign_id
            || self.repository_id != repository_id
            || self.initial_head != initial_head
        {
            return Err(RuntimeError::Authority);
        }
        Ok(())
    }

    pub(crate) fn elapsed_ms(&self) -> Result<u64, RuntimeError> {
        Ok(timestamp_ms()?.saturating_sub(self.started_at_ms))
    }
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

fn clearance_path(root: &Path, request_id: &str) -> PathBuf {
    root.join("blocker-clearances")
        .join(format!("{request_id}.json"))
}

fn trigger_path(root: &Path, request_id: &str) -> PathBuf {
    root.join("deferral-trigger-observations")
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
        }
    }

    pub(crate) const fn actor(self) -> &'static str {
        match self {
            Self::Planning => "codex-lead",
            Self::RunningUnit => "pod",
            Self::Integrating => "integration",
            Self::Ready | Self::Paused | Self::Blocked | Self::Complete => "coordinator",
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
    fs::create_dir_all(path).map_err(|_| RuntimeError::State)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| RuntimeError::State)?;
    }
    Ok(())
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
        )
        .unwrap()
    }

    #[test]
    fn checkpoint_round_trips_and_rejects_tampering() {
        let root = root("round-trip");
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
        checkpoint.pending_integration = Some(PendingIntegration {
            task_id: "1.1.1.1".to_owned(),
            expected_head: "b".repeat(40),
            target_head: "c".repeat(40),
            owned_paths: vec![PathBuf::from("src")],
        });
        checkpoint.persist(&root).unwrap();
        assert_eq!(CampaignCheckpoint::load(&root).unwrap(), Some(checkpoint));

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
        assert!(matches!(
            record.event.kind,
            EventKind::CampaignCheckpointed {
                ref phase,
                completed_units: 0,
                blocked_tasks: 1,
                deferred_tasks: 1,
                satisfied_deferrals: 0,
                human_decisions: 1,
                rejected_proposals: 1,
                accepted_outcomes: 3,
                max_outcomes: None,
                active_unit: false,
                provider_attempts: None,
                correction_round: None,
                operator_paused: None,
                stop_after_unit: None,
                cancelled: None,
            } if phase == "integrating"
        ));
        assert_eq!(record.event.evidence.len(), 1);
        assert_eq!(record.event.redactions.len(), 5);
        let journal_bytes = fs::read(root.join("events.jsonl")).unwrap();
        assert!(
            !journal_bytes
                .windows(b"provider prose".len())
                .any(|window| window == b"provider prose")
        );
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
    fn checkpoint_binds_original_authority() {
        let checkpoint = checkpoint();
        checkpoint
            .validate_authority(&"a".repeat(64), "campaign-1", "repo-1", &"b".repeat(40))
            .unwrap();
        assert_eq!(
            checkpoint.validate_authority(&"c".repeat(64), "campaign-1", "repo-1", &"b".repeat(40)),
            Err(RuntimeError::Authority)
        );
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
}
