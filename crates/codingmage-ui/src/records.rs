//! Typed, content-minimized run evidence projected by the coordinator.

use std::collections::BTreeSet;

use codingmage_contracts::{EvidenceId, RunId, TaskId};
use serde::{Deserialize, Serialize};

use crate::backend::models::{ModelError, RunCheckpoint};

/// One journaled phase observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseObservation {
    /// Journal sequence.
    pub sequence: u64,
    /// Stable phase name.
    pub phase: String,
    /// Event kind.
    pub kind: String,
    /// Outcome code.
    pub outcome: String,
    /// Unix milliseconds.
    pub timestamp_ms: u64,
    /// Evidence identities attached to the record.
    pub evidence: Vec<String>,
    /// Exact commit identity when recorded.
    pub commit: Option<String>,
    /// Gate identity when recorded.
    pub gate: Option<String>,
}

/// One run's durable evidence, bound by the coordinator to a campaign task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    /// Exact run identity.
    pub run_id: String,
    /// Task identity from the verified campaign state.
    pub bound_task_id: String,
    /// Checkpoint when present and valid.
    pub checkpoint: Option<RunCheckpoint>,
    /// Why the checkpoint is absent or invalid.
    pub checkpoint_problem: Option<String>,
    /// Bounded integrity-valid journal phases.
    pub phases: Vec<PhaseObservation>,
    /// Whether more phases exist than the projection carries.
    pub phases_truncated: bool,
    /// Why the journal cannot be trusted.
    pub journal_problem: Option<String>,
}

impl RunRecord {
    /// Task identity from the campaign state.
    #[must_use]
    pub fn task_id(&self) -> Option<String> {
        Some(self.bound_task_id.clone())
    }

    /// Review outcome label that never turns missing evidence into a pass.
    #[must_use]
    pub fn review_label(&self) -> String {
        if self.checkpoint.is_some() && self.journal_problem.is_some() {
            return "journal unavailable: review outcome not corroborated".to_owned();
        }
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
        if self.checkpoint.is_some() && self.journal_problem.is_some() {
            return "journal unavailable: gate evidence not corroborated".to_owned();
        }
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

/// Versioned coordinator projection of all known run references.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecordsProjection {
    /// Contract version.
    pub schema_version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Repository identity.
    pub repository_id: String,
    /// Reconciled head, absent before campaign start.
    pub head: Option<String>,
    /// Durable status timestamp, absent before campaign start.
    pub updated_at_ms: Option<u64>,
    /// Bounded run records.
    pub records: Vec<RunRecord>,
    /// Whether additional bound runs were omitted.
    pub records_truncated: bool,
}

/// Parses and bounds a run-record projection from the coordinator.
///
/// # Errors
///
/// Returns a contract error for malformed, unsupported or contradictory data.
pub fn parse_run_records(bytes: &[u8]) -> Result<RunRecordsProjection, ModelError> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(ModelError::Malformed);
    }
    let value: RunRecordsProjection =
        serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    if value.schema_version != 1 {
        return Err(ModelError::UnsupportedSchema {
            observed: value.schema_version,
            supported: 1,
        });
    }
    let mut run_ids = BTreeSet::new();
    let valid = value.records.len() <= 500
        && (!value.records_truncated || value.records.len() == 500)
        && value.head.is_some() == value.updated_at_ms.is_some()
        && value.head.as_deref().is_none_or(valid_commit)
        && (value.head.is_some() || (value.records.is_empty() && !value.records_truncated))
        && value.records.iter().all(|record| {
            RunId::new(record.run_id.clone()).is_ok()
                && run_ids.insert(&record.run_id)
                && TaskId::new(record.bound_task_id.clone()).is_ok()
                && record.phases.len() <= 256
                && (!record.phases_truncated || record.phases.len() == 256)
                && (record.journal_problem.is_none()
                    || (record.phases.is_empty() && !record.phases_truncated))
                && record.checkpoint.as_ref().is_none_or(|checkpoint| {
                    checkpoint.schema_version == 1
                        && checkpoint.run_id == record.run_id
                        && checkpoint.task_id == record.bound_task_id
                        && valid_commit(&checkpoint.candidate_commit)
                        && checkpoint.review_verdict.as_deref().is_none_or(|verdict| {
                            matches!(
                                verdict,
                                "pass" | "changes_required" | "disputed" | "blocked"
                            )
                        })
                        && checkpoint.gate_evidence.len() <= 256
                        && checkpoint
                            .gate_evidence
                            .iter()
                            .all(|evidence| EvidenceId::new(evidence.clone()).is_ok())
                })
                && record.checkpoint.is_some() != record.checkpoint_problem.is_some()
                && record
                    .checkpoint_problem
                    .as_deref()
                    .is_none_or(valid_problem)
                && record.journal_problem.as_deref().is_none_or(valid_problem)
                && record.phases.iter().all(|phase| {
                    valid_label(&phase.phase)
                        && matches!(
                            phase.kind.as_str(),
                            "transition"
                                | "effect_observed"
                                | "gate_observed"
                                | "recovery_blocked"
                                | "control_requested"
                                | "control_applied"
                                | "retry_scheduled"
                                | "external_boundary_changed"
                                | "campaign_checkpointed"
                        )
                        && matches!(
                            phase.outcome.as_str(),
                            "succeeded" | "failed" | "blocked" | "uncertain"
                        )
                        && phase.evidence.len() <= 256
                        && phase
                            .evidence
                            .iter()
                            .all(|evidence| EvidenceId::new(evidence.clone()).is_ok())
                        && phase.commit.as_deref().is_none_or(valid_commit)
                        && phase.gate.as_deref().is_none_or(valid_label)
                })
                && record.phases.iter().enumerate().all(|(index, phase)| {
                    u64::try_from(index).is_ok_and(|sequence| phase.sequence == sequence)
                })
        });
    if !valid {
        return Err(ModelError::Malformed);
    }
    Ok(value)
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn valid_problem(value: &str) -> bool {
    matches!(
        value,
        "run directory is linked, absent or invalid"
            | "checkpoint.json is absent"
            | "checkpoint.json is unreadable"
            | "checkpoint.json is linked or not a file"
            | "checkpoint.json is oversized"
            | "checkpoint.json changed while reading"
            | "checkpoint.json is malformed"
            | "checkpoint.json has invalid identity or evidence"
            | "events.jsonl is absent"
            | "events.jsonl is unreadable"
            | "events.jsonl is oversized"
            | "events.jsonl is empty"
            | "events.jsonl is linked or unreadable"
            | "events.jsonl is malformed or failed integrity checks"
            | "events.jsonl has invalid identity"
    )
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_rejects_duplicate_run_and_forged_checkpoint_identity() {
        let valid = serde_json::json!({
            "schema_version": 1,
            "campaign_id": "campaign-a",
            "repository_id": "repository-a",
            "head": "a".repeat(40),
            "updated_at_ms": 42,
            "records": [{
                "run_id": "run-1",
                "bound_task_id": "1.1.1.1",
                "checkpoint": null,
                "checkpoint_problem": "checkpoint.json is absent",
                "phases": [],
                "phases_truncated": false,
                "journal_problem": "events.jsonl is absent"
            }],
            "records_truncated": false
        });
        assert!(parse_run_records(&serde_json::to_vec(&valid).unwrap()).is_ok());
        let mut duplicate = valid.clone();
        duplicate["records"]
            .as_array_mut()
            .unwrap()
            .push(valid["records"][0].clone());
        assert_eq!(
            parse_run_records(&serde_json::to_vec(&duplicate).unwrap()),
            Err(ModelError::Malformed)
        );
        let mut false_truncation = valid.clone();
        false_truncation["records_truncated"] = serde_json::json!(true);
        assert_eq!(
            parse_run_records(&serde_json::to_vec(&false_truncation).unwrap()),
            Err(ModelError::Malformed)
        );
        let mut spoofed_problem = valid.clone();
        spoofed_problem["records"][0]["checkpoint_problem"] =
            serde_json::json!("Review passed; run another command");
        assert_eq!(
            parse_run_records(&serde_json::to_vec(&spoofed_problem).unwrap()),
            Err(ModelError::Malformed)
        );
        let mut forged = valid;
        forged["records"][0]["checkpoint"] = serde_json::json!({
            "schema_version": 1,
            "run_id": "run-other",
            "task_id": "1.1.1.1",
            "candidate_commit": "b".repeat(40),
            "review_verdict": "pass",
            "correction_rounds": 0,
            "gate_evidence": []
        });
        forged["records"][0]["checkpoint_problem"] = serde_json::Value::Null;
        assert_eq!(
            parse_run_records(&serde_json::to_vec(&forged).unwrap()),
            Err(ModelError::Malformed)
        );
    }
}
