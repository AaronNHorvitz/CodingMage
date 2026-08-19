use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use codingmage_contracts::{EvidenceId, RepositoryId, RunId, TaskId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::snapshot::{Snapshot, SnapshotEnvelope, SnapshotError};

/// Maximum encoded size of one journal record, including its line terminator.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
const SCHEMA_VERSION: u16 = 1;
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Whether a phase can be retried after an interrupted attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    /// The phase only observes existing state.
    ReadOnly,
    /// Repetition has the same externally visible result.
    Idempotent,
    /// Repetition could duplicate or contradict an external effect.
    StateChanging,
}

/// Stable journal event categories.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum EventKind {
    /// A lifecycle transition was accepted.
    Transition {
        /// Stable phase name without source or prompt content.
        phase: String,
        /// Recovery semantics of the phase.
        effect: EffectClass,
    },
    /// An external effect was observed after execution.
    EffectObserved {
        /// Stable phase name.
        phase: String,
    },
    /// A deterministic gate produced evidence.
    GateObserved {
        /// Stable gate identifier.
        gate_id: String,
    },
    /// Recovery made an uncertainty visible.
    RecoveryBlocked {
        /// Stable machine-readable reason.
        reason: String,
    },
    /// An authenticated local lifecycle control was durably accepted.
    ControlApplied {
        /// Idempotency identity of the request.
        request_id: String,
        /// Stable lifecycle action.
        action: String,
    },
    /// A bounded provider retry or terminal stop was scheduled.
    RetryScheduled {
        /// One-based retry attempt.
        attempt: u32,
        /// Next Unix millisecond, absent for a terminal stop.
        next_at_ms: Option<u64>,
        /// Stable capacity reason.
        reason: String,
    },
}

/// Stable event result, never provider prose.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    /// Operation completed and was observed.
    Succeeded,
    /// Operation did not complete.
    Failed,
    /// Operation cannot continue yet.
    Blocked,
    /// Completion cannot be established safely.
    Uncertain,
}

/// Marker proving a named sensitive field was removed before persistence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedField {
    /// Stable field category, such as `provider_output`.
    pub category: String,
}

impl RedactedField {
    /// Creates a marker after validating its content-free category.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError::InvalidField`] for a noncanonical category.
    pub fn new(category: impl Into<String>) -> Result<Self, JournalError> {
        let category = category.into();
        validate_label(&category)?;
        Ok(Self { category })
    }
}

/// Caller-supplied content-minimized event payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEvent {
    /// Event timestamp as Unix milliseconds from the caller's trusted clock.
    pub timestamp_ms: u64,
    /// Exact run identity.
    pub run_id: RunId,
    /// Exact task identity.
    pub task_id: TaskId,
    /// Exact repository identity.
    pub repository_id: RepositoryId,
    /// Typed event kind.
    pub kind: EventKind,
    /// Typed result.
    pub outcome: EventOutcome,
    /// Immutable evidence references only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceId>,
    /// Categories removed by the caller before persistence; removed values are never accepted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redactions: Vec<RedactedField>,
}

/// Canonical append-only record including chain metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalRecord {
    /// Monotonic zero-based sequence.
    pub sequence: u64,
    /// Record schema version.
    pub schema_version: u16,
    /// SHA-256 of the previous canonical record, or the genesis hash.
    pub prior_hash: String,
    /// Content-minimized event.
    pub event: JournalEvent,
    /// SHA-256 of the canonical record fields excluding this field.
    pub record_hash: String,
}

#[derive(Serialize)]
struct HashableRecord<'a> {
    sequence: u64,
    schema_version: u16,
    prior_hash: &'a str,
    event: &'a JournalEvent,
}

impl JournalRecord {
    fn create(
        sequence: u64,
        prior_hash: String,
        event: JournalEvent,
    ) -> Result<Self, JournalError> {
        validate_event(&event)?;
        let hashable = HashableRecord {
            sequence,
            schema_version: SCHEMA_VERSION,
            prior_hash: &prior_hash,
            event: &event,
        };
        let canonical = serde_json::to_vec(&hashable).map_err(|_| JournalError::Encoding)?;
        let record_hash = sha256_hex(&canonical);
        Ok(Self {
            sequence,
            schema_version: SCHEMA_VERSION,
            prior_hash,
            event,
            record_hash,
        })
    }

    fn verify(&self, sequence: u64, prior_hash: &str) -> Result<(), JournalError> {
        if self.sequence != sequence {
            return Err(JournalError::Sequence { sequence });
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(JournalError::Schema { sequence });
        }
        if self.prior_hash != prior_hash {
            return Err(JournalError::Chain { sequence });
        }
        validate_event(&self.event)?;
        let canonical = serde_json::to_vec(&HashableRecord {
            sequence: self.sequence,
            schema_version: self.schema_version,
            prior_hash: &self.prior_hash,
            event: &self.event,
        })
        .map_err(|_| JournalError::Encoding)?;
        if sha256_hex(&canonical) != self.record_hash {
            return Err(JournalError::Hash { sequence });
        }
        Ok(())
    }
}

/// Exclusive exact-owner lock for one journal writer.
#[derive(Debug)]
pub struct JournalLock {
    file: File,
}

impl JournalLock {
    /// Acquires a nonblocking OS lock containing a caller-generated opaque owner token.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError::Locked`] if another owner exists, or an I/O/validation error.
    pub fn acquire(root: &Path, token: impl Into<String>) -> Result<Self, JournalError> {
        let token = token.into();
        validate_label(&token)?;
        private_directory(root)?;
        let path = root.join("journal.lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| JournalError::Io)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => JournalError::Locked,
            std::fs::TryLockError::Error(_) => JournalError::Io,
        })?;
        file.set_len(0).map_err(|_| JournalError::Io)?;
        file.write_all(token.as_bytes())
            .map_err(|_| JournalError::Io)?;
        file.sync_all().map_err(|_| JournalError::Io)?;
        sync_directory(root)?;
        Ok(Self { file })
    }
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.file);
    }
}

/// Open durable journal held by one exact lock owner.
#[derive(Debug)]
pub struct Journal {
    root: PathBuf,
    path: PathBuf,
    records: Vec<JournalRecord>,
    _lock: JournalLock,
}

impl Journal {
    /// Opens and fully validates a journal under an acquired exact-owner lock.
    ///
    /// # Errors
    ///
    /// Returns the exact first validation category or a lock/I/O failure.
    pub fn open(root: &Path, token: impl Into<String>) -> Result<Self, JournalError> {
        let lock = JournalLock::acquire(root, token)?;
        let path = root.join("events.jsonl");
        let records = load_records(&path)?;
        Ok(Self {
            root: root.to_path_buf(),
            path,
            records,
            _lock: lock,
        })
    }

    /// Returns all accepted records.
    #[must_use]
    pub fn records(&self) -> &[JournalRecord] {
        &self.records
    }

    /// Appends one canonical record and synchronizes it before returning.
    ///
    /// # Errors
    ///
    /// Returns a validation, size, encoding, or durable I/O error.
    pub fn append(&mut self, event: JournalEvent) -> Result<&JournalRecord, JournalError> {
        let sequence = u64::try_from(self.records.len())
            .map_err(|_| JournalError::Sequence { sequence: u64::MAX })?;
        let prior_hash = self.records.last().map_or_else(
            || GENESIS_HASH.to_owned(),
            |record| record.record_hash.clone(),
        );
        let record = JournalRecord::create(sequence, prior_hash, event)?;
        let mut encoded = serde_json::to_vec(&record).map_err(|_| JournalError::Encoding)?;
        encoded.push(b'\n');
        if encoded.len() > MAX_RECORD_BYTES {
            return Err(JournalError::TooLarge { sequence });
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|_| JournalError::Io)?;
        file.write_all(&encoded).map_err(|_| JournalError::Io)?;
        file.sync_all().map_err(|_| JournalError::Io)?;
        sync_directory(&self.root)?;
        self.records.push(record);
        Ok(&self.records[self.records.len() - 1])
    }

    /// Derives and atomically persists a snapshot at the current accepted journal tip.
    ///
    /// # Errors
    ///
    /// Returns a snapshot encoding, integrity, or durable I/O error.
    pub fn write_snapshot(&self) -> Result<SnapshotEnvelope, SnapshotError> {
        Snapshot::derive(&self.records).write_atomic(&self.root)
    }
}

fn load_records(path: &Path) -> Result<Vec<JournalRecord>, JournalError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(JournalError::Io),
    };
    let mut records = Vec::new();
    let mut prior = GENESIS_HASH.to_owned();
    for (index, line) in BufReader::new(file).split(b'\n').enumerate() {
        let line = line.map_err(|_| JournalError::Io)?;
        if line.is_empty() {
            continue;
        }
        let sequence =
            u64::try_from(index).map_err(|_| JournalError::Sequence { sequence: u64::MAX })?;
        if line.len() + 1 > MAX_RECORD_BYTES {
            return Err(JournalError::TooLarge { sequence });
        }
        let record: JournalRecord =
            serde_json::from_slice(&line).map_err(|_| JournalError::Corrupt { sequence })?;
        record.verify(sequence, &prior)?;
        prior.clone_from(&record.record_hash);
        records.push(record);
    }
    Ok(records)
}

fn validate_event(event: &JournalEvent) -> Result<(), JournalError> {
    match &event.kind {
        EventKind::Transition { phase, .. } | EventKind::EffectObserved { phase } => {
            validate_label(phase)?;
        }
        EventKind::GateObserved { gate_id } => validate_label(gate_id)?,
        EventKind::RecoveryBlocked { reason } | EventKind::RetryScheduled { reason, .. } => {
            validate_label(reason)?;
        }
        EventKind::ControlApplied { request_id, action } => {
            validate_label(request_id)?;
            validate_label(action)?;
        }
    }
    for redaction in &event.redactions {
        validate_label(&redaction.category)?;
    }
    Ok(())
}

fn validate_label(value: &str) -> Result<(), JournalError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(JournalError::InvalidField);
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(crate) fn private_directory(path: &Path) -> Result<(), JournalError> {
    fs::create_dir_all(path).map_err(|_| JournalError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| JournalError::Io)?;
    }
    Ok(())
}

pub(crate) fn sync_directory(path: &Path) -> Result<(), JournalError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| JournalError::Io)
}

/// Journal persistence or validation failure without sensitive content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalError {
    /// Another writer owns the journal.
    Locked,
    /// Durable file operation failed.
    Io,
    /// Serialization failed.
    Encoding,
    /// Stable label validation failed.
    InvalidField,
    /// Record exceeded the public bound.
    TooLarge {
        /// First affected sequence.
        sequence: u64,
    },
    /// JSON or required fields were malformed at the sequence.
    Corrupt {
        /// First affected sequence.
        sequence: u64,
    },
    /// Sequence was duplicate, missing, or reordered.
    Sequence {
        /// First affected sequence.
        sequence: u64,
    },
    /// Schema version is unsupported.
    Schema {
        /// First affected sequence.
        sequence: u64,
    },
    /// Prior-record hash did not match.
    Chain {
        /// First affected sequence.
        sequence: u64,
    },
    /// Record hash did not match canonical content.
    Hash {
        /// First affected sequence.
        sequence: u64,
    },
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Locked => "journal is owned by another writer",
            Self::Io => "journal I/O failed",
            Self::Encoding => "journal encoding failed",
            Self::InvalidField => "journal field is invalid",
            Self::TooLarge { .. } => "journal record exceeds size limit",
            Self::Corrupt { .. } => "journal record is corrupt or truncated",
            Self::Sequence { .. } => "journal sequence is invalid",
            Self::Schema { .. } => "journal schema is unsupported",
            Self::Chain { .. } => "journal hash chain is broken",
            Self::Hash { .. } => "journal record hash is invalid",
        })
    }
}

impl std::error::Error for JournalError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-state-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn event(timestamp_ms: u64) -> JournalEvent {
        JournalEvent {
            timestamp_ms,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            repository_id: RepositoryId::new("repo-1").unwrap(),
            kind: EventKind::Transition {
                phase: "implementing".to_owned(),
                effect: EffectClass::StateChanging,
            },
            outcome: EventOutcome::Succeeded,
            evidence: vec![EvidenceId::new(format!("evidence-{timestamp_ms}")).unwrap()],
            redactions: vec![RedactedField::new("provider_output").unwrap()],
        }
    }

    #[test]
    fn lock_holder_subprocess() {
        let Ok(root) = std::env::var("CODINGMAGE_TEST_LOCK_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let _journal = Journal::open(&root, "child-owner").unwrap();
        fs::write(root.join("child-ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn appends_reopens_and_enforces_exact_writer() {
        let root = root("round-trip");
        let mut journal = Journal::open(&root, "owner-one").unwrap();
        assert_eq!(journal.append(event(1)).unwrap().sequence, 0);
        assert_eq!(journal.append(event(2)).unwrap().sequence, 1);
        assert_eq!(
            Journal::open(&root, "owner-two").unwrap_err(),
            JournalError::Locked
        );
        drop(journal);
        let reopened = Journal::open(&root, "owner-two").unwrap();
        assert_eq!(reopened.records().len(), 2);
        assert_eq!(
            reopened.records()[1].prior_hash,
            reopened.records()[0].record_hash
        );
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lock_releases_automatically_and_never_removes_owner_record() {
        let root = root("lock-replacement");
        let lock = JournalLock::acquire(&root, "owner-one").unwrap();
        assert_eq!(
            JournalLock::acquire(&root, "owner-two").unwrap_err(),
            JournalError::Locked
        );
        drop(lock);
        let second = JournalLock::acquire(&root, "owner-two").unwrap();
        assert_eq!(
            fs::read_to_string(root.join("journal.lock")).unwrap(),
            "owner-two"
        );
        drop(second);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hard_process_exit_releases_writer_ownership() {
        let root = root("crash-lock");
        fs::create_dir_all(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::tests::lock_holder_subprocess",
                "--nocapture",
            ])
            .env("CODINGMAGE_TEST_LOCK_ROOT", &root)
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if root.join("child-ready").exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(root.join("child-ready").exists());
        assert_eq!(
            Journal::open(&root, "parent-owner").unwrap_err(),
            JournalError::Locked
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let recovered = Journal::open(&root, "parent-owner").unwrap();
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_each_field_mutation_and_unknown_field() {
        let root = root("mutations");
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event(1)).unwrap();
        drop(journal);
        let path = root.join("events.jsonl");
        let original = fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(original.trim()).unwrap();
        let mutations = [
            ("/sequence", serde_json::Value::from(1)),
            ("/schema_version", serde_json::Value::from(2)),
            ("/prior_hash", serde_json::Value::String("1".repeat(64))),
            ("/event/timestamp_ms", serde_json::Value::from(2)),
            (
                "/event/run_id",
                serde_json::Value::String("run-2".to_owned()),
            ),
            (
                "/event/task_id",
                serde_json::Value::String("task-2".to_owned()),
            ),
            (
                "/event/repository_id",
                serde_json::Value::String("repo-2".to_owned()),
            ),
            (
                "/event/kind/transition/phase",
                serde_json::Value::String("review".to_owned()),
            ),
            (
                "/event/kind/transition/effect",
                serde_json::Value::String("read_only".to_owned()),
            ),
            (
                "/event/outcome",
                serde_json::Value::String("blocked".to_owned()),
            ),
            (
                "/event/evidence/0",
                serde_json::Value::String("evidence-other".to_owned()),
            ),
            (
                "/event/redactions/0/category",
                serde_json::Value::String("source_content".to_owned()),
            ),
            ("/record_hash", serde_json::Value::String("2".repeat(64))),
        ];
        for (pointer, replacement) in mutations {
            let mut mutated = value.clone();
            *mutated.pointer_mut(pointer).unwrap() = replacement;
            fs::write(
                &path,
                format!("{}\n", serde_json::to_string(&mutated).unwrap()),
            )
            .unwrap();
            assert!(
                Journal::open(&root, format!("mutator-{}", pointer.replace('/', "-"))).is_err()
            );
        }
        let mut unknown = value;
        unknown
            .as_object_mut()
            .unwrap()
            .insert("future_critical".to_owned(), 1.into());
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&unknown).unwrap()),
        )
        .unwrap();
        assert!(matches!(
            Journal::open(&root, "unknown"),
            Err(JournalError::Corrupt { sequence: 0 })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_torn_duplicate_reordered_and_chain_broken_records() {
        let root = root("corruption");
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event(1)).unwrap();
        journal.append(event(2)).unwrap();
        drop(journal);
        let path = root.join("events.jsonl");
        let original = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = original.lines().collect();
        for content in [
            format!("{}\n{{\"sequence\":", lines[0]),
            format!("{}\n{}\n", lines[0], lines[0]),
            format!("{}\n{}\n", lines[1], lines[0]),
        ] {
            fs::write(&path, content).unwrap();
            assert!(Journal::open(&root, "reader").is_err());
        }
        let mut second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        second["prior_hash"] = serde_json::Value::String(GENESIS_HASH.to_owned());
        fs::write(
            &path,
            format!(
                "{}\n{}\n",
                lines[0],
                serde_json::to_string(&second).unwrap()
            ),
        )
        .unwrap();
        assert_eq!(
            Journal::open(&root, "reader").unwrap_err(),
            JournalError::Chain { sequence: 1 }
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persists_only_redaction_markers_and_enforces_size_bound() {
        let root = root("redaction");
        let mut journal = Journal::open(&root, "owner").unwrap();
        journal.append(event(1)).unwrap();
        let persisted = fs::read_to_string(root.join("events.jsonl")).unwrap();
        assert!(persisted.contains("provider_output"));
        assert!(!persisted.contains("raw provider transcript"));
        let mut oversized = event(2);
        oversized.evidence = (0..6_000)
            .map(|index| EvidenceId::new(format!("evidence-{index:04}")).unwrap())
            .collect();
        assert!(matches!(
            journal.append(oversized),
            Err(JournalError::TooLarge { sequence: 1 })
        ));
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }
}
