//! Real durable records behind the changes and reviews screen.
//!
//! Run checkpoints and journals are read from the campaign's private state with the existing
//! record types. Nothing here interprets missing evidence as a pass: a run without a checkpoint,
//! a verdict or gate evidence is reported exactly as such.

use std::{
    fs,
    path::{Path, PathBuf},
};

use codingmage_state::JournalRecord;
use serde::{Deserialize, Serialize};

use crate::backend::models::{RunCheckpoint, parse_run_checkpoint};

/// Maximum run directories scanned.
pub const MAX_RUNS: usize = 500;
/// Maximum journal bytes read per run.
pub const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum directory depth searched for `runs` directories beneath a campaign.
pub const MAX_DEPTH: usize = 4;

/// One journaled phase observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseObservation {
    /// Journal sequence.
    pub sequence: u64,
    /// Stable phase name.
    pub phase: String,
    /// `transition`, `effect_observed`, `gate_observed`, `recovery_blocked` or another kind.
    pub kind: String,
    /// Outcome code.
    pub outcome: String,
    /// Unix milliseconds.
    pub timestamp_ms: u64,
    /// Evidence identities attached to the record.
    pub evidence: Vec<String>,
    /// Exact commit identity when the record carries one.
    pub commit: Option<String>,
    /// Gate identity when the record carries one.
    pub gate: Option<String>,
}

/// One run's durable records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    /// Run identity from the directory name.
    pub run_id: String,
    /// Directory the records were read from.
    pub directory: PathBuf,
    /// Checkpoint when present and valid.
    pub checkpoint: Option<RunCheckpoint>,
    /// Why the checkpoint is absent or invalid.
    pub checkpoint_problem: Option<String>,
    /// Journaled phases in sequence order.
    pub phases: Vec<PhaseObservation>,
    /// Number of journal lines that could not be parsed.
    pub malformed_journal_lines: u32,
    /// Why the journal could not be read at all.
    pub journal_problem: Option<String>,
    /// Task identity from the first journal record, when the checkpoint is absent.
    pub journal_task: Option<String>,
}

impl RunRecord {
    /// Task identity from the checkpoint or the journal.
    #[must_use]
    pub fn task_id(&self) -> Option<String> {
        self.checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.task_id.clone())
            .or_else(|| self.journal_task.clone())
    }

    /// Review outcome label that never turns missing evidence into a pass.
    #[must_use]
    pub fn review_label(&self) -> String {
        match &self.checkpoint {
            None => "no checkpoint: review outcome not recorded".to_owned(),
            Some(checkpoint) => match &checkpoint.review_verdict {
                Some(verdict) => format!("review verdict: {verdict}"),
                None => "review verdict not recorded (review did not complete)".to_owned(),
            },
        }
    }

    /// Gate evidence label that distinguishes absence from a pass.
    #[must_use]
    pub fn gate_label(&self) -> String {
        match &self.checkpoint {
            None => "no checkpoint: gate evidence not recorded".to_owned(),
            Some(checkpoint) if checkpoint.gate_evidence.is_empty() => {
                "no gate evidence recorded".to_owned()
            }
            Some(checkpoint) => format!(
                "{} gate evidence record(s): {}",
                checkpoint.gate_evidence.len(),
                checkpoint.gate_evidence.join(", ")
            ),
        }
    }
}

/// Scans every `runs/<run_id>` directory beneath a campaign state directory.
#[must_use]
pub fn scan_run_records(campaign_dir: &Path) -> Vec<RunRecord> {
    let mut run_dirs = Vec::new();
    collect_run_dirs(campaign_dir, 0, &mut run_dirs);
    run_dirs.sort();
    run_dirs.truncate(MAX_RUNS);
    run_dirs.iter().map(|dir| read_run(dir)).collect()
}

fn collect_run_dirs(directory: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == "runs") {
            if let Ok(runs) = fs::read_dir(&path) {
                for run in runs.flatten() {
                    let run_path = run.path();
                    if fs::symlink_metadata(&run_path).is_ok_and(|m| m.is_dir())
                        && run_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with("run-"))
                    {
                        out.push(run_path);
                    }
                }
            }
        } else {
            collect_run_dirs(&path, depth + 1, out);
        }
    }
}

fn read_run(directory: &Path) -> RunRecord {
    let run_id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_owned();
    let (checkpoint, checkpoint_problem) = match fs::read(directory.join("checkpoint.json")) {
        Ok(bytes) => match parse_run_checkpoint(&bytes) {
            Ok(checkpoint) => (Some(checkpoint), None),
            Err(error) => (None, Some(error.to_string())),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (None, Some("checkpoint.json is absent".to_owned()))
        }
        Err(_) => (None, Some("checkpoint.json is unreadable".to_owned())),
    };
    let (phases, malformed, journal_problem, journal_task) =
        read_journal(&directory.join("events.jsonl"));
    RunRecord {
        run_id,
        directory: directory.to_path_buf(),
        checkpoint,
        checkpoint_problem,
        phases,
        malformed_journal_lines: malformed,
        journal_problem,
        journal_task,
    }
}

type JournalScan = (Vec<PhaseObservation>, u32, Option<String>, Option<String>);

fn read_journal(path: &Path) -> JournalScan {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (
                Vec::new(),
                0,
                Some("events.jsonl is absent".to_owned()),
                None,
            );
        }
        Err(_) => {
            return (
                Vec::new(),
                0,
                Some("events.jsonl is unreadable".to_owned()),
                None,
            );
        }
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_JOURNAL_BYTES
    {
        return (
            Vec::new(),
            0,
            Some("events.jsonl is linked, not a file or oversized".to_owned()),
            None,
        );
    }
    let Ok(text) = fs::read_to_string(path) else {
        return (
            Vec::new(),
            0,
            Some("events.jsonl is unreadable".to_owned()),
            None,
        );
    };
    let mut phases = Vec::new();
    let mut malformed = 0_u32;
    let mut task = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<JournalRecord>(line) {
            Ok(record) => {
                task.get_or_insert_with(|| record.event.task_id.as_str().to_owned());
                let (kind, phase) = describe_kind(&record);
                phases.push(PhaseObservation {
                    sequence: record.sequence,
                    phase,
                    kind,
                    outcome: format!("{:?}", record.event.outcome).to_lowercase(),
                    timestamp_ms: record.event.timestamp_ms,
                    evidence: record
                        .event
                        .evidence
                        .iter()
                        .map(|evidence| evidence.as_str().to_owned())
                        .collect(),
                    commit: record.event.identities.commit.clone(),
                    gate: record.event.identities.gate.clone(),
                });
            }
            Err(_) => malformed += 1,
        }
    }
    (phases, malformed, None, task)
}

fn describe_kind(record: &JournalRecord) -> (String, String) {
    use codingmage_state::EventKind;
    match &record.event.kind {
        EventKind::Transition { phase, .. } => ("transition".to_owned(), phase.clone()),
        EventKind::EffectObserved { phase } => ("effect_observed".to_owned(), phase.clone()),
        EventKind::GateObserved { gate_id } => ("gate_observed".to_owned(), gate_id.clone()),
        EventKind::RecoveryBlocked { reason } => ("recovery_blocked".to_owned(), reason.clone()),
        EventKind::ControlRequested { action, .. } => {
            ("control_requested".to_owned(), action.clone())
        }
        EventKind::ControlApplied { action, .. } => ("control_applied".to_owned(), action.clone()),
        EventKind::RetryScheduled { reason, .. } => ("retry_scheduled".to_owned(), reason.clone()),
        EventKind::ExternalBoundaryChanged { system, change } => (
            "external_boundary_changed".to_owned(),
            format!("{system}:{change}"),
        ),
        EventKind::CampaignCheckpointed { projection } => {
            ("campaign_checkpointed".to_owned(), projection.phase.clone())
        }
    }
}

/// One changed file between two commits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    /// Repository-relative path.
    pub path: String,
    /// Added lines, `None` for binary.
    pub added: Option<u64>,
    /// Deleted lines, `None` for binary.
    pub deleted: Option<u64>,
}

/// One coordinator commit on the campaign branch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitSummary {
    /// Full commit identity.
    pub id: String,
    /// Subject line written by the coordinator.
    pub subject: String,
    /// Commit timestamp as Unix seconds.
    pub timestamp: u64,
}

/// Parses `git diff --numstat -z` output.
#[must_use]
pub fn parse_numstat(bytes: &[u8]) -> Vec<FileChange> {
    let text = String::from_utf8_lossy(bytes);
    let mut changes = Vec::new();
    for entry in text.split('\0').filter(|entry| !entry.is_empty()) {
        let mut parts = entry.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        changes.push(FileChange {
            path: path.to_owned(),
            added: added.parse().ok(),
            deleted: deleted.parse().ok(),
        });
    }
    changes
}

/// Parses `git log --format=%H%x1f%s%x1f%ct` output.
#[must_use]
pub fn parse_log(bytes: &[u8]) -> Vec<CommitSummary> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            let id = parts.next()?.trim().to_owned();
            let subject = parts.next()?.to_owned();
            let timestamp = parts.next()?.trim().parse().ok()?;
            (id.len() == 40).then_some(CommitSummary {
                id,
                subject,
                timestamp,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_and_log_parse_including_binary_and_malformed_lines() {
        let numstat = b"3\t1\tsrc/lib.rs\0-\t-\timage.png\0garbage\0";
        let changes = parse_numstat(numstat);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].added, Some(3));
        assert_eq!(changes[1].added, None);
        let log = format!(
            "{}\u{1f}codingmage: complete 1\u{1f}1700000000\nshort\u{1f}x\u{1f}1\n",
            "a".repeat(40)
        );
        let commits = parse_log(log.as_bytes());
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "codingmage: complete 1");
    }

    #[test]
    fn missing_and_malformed_records_are_reported_not_passed() {
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-records-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let run = root.join("state/runs/run-1");
        fs::create_dir_all(&run).unwrap();
        fs::write(run.join("events.jsonl"), "not json\n").unwrap();
        let records = scan_run_records(&root);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].malformed_journal_lines, 1);
        assert_eq!(
            records[0].checkpoint_problem.as_deref(),
            Some("checkpoint.json is absent")
        );
        assert!(records[0].review_label().contains("not recorded"));
        assert!(records[0].gate_label().contains("not recorded"));
        fs::write(
            run.join("checkpoint.json"),
            r#"{"schema_version":1,"run_id":"run-1","task_id":"1.1.1.1","candidate_commit":"c","review_verdict":null,"correction_rounds":0,"gate_evidence":[]}"#,
        )
        .unwrap();
        let records = scan_run_records(&root);
        assert_eq!(
            records[0].review_label(),
            "review verdict not recorded (review did not complete)"
        );
        assert_eq!(records[0].gate_label(), "no gate evidence recorded");
        fs::remove_dir_all(root).unwrap();
    }
}
