//! Read-only, content-minimized run evidence for one authorized campaign.

use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use codingmage_campaign::{CampaignExecutionMode, CampaignSpec, TeamCampaignSnapshot};
use codingmage_contracts::{EvidenceId, RunId};
use codingmage_core::Config;
use codingmage_state::{
    EventKind, IntegrityDocument, JournalError, JournalRecord, read_verified_records,
};
use serde::{Deserialize, Serialize};

use crate::{CampaignStatus, RuntimeError, campaign_state::CampaignCheckpoint, campaign_status};

const MAX_RECORDS: usize = 500;
const MAX_PHASES: usize = 256;
const MAX_CAMPAIGN_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RUN_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize)]
struct Projection {
    schema_version: u16,
    campaign_id: String,
    repository_id: String,
    head: Option<String>,
    updated_at_ms: Option<u64>,
    records: Vec<RunRecord>,
    records_truncated: bool,
}

#[derive(Serialize)]
struct RunRecord {
    run_id: String,
    bound_task_id: String,
    checkpoint: Option<RunCheckpoint>,
    checkpoint_problem: Option<&'static str>,
    phases: Vec<PhaseObservation>,
    phases_truncated: bool,
    journal_problem: Option<&'static str>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RunCheckpoint {
    schema_version: u16,
    run_id: String,
    task_id: String,
    candidate_commit: String,
    review_verdict: Option<String>,
    correction_rounds: u16,
    gate_evidence: Vec<String>,
}

#[derive(Serialize)]
struct PhaseObservation {
    sequence: u64,
    phase: String,
    kind: String,
    outcome: String,
    timestamp_ms: u64,
    evidence: Vec<String>,
    commit: Option<String>,
    gate: Option<String>,
}

struct RunReference {
    run_id: String,
    task_id: String,
    path: PathBuf,
}

/// Projects bounded, integrity-checked run evidence under one exact campaign authority.
///
/// This never accepts a caller-supplied state directory and performs no provider or
/// repository mutation. Absence and invalid records remain distinct from a pass.
///
/// # Errors
///
/// Returns a runtime error for invalid campaign state, a changing status or an
/// oversized projection.
pub fn campaign_run_records(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
) -> Result<String, RuntimeError> {
    if RunId::new(spec.campaign_id.clone()).is_err() {
        return Err(RuntimeError::Authority);
    }
    let before = campaign_status(config, spec, codingmage_binary)?;
    let Some(status) = before else {
        return encode(&Projection {
            schema_version: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            head: None,
            updated_at_ms: None,
            records: Vec::new(),
            records_truncated: false,
        });
    };
    let references = if spec
        .multi_agent
        .as_ref()
        .is_some_and(|policy| policy.execution_mode == CampaignExecutionMode::Parallel)
    {
        parallel_references(config, spec, &status)?
    } else {
        serial_references(config, spec, &status)?
    };
    let records_truncated = references.len() > MAX_RECORDS;
    let records = references
        .iter()
        .take(MAX_RECORDS)
        .map(|reference| read_record(reference, &config.state_root))
        .collect();
    let after = campaign_status(config, spec, codingmage_binary)?;
    if after.as_ref().is_none_or(|current| {
        current.head != status.head || current.updated_at_ms != status.updated_at_ms
    }) {
        return Err(RuntimeError::State);
    }
    encode(&Projection {
        schema_version: 1,
        campaign_id: spec.campaign_id.clone(),
        repository_id: spec.repository_id.clone(),
        head: Some(status.head),
        updated_at_ms: Some(status.updated_at_ms),
        records,
        records_truncated,
    })
}

fn encode(projection: &Projection) -> Result<String, RuntimeError> {
    let encoded = serde_json::to_string(projection).map_err(|_| RuntimeError::State)?;
    if encoded.len() >= MAX_OUTPUT_BYTES {
        return Err(RuntimeError::State);
    }
    Ok(encoded)
}

fn serial_references(
    config: &Config,
    spec: &CampaignSpec,
    status: &CampaignStatus,
) -> Result<Vec<RunReference>, RuntimeError> {
    let root = config.state_root.join("campaigns").join(&spec.campaign_id);
    let checkpoint = CampaignCheckpoint::load(&root)?.ok_or(RuntimeError::State)?;
    checkpoint.validate_authority(
        &spec.authority_sha256().map_err(RuntimeError::Campaign)?,
        &spec.campaign_id,
        &spec.repository_id,
        &spec.initial_commit,
        spec.max_units,
        &spec.limits,
    )?;
    if checkpoint.head != status.head {
        return Err(RuntimeError::State);
    }
    let journal = read_verified_records(&root, MAX_CAMPAIGN_JOURNAL_BYTES)
        .map_err(|_| RuntimeError::State)?;
    let mut known = BTreeMap::new();
    for record in journal {
        if record.event.run_id != checkpoint.campaign_run_id
            || record.event.repository_id.as_str() != spec.repository_id
        {
            return Err(RuntimeError::State);
        }
        if let EventKind::CampaignCheckpointed { projection } = record.event.kind
            && let Some(run_id) = projection.active_pod_run_id
        {
            if RunId::new(run_id.clone()).is_err() {
                return Err(RuntimeError::State);
            }
            let task_id = record.event.task_id.as_str().to_owned();
            if let Some(prior) = known.get(&run_id) {
                if prior != &task_id {
                    return Err(RuntimeError::State);
                }
            } else if known.len() <= MAX_RECORDS {
                known.insert(run_id, task_id);
            }
        }
    }
    if known.len() <= MAX_RECORDS
        && let Some(active) = &checkpoint.active_unit
        && let Some(run_id) = &active.run_id
        && known
            .get(run_id.as_str())
            .is_none_or(|task| task != &active.task_id)
    {
        return Err(RuntimeError::State);
    }
    let completed = usize::try_from(checkpoint.completed_units).map_err(|_| RuntimeError::State)?;
    if known.len() <= MAX_RECORDS && known.len() < completed {
        return Err(RuntimeError::State);
    }
    Ok(known
        .into_iter()
        .map(|(run_id, task_id)| RunReference {
            path: root.join("state").join("runs").join(&run_id),
            run_id,
            task_id,
        })
        .collect())
}

fn parallel_references(
    config: &Config,
    spec: &CampaignSpec,
    status: &CampaignStatus,
) -> Result<Vec<RunReference>, RuntimeError> {
    let root = config
        .state_root
        .join("team-campaigns")
        .join(&spec.campaign_id);
    let snapshot = IntegrityDocument::<TeamCampaignSnapshot>::load(
        &root.join("state"),
        "team-campaign.json",
        |value| value.verify().is_ok(),
    )
    .map_err(|_| RuntimeError::State)?
    .payload;
    if snapshot.campaign_id != spec.campaign_id || snapshot.campaign_head != status.head {
        return Err(RuntimeError::State);
    }
    let mut refs = Vec::new();
    for task in snapshot.tasks.values() {
        if let (Some(run_id), Some(lease_id)) = (&task.run_id, &task.lease_id) {
            if RunId::new(run_id.clone()).is_err() || RunId::new(lease_id.clone()).is_err() {
                return Err(RuntimeError::State);
            }
            if refs.len() <= MAX_RECORDS {
                refs.push(RunReference {
                    path: root
                        .join("execution")
                        .join("pods")
                        .join(lease_id)
                        .join("state")
                        .join("runs")
                        .join(run_id),
                    run_id: run_id.clone(),
                    task_id: task.task_id.clone(),
                });
            }
        }
    }
    Ok(refs)
}

fn read_record(reference: &RunReference, state_root: &Path) -> RunRecord {
    if !regular_directory_chain(state_root, &reference.path) {
        return RunRecord {
            run_id: reference.run_id.clone(),
            bound_task_id: reference.task_id.clone(),
            checkpoint: None,
            checkpoint_problem: Some("run directory is linked, absent or invalid"),
            phases: Vec::new(),
            phases_truncated: false,
            journal_problem: Some("run directory is linked, absent or invalid"),
        };
    }
    let (checkpoint, checkpoint_problem) = read_checkpoint(reference);
    let (phases, phases_truncated, journal_problem) = read_phases(reference);
    RunRecord {
        run_id: reference.run_id.clone(),
        bound_task_id: reference.task_id.clone(),
        checkpoint,
        checkpoint_problem,
        phases,
        phases_truncated,
        journal_problem,
    }
}

fn regular_directory_chain(state_root: &Path, run_dir: &Path) -> bool {
    let Ok(relative) = run_dir.strip_prefix(state_root) else {
        return false;
    };
    let mut current = state_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if !fs::symlink_metadata(&current)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        {
            return false;
        }
    }
    true
}

fn read_checkpoint(reference: &RunReference) -> (Option<RunCheckpoint>, Option<&'static str>) {
    let path = reference.path.join("checkpoint.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (None, Some("checkpoint.json is absent"));
        }
        Err(_) => return (None, Some("checkpoint.json is unreadable")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return (None, Some("checkpoint.json is linked or not a file"));
    }
    if metadata.len() > MAX_CHECKPOINT_BYTES {
        return (None, Some("checkpoint.json is oversized"));
    }
    let Ok(file) = fs::File::open(&path) else {
        return (None, Some("checkpoint.json is unreadable"));
    };
    let Ok(opened) = file.metadata() else {
        return (None, Some("checkpoint.json changed while reading"));
    };
    if !opened.is_file() || opened.len() != metadata.len() {
        return (None, Some("checkpoint.json changed while reading"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return (None, Some("checkpoint.json changed while reading"));
        }
    }
    let mut bytes = Vec::new();
    if file
        .take(MAX_CHECKPOINT_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return (None, Some("checkpoint.json is unreadable"));
    }
    if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
        return (None, Some("checkpoint.json is oversized"));
    }
    if bytes.len() as u64 != metadata.len() {
        return (None, Some("checkpoint.json changed while reading"));
    }
    let Ok(checkpoint) = serde_json::from_slice::<RunCheckpoint>(&bytes) else {
        return (None, Some("checkpoint.json is malformed"));
    };
    let valid = checkpoint.schema_version == 1
        && checkpoint.run_id == reference.run_id
        && checkpoint.task_id == reference.task_id
        && matches!(checkpoint.candidate_commit.len(), 40 | 64)
        && checkpoint
            .candidate_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
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
            .all(|value| EvidenceId::new(value.clone()).is_ok());
    if !valid {
        return (
            None,
            Some("checkpoint.json has invalid identity or evidence"),
        );
    }
    (Some(checkpoint), None)
}

fn read_phases(reference: &RunReference) -> (Vec<PhaseObservation>, bool, Option<&'static str>) {
    let path = reference.path.join("events.jsonl");
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), false, Some("events.jsonl is absent"));
        }
        Err(_) => {
            return (Vec::new(), false, Some("events.jsonl is unreadable"));
        }
    }
    let records = match read_verified_records(&reference.path, MAX_RUN_JOURNAL_BYTES) {
        Ok(records) => records,
        Err(JournalError::TooLarge { .. }) => {
            return (Vec::new(), false, Some("events.jsonl is oversized"));
        }
        Err(JournalError::Io | JournalError::Locked) => {
            return (
                Vec::new(),
                false,
                Some("events.jsonl is linked or unreadable"),
            );
        }
        Err(_) => {
            return (
                Vec::new(),
                false,
                Some("events.jsonl is malformed or failed integrity checks"),
            );
        }
    };
    // A unit runs against the coordinator-owned campaign worktree. Its journal's repository
    // identity therefore differs from the caller's original repository identity. The verified
    // campaign state supplies the run/task binding and exact private run path; require every
    // journal event under that path to retain one internal worktree identity.
    let Some(internal_repository_id) = records.first().map(|record| &record.event.repository_id)
    else {
        return (Vec::new(), false, Some("events.jsonl is empty"));
    };
    if records.iter().any(|record| {
        record.event.run_id.as_str() != reference.run_id
            || record.event.task_id.as_str() != reference.task_id
            || record.event.repository_id != *internal_repository_id
    }) {
        return (Vec::new(), false, Some("events.jsonl has invalid identity"));
    }
    let truncated = records.len() > MAX_PHASES;
    let phases = records
        .iter()
        .take(MAX_PHASES)
        .map(describe_phase)
        .collect();
    (phases, truncated, None)
}

fn describe_phase(record: &JournalRecord) -> PhaseObservation {
    let (kind, phase) = match &record.event.kind {
        EventKind::Transition { phase, .. } => ("transition", phase.as_str()),
        EventKind::EffectObserved { phase } => ("effect_observed", phase.as_str()),
        EventKind::GateObserved { gate_id } => ("gate_observed", gate_id.as_str()),
        EventKind::RecoveryBlocked { reason } => ("recovery_blocked", reason.as_str()),
        EventKind::ControlRequested { action, .. } => ("control_requested", action.as_str()),
        EventKind::ControlApplied { action, .. } => ("control_applied", action.as_str()),
        EventKind::RetryScheduled { reason, .. } => ("retry_scheduled", reason.as_str()),
        EventKind::ExternalBoundaryChanged { system, .. } => {
            ("external_boundary_changed", system.as_str())
        }
        EventKind::CampaignCheckpointed { projection } => {
            ("campaign_checkpointed", projection.phase.as_str())
        }
    };
    PhaseObservation {
        sequence: record.sequence,
        phase: phase.to_owned(),
        kind: kind.to_owned(),
        outcome: format!("{:?}", record.event.outcome).to_lowercase(),
        timestamp_ms: record.event.timestamp_ms,
        evidence: record
            .event
            .evidence
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect(),
        commit: record.event.identities.commit.clone(),
        gate: record.event.identities.gate.clone(),
    }
}
