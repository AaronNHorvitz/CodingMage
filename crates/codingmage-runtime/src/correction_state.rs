use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use codingmage_contracts::{AttemptId, RepositoryId, RunId, TaskId, WorktreeId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RuntimeError;

const SCHEMA_VERSION: u16 = 1;
const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionPhase {
    Prepared,
    ProviderBlocked,
    CommitObserved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionCheckpoint {
    pub schema_version: u16,
    pub repository_id: RepositoryId,
    pub run_id: RunId,
    pub task_id: TaskId,
    pub worktree_id: WorktreeId,
    pub branch: String,
    pub source_commit: String,
    pub parent_commit: String,
    pub session_id: AttemptId,
    pub correction_round: u16,
    pub phase: CorrectionPhase,
    pub blocker_code: Option<String>,
    pub correction_commit: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CheckpointEnvelope {
    checkpoint: CorrectionCheckpoint,
    checkpoint_sha256: String,
}

impl CorrectionCheckpoint {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        repository_id: RepositoryId,
        run_id: RunId,
        task_id: TaskId,
        worktree_id: WorktreeId,
        branch: String,
        source_commit: String,
        parent_commit: String,
        session_id: AttemptId,
        correction_round: u16,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            repository_id,
            run_id,
            task_id,
            worktree_id,
            branch,
            source_commit,
            parent_commit,
            session_id,
            correction_round,
            phase: CorrectionPhase::Prepared,
            blocker_code: None,
            correction_commit: None,
        }
    }

    pub(crate) fn path(run_root: &Path, correction_round: u16) -> PathBuf {
        run_root.join(format!("correction-{correction_round}-checkpoint.json"))
    }

    pub(crate) fn load(
        run_root: &Path,
        correction_round: u16,
    ) -> Result<Option<Self>, RuntimeError> {
        let path = Self::path(run_root, correction_round);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeError::State),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_CHECKPOINT_BYTES
        {
            return Err(RuntimeError::State);
        }
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|_| RuntimeError::State)?;
        let envelope: CheckpointEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
        if envelope.checkpoint.schema_version != SCHEMA_VERSION {
            return Err(RuntimeError::State);
        }
        let canonical =
            serde_json::to_vec(&envelope.checkpoint).map_err(|_| RuntimeError::State)?;
        if sha256_hex(&canonical) != envelope.checkpoint_sha256 {
            return Err(RuntimeError::State);
        }
        Ok(Some(envelope.checkpoint))
    }

    pub(crate) fn latest(run_root: &Path) -> Result<Option<Self>, RuntimeError> {
        let entries = match fs::read_dir(run_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeError::State),
        };
        let mut latest = None;
        for entry in entries {
            let entry = entry.map_err(|_| RuntimeError::State)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(round) = name
                .strip_prefix("correction-")
                .and_then(|value| value.strip_suffix("-checkpoint.json"))
                .and_then(|value| value.parse::<u16>().ok())
            else {
                continue;
            };
            if round == 0 || round > 100 {
                return Err(RuntimeError::State);
            }
            let checkpoint = Self::load(run_root, round)?.ok_or(RuntimeError::State)?;
            if latest
                .as_ref()
                .is_none_or(|current: &Self| current.correction_round < round)
            {
                latest = Some(checkpoint);
            }
        }
        Ok(latest)
    }

    pub(crate) fn persist(&self, run_root: &Path) -> Result<(), RuntimeError> {
        private_directory(run_root)?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = CheckpointEnvelope {
            checkpoint: self.clone(),
            checkpoint_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if u64::try_from(bytes.len()).map_err(|_| RuntimeError::State)? > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let temporary = run_root.join(format!(
            ".correction-checkpoint.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| RuntimeError::State)?;
        set_file_private(&file)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RuntimeError::State)?;
        fs::rename(temporary, Self::path(run_root, self.correction_round))
            .map_err(|_| RuntimeError::State)?;
        File::open(run_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate(
        &self,
        repository_id: &RepositoryId,
        run_id: &RunId,
        task_id: &TaskId,
        worktree_id: &WorktreeId,
        branch: &str,
        source_commit: &str,
        parent_commit: &str,
        correction_round: u16,
    ) -> Result<(), RuntimeError> {
        if &self.repository_id != repository_id
            || &self.run_id != run_id
            || &self.task_id != task_id
            || &self.worktree_id != worktree_id
            || self.branch != branch
            || self.source_commit != source_commit
            || self.parent_commit != parent_commit
            || self.correction_round != correction_round
        {
            return Err(RuntimeError::State);
        }
        Ok(())
    }
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
            "codingmage-correction-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn checkpoint() -> CorrectionCheckpoint {
        CorrectionCheckpoint::new(
            RepositoryId::new("repo-1").unwrap(),
            RunId::new("run-1").unwrap(),
            TaskId::new("20.1.3.4").unwrap(),
            WorktreeId::new("worktree-1").unwrap(),
            "codingmage/task".to_owned(),
            "a".repeat(40),
            "b".repeat(40),
            AttemptId::new("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            1,
        )
    }

    #[test]
    fn checkpoint_round_trips_and_rejects_mutation() {
        let root = root("round-trip");
        let checkpoint = checkpoint();
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            CorrectionCheckpoint::load(&root, 1).unwrap(),
            Some(checkpoint)
        );

        let path = CorrectionCheckpoint::path(&root, 1);
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'a').unwrap();
        bytes[index] = b'c';
        fs::write(path, bytes).unwrap();
        assert_eq!(
            CorrectionCheckpoint::load(&root, 1),
            Err(RuntimeError::State)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn latest_selects_only_the_highest_integrity_checked_round() {
        let root = root("latest");
        let first = checkpoint();
        first.persist(&root).unwrap();
        let mut second = first.clone();
        second.correction_round = 2;
        second.session_id = AttemptId::new("123e4567-e89b-12d3-a456-426614174001").unwrap();
        second.persist(&root).unwrap();
        assert_eq!(CorrectionCheckpoint::latest(&root).unwrap(), Some(second));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_exact_identity_is_load_bearing() {
        let checkpoint = checkpoint();
        checkpoint
            .validate(
                &checkpoint.repository_id,
                &checkpoint.run_id,
                &checkpoint.task_id,
                &checkpoint.worktree_id,
                &checkpoint.branch,
                &checkpoint.source_commit,
                &checkpoint.parent_commit,
                checkpoint.correction_round,
            )
            .unwrap();

        for field in 0..8 {
            let mut changed = checkpoint.clone();
            match field {
                0 => changed.repository_id = RepositoryId::new("repo-2").unwrap(),
                1 => changed.run_id = RunId::new("run-2").unwrap(),
                2 => changed.task_id = TaskId::new("20.1.3.5").unwrap(),
                3 => changed.worktree_id = WorktreeId::new("worktree-2").unwrap(),
                4 => changed.branch.push_str("-changed"),
                5 => changed.source_commit = "c".repeat(40),
                6 => changed.parent_commit = "d".repeat(40),
                _ => changed.correction_round = 2,
            }
            assert_eq!(
                changed.validate(
                    &checkpoint.repository_id,
                    &checkpoint.run_id,
                    &checkpoint.task_id,
                    &checkpoint.worktree_id,
                    &checkpoint.branch,
                    &checkpoint.source_commit,
                    &checkpoint.parent_commit,
                    checkpoint.correction_round,
                ),
                Err(RuntimeError::State)
            );
        }
    }
}
