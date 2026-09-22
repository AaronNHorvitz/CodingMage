//! Strict mirror models of the machine-readable `codingmage` CLI contracts.
//!
//! Every model rejects unknown fields and the caller checks the schema version, so a backend
//! contract change surfaces as an explicit "unsupported backend output" failure instead of a
//! partially rendered screen.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Schema version accepted for `doctor`, `campaign-status` and `campaign-explain-blocker`.
pub const SUPPORTED_SCHEMA_VERSION: u16 = 1;
/// Report version accepted for `campaign-report`.
pub const SUPPORTED_REPORT_VERSION: u16 = 2;
/// Run checkpoint schema version accepted from `<state_root>/runs/<run_id>/checkpoint.json`.
pub const SUPPORTED_RUN_CHECKPOINT_VERSION: u16 = 1;

/// Output of `codingmage doctor` and `codingmage status`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // exact mirror of the coordinator's JSON contract
pub struct Diagnosis {
    /// Output schema version.
    pub schema_version: u16,
    /// Command that produced the output.
    pub command: String,
    /// `ready` or `blocked-dirty`.
    pub state: String,
    /// Stable repository identity derived from filesystem identity.
    pub repository_id: String,
    /// Exact current head commit.
    pub head: String,
    /// Current branch when the checkout is not detached.
    pub branch: Option<String>,
    /// Whether the active checkout is clean.
    pub clean: bool,
    /// Whether unsupported checkout features were observed.
    pub unsafe_checkout_features: bool,
    /// Digest of the exact task-source bytes.
    pub task_source_sha256: String,
    /// Parsed sprint count.
    pub sprints: usize,
    /// Parsed story count.
    pub stories: usize,
    /// Parsed checklist item count.
    pub items: usize,
    /// Redacted configuration view.
    pub configuration: RedactedConfiguration,
    /// Whether execution is available in this build.
    pub execution_available: bool,
    /// Whether supervised execution requires a run specification.
    pub requires_run_spec: bool,
}

/// Content-minimized configuration view reported by the coordinator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedactedConfiguration {
    /// Configuration schema version.
    pub version: u16,
    /// Fingerprint of the target path.
    pub target_path_sha256: String,
    /// Fingerprint of the scratch root.
    pub scratch_root_sha256: String,
    /// Fingerprint of the state root.
    pub state_root_sha256: String,
    /// Number of configured agent profiles.
    pub agent_profile_count: usize,
    /// Number of configured gate commands.
    pub gate_command_count: usize,
    /// Correction limit.
    pub correction_limit: u16,
    /// External capability grants.
    pub capabilities: Capabilities,
    /// Publication policy.
    pub publication: Publication,
    /// Parent-discovery setting.
    pub allow_parent_discovery: bool,
}

/// Explicit external capability grants.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Network grant.
    pub network: String,
    /// Push grant.
    pub push: String,
    /// Issues grant.
    pub issues: String,
    /// Pull-request grant.
    pub pull_requests: String,
    /// Task-merge grant.
    pub task_merge: String,
    /// Destination-merge grant.
    pub destination_merge: String,
}

impl Capabilities {
    /// Returns true when every external grant is denied.
    #[must_use]
    pub fn all_denied(&self) -> bool {
        [
            &self.network,
            &self.push,
            &self.issues,
            &self.pull_requests,
            &self.task_merge,
            &self.destination_merge,
        ]
        .iter()
        .all(|grant| grant.as_str() == "denied")
    }
}

/// Publication policy view.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    /// Publication mode code.
    pub mode: String,
}

/// Durable campaign status projection from `campaign-status`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignStatus {
    /// Status schema version.
    pub schema_version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Durable lifecycle phase code.
    pub state: String,
    /// Actor category owning the phase.
    pub actor: String,
    /// Active model identity when a provider owns the phase.
    pub model: Option<String>,
    /// Coordinator-owned campaign branch.
    pub branch: String,
    /// Last reconciled campaign head.
    pub head: String,
    /// Current task while a unit is active.
    pub current_task_id: Option<String>,
    /// Current correction round for the active unit.
    pub current_round: Option<u16>,
    /// Every concurrently active task.
    pub active_tasks: Vec<ActiveTask>,
    /// Last selected task at a clean boundary.
    pub last_task_id: Option<String>,
    /// Accepted, reconciled units.
    pub completed_units: u32,
    /// Aggregate provider attempts.
    pub attempt_count: u32,
    /// Latest planning generation.
    pub planning_generation: u32,
    /// Consecutive identical planning generations.
    pub identical_planning_generations: u16,
    /// Pending planning trigger codes.
    pub pending_planning_triggers: Vec<String>,
    /// Liveness state code.
    pub watchdog_state: String,
    /// Reconciliation state code.
    pub reconciliation_state: String,
    /// Outcome counters.
    pub outcomes: Outcomes,
    /// Aggregate utilization.
    pub utilization: Utilization,
    /// Aggregate limits.
    pub limits: Limits,
    /// Number of durable blockers or holds.
    pub blocker_count: u32,
    /// Campaign-level blocker code.
    pub blocker_code: Option<String>,
    /// Blocked tasks.
    pub blockers: Vec<TaskReason>,
    /// Deferred tasks.
    pub deferrals: Vec<Deferral>,
    /// Tasks awaiting a human decision.
    pub human_decisions: Vec<TaskReason>,
    /// Milliseconds since the campaign was created.
    pub elapsed_ms: u64,
    /// Last checkpoint timestamp.
    pub updated_at_ms: u64,
}

/// One active task projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveTask {
    /// Task identity.
    pub task_id: String,
    /// Pod identity when leased.
    pub pod_id: Option<String>,
    /// Durable task state code.
    pub state: String,
    /// Actor responsible for progress.
    pub actor: String,
    /// Model identity while a provider owns the phase.
    pub model: Option<String>,
    /// Completed correction sessions.
    pub correction_round: u16,
    /// Latest heartbeat sequence.
    pub heartbeat_sequence: u64,
}

/// Independent outcome counters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Outcomes {
    /// Reconciled completed units.
    pub completed: u32,
    /// Blocked tasks.
    pub blocked: u32,
    /// Deferred tasks.
    pub deferred: u32,
    /// Tasks awaiting a human decision.
    pub pending_human_decision: u32,
    /// Rejected lead proposals.
    pub rejected_proposals: u32,
    /// Outcomes consumed against the ceiling.
    pub accepted: u32,
    /// Accepted-outcome ceiling.
    pub max_accepted: u32,
}

/// Aggregate utilization counters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Utilization {
    /// Provider attempts.
    pub provider_attempts: u32,
    /// Malformed-report repairs.
    pub malformed_report_repairs: u32,
    /// Correction rounds.
    pub correction_rounds: u32,
    /// Process invocations.
    pub process_invocations: u32,
    /// Observed output bytes.
    pub output_bytes: u64,
    /// Retained state bytes.
    pub retained_state_bytes: u64,
    /// Execution milliseconds.
    pub execution_elapsed_ms: u64,
}

/// Aggregate campaign limits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum provider attempts.
    pub provider_attempts: u32,
    /// Maximum malformed-report repairs.
    pub malformed_report_repairs: u32,
    /// Maximum correction rounds.
    pub correction_rounds: u32,
    /// Maximum process invocations.
    pub process_invocations: u32,
    /// Maximum output bytes.
    pub output_bytes: u64,
    /// Maximum retained state bytes.
    pub retained_state_bytes: u64,
    /// Maximum execution milliseconds.
    pub execution_elapsed_ms: u64,
}

/// Task identity with a closed reason code.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReason {
    /// Task identity.
    pub task_id: String,
    /// Closed reason code.
    pub reason_code: String,
}

/// Deferred task projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Deferral {
    /// Task identity.
    pub task_id: String,
    /// Closed reason code.
    pub reason_code: String,
    /// Closed trigger code.
    pub trigger_code: String,
    /// `pending` or `satisfied`.
    pub trigger_state: String,
}

/// Blocker explanation projection from `campaign-explain-blocker`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockerExplanation {
    /// Schema version.
    pub schema_version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Lifecycle phase code.
    pub state: String,
    /// Campaign-level blocker code.
    pub blocker_code: Option<String>,
    /// Blocked tasks.
    pub blockers: Vec<TaskReason>,
    /// Deferred tasks.
    pub deferrals: Vec<Deferral>,
    /// Tasks awaiting a human decision.
    pub human_decisions: Vec<TaskReason>,
}

/// Terminal result of one `codingmage campaign` invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignOutcome {
    /// Campaign identity.
    pub campaign_id: String,
    /// Terminal state code.
    pub state: String,
    /// Campaign branch.
    pub branch: String,
    /// Last integrated head.
    pub head: String,
    /// Units integrated by the invocation.
    pub completed_units: u32,
    /// Closed stop reason.
    pub stop_reason: String,
    /// Last selected task.
    pub last_task_id: Option<String>,
    /// Blocker code when blocked or paused.
    pub blocker_code: Option<String>,
}

/// Result of one `campaign-control` request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlOutcome {
    /// Campaign identity.
    pub campaign_id: String,
    /// Request identity.
    pub request_id: String,
    /// Requested action code.
    pub action: String,
    /// True only when this invocation created the durable request.
    pub created: bool,
}

/// Source-free preflight report from `campaign-preflight`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightReport {
    /// Report schema version.
    pub schema_version: u16,
    /// Closed terminal state; successful reports contain `ready`.
    pub state: String,
    /// Digest of the campaign authority.
    pub authority_sha256: String,
    /// Digest of the authorization record.
    pub operator_authorization_sha256: String,
    /// Repository baseline.
    pub repository: PreflightRepository,
    /// Policy summary.
    pub policy: PreflightPolicy,
    /// Provider probes.
    pub providers: Vec<PreflightProvider>,
    /// Gate registry identity.
    pub gates: PreflightGates,
    /// Control identities.
    pub controls: PreflightControls,
    /// Storage observation.
    pub storage: PreflightStorage,
    /// True when the report excludes source, paths, model names and process output.
    pub source_free: bool,
}

/// Repository baseline inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightRepository {
    /// Repository identity.
    pub repository_id: String,
    /// Starting commit.
    pub initial_commit: String,
    /// Branch name digest.
    pub branch_sha256: String,
    /// Task-source digest.
    pub task_source_sha256: String,
    /// Porcelain status digest.
    pub status_sha256: String,
    /// Reference digest.
    pub references_sha256: String,
    /// Worktree digest.
    pub worktrees_sha256: String,
    /// Plan item count.
    pub plan_item_count: usize,
    /// Open sub-task count.
    pub open_subtask_count: usize,
    /// Whether the checkout is clean.
    pub clean: bool,
    /// Whether the branch is dedicated.
    pub dedicated_branch: bool,
    /// Whether unsupported checkout features were absent.
    pub checkout_safe: bool,
}

/// Policy summary inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightPolicy {
    /// Maximum parallel pods.
    pub max_parallel_pods: u16,
    /// Accepted-outcome ceiling.
    pub max_accepted_outcomes: u32,
    /// Publication code.
    pub publication: String,
    /// Whether the default branch is protected.
    pub default_branch_protected: bool,
    /// Allowed path root count.
    pub allowed_path_count: usize,
    /// Allowed path digest.
    pub allowed_paths_sha256: String,
    /// Task path authority count.
    pub task_path_authority_count: usize,
    /// Task path authority digest.
    pub task_path_authority_sha256: String,
    /// Denied path count.
    pub denied_path_count: usize,
    /// Denied path digest.
    pub denied_paths_sha256: String,
    /// Whether every external capability is denied.
    pub external_capabilities_denied: bool,
}

/// One provider probe inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightProvider {
    /// Provider role.
    pub role: String,
    /// Executable digest.
    pub executable_sha256: String,
    /// Profile digest.
    pub profile_sha256: String,
    /// Capability digest.
    pub capabilities_sha256: String,
    /// Authentication boundary code.
    pub authentication: String,
    /// Guarded probe process count.
    pub probe_process_count: u32,
    /// Whether every required capability passed.
    pub capability_verified: bool,
}

/// Gate registry identity inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightGates {
    /// Gate command count.
    pub command_count: usize,
    /// Registry digest.
    pub registry_sha256: String,
    /// Executable digests.
    pub executable_sha256: Vec<String>,
    /// Tier count.
    pub tier_count: usize,
    /// Tier digest.
    pub tiers_sha256: String,
}

/// Control identities inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightControls {
    /// Process guard digest.
    pub process_guard_sha256: String,
    /// Operator control count.
    pub operator_control_count: usize,
    /// Operator control digest.
    pub operator_controls_sha256: String,
    /// Whether the guard was verified.
    pub process_guard_verified: bool,
}

/// Storage observation inside a preflight report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightStorage {
    /// Whether scratch storage suffices.
    pub scratch_sufficient: bool,
    /// Whether state storage suffices.
    pub state_sufficient: bool,
    /// Required available bytes.
    pub required_available_bytes: u64,
    /// Whether both roots suffice.
    pub sufficient: bool,
}

/// Final parallel-campaign report from `campaign-report`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignReport {
    /// Report version.
    pub version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Repository identity.
    pub repository_id: String,
    /// Campaign branch.
    pub branch: String,
    /// Base commit.
    pub initial_commit: String,
    /// Final verified commit.
    pub final_commit: String,
    /// Completed task-source digest.
    pub task_source_sha256: String,
    /// Task completion identities.
    pub tasks: BTreeMap<String, TaskCompletion>,
    /// Terminal reconciliation.
    pub reconciliation: Reconciliation,
    /// Final gate evidence digest.
    pub final_gate_evidence_sha256: String,
    /// Final review evidence digest.
    pub final_review_evidence_sha256: String,
    /// Completion timestamp.
    pub completed_at_ms: u64,
}

/// One task's completion identities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCompletion {
    /// Reviewed candidate commit.
    pub reviewed_commit: String,
    /// Integration commit.
    pub integration_commit: String,
    /// Completion-marker commit.
    pub completion_commit: String,
}

/// Terminal reconciliation counters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reconciliation {
    /// Snapshot digest.
    pub state_sha256: String,
    /// Completed task identity digest.
    pub completed_task_ids_sha256: String,
    /// Removed worktree digest.
    pub removed_worktree_ids_sha256: String,
    /// Task evidence digest.
    pub task_evidence_sha256: String,
    /// Checked task count.
    pub checked_task_count: u32,
    /// Removed worktree count.
    pub removed_worktree_count: u32,
    /// Process-control root count.
    pub process_control_root_count: u32,
    /// Process-control residue count.
    pub process_control_residue_count: u64,
    /// Active lease count.
    pub active_lease_count: u32,
    /// Active reservation count.
    pub active_reservation_count: u32,
    /// Integration queue count.
    pub integration_queue_count: u32,
    /// Whether the journal reconciled.
    pub journal_reconciled: bool,
    /// Whether the task source reconciled.
    pub task_source_reconciled: bool,
}

/// Private run checkpoint retained under `<state_root>/runs/<run_id>/checkpoint.json`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunCheckpoint {
    /// Checkpoint schema version.
    pub schema_version: u16,
    /// Run identity.
    pub run_id: String,
    /// Task identity.
    pub task_id: String,
    /// Reviewed candidate commit.
    pub candidate_commit: String,
    /// Review verdict when review ran.
    pub review_verdict: Option<String>,
    /// Correction rounds consumed.
    pub correction_rounds: u16,
    /// Gate evidence identities.
    pub gate_evidence: Vec<String>,
}

/// Failure to accept backend output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    /// Output was not valid JSON for the expected model.
    Malformed,
    /// Output carried an unsupported schema version.
    UnsupportedSchema {
        /// Version observed.
        observed: u16,
        /// Version supported by this build.
        supported: u16,
    },
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => {
                formatter.write_str("backend output did not match the expected contract")
            }
            Self::UnsupportedSchema {
                observed,
                supported,
            } => write!(
                formatter,
                "backend output schema {observed} is not supported by this interface (expected {supported})"
            ),
        }
    }
}

impl std::error::Error for ModelError {}

fn check_version(observed: u16, supported: u16) -> Result<(), ModelError> {
    if observed == supported {
        Ok(())
    } else {
        Err(ModelError::UnsupportedSchema {
            observed,
            supported,
        })
    }
}

/// Parses `doctor` output.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported schema.
pub fn parse_diagnosis(bytes: &[u8]) -> Result<Diagnosis, ModelError> {
    let value: Diagnosis = serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    check_version(value.schema_version, SUPPORTED_SCHEMA_VERSION)?;
    Ok(value)
}

/// Parses `campaign-status` output; `null` means no durable campaign state exists.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported schema.
pub fn parse_campaign_status(bytes: &[u8]) -> Result<Option<CampaignStatus>, ModelError> {
    let value: Option<CampaignStatus> =
        serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    if let Some(status) = &value {
        check_version(status.schema_version, SUPPORTED_SCHEMA_VERSION)?;
    }
    Ok(value)
}

/// Parses `campaign-explain-blocker` output.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported schema.
pub fn parse_blocker_explanation(bytes: &[u8]) -> Result<BlockerExplanation, ModelError> {
    let value: BlockerExplanation =
        serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    check_version(value.schema_version, SUPPORTED_SCHEMA_VERSION)?;
    Ok(value)
}

/// Parses `campaign-preflight` output.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported schema.
pub fn parse_preflight(bytes: &[u8]) -> Result<PreflightReport, ModelError> {
    let value: PreflightReport =
        serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    check_version(value.schema_version, SUPPORTED_SCHEMA_VERSION)?;
    Ok(value)
}

/// Parses `campaign-control` output.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output.
pub fn parse_control_outcome(bytes: &[u8]) -> Result<ControlOutcome, ModelError> {
    serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)
}

/// Parses the terminal JSON of a `campaign` invocation.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output.
pub fn parse_campaign_outcome(bytes: &[u8]) -> Result<CampaignOutcome, ModelError> {
    serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)
}

/// Parses `campaign-report` output; `null` means no final report exists.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported version.
pub fn parse_campaign_report(bytes: &[u8]) -> Result<Option<CampaignReport>, ModelError> {
    let value: Option<CampaignReport> =
        serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    if let Some(report) = &value {
        check_version(report.version, SUPPORTED_REPORT_VERSION)?;
    }
    Ok(value)
}

/// Parses a private run checkpoint.
///
/// # Errors
///
/// Returns [`ModelError`] for malformed output or an unsupported schema.
pub fn parse_run_checkpoint(bytes: &[u8]) -> Result<RunCheckpoint, ModelError> {
    let value: RunCheckpoint = serde_json::from_slice(bytes).map_err(|_| ModelError::Malformed)?;
    check_version(value.schema_version, SUPPORTED_RUN_CHECKPOINT_VERSION)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_status_and_report_mean_absent_state() {
        assert_eq!(parse_campaign_status(b"null").unwrap(), None);
        assert_eq!(parse_campaign_report(b"null").unwrap(), None);
    }

    #[test]
    fn unknown_fields_and_wrong_schema_fail_closed() {
        let control =
            br#"{"campaign_id":"c","request_id":"r","action":"pause","created":true,"extra":1}"#;
        assert_eq!(parse_control_outcome(control), Err(ModelError::Malformed));
        let checkpoint = br#"{"schema_version":9,"run_id":"r","task_id":"t","candidate_commit":"c","review_verdict":null,"correction_rounds":0,"gate_evidence":[]}"#;
        assert_eq!(
            parse_run_checkpoint(checkpoint),
            Err(ModelError::UnsupportedSchema {
                observed: 9,
                supported: 1
            })
        );
        assert_eq!(parse_diagnosis(b"not json"), Err(ModelError::Malformed));
    }
}
