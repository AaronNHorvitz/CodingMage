use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{LeadBlockedReason, RunId, WorktreeId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RuntimeError;

const SCHEMA_VERSION: u16 = 1;
const MAX_CHECKPOINT_BYTES: usize = 1024 * 1024;
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyCheckpointEnvelopeWithBlockedIds {
    checkpoint: LegacyCampaignCheckpointWithBlockedIds,
    checkpoint_sha256: String,
}

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
            return load_legacy_with_blocked_ids(&bytes)
                .or_else(|_| load_legacy_v1(&bytes))
                .map(Some);
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
            .map_err(|_| RuntimeError::State)
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

fn load_legacy_with_blocked_ids(bytes: &[u8]) -> Result<CampaignCheckpoint, RuntimeError> {
    let envelope: LegacyCheckpointEnvelopeWithBlockedIds =
        serde_json::from_slice(bytes).map_err(|_| RuntimeError::State)?;
    let canonical = serde_json::to_vec(&envelope.checkpoint).map_err(|_| RuntimeError::State)?;
    if envelope.checkpoint.schema_version != SCHEMA_VERSION
        || sha256_hex(&canonical) != envelope.checkpoint_sha256
    {
        return Err(RuntimeError::State);
    }
    let legacy = envelope.checkpoint;
    Ok(CampaignCheckpoint {
        schema_version: legacy.schema_version,
        authority_sha256: legacy.authority_sha256,
        campaign_id: legacy.campaign_id,
        repository_id: legacy.repository_id,
        campaign_run_id: legacy.campaign_run_id,
        worktree_id: legacy.worktree_id,
        branch: legacy.branch,
        initial_head: legacy.initial_head,
        head: legacy.head,
        completed_units: legacy.completed_units,
        last_task_id: legacy.last_task_id,
        phase: legacy.phase,
        blocker_code: legacy.blocker_code,
        blocked_task_ids: legacy.blocked_task_ids,
        blocked_reasons: BTreeMap::new(),
        active_unit: legacy.active_unit,
        pending_integration: legacy.pending_integration,
        started_at_ms: legacy.started_at_ms,
        updated_at_ms: legacy.updated_at_ms,
    })
}

fn load_legacy_v1(bytes: &[u8]) -> Result<CampaignCheckpoint, RuntimeError> {
    let envelope: LegacyCheckpointEnvelopeV1 =
        serde_json::from_slice(bytes).map_err(|_| RuntimeError::State)?;
    let canonical = serde_json::to_vec(&envelope.checkpoint).map_err(|_| RuntimeError::State)?;
    if envelope.checkpoint.schema_version != SCHEMA_VERSION
        || sha256_hex(&canonical) != envelope.checkpoint_sha256
    {
        return Err(RuntimeError::State);
    }
    let legacy = envelope.checkpoint;
    Ok(CampaignCheckpoint {
        schema_version: legacy.schema_version,
        authority_sha256: legacy.authority_sha256,
        campaign_id: legacy.campaign_id,
        repository_id: legacy.repository_id,
        campaign_run_id: legacy.campaign_run_id,
        worktree_id: legacy.worktree_id,
        branch: legacy.branch,
        initial_head: legacy.initial_head,
        head: legacy.head,
        completed_units: legacy.completed_units,
        last_task_id: legacy.last_task_id,
        phase: legacy.phase,
        blocker_code: legacy.blocker_code,
        blocked_task_ids: BTreeSet::new(),
        blocked_reasons: BTreeMap::new(),
        active_unit: legacy.active_unit,
        pending_integration: legacy.pending_integration,
        started_at_ms: legacy.started_at_ms,
        updated_at_ms: legacy.updated_at_ms,
    })
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
        checkpoint.pending_integration = Some(PendingIntegration {
            task_id: "1.1.1.1".to_owned(),
            expected_head: "b".repeat(40),
            target_head: "c".repeat(40),
            owned_paths: vec![PathBuf::from("src")],
        });
        checkpoint.persist(&root).unwrap();
        assert_eq!(CampaignCheckpoint::load(&root).unwrap(), Some(checkpoint));

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
    fn checkpoint_loads_integrity_verified_legacy_v1_without_blocked_tasks() {
        let root = root("legacy-v1");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let legacy = LegacyCampaignCheckpointV1 {
            schema_version: current.schema_version,
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

        let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
        assert_eq!(loaded.campaign_id, current.campaign_id);
        assert!(loaded.blocked_task_ids.is_empty());
        assert!(loaded.blocked_reasons.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_loads_integrity_verified_blocked_id_legacy_shape() {
        let root = root("legacy-blocked-ids");
        fs::create_dir_all(&root).unwrap();
        let current = checkpoint();
        let legacy = LegacyCampaignCheckpointWithBlockedIds {
            schema_version: current.schema_version,
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

        let loaded = CampaignCheckpoint::load(&root).unwrap().unwrap();
        assert_eq!(
            loaded.blocked_task_ids,
            BTreeSet::from(["1.1.1.2".to_owned()])
        );
        assert!(loaded.blocked_reasons.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
