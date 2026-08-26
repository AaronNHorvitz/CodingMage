use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use codingmage_claude::ClaudeCompletionReport;
use codingmage_contracts::{AttemptId, RepositoryId, RunId, TaskId, WorktreeId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RuntimeError;

const SCHEMA_VERSION: u16 = 1;
const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024;
const MAX_PROVIDER_ATTEMPT_SCOPES: usize = 256;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionPhase {
    Prepared,
    ProviderBlocked,
    CommitObserved,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionDiagnosticKind {
    Gate,
    Review,
}

/// Content-minimized identity of the diagnostics delegated to one correction increment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionDiagnosticProjection {
    pub kind: CorrectionDiagnosticKind,
    pub item_count: u16,
    pub included_ids: Vec<String>,
    pub content_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InitialPhase {
    Prepared,
    SessionBound,
    ReportObserved,
    CandidateObserved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialCheckpoint {
    pub schema_version: u16,
    pub repository_id: RepositoryId,
    pub run_id: RunId,
    pub task_id: TaskId,
    pub worktree_id: WorktreeId,
    pub branch: String,
    pub source_commit: String,
    pub session_id: Option<AttemptId>,
    pub phase: InitialPhase,
    pub report: Option<ClaudeCompletionReport>,
    pub candidate_commit: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<CorrectionDiagnosticProjection>,
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InitialCheckpointEnvelope {
    checkpoint: InitialCheckpoint,
    checkpoint_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "stage")]
pub(crate) enum ProviderAttemptScope {
    InitialImplementation {
        run_id: RunId,
        source_commit: String,
        session_id: AttemptId,
    },
    Correction {
        run_id: RunId,
        correction_round: u16,
        parent_commit: String,
        session_id: AttemptId,
    },
    Review {
        run_id: RunId,
        candidate_commit: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderAttemptEntry {
    scope: ProviderAttemptScope,
    attempts: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderAttemptLedger {
    schema_version: u16,
    entries: Vec<ProviderAttemptEntry>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderAttemptLedgerEnvelope {
    ledger: ProviderAttemptLedger,
    ledger_sha256: String,
}

impl ProviderAttemptLedger {
    fn path(run_root: &Path) -> PathBuf {
        run_root.join("provider-attempts.json")
    }

    fn load(run_root: &Path) -> Result<Self, RuntimeError> {
        let path = Self::path(run_root);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    schema_version: SCHEMA_VERSION,
                    entries: Vec::new(),
                });
            }
            Err(_) => return Err(RuntimeError::State),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_CHECKPOINT_BYTES
        {
            return Err(RuntimeError::State);
        }
        let bytes = fs::read(path).map_err(|_| RuntimeError::State)?;
        let envelope: ProviderAttemptLedgerEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| RuntimeError::State)?;
        let canonical = serde_json::to_vec(&envelope.ledger).map_err(|_| RuntimeError::State)?;
        if envelope.ledger.validate().is_err() || sha256_hex(&canonical) != envelope.ledger_sha256 {
            return Err(RuntimeError::State);
        }
        Ok(envelope.ledger)
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != SCHEMA_VERSION
            || self.entries.len() > MAX_PROVIDER_ATTEMPT_SCOPES
            || self.entries.iter().any(|entry| entry.attempts == 0)
        {
            return Err(RuntimeError::State);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if self.entries[..index]
                .iter()
                .any(|prior| prior.scope == entry.scope)
            {
                return Err(RuntimeError::State);
            }
        }
        Ok(())
    }

    fn persist(&self, run_root: &Path) -> Result<(), RuntimeError> {
        self.validate()?;
        private_directory(run_root)?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = ProviderAttemptLedgerEnvelope {
            ledger: self.clone(),
            ledger_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if u64::try_from(bytes.len()).map_err(|_| RuntimeError::State)? > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let temporary = run_root.join(format!(
            ".provider-attempts.{}.{}.tmp",
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
        fs::rename(temporary, Self::path(run_root)).map_err(|_| RuntimeError::State)?;
        File::open(run_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)
    }

    pub(crate) fn reserve(
        run_root: &Path,
        scope: ProviderAttemptScope,
        limit: u8,
    ) -> Result<u8, RuntimeError> {
        if limit == 0 {
            return Err(RuntimeError::State);
        }
        let mut ledger = Self::load(run_root)?;
        if let Some(entry) = ledger.entries.iter_mut().find(|entry| entry.scope == scope) {
            if entry.attempts >= limit {
                return Err(RuntimeError::ProviderAttemptLimit);
            }
            entry.attempts = entry.attempts.checked_add(1).ok_or(RuntimeError::State)?;
            let attempts = entry.attempts;
            ledger.persist(run_root)?;
            return Ok(attempts);
        }
        if ledger.entries.len() >= MAX_PROVIDER_ATTEMPT_SCOPES {
            return Err(RuntimeError::State);
        }
        ledger
            .entries
            .push(ProviderAttemptEntry { scope, attempts: 1 });
        ledger.persist(run_root)?;
        Ok(1)
    }
}

impl InitialCheckpoint {
    pub(crate) fn new(
        repository_id: RepositoryId,
        run_id: RunId,
        task_id: TaskId,
        worktree_id: WorktreeId,
        branch: String,
        source_commit: String,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            repository_id,
            run_id,
            task_id,
            worktree_id,
            branch,
            source_commit,
            session_id: None,
            phase: InitialPhase::Prepared,
            report: None,
            candidate_commit: None,
        }
    }

    pub(crate) fn path(run_root: &Path) -> PathBuf {
        run_root.join("initial-checkpoint.json")
    }

    pub(crate) fn load(run_root: &Path) -> Result<Option<Self>, RuntimeError> {
        let path = Self::path(run_root);
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
        let bytes = fs::read(path).map_err(|_| RuntimeError::State)?;
        let envelope: InitialCheckpointEnvelope =
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

    pub(crate) fn persist(&self, run_root: &Path) -> Result<(), RuntimeError> {
        private_directory(run_root)?;
        let canonical = serde_json::to_vec(self).map_err(|_| RuntimeError::State)?;
        let envelope = InitialCheckpointEnvelope {
            checkpoint: self.clone(),
            checkpoint_sha256: sha256_hex(&canonical),
        };
        let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| RuntimeError::State)?;
        if u64::try_from(bytes.len()).map_err(|_| RuntimeError::State)? > MAX_CHECKPOINT_BYTES {
            return Err(RuntimeError::State);
        }
        let temporary = run_root.join(format!(
            ".initial-checkpoint.{}.{}.tmp",
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
        fs::rename(temporary, Self::path(run_root)).map_err(|_| RuntimeError::State)?;
        File::open(run_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RuntimeError::State)
    }

    pub(crate) fn validate(
        &self,
        repository_id: &RepositoryId,
        run_id: &RunId,
        task_id: &TaskId,
        worktree_id: &WorktreeId,
        branch: &str,
        source_commit: &str,
    ) -> Result<(), RuntimeError> {
        if self.schema_version != SCHEMA_VERSION
            || &self.repository_id != repository_id
            || &self.run_id != run_id
            || &self.task_id != task_id
            || &self.worktree_id != worktree_id
            || self.branch != branch
            || self.source_commit != source_commit
            || self.phase == InitialPhase::Prepared
                && (self.session_id.is_some()
                    || self.report.is_some()
                    || self.candidate_commit.is_some())
            || self.phase == InitialPhase::SessionBound
                && (self.session_id.is_none()
                    || self.report.is_some()
                    || self.candidate_commit.is_some())
            || self.phase == InitialPhase::ReportObserved
                && (self.session_id.is_none()
                    || self.report.is_none()
                    || self.candidate_commit.is_some())
            || self.phase == InitialPhase::CandidateObserved
                && (self.session_id.is_none()
                    || self.report.is_none()
                    || self.candidate_commit.is_none())
            || self
                .report
                .as_ref()
                .is_some_and(|report| report.validate().is_err())
        {
            return Err(RuntimeError::State);
        }
        Ok(())
    }
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
            diagnostic: None,
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
        if self
            .diagnostic
            .as_ref()
            .is_some_and(|diagnostic| diagnostic.validate().is_err())
        {
            return Err(RuntimeError::State);
        }
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
            || self
                .diagnostic
                .as_ref()
                .is_some_and(|diagnostic| diagnostic.validate().is_err())
        {
            return Err(RuntimeError::State);
        }
        Ok(())
    }

    pub(crate) fn bind_diagnostic(
        &mut self,
        diagnostic: CorrectionDiagnosticProjection,
    ) -> Result<(), RuntimeError> {
        diagnostic.validate()?;
        match &self.diagnostic {
            Some(existing) if existing != &diagnostic => Err(RuntimeError::State),
            Some(_) => Ok(()),
            None => {
                self.diagnostic = Some(diagnostic);
                Ok(())
            }
        }
    }
}

impl CorrectionDiagnosticProjection {
    pub(crate) fn new(
        kind: CorrectionDiagnosticKind,
        item_count: usize,
        included_ids: Vec<String>,
        content: &[u8],
    ) -> Result<Self, RuntimeError> {
        let projection = Self {
            kind,
            item_count: u16::try_from(item_count).map_err(|_| RuntimeError::State)?,
            included_ids,
            content_sha256: sha256_hex(content),
        };
        projection.validate()?;
        Ok(projection)
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        let unique = self.included_ids.iter().collect::<BTreeSet<_>>();
        if self.item_count == 0
            || self.included_ids.is_empty()
            || self.included_ids.len() > 4
            || usize::from(self.item_count) < self.included_ids.len()
            || unique.len() != self.included_ids.len()
            || self.included_ids.iter().any(|id| {
                id.is_empty()
                    || id.len() > 128
                    || !id.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            })
            || self.content_sha256.len() != 64
            || !self
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

    fn initial_checkpoint() -> InitialCheckpoint {
        InitialCheckpoint::new(
            RepositoryId::new("repo-1").unwrap(),
            RunId::new("run-1").unwrap(),
            TaskId::new("20.1.3.4").unwrap(),
            WorktreeId::new("worktree-1").unwrap(),
            "codingmage/task".to_owned(),
            "a".repeat(40),
        )
    }

    #[test]
    fn initial_checkpoint_round_trips_and_every_identity_is_load_bearing() {
        let root = root("initial-round-trip");
        let mut checkpoint = initial_checkpoint();
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            InitialCheckpoint::load(&root).unwrap(),
            Some(checkpoint.clone())
        );
        checkpoint.session_id =
            Some(AttemptId::new("123e4567-e89b-12d3-a456-426614174000").unwrap());
        checkpoint.phase = InitialPhase::SessionBound;
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            InitialCheckpoint::load(&root).unwrap(),
            Some(checkpoint.clone())
        );
        checkpoint.report = Some(ClaudeCompletionReport {
            changed_paths: vec![PathBuf::from("src/lib.rs")],
            tests: Vec::new(),
            commit: None,
            ready_for_commit: true,
            limitations: Vec::new(),
            blocker_code: None,
        });
        checkpoint.phase = InitialPhase::ReportObserved;
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            InitialCheckpoint::load(&root).unwrap(),
            Some(checkpoint.clone())
        );
        checkpoint.candidate_commit = Some("b".repeat(40));
        checkpoint.phase = InitialPhase::CandidateObserved;
        checkpoint.persist(&root).unwrap();
        assert_eq!(
            InitialCheckpoint::load(&root).unwrap(),
            Some(checkpoint.clone())
        );
        checkpoint
            .validate(
                &checkpoint.repository_id,
                &checkpoint.run_id,
                &checkpoint.task_id,
                &checkpoint.worktree_id,
                &checkpoint.branch,
                &checkpoint.source_commit,
            )
            .unwrap();

        for field in 0..12 {
            let mut changed = checkpoint.clone();
            match field {
                0 => changed.repository_id = RepositoryId::new("repo-2").unwrap(),
                1 => changed.run_id = RunId::new("run-2").unwrap(),
                2 => changed.task_id = TaskId::new("20.1.3.5").unwrap(),
                3 => changed.worktree_id = WorktreeId::new("worktree-2").unwrap(),
                4 => changed.branch.push_str("-changed"),
                5 => changed.source_commit = "c".repeat(40),
                6 => changed.session_id = None,
                7 => changed.phase = InitialPhase::Prepared,
                8 => changed.report = None,
                9 => changed.candidate_commit = None,
                10 => {
                    changed.report.as_mut().unwrap().changed_paths =
                        vec![PathBuf::from("../escape")];
                }
                _ => changed.schema_version = 2,
            }
            assert!(
                changed
                    .validate(
                        &checkpoint.repository_id,
                        &checkpoint.run_id,
                        &checkpoint.task_id,
                        &checkpoint.worktree_id,
                        &checkpoint.branch,
                        &checkpoint.source_commit,
                    )
                    .is_err()
                    || changed.schema_version != SCHEMA_VERSION
            );
        }

        let path = InitialCheckpoint::path(&root);
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'a').unwrap();
        bytes[index] = b'c';
        fs::write(path, bytes).unwrap();
        assert_eq!(InitialCheckpoint::load(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
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
    fn diagnostic_projection_is_private_content_minimized_and_load_bearing() {
        let root = root("diagnostic-projection");
        let secret_prose = b"SECRET reviewer prose and source content must not persist";
        let projection = CorrectionDiagnosticProjection::new(
            CorrectionDiagnosticKind::Review,
            6,
            vec![
                "FIX-1".to_owned(),
                "FIX-2".to_owned(),
                "FIX-3".to_owned(),
                "FIX-4".to_owned(),
            ],
            secret_prose,
        )
        .unwrap();
        let mut checkpoint = checkpoint();
        checkpoint.bind_diagnostic(projection.clone()).unwrap();
        checkpoint.persist(&root).unwrap();

        let bytes = fs::read(CorrectionCheckpoint::path(&root, 1)).unwrap();
        assert!(
            !bytes
                .windows(secret_prose.len())
                .any(|window| window == secret_prose)
        );
        let mut loaded = CorrectionCheckpoint::load(&root, 1).unwrap().unwrap();
        assert_eq!(loaded.diagnostic, Some(projection.clone()));
        assert_eq!(loaded.bind_diagnostic(projection.clone()), Ok(()));

        let conflicting = CorrectionDiagnosticProjection::new(
            CorrectionDiagnosticKind::Review,
            1,
            vec!["OTHER-1".to_owned()],
            b"other",
        )
        .unwrap();
        assert_eq!(
            loaded.clone().bind_diagnostic(conflicting),
            Err(RuntimeError::State)
        );

        let mut malformed = projection;
        malformed.included_ids.push("FIX-1".to_owned());
        let mut invalid = checkpoint;
        invalid.diagnostic = Some(malformed);
        assert_eq!(invalid.persist(&root), Err(RuntimeError::State));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_attempts_are_durable_bounded_and_scoped() {
        let root = root("provider-attempts");
        let run_id = RunId::new("run-1").unwrap();
        let correction = ProviderAttemptScope::Correction {
            run_id: run_id.clone(),
            correction_round: 2,
            parent_commit: "b".repeat(40),
            session_id: AttemptId::new("123e4567-e89b-12d3-a456-426614174000").unwrap(),
        };
        assert_eq!(
            ProviderAttemptLedger::reserve(&root, correction.clone(), 3),
            Ok(1)
        );
        assert_eq!(
            ProviderAttemptLedger::reserve(&root, correction.clone(), 3),
            Ok(2)
        );
        assert_eq!(
            ProviderAttemptLedger::reserve(&root, correction.clone(), 3),
            Ok(3)
        );
        assert_eq!(
            ProviderAttemptLedger::reserve(&root, correction, 3),
            Err(RuntimeError::ProviderAttemptLimit)
        );

        let review = ProviderAttemptScope::Review {
            run_id,
            candidate_commit: "c".repeat(40),
        };
        assert_eq!(ProviderAttemptLedger::reserve(&root, review, 3), Ok(1));

        let path = ProviderAttemptLedger::path(&root);
        let mut bytes = fs::read(&path).unwrap();
        let index = bytes.iter().position(|byte| *byte == b'c').unwrap();
        bytes[index] = b'd';
        fs::write(path, bytes).unwrap();
        assert_eq!(ProviderAttemptLedger::load(&root), Err(RuntimeError::State));
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
