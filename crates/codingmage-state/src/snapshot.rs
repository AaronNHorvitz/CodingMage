use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use codingmage_contracts::TaskId;
use serde::{Deserialize, Serialize};

use crate::journal::{
    EventKind, EventOutcome, JournalError, JournalRecord, private_directory, sha256_hex,
    sync_directory,
};

const SNAPSHOT_SCHEMA_VERSION: u16 = 1;
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// Current content-minimized projection for one task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProjection {
    /// Last stable phase observed for the task.
    pub phase: String,
    /// Last stable outcome observed for the task.
    pub outcome: EventOutcome,
    /// Last journal sequence affecting the task.
    pub sequence: u64,
}

/// Current state derived only from accepted journal records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// Snapshot schema version.
    pub schema_version: u16,
    /// Last accepted journal sequence, absent for an empty journal.
    pub journal_sequence: Option<u64>,
    /// Last accepted journal hash, absent for an empty journal.
    pub journal_hash: Option<String>,
    /// Per-task projection in canonical identifier order.
    pub tasks: BTreeMap<TaskId, TaskProjection>,
}

impl Snapshot {
    /// Derives state solely from already validated journal records.
    #[must_use]
    pub fn derive(records: &[JournalRecord]) -> Self {
        let mut tasks = BTreeMap::new();
        for record in records {
            let phase = match &record.event.kind {
                EventKind::Transition { phase, .. } | EventKind::EffectObserved { phase } => {
                    phase.clone()
                }
                EventKind::GateObserved { gate_id } => format!("gate.{gate_id}"),
                EventKind::RecoveryBlocked { reason } => format!("blocked.{reason}"),
                EventKind::ControlApplied { action, .. } => format!("control.{action}"),
                EventKind::RetryScheduled { reason, .. } => format!("retry.{reason}"),
                EventKind::ExternalBoundaryChanged { system, change } => {
                    format!("boundary.{system}.{change}")
                }
            };
            tasks.insert(
                record.event.task_id.clone(),
                TaskProjection {
                    phase,
                    outcome: record.event.outcome,
                    sequence: record.sequence,
                },
            );
        }
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            journal_sequence: records.last().map(|record| record.sequence),
            journal_hash: records.last().map(|record| record.record_hash.clone()),
            tasks,
        }
    }

    /// Encodes, flushes, and atomically replaces the current snapshot.
    ///
    /// # Errors
    ///
    /// Returns a bounded encoding or durable I/O error.
    pub fn write_atomic(&self, root: &Path) -> Result<SnapshotEnvelope, SnapshotError> {
        private_directory(root).map_err(SnapshotError::Journal)?;
        let envelope = SnapshotEnvelope::create(self.clone())?;
        let encoded = serde_json::to_vec(&envelope).map_err(|_| SnapshotError::Encoding)?;
        if encoded.len() > MAX_SNAPSHOT_BYTES {
            return Err(SnapshotError::TooLarge);
        }
        let temporary = root.join("snapshot.json.tmp");
        let current = root.join("snapshot.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|_| SnapshotError::Io)?;
        file.write_all(&encoded).map_err(|_| SnapshotError::Io)?;
        file.sync_all().map_err(|_| SnapshotError::Io)?;
        fs::rename(&temporary, &current).map_err(|_| SnapshotError::Io)?;
        sync_directory(root).map_err(SnapshotError::Journal)?;
        Ok(envelope)
    }
}

/// Snapshot plus canonical integrity hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEnvelope {
    /// Derived current state.
    pub snapshot: Snapshot,
    /// SHA-256 of canonical snapshot bytes.
    pub snapshot_hash: String,
}

impl SnapshotEnvelope {
    fn create(snapshot: Snapshot) -> Result<Self, SnapshotError> {
        let bytes = serde_json::to_vec(&snapshot).map_err(|_| SnapshotError::Encoding)?;
        Ok(Self {
            snapshot,
            snapshot_hash: sha256_hex(&bytes),
        })
    }

    /// Loads and validates the snapshot hash and its exact journal position.
    ///
    /// # Errors
    ///
    /// Returns an encoding, integrity, position, size, or I/O error.
    pub fn load(root: &Path, records: &[JournalRecord]) -> Result<Self, SnapshotError> {
        let mut bytes = Vec::new();
        File::open(root.join("snapshot.json"))
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|_| SnapshotError::Io)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(SnapshotError::TooLarge);
        }
        let envelope: Self = serde_json::from_slice(&bytes).map_err(|_| SnapshotError::Encoding)?;
        let canonical =
            serde_json::to_vec(&envelope.snapshot).map_err(|_| SnapshotError::Encoding)?;
        if sha256_hex(&canonical) != envelope.snapshot_hash {
            return Err(SnapshotError::Hash);
        }
        let expected = Snapshot::derive(records);
        if envelope.snapshot.journal_sequence != expected.journal_sequence
            || envelope.snapshot.journal_hash != expected.journal_hash
        {
            return Err(SnapshotError::JournalPosition);
        }
        if envelope.snapshot != expected {
            return Err(SnapshotError::Projection);
        }
        Ok(envelope)
    }
}

/// Snapshot persistence or verification failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// Underlying journal-directory operation failed.
    Journal(JournalError),
    /// Snapshot I/O failed.
    Io,
    /// Snapshot could not be encoded or decoded.
    Encoding,
    /// Snapshot exceeded the stable bound.
    TooLarge,
    /// Snapshot integrity hash did not match.
    Hash,
    /// Snapshot does not identify the journal tip.
    JournalPosition,
    /// Snapshot projection differs from accepted journal events.
    Projection,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Journal(_) => "snapshot directory operation failed",
            Self::Io => "snapshot I/O failed",
            Self::Encoding => "snapshot encoding failed",
            Self::TooLarge => "snapshot exceeds size limit",
            Self::Hash => "snapshot hash is invalid",
            Self::JournalPosition => "snapshot journal position is invalid",
            Self::Projection => "snapshot projection is invalid",
        })
    }
}

impl std::error::Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EffectClass, Journal, JournalEvent, RedactedField};
    use codingmage_contracts::{EvidenceId, RepositoryId, RunId};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-snapshot-{}-{unique}",
            std::process::id()
        ))
    }

    fn event(task: &str, phase: &str, timestamp_ms: u64) -> JournalEvent {
        JournalEvent {
            timestamp_ms,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new(task).unwrap(),
            repository_id: RepositoryId::new("repo-1").unwrap(),
            kind: EventKind::Transition {
                phase: phase.to_owned(),
                effect: EffectClass::ReadOnly,
            },
            outcome: EventOutcome::Succeeded,
            evidence: vec![EvidenceId::new(format!("evidence-{timestamp_ms}")).unwrap()],
            redactions: vec![RedactedField::new("source_content").unwrap()],
        }
    }

    #[test]
    fn atomic_snapshot_round_trips_exact_projection() {
        let root = root();
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event("task-1", "ready", 1)).unwrap();
        journal.append(event("task-1", "review", 2)).unwrap();
        journal.append(event("task-2", "ready", 3)).unwrap();
        let snapshot = Snapshot::derive(journal.records());
        let written = snapshot.write_atomic(&root).unwrap();
        assert_eq!(
            SnapshotEnvelope::load(&root, journal.records()).unwrap(),
            written
        );
        assert!(!root.join("snapshot.json.tmp").exists());
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_hash_projection_and_journal_position_mutations() {
        let root = root();
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event("task-1", "ready", 1)).unwrap();
        Snapshot::derive(journal.records())
            .write_atomic(&root)
            .unwrap();
        let path = root.join("snapshot.json");
        let original = fs::read_to_string(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
        value["snapshot_hash"] = serde_json::Value::String("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            SnapshotEnvelope::load(&root, journal.records()).unwrap_err(),
            SnapshotError::Hash
        );

        let mut forged: SnapshotEnvelope = serde_json::from_str(&original).unwrap();
        forged
            .snapshot
            .tasks
            .get_mut(&TaskId::new("task-1").unwrap())
            .unwrap()
            .phase = "forged".to_owned();
        forged.snapshot_hash = sha256_hex(&serde_json::to_vec(&forged.snapshot).unwrap());
        fs::write(&path, serde_json::to_vec(&forged).unwrap()).unwrap();
        assert_eq!(
            SnapshotEnvelope::load(&root, journal.records()).unwrap_err(),
            SnapshotError::Projection
        );

        Snapshot::derive(journal.records())
            .write_atomic(&root)
            .unwrap();
        journal.append(event("task-1", "review", 2)).unwrap();
        assert_eq!(
            SnapshotEnvelope::load(&root, journal.records()).unwrap_err(),
            SnapshotError::JournalPosition
        );
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abandoned_temporary_write_cannot_replace_last_durable_snapshot() {
        let root = root();
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event("task-1", "ready", 1)).unwrap();
        let expected = Snapshot::derive(journal.records())
            .write_atomic(&root)
            .unwrap();
        fs::write(root.join("snapshot.json.tmp"), b"{torn").unwrap();
        assert_eq!(
            SnapshotEnvelope::load(&root, journal.records()).unwrap(),
            expected
        );
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }
}
