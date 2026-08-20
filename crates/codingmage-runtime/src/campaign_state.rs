use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{RunId, WorktreeId};
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
        if envelope.checkpoint.schema_version != SCHEMA_VERSION
            || sha256_hex(&canonical) != envelope.checkpoint_sha256
        {
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
}

fn timestamp_ms() -> Result<u64, RuntimeError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeError::State)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| RuntimeError::State)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    const HEX: &[u8; 16] = b"0123456789abcdef";
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
}
