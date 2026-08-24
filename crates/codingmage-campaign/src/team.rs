//! Durable multi-agent campaign policy, task identity, and scheduler contracts.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use sha2::Digest;

use super::{
    CampaignError, CampaignSpec, PodProposal, canonical_sha256, paths_overlap, safe_relative,
    valid_branch, valid_commit, valid_component, valid_sha256,
};

const TEAM_POLICY_VERSION: u16 = 1;
/// Current closed schema version for durable multi-agent state projections.
pub const TEAM_STATE_SCHEMA_VERSION: u16 = 3;
const TEAM_STATE_VERSION: u16 = TEAM_STATE_SCHEMA_VERSION;
const MAX_PROVIDER_TOKENS: u64 = 1_000_000_000_000;
const MAX_TASK_RECORDS: usize = 1_000_000;
const MAX_SESSIONS: usize = 1_024;
const MAX_RESOURCE_BYTES: u64 = 1 << 50;
const MAX_ELAPSED_MS: u64 = 365 * 24 * 60 * 60 * 1_000;
const MAX_GITHUB_CHECKS: usize = 256;

/// Campaign execution shape. Serial mode remains the compatibility default.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignExecutionMode {
    /// Admit and execute one pod at a time.
    #[default]
    Serial,
    /// Admit dependency-ready nonconflicting pods concurrently.
    Parallel,
}

/// Remote review surface available to a campaign.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPublicationMode {
    /// Retain all task and campaign branches locally.
    #[default]
    LocalOnly,
    /// Maintain one issue and one draft pull request for each admitted task.
    PerTaskDraftPullRequest,
    /// Publish only one final campaign draft pull request.
    CampaignDraftPullRequest,
}

/// Coordinator policy for incorporating an accepted task into the isolated campaign branch.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskIntegrationPolicy {
    /// Never integrate automatically.
    Never,
    /// Require an exact operator decision for every task.
    HumanRequired,
    /// Integrate automatically after every configured deterministic and independent review gate.
    #[default]
    AutoToCampaignBranch,
}

/// Coordinator policy for promoting the completed campaign to its destination branch.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationPromotionPolicy {
    /// Never promote through `CodingMage`.
    Never,
    /// Require an exact operator decision bound to the final evidence.
    #[default]
    HumanRequired,
    /// Permit promotion only after every configured final prerequisite passes.
    AutoToDefaultBranch,
}

/// Deterministic task integration strategy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMergeStrategy {
    /// Transfer the reviewed task delta and create one coordinator-owned integration commit.
    #[default]
    Squash,
    /// Fast-forward only when the reviewed task commit directly descends from the campaign head.
    FastForwardOnly,
}

/// Exact GitHub endpoint and destination authority for a remotely visible campaign.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubCampaignPolicy {
    /// Absolute GitHub CLI executable selected by the operator.
    pub cli_executable: PathBuf,
    /// Expected authenticated account.
    pub account: String,
    /// Exact GitHub API host.
    pub host: String,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repository: String,
    /// Exact configured Git remote name.
    pub remote: String,
    /// Protected destination branch for the final campaign pull request.
    pub destination_branch: String,
    /// Required commit-check names, in deterministic order.
    #[serde(default)]
    pub required_checks: Vec<String>,
}

impl GitHubCampaignPolicy {
    fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        if !self.cli_executable.is_absolute()
            || !valid_component(&self.account)
            || !valid_host(&self.host)
            || !valid_component(&self.owner)
            || !valid_component(&self.repository)
            || !valid_component(&self.remote)
            || !valid_branch(&self.destination_branch)
            || !spec.protected_branches.contains(&self.destination_branch)
            || self.destination_branch == spec.campaign_branch
            || self.required_checks.len() > MAX_GITHUB_CHECKS
            || self
                .required_checks
                .iter()
                .any(|check| !valid_external_name(check))
            || self.required_checks.iter().collect::<BTreeSet<_>>().len()
                != self.required_checks.len()
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
}

/// Independent actor and worker ceilings for one campaign.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignConcurrency {
    /// Maximum active Claude implementation pods.
    pub claude_implementers: u16,
    /// Maximum active Codex team-lead calls. The initial architecture requires one.
    pub codex_team_leads: u16,
    /// Maximum active independent Codex review calls.
    pub codex_reviewers: u16,
    /// Maximum active deterministic gate workers.
    pub test_workers: u16,
    /// Maximum active GitHub writers. The initial architecture requires one.
    pub github_writers: u16,
    /// Maximum active integration workers. The initial architecture requires one.
    pub integration_workers: u16,
}

/// Coordinator-owned resource and liveness policy for concurrent campaign work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamResourcePolicy {
    /// Total abstract CPU units available to active task effects.
    pub total_cpu_units: u16,
    /// CPU units reserved for one implementation pod.
    pub implementation_cpu_units: u16,
    /// Total memory bytes available to active task effects.
    pub total_memory_bytes: u64,
    /// Memory bytes reserved for one implementation pod.
    pub implementation_memory_bytes: u64,
    /// Total temporary disk bytes available to active task effects.
    pub total_disk_bytes: u64,
    /// Temporary disk bytes reserved for one implementation pod.
    pub implementation_disk_bytes: u64,
    /// Total process slots across active task effects.
    pub total_processes: u32,
    /// Process slots reserved for one implementation pod.
    pub implementation_processes: u32,
    /// Hard elapsed ceiling for one implementation pod.
    pub pod_timeout_ms: u64,
    /// Expected interval between coordinator observations of an active pod.
    pub heartbeat_interval_ms: u64,
    /// Elapsed time after which an unobserved pod is stale.
    pub stale_after_ms: u64,
    /// Maximum retry attempts for one retryable provider effect.
    pub provider_retry_limit: u16,
    /// Consecutive retryable failures that open the provider circuit.
    pub provider_failure_threshold: u16,
    /// Minimum elapsed time before an open provider circuit permits one probe.
    pub provider_cooldown_ms: u64,
}

impl Default for TeamResourcePolicy {
    fn default() -> Self {
        Self {
            total_cpu_units: 5,
            implementation_cpu_units: 1,
            total_memory_bytes: 20 * 1024 * 1024 * 1024,
            implementation_memory_bytes: 4 * 1024 * 1024 * 1024,
            total_disk_bytes: 50 * 1024 * 1024 * 1024,
            implementation_disk_bytes: 10 * 1024 * 1024 * 1024,
            total_processes: 320,
            implementation_processes: 64,
            pod_timeout_ms: 60 * 60 * 1_000,
            heartbeat_interval_ms: 5_000,
            stale_after_ms: 30_000,
            provider_retry_limit: 3,
            provider_failure_threshold: 3,
            provider_cooldown_ms: 60_000,
        }
    }
}

impl TeamResourcePolicy {
    fn verify(&self, concurrency: &CampaignConcurrency) -> Result<(), CampaignError> {
        let implementers = u64::from(concurrency.claude_implementers);
        if self.total_cpu_units == 0
            || self.implementation_cpu_units == 0
            || self.implementation_cpu_units > self.total_cpu_units
            || u64::from(self.implementation_cpu_units).saturating_mul(implementers)
                > u64::from(self.total_cpu_units)
            || self.total_memory_bytes == 0
            || self.total_memory_bytes > MAX_RESOURCE_BYTES
            || self.implementation_memory_bytes == 0
            || self
                .implementation_memory_bytes
                .saturating_mul(implementers)
                > self.total_memory_bytes
            || self.total_disk_bytes == 0
            || self.total_disk_bytes > MAX_RESOURCE_BYTES
            || self.implementation_disk_bytes == 0
            || self.implementation_disk_bytes.saturating_mul(implementers) > self.total_disk_bytes
            || self.total_processes == 0
            || self.implementation_processes == 0
            || u64::from(self.implementation_processes).saturating_mul(implementers)
                > u64::from(self.total_processes)
            || self.pod_timeout_ms == 0
            || self.pod_timeout_ms > MAX_ELAPSED_MS
            || self.heartbeat_interval_ms == 0
            || self.heartbeat_interval_ms >= self.stale_after_ms
            || self.stale_after_ms > self.pod_timeout_ms
            || self.provider_retry_limit == 0
            || self.provider_retry_limit > 100
            || self.provider_failure_threshold == 0
            || self.provider_failure_threshold > 100
            || self.provider_cooldown_ms == 0
            || self.provider_cooldown_ms > MAX_ELAPSED_MS
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
}

impl Default for CampaignConcurrency {
    fn default() -> Self {
        Self {
            claude_implementers: 1,
            codex_team_leads: 1,
            codex_reviewers: 1,
            test_workers: 1,
            github_writers: 1,
            integration_workers: 1,
        }
    }
}

impl CampaignConcurrency {
    fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        if self.claude_implementers == 0
            || self.claude_implementers > spec.max_parallel_pods
            || self.codex_team_leads != 1
            || self.codex_reviewers == 0
            || self.codex_reviewers > 16
            || self.test_workers == 0
            || self.test_workers > 64
            || self.github_writers != 1
            || self.integration_workers != 1
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
}

/// Independently bounded coordinator actor class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorClass {
    /// Read-only campaign planning.
    TeamLead,
    /// Claude implementation or correction.
    Implementer,
    /// Deterministic gate execution.
    Gate,
    /// Fresh read-only Codex review.
    Reviewer,
    /// Idempotent GitHub synchronization.
    GitHub,
    /// Serialized campaign-head integration.
    Integration,
}

/// Multi-agent authority added to a campaign without changing legacy serial authority bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MultiAgentPolicy {
    /// Closed policy schema version.
    pub version: u16,
    /// Serial or parallel execution.
    pub execution_mode: CampaignExecutionMode,
    /// Optional remote publication shape.
    pub publication_mode: TaskPublicationMode,
    /// Task-to-campaign integration authority.
    pub task_integration_policy: TaskIntegrationPolicy,
    /// Campaign-to-destination promotion authority.
    pub destination_promotion_policy: DestinationPromotionPolicy,
    /// Integration commit strategy.
    pub task_merge_strategy: TaskMergeStrategy,
    /// Exact remote authority, present only when remote publication is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubCampaignPolicy>,
    /// Independent actor and worker ceilings.
    pub concurrency: CampaignConcurrency,
    /// Coordinator-owned concurrent resource and liveness ceilings.
    pub resources: TeamResourcePolicy,
    /// Maximum provider tokens observed for the whole campaign.
    pub max_campaign_tokens: u64,
    /// Maximum provider tokens observed for one task.
    pub max_task_tokens: u64,
    /// Maximum correction cycles for one task.
    pub max_task_correction_cycles: u16,
    /// Maximum follow-up tasks accepted inside the original authority.
    pub max_follow_up_tasks: u16,
    /// Number of task integrations between cumulative campaign validation checkpoints.
    #[serde(default = "default_integration_validation_interval")]
    pub integration_validation_interval: u16,
}

const fn default_integration_validation_interval() -> u16 {
    1
}

impl MultiAgentPolicy {
    /// Validates the complete multi-agent authority against the enclosing campaign.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError::InvalidAuthority`] for contradictory or unbounded policy.
    pub fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        self.concurrency.verify(spec)?;
        self.resources.verify(&self.concurrency)?;
        let remote_publication = self.publication_mode != TaskPublicationMode::LocalOnly;
        if self.version != TEAM_POLICY_VERSION
            || self.max_campaign_tokens == 0
            || self.max_campaign_tokens > MAX_PROVIDER_TOKENS
            || self.max_task_tokens == 0
            || self.max_task_tokens > self.max_campaign_tokens
            || self.max_task_correction_cycles == 0
            || self.max_task_correction_cycles > 100
            || self.max_follow_up_tasks > 10_000
            || self.integration_validation_interval == 0
            || self.integration_validation_interval > 10_000
            || (self.execution_mode == CampaignExecutionMode::Serial
                && self.concurrency.claude_implementers != 1)
            || (self.publication_mode != TaskPublicationMode::LocalOnly
                && matches!(spec.publication, super::CampaignPublication::LocalOnly))
            || remote_publication != self.github.is_some()
            || self
                .github
                .as_ref()
                .is_some_and(|github| github.verify(spec).is_err())
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
}

fn valid_host(value: &str) -> bool {
    valid_component(value) && value.contains('.') && !value.starts_with('.')
}

fn valid_external_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(char::is_control)
        && !value.contains(['\0', '\n', '\r'])
}

/// Durable campaign-level lifecycle for one authorized task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignTaskState {
    /// Task exists in the authorized plan.
    Planned,
    /// Dependencies and policy permit proposal.
    Ready,
    /// Team lead proposed the task for one immutable generation.
    Proposed,
    /// Exact pod, paths, and resources are leased.
    Leased,
    /// Claude implementation is active.
    Implementing,
    /// Deterministic local gates are active.
    LocalGates,
    /// Fresh independent Codex review is active.
    Reviewing,
    /// The matching Claude lineage is applying bounded findings.
    Correcting,
    /// Local gates and independent review permit publication or local integration.
    PublicationReady,
    /// One exact draft task pull request exists.
    PullRequestOpen,
    /// Required remote checks are pending.
    CiWaiting,
    /// Accepted work is in the serialized integration queue.
    IntegrationQueued,
    /// Current-head and policy checks permit one integration attempt.
    MergeReady,
    /// A durable integration intent is being reconciled.
    Integrating,
    /// The task is present in the authoritative campaign head.
    Merged,
    /// A typed prerequisite prevents completion.
    Blocked,
    /// Independent judgment requires an operator decision.
    Disputed,
    /// The bounded task failed terminally.
    Failed,
    /// An authenticated campaign control cancelled the task.
    Cancelled,
}

/// Closed content-free reason retained when a campaign task becomes terminal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTerminalReason {
    /// Task was integrated into the authoritative campaign head.
    Merged,
    /// A required prerequisite is unavailable.
    PrerequisiteBlocked,
    /// Implementation provider reported a bounded blocker.
    ProviderBlocked,
    /// Independent reviewer reported a blocker.
    ReviewBlocked,
    /// Independent reviewer and implementation evidence require human judgment.
    ReviewDisputed,
    /// Required deterministic verification failed terminally.
    GateFailed,
    /// Provider failed beyond bounded retry policy.
    ProviderFailed,
    /// Exact owned provider process crashed.
    ProcessCrashed,
    /// Exact task deadline elapsed.
    TimedOut,
    /// A configured attempt, token, process, output, storage, or elapsed limit was reached.
    LimitExceeded,
    /// Deterministic integration detected a conflict.
    IntegrationConflict,
    /// Repository, branch, worktree, commit, issue, or pull-request identity became stale.
    StaleIdentity,
    /// Deny-first policy refused the requested effect.
    PolicyDenied,
    /// Authenticated operator cancelled the exact task or campaign.
    OperatorCancelled,
    /// Required commit-bound remote checks failed.
    CiFailed,
    /// An explicitly external prerequisite remains unavailable.
    ExternalBlocked,
}

impl TaskTerminalReason {
    /// Returns the stable persisted reason code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::PrerequisiteBlocked => "prerequisite_blocked",
            Self::ProviderBlocked => "provider_blocked",
            Self::ReviewBlocked => "review_blocked",
            Self::ReviewDisputed => "review_disputed",
            Self::GateFailed => "gate_failed",
            Self::ProviderFailed => "provider_failed",
            Self::ProcessCrashed => "process_crashed",
            Self::TimedOut => "timed_out",
            Self::LimitExceeded => "limit_exceeded",
            Self::IntegrationConflict => "integration_conflict",
            Self::StaleIdentity => "stale_identity",
            Self::PolicyDenied => "policy_denied",
            Self::OperatorCancelled => "operator_cancelled",
            Self::CiFailed => "ci_failed",
            Self::ExternalBlocked => "external_blocked",
        }
    }

    const fn valid_for(self, state: CampaignTaskState) -> bool {
        match state {
            CampaignTaskState::Merged => matches!(self, Self::Merged),
            CampaignTaskState::Disputed => matches!(self, Self::ReviewDisputed),
            CampaignTaskState::Cancelled => matches!(self, Self::OperatorCancelled),
            CampaignTaskState::Blocked => matches!(
                self,
                Self::PrerequisiteBlocked
                    | Self::ProviderBlocked
                    | Self::ReviewBlocked
                    | Self::IntegrationConflict
                    | Self::StaleIdentity
                    | Self::PolicyDenied
                    | Self::ExternalBlocked
            ),
            CampaignTaskState::Failed => matches!(
                self,
                Self::GateFailed
                    | Self::ProviderFailed
                    | Self::ProcessCrashed
                    | Self::TimedOut
                    | Self::LimitExceeded
                    | Self::CiFailed
                    | Self::StaleIdentity
                    | Self::PolicyDenied
            ),
            _ => false,
        }
    }
}

impl CampaignTaskState {
    /// Returns whether this state cannot advance without a new operator-authored campaign decision.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Merged | Self::Blocked | Self::Disputed | Self::Failed | Self::Cancelled
        )
    }
}

/// Content-free utilization retained for one task.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskUtilization {
    /// Attempted provider calls.
    pub provider_attempts: u32,
    /// Provider-reported or tokenizer-observed tokens when available.
    pub provider_tokens: u64,
    /// Provider and gate processes.
    pub process_invocations: u32,
    /// Observed output bytes.
    pub output_bytes: u64,
    /// Retained private state bytes.
    pub retained_state_bytes: u64,
    /// Observed execution milliseconds.
    pub execution_elapsed_ms: u64,
}

impl TaskUtilization {
    fn checked_add(&self, other: &Self) -> Option<Self> {
        Some(Self {
            provider_attempts: self
                .provider_attempts
                .checked_add(other.provider_attempts)?,
            provider_tokens: self.provider_tokens.checked_add(other.provider_tokens)?,
            process_invocations: self
                .process_invocations
                .checked_add(other.process_invocations)?,
            output_bytes: self.output_bytes.checked_add(other.output_bytes)?,
            retained_state_bytes: self
                .retained_state_bytes
                .checked_add(other.retained_state_bytes)?,
            execution_elapsed_ms: self
                .execution_elapsed_ms
                .checked_add(other.execution_elapsed_ms)?,
        })
    }

    fn dominates(&self, prior: &Self) -> bool {
        self.provider_attempts >= prior.provider_attempts
            && self.provider_tokens >= prior.provider_tokens
            && self.process_invocations >= prior.process_invocations
            && self.output_bytes >= prior.output_bytes
            && self.retained_state_bytes >= prior.retained_state_bytes
            && self.execution_elapsed_ms >= prior.execution_elapsed_ms
    }
}

/// One exact active resource reservation owned by a task effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResourceReservation {
    /// Deterministic reservation identity.
    pub reservation_id: String,
    /// Canonical task identity.
    pub task_id: String,
    /// Exact active pod lease.
    pub lease_id: String,
    /// Independently limited actor class.
    pub actor: ActorClass,
    /// Reserved abstract CPU units.
    pub cpu_units: u16,
    /// Reserved memory bytes.
    pub memory_bytes: u64,
    /// Reserved temporary disk bytes.
    pub disk_bytes: u64,
    /// Reserved process slots.
    pub process_slots: u32,
    /// Exclusive named resources held for this effect.
    pub exclusive_resources: Vec<String>,
    /// Effect start observation.
    pub started_at_ms: u64,
    /// Hard effect deadline.
    pub deadline_ms: u64,
    /// Latest monotonic heartbeat sequence.
    pub heartbeat_sequence: u64,
    /// Latest heartbeat observation.
    pub heartbeat_timestamp_ms: u64,
    /// Monotonic utilization observed for this active effect.
    pub observed: TaskUtilization,
}

impl TaskResourceReservation {
    fn verify(&self, policy: &TeamResourcePolicy) -> Result<(), TeamStateError> {
        if !valid_component(&self.reservation_id)
            || codingmage_contracts::TaskId::new(self.task_id.clone()).is_err()
            || !valid_component(&self.lease_id)
            || self.cpu_units == 0
            || self.cpu_units > policy.total_cpu_units
            || self.memory_bytes == 0
            || self.memory_bytes > policy.total_memory_bytes
            || self.disk_bytes > policy.total_disk_bytes
            || self.process_slots == 0
            || self.process_slots > policy.total_processes
            || self.exclusive_resources.len() > 1_024
            || self
                .exclusive_resources
                .iter()
                .any(|resource| !valid_component(resource))
            || self
                .exclusive_resources
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.exclusive_resources.len()
            || self.started_at_ms > self.heartbeat_timestamp_ms
            || self.deadline_ms <= self.started_at_ms
            || self.deadline_ms.saturating_sub(self.started_at_ms) > policy.pod_timeout_ms
            || self.observed.provider_tokens > MAX_PROVIDER_TOKENS
        {
            return Err(TeamStateError::InvalidResource);
        }
        Ok(())
    }
}

/// Closed provider circuit state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCircuitStatus {
    /// Ordinary calls may begin.
    Closed,
    /// Calls are denied until the configured cooldown elapses.
    Open,
    /// Exactly one recovery probe may be in flight.
    HalfOpen,
}

/// Durable retry-storm prevention for one provider role.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCircuit {
    /// Closed circuit lifecycle.
    pub status: ProviderCircuitStatus,
    /// Consecutive retryable failures since the last success.
    pub consecutive_failures: u16,
    /// Last failure observation.
    pub last_failure_ms: Option<u64>,
    /// Number of attempts consumed by the current bounded operation.
    pub operation_attempts: u16,
}

impl Default for ProviderCircuit {
    fn default() -> Self {
        Self {
            status: ProviderCircuitStatus::Closed,
            consecutive_failures: 0,
            last_failure_ms: None,
            operation_attempts: 0,
        }
    }
}

impl ProviderCircuit {
    fn verify(&self, policy: &TeamResourcePolicy) -> Result<(), TeamStateError> {
        if self.consecutive_failures > policy.provider_failure_threshold
            || self.operation_attempts > policy.provider_retry_limit
            || (self.status == ProviderCircuitStatus::Closed
                && self.consecutive_failures >= policy.provider_failure_threshold)
            || (self.status != ProviderCircuitStatus::Closed && self.last_failure_ms.is_none())
        {
            return Err(TeamStateError::InvalidCircuit);
        }
        Ok(())
    }
}

/// Complete persistent concurrent-resource projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamResourceSnapshot {
    /// Closed snapshot schema version.
    pub version: u16,
    /// Actor concurrency ceilings.
    pub concurrency: CampaignConcurrency,
    /// Physical, liveness, and retry ceilings.
    pub policy: TeamResourcePolicy,
    /// Campaign provider-token ceiling.
    pub max_campaign_tokens: u64,
    /// Per-task provider-token ceiling.
    pub max_task_tokens: u64,
    /// Active reservations indexed by exact identity.
    pub active: BTreeMap<String, TaskResourceReservation>,
    /// Released reservation identities that cannot be reused.
    pub released: BTreeSet<String>,
    /// Utilization already released into the campaign total.
    pub consumed: TaskUtilization,
    /// Provider circuits indexed by operator-owned role identity.
    pub provider_circuits: BTreeMap<String, ProviderCircuit>,
    /// Next monotonic reservation sequence.
    pub next_sequence: u64,
}

impl TeamResourceSnapshot {
    /// Revalidates all active reservations, actor limits, resource sums, and provider circuits.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for malformed identity, overlap, overcommit, or budget drift.
    pub fn verify(&self) -> Result<(), TeamStateError> {
        if self.version != TEAM_STATE_VERSION
            || self.max_campaign_tokens == 0
            || self.max_campaign_tokens > MAX_PROVIDER_TOKENS
            || self.max_task_tokens == 0
            || self.max_task_tokens > self.max_campaign_tokens
            || self.active.keys().any(|id| !valid_component(id))
            || self.released.iter().any(|id| !valid_component(id))
            || self.active.keys().any(|id| self.released.contains(id))
            || self
                .provider_circuits
                .keys()
                .any(|provider| !valid_component(provider))
        {
            return Err(TeamStateError::InvalidResource);
        }
        self.policy
            .verify(&self.concurrency)
            .map_err(|_| TeamStateError::InvalidResource)?;
        let mut cpu = 0_u64;
        let mut memory = 0_u64;
        let mut disk = 0_u64;
        let mut processes = 0_u64;
        let mut actors = BTreeMap::<ActorClass, usize>::new();
        let mut resources = BTreeSet::new();
        let mut active_usage = TaskUtilization::default();
        for (reservation_id, reservation) in &self.active {
            reservation.verify(&self.policy)?;
            if reservation_id != &reservation.reservation_id
                || reservation
                    .exclusive_resources
                    .iter()
                    .any(|resource| !resources.insert(resource))
                || reservation.observed.provider_tokens > self.max_task_tokens
            {
                return Err(TeamStateError::InvalidResource);
            }
            cpu = cpu.saturating_add(u64::from(reservation.cpu_units));
            memory = memory.saturating_add(reservation.memory_bytes);
            disk = disk.saturating_add(reservation.disk_bytes);
            processes = processes.saturating_add(u64::from(reservation.process_slots));
            *actors.entry(reservation.actor).or_default() += 1;
            active_usage = active_usage
                .checked_add(&reservation.observed)
                .ok_or(TeamStateError::InvalidResource)?;
        }
        let total_usage = self
            .consumed
            .checked_add(&active_usage)
            .ok_or(TeamStateError::InvalidResource)?;
        if cpu > u64::from(self.policy.total_cpu_units)
            || memory > self.policy.total_memory_bytes
            || disk > self.policy.total_disk_bytes
            || processes > u64::from(self.policy.total_processes)
            || total_usage.provider_tokens > self.max_campaign_tokens
            || actor_count(&actors, ActorClass::Implementer)
                > usize::from(self.concurrency.claude_implementers)
            || actor_count(&actors, ActorClass::TeamLead)
                > usize::from(self.concurrency.codex_team_leads)
            || actor_count(&actors, ActorClass::Reviewer)
                > usize::from(self.concurrency.codex_reviewers)
            || actor_count(&actors, ActorClass::Gate) > usize::from(self.concurrency.test_workers)
            || actor_count(&actors, ActorClass::GitHub)
                > usize::from(self.concurrency.github_writers)
            || actor_count(&actors, ActorClass::Integration)
                > usize::from(self.concurrency.integration_workers)
        {
            return Err(TeamStateError::InvalidResource);
        }
        for circuit in self.provider_circuits.values() {
            circuit.verify(&self.policy)?;
        }
        Ok(())
    }
}

/// Persistent campaign-wide resource admission and provider circuit controller.
#[derive(Clone, Debug)]
pub struct TeamResourceController {
    snapshot: TeamResourceSnapshot,
}

impl TeamResourceController {
    /// Creates an empty resource controller from verified multi-agent authority.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when the policy is absent or invalid.
    pub fn new(spec: &CampaignSpec) -> Result<Self, TeamStateError> {
        spec.verify().map_err(|_| TeamStateError::Authority)?;
        let policy = spec.multi_agent.as_ref().ok_or(TeamStateError::Authority)?;
        let snapshot = TeamResourceSnapshot {
            version: TEAM_STATE_VERSION,
            concurrency: policy.concurrency.clone(),
            policy: policy.resources.clone(),
            max_campaign_tokens: policy.max_campaign_tokens,
            max_task_tokens: policy.max_task_tokens,
            active: BTreeMap::new(),
            released: BTreeSet::new(),
            consumed: TaskUtilization::default(),
            provider_circuits: BTreeMap::new(),
            next_sequence: 0,
        };
        snapshot.verify()?;
        Ok(Self { snapshot })
    }

    /// Reconstructs the controller from a complete verified snapshot and exact authority.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] if the snapshot or configured authority differs.
    pub fn from_snapshot(
        spec: &CampaignSpec,
        snapshot: TeamResourceSnapshot,
    ) -> Result<Self, TeamStateError> {
        let expected = Self::new(spec)?;
        snapshot.verify()?;
        if snapshot.concurrency != expected.snapshot.concurrency
            || snapshot.policy != expected.snapshot.policy
            || snapshot.max_campaign_tokens != expected.snapshot.max_campaign_tokens
            || snapshot.max_task_tokens != expected.snapshot.max_task_tokens
        {
            return Err(TeamStateError::Authority);
        }
        Ok(Self { snapshot })
    }

    /// Returns the complete persistent projection.
    #[must_use]
    pub const fn snapshot(&self) -> &TeamResourceSnapshot {
        &self.snapshot
    }

    /// Creates the configured implementation reservation shape for one active lease.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid lease identity or time overflow.
    pub fn implementation_request(
        &self,
        lease: &DurablePodLease,
        started_at_ms: u64,
    ) -> Result<TaskResourceReservation, TeamStateError> {
        lease.verify()?;
        let sequence = self.snapshot.next_sequence;
        let reservation_id =
            bounded_identity("resource", &lease.lease_id, &lease.task_id, sequence);
        Ok(TaskResourceReservation {
            reservation_id,
            task_id: lease.task_id.clone(),
            lease_id: lease.lease_id.clone(),
            actor: ActorClass::Implementer,
            cpu_units: self.snapshot.policy.implementation_cpu_units,
            memory_bytes: self.snapshot.policy.implementation_memory_bytes,
            disk_bytes: self.snapshot.policy.implementation_disk_bytes,
            process_slots: self.snapshot.policy.implementation_processes,
            exclusive_resources: lease.test_resources.clone(),
            started_at_ms,
            deadline_ms: started_at_ms
                .checked_add(self.snapshot.policy.pod_timeout_ms)
                .ok_or(TeamStateError::InvalidResource)?,
            heartbeat_sequence: 0,
            heartbeat_timestamp_ms: started_at_ms,
            observed: TaskUtilization::default(),
        })
    }

    /// Reserves one exact actor effect after all actor, physical, and exclusive-resource checks.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] without mutation when any ceiling or identity would conflict.
    pub fn reserve(&mut self, reservation: TaskResourceReservation) -> Result<(), TeamStateError> {
        reservation.verify(&self.snapshot.policy)?;
        if self
            .snapshot
            .active
            .contains_key(&reservation.reservation_id)
            || self.snapshot.released.contains(&reservation.reservation_id)
            || self.snapshot.active.values().any(|active| {
                active.lease_id == reservation.lease_id && active.actor == reservation.actor
            })
            || self.snapshot.active.values().any(|active| {
                active.exclusive_resources.iter().any(|left| {
                    reservation
                        .exclusive_resources
                        .iter()
                        .any(|right| left == right)
                })
            })
        {
            return Err(TeamStateError::ResourceConflict);
        }
        let mut candidate = self.snapshot.clone();
        candidate.next_sequence = candidate.next_sequence.saturating_add(1);
        candidate
            .active
            .insert(reservation.reservation_id.clone(), reservation);
        candidate.verify().map_err(|error| match error {
            TeamStateError::InvalidResource => TeamStateError::ResourceCapacity,
            other => other,
        })?;
        self.snapshot = candidate;
        Ok(())
    }

    /// Records monotonic utilization and heartbeat for one exact active reservation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for unknown identity, regression, timeout, or budget exhaustion.
    pub fn observe(
        &mut self,
        reservation_id: &str,
        heartbeat_sequence: u64,
        timestamp_ms: u64,
        utilization: TaskUtilization,
    ) -> Result<(), TeamStateError> {
        let mut candidate = self.snapshot.clone();
        let reservation = candidate
            .active
            .get_mut(reservation_id)
            .ok_or(TeamStateError::UnknownResource)?;
        if utilization.provider_tokens > candidate.max_task_tokens {
            return Err(TeamStateError::ResourceCapacity);
        }
        if heartbeat_sequence <= reservation.heartbeat_sequence
            || timestamp_ms < reservation.heartbeat_timestamp_ms
            || timestamp_ms > reservation.deadline_ms
            || !utilization.dominates(&reservation.observed)
        {
            return Err(TeamStateError::InvalidHeartbeat);
        }
        reservation.heartbeat_sequence = heartbeat_sequence;
        reservation.heartbeat_timestamp_ms = timestamp_ms;
        reservation.observed = utilization;
        candidate.verify().map_err(|error| match error {
            TeamStateError::InvalidResource => TeamStateError::ResourceCapacity,
            other => other,
        })?;
        self.snapshot = candidate;
        Ok(())
    }

    /// Returns active reservations whose heartbeat exceeded the configured stale interval.
    #[must_use]
    pub fn stale_at(&self, timestamp_ms: u64) -> Vec<String> {
        self.snapshot
            .active
            .iter()
            .filter(|(_, reservation)| {
                timestamp_ms.saturating_sub(reservation.heartbeat_timestamp_ms)
                    >= self.snapshot.policy.stale_after_ms
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Releases one exact reservation and permanently accounts its observed utilization.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for unknown/reused identity or arithmetic contradiction.
    pub fn release(
        &mut self,
        reservation_id: &str,
    ) -> Result<TaskResourceReservation, TeamStateError> {
        let mut candidate = self.snapshot.clone();
        let reservation = candidate
            .active
            .remove(reservation_id)
            .ok_or(TeamStateError::UnknownResource)?;
        candidate.consumed = candidate
            .consumed
            .checked_add(&reservation.observed)
            .ok_or(TeamStateError::InvalidResource)?;
        if !candidate.released.insert(reservation_id.to_owned()) {
            return Err(TeamStateError::UnknownResource);
        }
        candidate.verify()?;
        self.snapshot = candidate;
        Ok(reservation)
    }

    /// Begins one provider operation only when retry and circuit policy permit it.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for malformed provider identity or an open/exhausted circuit.
    pub fn begin_provider_attempt(
        &mut self,
        provider: &str,
        timestamp_ms: u64,
    ) -> Result<(), TeamStateError> {
        if !valid_component(provider) {
            return Err(TeamStateError::InvalidCircuit);
        }
        let circuit = self
            .snapshot
            .provider_circuits
            .entry(provider.to_owned())
            .or_default();
        match circuit.status {
            ProviderCircuitStatus::Closed => {}
            ProviderCircuitStatus::Open
                if timestamp_ms.saturating_sub(
                    circuit
                        .last_failure_ms
                        .ok_or(TeamStateError::InvalidCircuit)?,
                ) >= self.snapshot.policy.provider_cooldown_ms =>
            {
                circuit.status = ProviderCircuitStatus::HalfOpen;
                circuit.operation_attempts = 0;
            }
            ProviderCircuitStatus::Open | ProviderCircuitStatus::HalfOpen => {
                return Err(TeamStateError::CircuitOpen);
            }
        }
        if circuit.operation_attempts >= self.snapshot.policy.provider_retry_limit {
            return Err(TeamStateError::CircuitOpen);
        }
        circuit.operation_attempts = circuit.operation_attempts.saturating_add(1);
        self.snapshot.verify()
    }

    /// Records one provider result and deterministically closes or opens its circuit.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when no matching attempt exists or the circuit is malformed.
    pub fn finish_provider_attempt(
        &mut self,
        provider: &str,
        timestamp_ms: u64,
        succeeded: bool,
        retryable_failure: bool,
    ) -> Result<(), TeamStateError> {
        let circuit = self
            .snapshot
            .provider_circuits
            .get_mut(provider)
            .ok_or(TeamStateError::InvalidCircuit)?;
        if circuit.operation_attempts == 0 || succeeded == retryable_failure {
            return Err(TeamStateError::InvalidCircuit);
        }
        if succeeded {
            *circuit = ProviderCircuit::default();
        } else if retryable_failure {
            circuit.consecutive_failures = circuit.consecutive_failures.saturating_add(1);
            circuit.last_failure_ms = Some(timestamp_ms);
            if circuit.consecutive_failures >= self.snapshot.policy.provider_failure_threshold {
                circuit.status = ProviderCircuitStatus::Open;
                circuit.consecutive_failures = self.snapshot.policy.provider_failure_threshold;
            } else {
                circuit.status = ProviderCircuitStatus::Closed;
            }
        } else {
            circuit.operation_attempts = self.snapshot.policy.provider_retry_limit;
        }
        self.snapshot.verify()
    }
}

/// One validated state transition for a durable task record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignTaskTransition {
    /// Required next sequence.
    pub sequence: u64,
    /// Immutable campaign identity.
    pub campaign_id: String,
    /// Immutable task identity.
    pub task_id: String,
    /// Immutable planning generation.
    pub generation: u64,
    /// Required current state.
    pub from: CampaignTaskState,
    /// Requested next state.
    pub to: CampaignTaskState,
    /// SHA-256 of deterministic evidence authorizing this transition.
    pub evidence_sha256: String,
}

/// One complete task-to-pod-to-publication identity record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignTaskRecord {
    /// Closed record schema version.
    pub version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Canonical task identity.
    pub task_id: String,
    /// Planning generation that admitted the task.
    pub generation: u64,
    /// Assigned pod identity, absent before leasing.
    pub pod_id: Option<String>,
    /// Exact lease identity, absent before leasing.
    pub lease_id: Option<String>,
    /// Exact one-unit runtime identity, absent before execution admission.
    pub run_id: Option<String>,
    /// Owned worktree identity, absent before creation.
    pub worktree_id: Option<String>,
    /// Task branch, absent before creation.
    pub branch: Option<String>,
    /// Immutable task base commit.
    pub base_commit: String,
    /// Latest coordinator-created candidate.
    pub candidate_commit: Option<String>,
    /// Exact candidate accepted by independent review.
    pub reviewed_commit: Option<String>,
    /// Coordinator-created commit prepared for serialized campaign integration.
    pub integration_commit: Option<String>,
    /// Exact mechanical canonical-task completion commit.
    pub completion_commit: Option<String>,
    /// Optional task issue number.
    pub issue_number: Option<u64>,
    /// Optional task pull-request number.
    pub pull_request_number: Option<u64>,
    /// Claude implementation session lineage.
    pub implementation_session: Option<String>,
    /// Ordered Claude correction session lineage.
    pub correction_sessions: Vec<String>,
    /// Fresh Codex review session identities in order.
    pub review_sessions: Vec<String>,
    /// Deterministic gate evidence digests in order.
    pub gate_evidence_sha256: Vec<String>,
    /// Independent review evidence digests in order.
    pub review_evidence_sha256: Vec<String>,
    /// Commit-bound CI evidence digests in order.
    pub ci_evidence_sha256: Vec<String>,
    /// Exact repository-relative path authority.
    pub owned_paths: Vec<PathBuf>,
    /// Exact shared-resource leases.
    pub test_resources: Vec<String>,
    /// Current task lifecycle state.
    pub state: CampaignTaskState,
    /// Required next transition sequence.
    pub next_transition: u64,
    /// Latest monotonic heartbeat sequence.
    pub heartbeat_sequence: u64,
    /// Latest heartbeat observation time.
    pub heartbeat_timestamp_ms: Option<u64>,
    /// Latest transition evidence digest.
    pub last_evidence_sha256: Option<String>,
    /// Content-free terminal reason code.
    pub terminal_reason: Option<String>,
    /// Bounded task utilization.
    pub utilization: TaskUtilization,
}

impl CampaignTaskRecord {
    /// Creates one planned task record with no synthesized runtime or remote identity.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid campaign, task, or base identity.
    pub fn planned(
        campaign_id: String,
        task_id: String,
        base_commit: String,
    ) -> Result<Self, TeamStateError> {
        let record = Self {
            version: TEAM_STATE_VERSION,
            campaign_id,
            task_id,
            generation: 0,
            pod_id: None,
            lease_id: None,
            run_id: None,
            worktree_id: None,
            branch: None,
            base_commit,
            candidate_commit: None,
            reviewed_commit: None,
            integration_commit: None,
            completion_commit: None,
            issue_number: None,
            pull_request_number: None,
            implementation_session: None,
            correction_sessions: Vec::new(),
            review_sessions: Vec::new(),
            gate_evidence_sha256: Vec::new(),
            review_evidence_sha256: Vec::new(),
            ci_evidence_sha256: Vec::new(),
            owned_paths: Vec::new(),
            test_resources: Vec::new(),
            state: CampaignTaskState::Planned,
            next_transition: 0,
            heartbeat_sequence: 0,
            heartbeat_timestamp_ms: None,
            last_evidence_sha256: None,
            terminal_reason: None,
            utilization: TaskUtilization::default(),
        };
        record.verify()?;
        Ok(record)
    }

    /// Revalidates internal identity and state consistency.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] after malformed or contradictory state.
    pub fn verify(&self) -> Result<(), TeamStateError> {
        if self.version != TEAM_STATE_VERSION
            || !valid_component(&self.campaign_id)
            || codingmage_contracts::TaskId::new(self.task_id.clone()).is_err()
            || !valid_commit(&self.base_commit)
            || self
                .candidate_commit
                .as_ref()
                .is_some_and(|v| !valid_commit(v))
            || self
                .reviewed_commit
                .as_ref()
                .is_some_and(|v| !valid_commit(v))
            || self.reviewed_commit.is_some() && self.candidate_commit.is_none()
            || self
                .integration_commit
                .as_ref()
                .is_some_and(|v| !valid_commit(v))
            || self.integration_commit.is_some() && self.reviewed_commit.is_none()
            || self.integration_commit.is_some()
                && !matches!(
                    self.state,
                    CampaignTaskState::Integrating
                        | CampaignTaskState::Merged
                        | CampaignTaskState::Blocked
                        | CampaignTaskState::Failed
                        | CampaignTaskState::Cancelled
                )
            || self
                .completion_commit
                .as_ref()
                .is_some_and(|v| !valid_commit(v))
            || self.completion_commit.is_some() && self.reviewed_commit.is_none()
            || self.state == CampaignTaskState::Merged
                && (self.integration_commit.is_none() || self.completion_commit.is_none())
            || self.issue_number == Some(0)
            || self.pull_request_number == Some(0)
            || self.pull_request_number.is_some() && self.issue_number.is_none()
            || !self.ci_evidence_sha256.is_empty() && self.pull_request_number.is_none()
            || self.review_sessions.len() > MAX_SESSIONS
            || self.correction_sessions.len() > MAX_SESSIONS
            || self.review_sessions.iter().any(|v| !valid_component(v))
            || self.correction_sessions.iter().any(|v| !valid_component(v))
            || self
                .correction_sessions
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.correction_sessions.len()
            || self.gate_evidence_sha256.len() > MAX_SESSIONS
            || self.review_evidence_sha256.len() > MAX_SESSIONS
            || self.ci_evidence_sha256.len() > MAX_SESSIONS
            || self
                .gate_evidence_sha256
                .iter()
                .chain(&self.review_evidence_sha256)
                .chain(&self.ci_evidence_sha256)
                .any(|value| !valid_sha256(value))
            || self
                .implementation_session
                .as_ref()
                .is_some_and(|v| !valid_component(v))
            || self.owned_paths.iter().any(|path| !safe_relative(path))
            || self.owned_paths.iter().enumerate().any(|(index, left)| {
                self.owned_paths[index + 1..]
                    .iter()
                    .any(|right| paths_overlap(left, right))
            })
            || self.test_resources.iter().any(|v| !valid_component(v))
            || self.test_resources.iter().collect::<BTreeSet<_>>().len()
                != self.test_resources.len()
            || self
                .last_evidence_sha256
                .as_ref()
                .is_some_and(|v| !valid_sha256(v))
            || self
                .terminal_reason
                .as_ref()
                .is_some_and(|v| !valid_reason(v))
            || self.state.is_terminal() != self.terminal_reason.is_some()
            || !valid_optional_component(self.pod_id.as_ref())
            || !valid_optional_component(self.lease_id.as_ref())
            || self
                .run_id
                .as_ref()
                .is_some_and(|value| codingmage_contracts::RunId::new(value.clone()).is_err())
            || !valid_optional_component(self.worktree_id.as_ref())
            || self.branch.as_ref().is_some_and(|v| !valid_branch(v))
            || !runtime_identity_shape(self)
        {
            return Err(TeamStateError::InvalidRecord);
        }
        Ok(())
    }

    /// Moves one ready task into an immutable planning generation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for a zero generation, wrong state, or invalid evidence.
    pub fn propose(
        &mut self,
        generation: u64,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if generation == 0 || self.state != CampaignTaskState::Ready {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.generation = generation;
        let transition = CampaignTaskTransition {
            sequence: candidate.next_transition,
            campaign_id: candidate.campaign_id.clone(),
            task_id: candidate.task_id.clone(),
            generation,
            from: CampaignTaskState::Ready,
            to: CampaignTaskState::Proposed,
            evidence_sha256,
        };
        candidate.transition(&transition)?;
        *self = candidate;
        Ok(())
    }

    /// Refreshes a dependency-ready unassigned task onto the current campaign head.
    ///
    /// Exact replay of the current base is observational. No task with an assigned runtime,
    /// candidate, review, or remote publication identity may be rebased through this operation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale state, malformed head, or an already-assigned task.
    pub fn refresh_ready_base(
        &mut self,
        campaign_head: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.state != CampaignTaskState::Ready {
            return Err(TeamStateError::InvalidTransition);
        }
        if self.base_commit == campaign_head {
            return Ok(());
        }
        if !valid_commit(&campaign_head)
            || !valid_sha256(&evidence_sha256)
            || self.pod_id.is_some()
            || self.candidate_commit.is_some()
            || self.issue_number.is_some()
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.base_commit = campaign_head;
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Applies one ordered legal transition.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for wrong identity, sequence, state, evidence, or transition.
    pub fn transition(
        &mut self,
        transition: &CampaignTaskTransition,
    ) -> Result<(), TeamStateError> {
        let reason = match transition.to {
            CampaignTaskState::Merged => Some(TaskTerminalReason::Merged),
            CampaignTaskState::Blocked => Some(TaskTerminalReason::ExternalBlocked),
            CampaignTaskState::Disputed => Some(TaskTerminalReason::ReviewDisputed),
            CampaignTaskState::Failed => Some(TaskTerminalReason::ProviderFailed),
            CampaignTaskState::Cancelled => Some(TaskTerminalReason::OperatorCancelled),
            _ => None,
        };
        self.transition_inner(transition, reason)
    }

    /// Applies one ordered terminal transition with an exact closed reason.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when the transition or reason does not match the target state.
    pub fn transition_terminal(
        &mut self,
        transition: &CampaignTaskTransition,
        reason: TaskTerminalReason,
    ) -> Result<(), TeamStateError> {
        if !transition.to.is_terminal() || !reason.valid_for(transition.to) {
            return Err(TeamStateError::InvalidTransition);
        }
        self.transition_inner(transition, Some(reason))
    }

    fn transition_inner(
        &mut self,
        transition: &CampaignTaskTransition,
        terminal_reason: Option<TaskTerminalReason>,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if transition.campaign_id != self.campaign_id
            || transition.task_id != self.task_id
            || transition.generation != self.generation
            || transition.sequence != self.next_transition
            || transition.from != self.state
            || !valid_sha256(&transition.evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&transition.evidence_sha256)
            || !legal_transition(transition.from, transition.to)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.state = transition.to;
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(transition.evidence_sha256.clone());
        if transition.to.is_terminal() {
            let reason = terminal_reason.ok_or(TeamStateError::InvalidTransition)?;
            if !reason.valid_for(transition.to) {
                return Err(TeamStateError::InvalidTransition);
            }
            candidate.terminal_reason = Some(reason.code().to_owned());
        } else if terminal_reason.is_some() {
            return Err(TeamStateError::InvalidTransition);
        }
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Records a strictly increasing heartbeat observation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for a nonactive state or nonmonotonic observation.
    pub fn heartbeat(&mut self, sequence: u64, timestamp_ms: u64) -> Result<(), TeamStateError> {
        if !matches!(
            self.state,
            CampaignTaskState::Leased
                | CampaignTaskState::Implementing
                | CampaignTaskState::LocalGates
                | CampaignTaskState::Reviewing
                | CampaignTaskState::Correcting
                | CampaignTaskState::PublicationReady
                | CampaignTaskState::PullRequestOpen
                | CampaignTaskState::CiWaiting
                | CampaignTaskState::IntegrationQueued
                | CampaignTaskState::MergeReady
                | CampaignTaskState::Integrating
        ) || sequence <= self.heartbeat_sequence
            || self
                .heartbeat_timestamp_ms
                .is_some_and(|previous| timestamp_ms < previous)
        {
            return Err(TeamStateError::InvalidHeartbeat);
        }
        self.heartbeat_sequence = sequence;
        self.heartbeat_timestamp_ms = Some(timestamp_ms);
        Ok(())
    }

    /// Binds one exact scheduler lease before any Git worktree effect begins.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale generation, mismatched identity, or invalid transition.
    pub fn bind_lease(
        &mut self,
        lease: &DurablePodLease,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        lease.verify()?;
        if self.state != CampaignTaskState::Proposed
            || lease.task_id != self.task_id
            || lease.generation != self.generation
            || lease.source_head != self.base_commit
            || !valid_sha256(&evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidLease);
        }
        let mut candidate = self.clone();
        candidate.pod_id = Some(lease.pod_id.clone());
        candidate.lease_id = Some(lease.lease_id.clone());
        candidate.owned_paths.clone_from(&lease.owned_paths);
        candidate.test_resources.clone_from(&lease.test_resources);
        candidate.state = CampaignTaskState::Leased;
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Binds the coordinator-created worktree and branch and starts implementation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for missing lease authority, malformed identity, or replay.
    pub fn bind_worktree(
        &mut self,
        worktree_id: String,
        branch: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.state != CampaignTaskState::Leased
            || !valid_component(&worktree_id)
            || !valid_branch(&branch)
            || !valid_sha256(&evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidLease);
        }
        let mut candidate = self.clone();
        candidate.worktree_id = Some(worktree_id);
        candidate.branch = Some(branch);
        candidate.state = CampaignTaskState::Implementing;
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Binds one exact one-unit runtime identity before any worker effect begins.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for an invalid, duplicate, or incorrectly ordered identity.
    pub fn bind_run(
        &mut self,
        run_id: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.state != CampaignTaskState::Leased
            || self.run_id.is_some()
            || codingmage_contracts::RunId::new(run_id.clone()).is_err()
            || !valid_sha256(&evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidLease);
        }
        let mut candidate = self.clone();
        candidate.run_id = Some(run_id);
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Binds the one task issue returned by an idempotent publication operation.
    ///
    /// Exact replay of the already-bound number is observational and succeeds without advancing
    /// the transition sequence. A different number is an identity conflict.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for a zero number, ineligible state, or conflicting identity.
    pub fn bind_issue_number(
        &mut self,
        issue_number: u64,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.issue_number == Some(issue_number) {
            return Ok(());
        }
        if issue_number == 0
            || self.issue_number.is_some()
            || !publication_identity_state(self.state)
            || !valid_sha256(&evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.issue_number = Some(issue_number);
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Binds one draft task pull request and enters the remote-publication lifecycle.
    ///
    /// Exact replay of the already-bound number is observational. The issue must already be bound
    /// so recovery can always reconstruct the one-to-one remote mapping.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale state, missing issue identity, or conflicting replay.
    pub fn bind_pull_request_number(
        &mut self,
        pull_request_number: u64,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.pull_request_number == Some(pull_request_number) {
            return Ok(());
        }
        if pull_request_number == 0
            || self.pull_request_number.is_some()
            || self.issue_number.is_none()
            || self.state != CampaignTaskState::PublicationReady
            || !valid_sha256(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.pull_request_number = Some(pull_request_number);
        let request = task_transition(
            &candidate,
            CampaignTaskState::PullRequestOpen,
            evidence_sha256,
        );
        candidate.transition(&request)?;
        *self = candidate;
        Ok(())
    }

    /// Starts commit-bound remote CI observation for the exact task pull request.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] unless the task owns one open pull request.
    pub fn begin_ci_waiting(&mut self, evidence_sha256: String) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.state == CampaignTaskState::CiWaiting {
            return Ok(());
        }
        if self.state != CampaignTaskState::PullRequestOpen
            || self.pull_request_number.is_none()
            || !valid_sha256(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let request = task_transition(self, CampaignTaskState::CiWaiting, evidence_sha256);
        self.transition(&request)
    }

    /// Records passing CI evidence for the immutable reviewed commit.
    ///
    /// Exact replay of an evidence digest is observational. A passing observation does not enqueue
    /// integration by itself; queue mutation remains a separate coordinator decision.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale commit identity, state, or malformed evidence.
    pub fn record_ci_pass(
        &mut self,
        reviewed_commit: &str,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self
            .ci_evidence_sha256
            .iter()
            .any(|value| value == &evidence_sha256)
        {
            return Ok(());
        }
        if self.state != CampaignTaskState::CiWaiting
            || self.reviewed_commit.as_deref() != Some(reviewed_commit)
            || !valid_sha256(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.ci_evidence_sha256.push(evidence_sha256.clone());
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Records failing CI evidence and returns the task to its bounded correction lineage.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale commit identity, state, or malformed evidence.
    pub fn record_ci_failure(
        &mut self,
        reviewed_commit: &str,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.state != CampaignTaskState::CiWaiting
            || self.reviewed_commit.as_deref() != Some(reviewed_commit)
            || !valid_sha256(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        candidate.ci_evidence_sha256.push(evidence_sha256.clone());
        let request = task_transition(&candidate, CampaignTaskState::Correcting, evidence_sha256);
        candidate.transition(&request)?;
        candidate.reviewed_commit = None;
        candidate.integration_commit = None;
        candidate.completion_commit = None;
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }
}

/// Complete integrity-validatable multi-agent campaign projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamCampaignSnapshot {
    /// Closed snapshot schema version.
    pub version: u16,
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Current planning generation.
    pub generation: u64,
    /// Current authoritative campaign head.
    pub campaign_head: String,
    /// Current canonical task-source digest.
    pub task_source_sha256: String,
    /// Reconstructible scheduler state.
    pub scheduler: DurableSchedulerSnapshot,
    /// Reconstructible resource, budget, and provider-circuit state.
    pub resources: TeamResourceSnapshot,
    /// Complete task records indexed by canonical task identity.
    pub tasks: BTreeMap<String, CampaignTaskRecord>,
    /// Stable serialized integration queue of task identities.
    pub integration_queue: Vec<String>,
}

impl TeamCampaignSnapshot {
    /// Revalidates every identity and all one-to-one task mappings.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for malformed, duplicate, stale, or contradictory state.
    pub fn verify(&self) -> Result<(), TeamStateError> {
        if self.version != TEAM_STATE_VERSION
            || !valid_component(&self.campaign_id)
            || !valid_commit(&self.campaign_head)
            || !valid_sha256(&self.task_source_sha256)
            || self.scheduler.campaign_id != self.campaign_id
            || self.scheduler.generation != self.generation
            || self.tasks.len() > MAX_TASK_RECORDS
            || self.integration_queue.len() > self.tasks.len()
            || self.integration_queue.iter().collect::<BTreeSet<_>>().len()
                != self.integration_queue.len()
        {
            return Err(TeamStateError::InvalidRecord);
        }
        self.scheduler.verify()?;
        self.resources.verify()?;
        let mut pod_ids = BTreeSet::new();
        let mut lease_ids = BTreeSet::new();
        let mut run_ids = BTreeSet::new();
        let mut worktree_ids = BTreeSet::new();
        let mut branches = BTreeSet::new();
        let mut issues = BTreeSet::new();
        let mut pull_requests = BTreeSet::new();
        for (task_id, record) in &self.tasks {
            record.verify()?;
            if task_id != &record.task_id
                || record.campaign_id != self.campaign_id
                || record.generation > self.generation
                || !insert_optional_unique(&mut pod_ids, record.pod_id.as_ref())
                || !insert_optional_unique(&mut lease_ids, record.lease_id.as_ref())
                || !insert_optional_unique(&mut run_ids, record.run_id.as_ref())
                || !insert_optional_unique(&mut worktree_ids, record.worktree_id.as_ref())
                || !insert_optional_unique(&mut branches, record.branch.as_ref())
                || record
                    .issue_number
                    .is_some_and(|value| !issues.insert(value))
                || record
                    .pull_request_number
                    .is_some_and(|value| !pull_requests.insert(value))
                || record.lease_id.as_ref().is_some_and(|lease_id| {
                    let active = self.scheduler.active.get(lease_id);
                    (state_requires_active_lease(record.state) && active.is_none())
                        || active.is_some_and(|lease| {
                            lease.task_id != record.task_id
                                || record.pod_id.as_ref() != Some(&lease.pod_id)
                                || record.owned_paths != lease.owned_paths
                                || record.test_resources != lease.test_resources
                        })
                        || (record.state.is_terminal()
                            && !self.scheduler.released.contains(lease_id))
                })
            {
                return Err(TeamStateError::InvalidRecord);
            }
        }
        if self.resources.active.values().any(|reservation| {
            self.scheduler
                .active
                .get(&reservation.lease_id)
                .is_none_or(|lease| lease.task_id != reservation.task_id)
        }) {
            return Err(TeamStateError::InvalidResource);
        }
        if self
            .integration_queue
            .iter()
            .enumerate()
            .any(|(index, task_id)| {
                self.tasks.get(task_id).is_none_or(|record| {
                    if index == 0 {
                        !matches!(
                            record.state,
                            CampaignTaskState::IntegrationQueued
                                | CampaignTaskState::MergeReady
                                | CampaignTaskState::Integrating
                        )
                    } else {
                        record.state != CampaignTaskState::IntegrationQueued
                    }
                })
            })
        {
            return Err(TeamStateError::InvalidRecord);
        }
        Ok(())
    }

    /// Enqueues one publication-ready task in canonical task order.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for an unknown, duplicate, ineligible, or invalid task.
    pub fn enqueue_integration(
        &mut self,
        task_id: &str,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self
            .integration_queue
            .iter()
            .any(|queued| queued == task_id)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        let record = candidate
            .tasks
            .get_mut(task_id)
            .ok_or(TeamStateError::InvalidRecord)?;
        if !matches!(
            record.state,
            CampaignTaskState::PublicationReady
                | CampaignTaskState::PullRequestOpen
                | CampaignTaskState::CiWaiting
        ) {
            return Err(TeamStateError::InvalidTransition);
        }
        let request = task_transition(
            record,
            CampaignTaskState::IntegrationQueued,
            evidence_sha256,
        );
        record.transition(&request)?;
        candidate.integration_queue.push(task_id.to_owned());
        let fixed_prefix = usize::from(candidate.integration_queue.first().is_some_and(|first| {
            candidate.tasks.get(first).is_some_and(|record| {
                matches!(
                    record.state,
                    CampaignTaskState::MergeReady | CampaignTaskState::Integrating
                )
            })
        }));
        candidate.integration_queue[fixed_prefix..].sort();
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Marks the exact queue head eligible for a serialized integration attempt.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when `task_id` is not the integration queue head.
    pub fn mark_merge_ready(
        &mut self,
        task_id: &str,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.transition_queue_head(
            task_id,
            CampaignTaskState::IntegrationQueued,
            CampaignTaskState::MergeReady,
            evidence_sha256,
        )
    }

    /// Records the durable intent to mutate the campaign head for the exact queue head.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when the task is stale, reordered, or not merge-ready.
    pub fn begin_integration(
        &mut self,
        task_id: &str,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.transition_queue_head(
            task_id,
            CampaignTaskState::MergeReady,
            CampaignTaskState::Integrating,
            evidence_sha256,
        )
    }

    /// Binds the exact prepared integration commit before campaign-head mutation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for a non-head task, wrong state, malformed commit, or replay.
    pub fn bind_integration_commit(
        &mut self,
        task_id: &str,
        integration_commit: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.integration_queue.first().map(String::as_str) != Some(task_id)
            || !valid_commit(&integration_commit)
            || !valid_sha256(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        let record = candidate
            .tasks
            .get_mut(task_id)
            .ok_or(TeamStateError::InvalidRecord)?;
        if record.state != CampaignTaskState::Integrating
            || record.integration_commit.is_some()
            || record.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidTransition);
        }
        record.integration_commit = Some(integration_commit);
        record.next_transition = record.next_transition.saturating_add(1);
        record.last_evidence_sha256 = Some(evidence_sha256);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Completes one observed integration and releases its exact scheduler lease.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale head identity, queue reordering, or invalid commit.
    pub fn complete_integration(
        &mut self,
        task_id: &str,
        expected_head: &str,
        observed_integration_commit: &str,
        completion_commit: String,
        task_source_sha256: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.integration_queue.first().map(String::as_str) != Some(task_id)
            || self.campaign_head != expected_head
            || completion_commit == expected_head
            || !valid_commit(observed_integration_commit)
            || !valid_commit(&completion_commit)
            || !valid_sha256(&task_source_sha256)
            || task_source_sha256 == self.task_source_sha256
        {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        let record = candidate
            .tasks
            .get_mut(task_id)
            .ok_or(TeamStateError::InvalidRecord)?;
        if record.state != CampaignTaskState::Integrating {
            return Err(TeamStateError::InvalidTransition);
        }
        if record.integration_commit.as_deref() != Some(observed_integration_commit) {
            return Err(TeamStateError::InvalidTransition);
        }
        record.completion_commit = Some(completion_commit.clone());
        let request = task_transition(record, CampaignTaskState::Merged, evidence_sha256);
        record.transition_terminal(&request, TaskTerminalReason::Merged)?;
        let lease_id = record
            .lease_id
            .clone()
            .ok_or(TeamStateError::InvalidLease)?;
        let mut scheduler = DurablePodScheduler::from_snapshot(candidate.scheduler.clone())?;
        scheduler.release(&lease_id)?;
        candidate.scheduler = scheduler.snapshot().clone();
        candidate.campaign_head = completion_commit;
        candidate.task_source_sha256 = task_source_sha256;
        candidate.integration_queue.remove(0);
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    fn transition_queue_head(
        &mut self,
        task_id: &str,
        from: CampaignTaskState,
        to: CampaignTaskState,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        if self.integration_queue.first().map(String::as_str) != Some(task_id) {
            return Err(TeamStateError::InvalidTransition);
        }
        let mut candidate = self.clone();
        let record = candidate
            .tasks
            .get_mut(task_id)
            .ok_or(TeamStateError::InvalidRecord)?;
        if record.state != from {
            return Err(TeamStateError::InvalidTransition);
        }
        let request = task_transition(record, to, evidence_sha256);
        record.transition(&request)?;
        candidate.verify()?;
        *self = candidate;
        Ok(())
    }

    /// Returns the canonical digest of the verified complete projection.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid state or serialization.
    pub fn sha256(&self) -> Result<String, TeamStateError> {
        self.verify()?;
        canonical_sha256(self).map_err(|_| TeamStateError::InvalidRecord)
    }
}

/// Exact active lease retained by the campaign scheduler.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurablePodLease {
    /// Deterministic lease identity.
    pub lease_id: String,
    /// Deterministic pod identity.
    pub pod_id: String,
    /// Canonical task identity.
    pub task_id: String,
    /// Planning generation.
    pub generation: u64,
    /// Exact campaign head used as task base.
    pub source_head: String,
    /// Exact task-source digest.
    pub task_source_sha256: String,
    /// Sealed lead proposal digest.
    pub proposal_sha256: String,
    /// Exact path authority.
    pub owned_paths: Vec<PathBuf>,
    /// Exact shared-resource authority.
    pub test_resources: Vec<String>,
    /// Latest heartbeat sequence observed by the scheduler.
    pub heartbeat_sequence: u64,
    /// Latest heartbeat time observed by the scheduler.
    pub heartbeat_timestamp_ms: Option<u64>,
}

impl DurablePodLease {
    fn verify(&self) -> Result<(), TeamStateError> {
        if !valid_component(&self.lease_id)
            || !valid_component(&self.pod_id)
            || codingmage_contracts::TaskId::new(self.task_id.clone()).is_err()
            || self.generation == 0
            || !valid_commit(&self.source_head)
            || !valid_sha256(&self.task_source_sha256)
            || !valid_sha256(&self.proposal_sha256)
            || self.owned_paths.is_empty()
            || self.owned_paths.iter().any(|path| !safe_relative(path))
            || self.test_resources.iter().any(|v| !valid_component(v))
        {
            return Err(TeamStateError::InvalidLease);
        }
        Ok(())
    }
}

/// Closed deterministic reason for nonadmission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionReason {
    /// No implementation slot remains.
    Capacity,
    /// The same task already has an active lease.
    DuplicateTask,
    /// Equal, ancestor, or descendant path authority overlaps.
    PathConflict,
    /// An exclusive declared resource overlaps.
    TestResourceConflict,
}

/// Result for each proposal in stable ready-set order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionDecision {
    /// Proposal received one exact active lease.
    Admitted(DurablePodLease),
    /// Proposal was not admitted and created no lease.
    Deferred {
        /// Exact task that was not admitted.
        task_id: String,
        /// Closed nonadmission reason.
        reason: AdmissionReason,
    },
}

/// Integrity-validatable scheduler snapshot for one campaign lifetime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSchedulerSnapshot {
    /// Closed snapshot schema version.
    pub version: u16,
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Maximum active implementation pods.
    pub max_parallel_pods: u16,
    /// Current immutable planning generation.
    pub generation: u64,
    /// Next monotonic lease sequence.
    pub next_sequence: u64,
    /// Active leases indexed by lease identity.
    pub active: BTreeMap<String, DurablePodLease>,
    /// Released lease identities that cannot be reused.
    pub released: BTreeSet<String>,
    /// Ready-task age in planning generations.
    pub ready_age: BTreeMap<String, u64>,
}

impl DurableSchedulerSnapshot {
    /// Revalidates all scheduler identities, capacity, uniqueness, and conflicts.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] after mutation or contradictory ownership.
    pub fn verify(&self) -> Result<(), TeamStateError> {
        if self.version != TEAM_STATE_VERSION
            || !valid_component(&self.campaign_id)
            || !(1..=16).contains(&self.max_parallel_pods)
            || self.active.len() > usize::from(self.max_parallel_pods)
            || self.active.len() > MAX_TASK_RECORDS
            || self.active.keys().any(|id| !valid_component(id))
            || self.released.iter().any(|id| !valid_component(id))
            || self.active.keys().any(|id| self.released.contains(id))
            || self
                .ready_age
                .iter()
                .any(|(task, _)| codingmage_contracts::TaskId::new(task.clone()).is_err())
        {
            return Err(TeamStateError::InvalidScheduler);
        }
        let leases = self.active.values().collect::<Vec<_>>();
        for (index, lease) in leases.iter().enumerate() {
            lease.verify()?;
            if self.active.get(&lease.lease_id) != Some(*lease)
                || lease.generation > self.generation
                || leases[index + 1..].iter().any(|other| {
                    lease.task_id == other.task_id
                        || lease.pod_id == other.pod_id
                        || leases_conflict(lease, other)
                })
            {
                return Err(TeamStateError::InvalidScheduler);
            }
        }
        Ok(())
    }

    /// Returns a canonical digest of the verified scheduler state.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid state or serialization.
    pub fn sha256(&self) -> Result<String, TeamStateError> {
        self.verify()?;
        canonical_sha256(self).map_err(|_| TeamStateError::InvalidScheduler)
    }
}

/// Persistent campaign-lifetime scheduler reconstructed from its verified snapshot.
#[derive(Clone, Debug)]
pub struct DurablePodScheduler {
    snapshot: DurableSchedulerSnapshot,
}

impl DurablePodScheduler {
    /// Creates an empty scheduler from verified campaign authority.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid campaign authority.
    pub fn new(spec: &CampaignSpec) -> Result<Self, TeamStateError> {
        spec.verify().map_err(|_| TeamStateError::Authority)?;
        let max_parallel_pods = spec
            .multi_agent
            .as_ref()
            .map_or(1, |policy| policy.concurrency.claude_implementers);
        let snapshot = DurableSchedulerSnapshot {
            version: TEAM_STATE_VERSION,
            campaign_id: spec.campaign_id.clone(),
            max_parallel_pods,
            generation: 0,
            next_sequence: 0,
            active: BTreeMap::new(),
            released: BTreeSet::new(),
            ready_age: BTreeMap::new(),
        };
        snapshot.verify()?;
        Ok(Self { snapshot })
    }

    /// Reconstructs a scheduler from one complete verified snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when any retained identity or conflict is invalid.
    pub fn from_snapshot(snapshot: DurableSchedulerSnapshot) -> Result<Self, TeamStateError> {
        snapshot.verify()?;
        Ok(Self { snapshot })
    }

    /// Returns the current immutable scheduler projection.
    #[must_use]
    pub const fn snapshot(&self) -> &DurableSchedulerSnapshot {
        &self.snapshot
    }

    /// Starts one planning generation and updates ready-task age deterministically.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for duplicate or malformed task identities.
    pub fn begin_generation(&mut self, ready_task_ids: &[String]) -> Result<u64, TeamStateError> {
        let unique = ready_task_ids.iter().collect::<BTreeSet<_>>();
        if unique.len() != ready_task_ids.len()
            || ready_task_ids
                .iter()
                .any(|id| codingmage_contracts::TaskId::new(id.clone()).is_err())
        {
            return Err(TeamStateError::InvalidScheduler);
        }
        self.snapshot.generation = self.snapshot.generation.saturating_add(1);
        self.snapshot
            .ready_age
            .retain(|task, _| ready_task_ids.contains(task));
        for task in ready_task_ids {
            self.snapshot
                .ready_age
                .entry(task.clone())
                .and_modify(|age| *age = age.saturating_add(1))
                .or_insert(1);
        }
        self.snapshot.verify()?;
        Ok(self.snapshot.generation)
    }

    /// Orders ready tasks by descending age and stable canonical input order.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] when a task was not observed in the current generation.
    pub fn prioritize(&self, ready_task_ids: &[String]) -> Result<Vec<String>, TeamStateError> {
        if ready_task_ids
            .iter()
            .any(|task| !self.snapshot.ready_age.contains_key(task))
        {
            return Err(TeamStateError::InvalidScheduler);
        }
        let input_order = ready_task_ids
            .iter()
            .enumerate()
            .map(|(index, task)| (task, index))
            .collect::<BTreeMap<_, _>>();
        let mut prioritized = ready_task_ids.to_vec();
        prioritized.sort_by_key(|task| {
            (
                std::cmp::Reverse(self.snapshot.ready_age[task]),
                input_order[task],
            )
        });
        Ok(prioritized)
    }

    /// Admits a sealed proposal or returns one closed deferral without mutation.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for invalid authority, stale generation, or malformed proposal.
    pub fn admit(
        &mut self,
        spec: &CampaignSpec,
        generation: u64,
        source_head: &str,
        proposal: &PodProposal,
    ) -> Result<AdmissionDecision, TeamStateError> {
        spec.verify().map_err(|_| TeamStateError::Authority)?;
        proposal
            .verify(spec)
            .map_err(|_| TeamStateError::Authority)?;
        if generation == 0
            || generation != self.snapshot.generation
            || !valid_commit(source_head)
            || proposal.task_source_sha256 != spec.task_source_sha256
            || !self.snapshot.ready_age.contains_key(&proposal.task_id)
        {
            return Err(TeamStateError::StaleGeneration);
        }
        let reason = if self
            .snapshot
            .active
            .values()
            .any(|lease| lease.task_id == proposal.task_id)
        {
            Some(AdmissionReason::DuplicateTask)
        } else if self.snapshot.active.len() >= usize::from(self.snapshot.max_parallel_pods) {
            Some(AdmissionReason::Capacity)
        } else if self.snapshot.active.values().any(|lease| {
            lease.owned_paths.iter().any(|left| {
                proposal
                    .owned_paths
                    .iter()
                    .any(|right| paths_overlap(left, right))
            })
        }) {
            Some(AdmissionReason::PathConflict)
        } else if self.snapshot.active.values().any(|lease| {
            lease
                .test_resources
                .iter()
                .any(|left| proposal.test_resources.iter().any(|right| left == right))
        }) {
            Some(AdmissionReason::TestResourceConflict)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(AdmissionDecision::Deferred {
                task_id: proposal.task_id.clone(),
                reason,
            });
        }
        let sequence = self.snapshot.next_sequence;
        let pod_id = bounded_identity("pod", &spec.campaign_id, &proposal.task_id, sequence);
        let lease_id = bounded_identity("lease", &spec.campaign_id, &proposal.task_id, sequence);
        self.snapshot.next_sequence = sequence.saturating_add(1);
        let lease = DurablePodLease {
            lease_id: lease_id.clone(),
            pod_id,
            task_id: proposal.task_id.clone(),
            generation,
            source_head: source_head.to_owned(),
            task_source_sha256: proposal.task_source_sha256.clone(),
            proposal_sha256: proposal.proposal_sha256.clone(),
            owned_paths: proposal.owned_paths.clone(),
            test_resources: proposal.test_resources.clone(),
            heartbeat_sequence: 0,
            heartbeat_timestamp_ms: None,
        };
        lease.verify()?;
        self.snapshot.active.insert(lease_id, lease.clone());
        self.snapshot.verify()?;
        Ok(AdmissionDecision::Admitted(lease))
    }

    /// Records one strictly increasing scheduler heartbeat for an exact active lease.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for unknown identity or nonmonotonic observation.
    pub fn heartbeat(
        &mut self,
        lease_id: &str,
        sequence: u64,
        timestamp_ms: u64,
    ) -> Result<(), TeamStateError> {
        let lease = self
            .snapshot
            .active
            .get_mut(lease_id)
            .ok_or(TeamStateError::UnknownLease)?;
        if sequence <= lease.heartbeat_sequence
            || lease
                .heartbeat_timestamp_ms
                .is_some_and(|previous| timestamp_ms < previous)
        {
            return Err(TeamStateError::InvalidHeartbeat);
        }
        lease.heartbeat_sequence = sequence;
        lease.heartbeat_timestamp_ms = Some(timestamp_ms);
        self.snapshot.verify()
    }

    /// Releases exactly one lease and permanently records its identity as consumed.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for unknown, reused, or contradictory identity.
    pub fn release(&mut self, lease_id: &str) -> Result<DurablePodLease, TeamStateError> {
        if self.snapshot.released.contains(lease_id) {
            return Err(TeamStateError::UnknownLease);
        }
        let lease = self
            .snapshot
            .active
            .remove(lease_id)
            .ok_or(TeamStateError::UnknownLease)?;
        if !self.snapshot.released.insert(lease_id.to_owned()) {
            return Err(TeamStateError::InvalidScheduler);
        }
        self.snapshot.ready_age.remove(&lease.task_id);
        self.snapshot.verify()?;
        Ok(lease)
    }
}

/// Content-free durable team-state error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamStateError {
    /// Campaign authority is invalid or inconsistent.
    Authority,
    /// Task record is malformed or contradictory.
    InvalidRecord,
    /// State transition is illegal, stale, duplicate, or unevidenced.
    InvalidTransition,
    /// Pod lease is malformed or contradictory.
    InvalidLease,
    /// Scheduler state is malformed or contradictory.
    InvalidScheduler,
    /// Proposal belongs to another planning generation.
    StaleGeneration,
    /// Heartbeat is unknown or nonmonotonic.
    InvalidHeartbeat,
    /// Exact active lease does not exist.
    UnknownLease,
    /// Concurrent resource projection is malformed or contradictory.
    InvalidResource,
    /// Exact CPU, memory, disk, process, actor, token, or exclusive-resource capacity is exhausted.
    ResourceCapacity,
    /// A duplicate actor or exclusive-resource reservation conflicts with active work.
    ResourceConflict,
    /// Exact active resource reservation does not exist.
    UnknownResource,
    /// Provider retry circuit is malformed.
    InvalidCircuit,
    /// Provider circuit or bounded operation-attempt limit denies another attempt.
    CircuitOpen,
}

impl fmt::Display for TeamStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Authority => "codingmage.team.authority",
            Self::InvalidRecord => "codingmage.team.task_record",
            Self::InvalidTransition => "codingmage.team.transition",
            Self::InvalidLease => "codingmage.team.lease",
            Self::InvalidScheduler => "codingmage.team.scheduler",
            Self::StaleGeneration => "codingmage.team.generation",
            Self::InvalidHeartbeat => "codingmage.team.heartbeat",
            Self::UnknownLease => "codingmage.team.unknown_lease",
            Self::InvalidResource => "codingmage.team.resource",
            Self::ResourceCapacity => "codingmage.team.resource_capacity",
            Self::ResourceConflict => "codingmage.team.resource_conflict",
            Self::UnknownResource => "codingmage.team.unknown_resource",
            Self::InvalidCircuit => "codingmage.team.provider_circuit",
            Self::CircuitOpen => "codingmage.team.provider_circuit_open",
        })
    }
}

impl std::error::Error for TeamStateError {}

fn legal_transition(from: CampaignTaskState, to: CampaignTaskState) -> bool {
    use CampaignTaskState::{
        Blocked, Cancelled, CiWaiting, Correcting, Disputed, Failed, Implementing, Integrating,
        IntegrationQueued, Leased, LocalGates, MergeReady, Merged, Planned, Proposed,
        PublicationReady, PullRequestOpen, Ready, Reviewing,
    };
    matches!(
        (from, to),
        (Planned, Ready | Blocked | Cancelled)
            | (Ready, Proposed | Blocked | Cancelled)
            | (Proposed, Leased | Ready | Blocked | Cancelled)
            | (Leased, Implementing | Ready | Blocked | Failed | Cancelled)
            | (Implementing, LocalGates | Blocked | Failed | Cancelled)
            | (
                LocalGates,
                Reviewing | Correcting | Blocked | Failed | Cancelled
            )
            | (
                Reviewing,
                PublicationReady | Correcting | Blocked | Disputed | Failed | Cancelled
            )
            | (
                Correcting,
                LocalGates | Blocked | Disputed | Failed | Cancelled
            )
            | (
                PublicationReady,
                PullRequestOpen | IntegrationQueued | Blocked | Failed | Cancelled
            )
            | (
                PullRequestOpen,
                CiWaiting | IntegrationQueued | Failed | Cancelled
            )
            | (
                CiWaiting,
                IntegrationQueued | Correcting | Blocked | Failed | Cancelled
            )
            | (IntegrationQueued, MergeReady | Blocked | Failed | Cancelled)
            | (MergeReady, Integrating | Blocked | Cancelled)
            | (Integrating, Merged | Blocked | Failed | Cancelled)
    )
}

fn runtime_identity_shape(record: &CampaignTaskRecord) -> bool {
    let assigned = record.pod_id.is_some()
        && record.lease_id.is_some()
        && record.run_id.is_some()
        && record.worktree_id.is_some()
        && record.branch.is_some()
        && !record.owned_paths.is_empty();
    let unassigned = record.pod_id.is_none()
        && record.lease_id.is_none()
        && record.run_id.is_none()
        && record.worktree_id.is_none()
        && record.branch.is_none()
        && record.owned_paths.is_empty()
        && record.test_resources.is_empty();
    let leased = record.pod_id.is_some()
        && record.lease_id.is_some()
        && record.worktree_id.is_none()
        && record.branch.is_none()
        && !record.owned_paths.is_empty();
    match record.state {
        CampaignTaskState::Planned | CampaignTaskState::Ready | CampaignTaskState::Proposed => {
            unassigned
        }
        CampaignTaskState::Leased => leased,
        CampaignTaskState::Blocked
        | CampaignTaskState::Disputed
        | CampaignTaskState::Failed
        | CampaignTaskState::Cancelled => assigned || leased || unassigned,
        _ => assigned,
    }
}

const fn publication_identity_state(state: CampaignTaskState) -> bool {
    matches!(
        state,
        CampaignTaskState::Leased
            | CampaignTaskState::Implementing
            | CampaignTaskState::LocalGates
            | CampaignTaskState::Reviewing
            | CampaignTaskState::Correcting
            | CampaignTaskState::PublicationReady
            | CampaignTaskState::PullRequestOpen
            | CampaignTaskState::CiWaiting
            | CampaignTaskState::IntegrationQueued
            | CampaignTaskState::MergeReady
            | CampaignTaskState::Integrating
            | CampaignTaskState::Merged
            | CampaignTaskState::Blocked
            | CampaignTaskState::Disputed
            | CampaignTaskState::Failed
            | CampaignTaskState::Cancelled
    )
}

const fn state_requires_active_lease(state: CampaignTaskState) -> bool {
    matches!(
        state,
        CampaignTaskState::Leased
            | CampaignTaskState::Implementing
            | CampaignTaskState::LocalGates
            | CampaignTaskState::Reviewing
            | CampaignTaskState::Correcting
            | CampaignTaskState::PublicationReady
            | CampaignTaskState::PullRequestOpen
            | CampaignTaskState::CiWaiting
            | CampaignTaskState::IntegrationQueued
            | CampaignTaskState::MergeReady
            | CampaignTaskState::Integrating
    )
}

fn valid_optional_component(value: Option<&String>) -> bool {
    value.is_none_or(|value| valid_component(value))
}

fn insert_optional_unique<'a>(
    values: &mut BTreeSet<&'a String>,
    value: Option<&'a String>,
) -> bool {
    value.is_none_or(|value| values.insert(value))
}

fn valid_reason(value: &str) -> bool {
    [
        TaskTerminalReason::Merged,
        TaskTerminalReason::PrerequisiteBlocked,
        TaskTerminalReason::ProviderBlocked,
        TaskTerminalReason::ReviewBlocked,
        TaskTerminalReason::ReviewDisputed,
        TaskTerminalReason::GateFailed,
        TaskTerminalReason::ProviderFailed,
        TaskTerminalReason::ProcessCrashed,
        TaskTerminalReason::TimedOut,
        TaskTerminalReason::LimitExceeded,
        TaskTerminalReason::IntegrationConflict,
        TaskTerminalReason::StaleIdentity,
        TaskTerminalReason::PolicyDenied,
        TaskTerminalReason::OperatorCancelled,
        TaskTerminalReason::CiFailed,
        TaskTerminalReason::ExternalBlocked,
    ]
    .into_iter()
    .any(|reason| reason.code() == value)
}

fn task_transition(
    record: &CampaignTaskRecord,
    to: CampaignTaskState,
    evidence_sha256: String,
) -> CampaignTaskTransition {
    CampaignTaskTransition {
        sequence: record.next_transition,
        campaign_id: record.campaign_id.clone(),
        task_id: record.task_id.clone(),
        generation: record.generation,
        from: record.state,
        to,
        evidence_sha256,
    }
}

fn leases_conflict(left: &DurablePodLease, right: &DurablePodLease) -> bool {
    left.owned_paths.iter().any(|left_path| {
        right
            .owned_paths
            .iter()
            .any(|right_path| paths_overlap(left_path, right_path))
    }) || left.test_resources.iter().any(|left_resource| {
        right
            .test_resources
            .iter()
            .any(|right_resource| left_resource == right_resource)
    })
}

fn actor_count(counts: &BTreeMap<ActorClass, usize>, actor: ActorClass) -> usize {
    counts.get(&actor).copied().unwrap_or_default()
}

fn bounded_identity(prefix: &str, campaign_id: &str, task_id: &str, sequence: u64) -> String {
    let material = format!("{prefix}\0{campaign_id}\0{task_id}\0{sequence}");
    let digest = sha2::Sha256::digest(material.as_bytes());
    let mut encoded = String::with_capacity(24);
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    format!("{prefix}-{sequence}-{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
        CampaignPublication, PodRisk,
    };

    fn spec(mode: CampaignExecutionMode, capacity: u16) -> CampaignSpec {
        CampaignSpec {
            version: 3,
            campaign_id: "campaign-team".to_owned(),
            repository_id: "repo-team".to_owned(),
            repository_path: PathBuf::from("/tmp/repository"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
            max_parallel_pods: capacity,
            max_units: 100,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 20,
                process_invocations: 1_000,
                output_bytes: 1_000_000,
                retained_state_bytes: 1_000_000,
                execution_elapsed_ms: 1_000_000,
            },
            team_lead: provider("codex"),
            implementer: provider("claude"),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider("codex"),
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["workspace".to_owned()],
            }],
            campaign_branch: "codingmage/campaign-team".to_owned(),
            allowed_paths: vec![PathBuf::from("crates"), PathBuf::from("docs")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: mode,
                publication_mode: TaskPublicationMode::LocalOnly,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
                task_merge_strategy: TaskMergeStrategy::Squash,
                github: None,
                concurrency: CampaignConcurrency {
                    claude_implementers: capacity,
                    codex_team_leads: 1,
                    codex_reviewers: capacity,
                    test_workers: capacity,
                    github_writers: 1,
                    integration_workers: 1,
                },
                resources: TeamResourcePolicy {
                    total_cpu_units: capacity.saturating_mul(2),
                    implementation_cpu_units: 1,
                    total_memory_bytes: u64::from(capacity) * 8 * 1024 * 1024,
                    implementation_memory_bytes: 4 * 1024 * 1024,
                    total_disk_bytes: u64::from(capacity) * 8 * 1024 * 1024,
                    implementation_disk_bytes: 4 * 1024 * 1024,
                    total_processes: u32::from(capacity) * 128,
                    implementation_processes: 64,
                    ..TeamResourcePolicy::default()
                },
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 10,
                integration_validation_interval: 1,
            }),
        }
    }

    fn provider(name: &str) -> CampaignProvider {
        CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "model".to_owned(),
            effort: "high".to_owned(),
        }
    }

    #[test]
    fn remote_publication_requires_exact_github_authority() {
        let mut value = spec(CampaignExecutionMode::Parallel, 2);
        value.publication = CampaignPublication::DraftStoryPullRequests;
        let policy = value.multi_agent.as_mut().unwrap();
        policy.publication_mode = TaskPublicationMode::PerTaskDraftPullRequest;
        assert_eq!(value.verify(), Err(CampaignError::InvalidAuthority));

        value.multi_agent.as_mut().unwrap().github = Some(GitHubCampaignPolicy {
            cli_executable: PathBuf::from("/usr/bin/gh"),
            account: "fixture-user".to_owned(),
            host: "github.com".to_owned(),
            owner: "fixture-owner".to_owned(),
            repository: "fixture-repository".to_owned(),
            remote: "origin".to_owned(),
            destination_branch: "main".to_owned(),
            required_checks: vec!["workspace tests".to_owned()],
        });
        value.verify().unwrap();

        value
            .multi_agent
            .as_mut()
            .unwrap()
            .github
            .as_mut()
            .unwrap()
            .destination_branch = "unprotected".to_owned();
        assert_eq!(value.verify(), Err(CampaignError::InvalidAuthority));
    }

    fn proposal(spec: &CampaignSpec, task: &str, path: &str, resource: &str) -> PodProposal {
        PodProposal::seal(
            PodProposal {
                version: 3,
                task_id: task.to_owned(),
                task_source_sha256: spec.task_source_sha256.clone(),
                owned_paths: vec![PathBuf::from(path)],
                dependencies: Vec::new(),
                gate_tiers: vec!["focused".to_owned()],
                test_resources: vec![resource.to_owned()],
                expected_artifacts: vec![PathBuf::from(path)],
                risk: PodRisk::Routine,
                rationale_summary: "bounded".to_owned(),
                proposal_sha256: "0".repeat(64),
            },
            spec,
        )
        .unwrap()
    }

    fn publication_ready_snapshot(task_ids: &[&str]) -> TeamCampaignSnapshot {
        let spec = spec(
            CampaignExecutionMode::Parallel,
            u16::try_from(task_ids.len()).unwrap(),
        );
        let tasks = task_ids
            .iter()
            .map(|task_id| (*task_id).to_owned())
            .collect::<Vec<_>>();
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let generation = scheduler.begin_generation(&tasks).unwrap();
        let mut records = BTreeMap::new();
        for (index, task_id) in tasks.into_iter().enumerate() {
            let AdmissionDecision::Admitted(lease) = scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(
                        &spec,
                        &task_id,
                        &format!("crates/queue-{index}"),
                        &format!("queue-{index}"),
                    ),
                )
                .unwrap()
            else {
                panic!("queue fixture must be admitted");
            };
            let mut record = CampaignTaskRecord::planned(
                spec.campaign_id.clone(),
                task_id.clone(),
                spec.initial_commit.clone(),
            )
            .unwrap();
            record
                .transition(&task_transition(
                    &record,
                    CampaignTaskState::Ready,
                    "1".repeat(64),
                ))
                .unwrap();
            record.propose(generation, "2".repeat(64)).unwrap();
            record.bind_lease(&lease, "3".repeat(64)).unwrap();
            record
                .bind_run(format!("run-queue-{index}"), "4".repeat(64))
                .unwrap();
            record
                .bind_worktree(
                    format!("worktree-queue-{index}"),
                    format!("codingmage/queue-{index}"),
                    "5".repeat(64),
                )
                .unwrap();
            record.candidate_commit = Some(format!("{:040x}", index.saturating_add(1)));
            record.reviewed_commit = record.candidate_commit.clone();
            for (state, evidence) in [
                (CampaignTaskState::LocalGates, "6".repeat(64)),
                (CampaignTaskState::Reviewing, "7".repeat(64)),
                (CampaignTaskState::PublicationReady, "8".repeat(64)),
            ] {
                record
                    .transition(&task_transition(&record, state, evidence))
                    .unwrap();
            }
            records.insert(task_id, record);
        }
        let snapshot = TeamCampaignSnapshot {
            version: TEAM_STATE_SCHEMA_VERSION,
            campaign_id: spec.campaign_id.clone(),
            generation,
            campaign_head: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            scheduler: scheduler.snapshot().clone(),
            resources: TeamResourceController::new(&spec)
                .unwrap()
                .snapshot()
                .clone(),
            tasks: records,
            integration_queue: Vec::new(),
        };
        snapshot.verify().unwrap();
        snapshot
    }

    #[test]
    fn serial_policy_remains_the_compatibility_default() {
        let legacy = CampaignSpec {
            multi_agent: None,
            ..spec(CampaignExecutionMode::Serial, 1)
        };
        legacy.verify().unwrap();
        let scheduler = DurablePodScheduler::new(&legacy).unwrap();
        assert_eq!(scheduler.snapshot().max_parallel_pods, 1);
    }

    #[test]
    fn task_state_machine_rejects_skips_replay_and_cross_identity() {
        let mut record = CampaignTaskRecord::planned(
            "campaign-team".to_owned(),
            "23.2.1.1".to_owned(),
            "a".repeat(40),
        )
        .unwrap();
        let ready = CampaignTaskTransition {
            sequence: 0,
            campaign_id: record.campaign_id.clone(),
            task_id: record.task_id.clone(),
            generation: 0,
            from: CampaignTaskState::Planned,
            to: CampaignTaskState::Ready,
            evidence_sha256: "1".repeat(64),
        };
        record.transition(&ready).unwrap();
        assert_eq!(record.state, CampaignTaskState::Ready);
        assert_eq!(
            record.transition(&ready),
            Err(TeamStateError::InvalidTransition)
        );
        let mut skipped = ready;
        skipped.sequence = 1;
        skipped.from = CampaignTaskState::Ready;
        skipped.to = CampaignTaskState::Merged;
        skipped.evidence_sha256 = "2".repeat(64);
        assert_eq!(
            record.transition(&skipped),
            Err(TeamStateError::InvalidTransition)
        );
    }

    #[test]
    fn scheduler_admits_five_independent_tasks_and_survives_round_trip() {
        let spec = spec(CampaignExecutionMode::Parallel, 5);
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let tasks = (1..=5)
            .map(|index| format!("23.3.1.{index}"))
            .collect::<Vec<_>>();
        let generation = scheduler.begin_generation(&tasks).unwrap();
        for (index, task) in tasks.iter().enumerate() {
            let decision = scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(
                        &spec,
                        task,
                        &format!("crates/unit-{index}"),
                        &format!("resource-{index}"),
                    ),
                )
                .unwrap();
            assert!(matches!(decision, AdmissionDecision::Admitted(_)));
        }
        assert_eq!(scheduler.snapshot().active.len(), 5);
        let encoded = serde_json::to_vec(scheduler.snapshot()).unwrap();
        let decoded: DurableSchedulerSnapshot = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            DurablePodScheduler::from_snapshot(decoded)
                .unwrap()
                .snapshot(),
            scheduler.snapshot()
        );
    }

    #[test]
    fn five_resource_reservations_enforce_actor_physical_and_exclusive_limits() {
        let spec = spec(CampaignExecutionMode::Parallel, 5);
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let tasks = (1..=5)
            .map(|index| format!("23.3.2.{index}"))
            .collect::<Vec<_>>();
        let generation = scheduler.begin_generation(&tasks).unwrap();
        let mut leases = Vec::new();
        for (index, task) in tasks.iter().enumerate() {
            let AdmissionDecision::Admitted(lease) = scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(
                        &spec,
                        task,
                        &format!("crates/resource-{index}"),
                        &format!("fixture-{index}"),
                    ),
                )
                .unwrap()
            else {
                panic!("independent lease must be admitted");
            };
            leases.push(lease);
        }
        let mut resources = TeamResourceController::new(&spec).unwrap();
        let mut reservation_ids = Vec::new();
        for (index, lease) in leases.iter().enumerate() {
            let reservation = resources.implementation_request(lease, 1_000).unwrap();
            reservation_ids.push(reservation.reservation_id.clone());
            resources.reserve(reservation).unwrap();
            resources
                .observe(
                    &reservation_ids[index],
                    1,
                    2_000,
                    TaskUtilization {
                        provider_attempts: 1,
                        provider_tokens: 1_000,
                        process_invocations: 1,
                        output_bytes: 10,
                        retained_state_bytes: 10,
                        execution_elapsed_ms: 1_000,
                    },
                )
                .unwrap();
        }
        assert_eq!(resources.snapshot().active.len(), 5);
        assert!(resources.stale_at(31_999).is_empty());
        assert_eq!(resources.stale_at(32_000).len(), 5);

        let mut conflict = resources.implementation_request(&leases[0], 3_000).unwrap();
        conflict.reservation_id = "resource-review-conflict".to_owned();
        conflict.actor = ActorClass::Reviewer;
        assert_eq!(
            resources.reserve(conflict),
            Err(TeamStateError::ResourceConflict)
        );

        for id in &reservation_ids {
            resources.release(id).unwrap();
        }
        assert!(resources.snapshot().active.is_empty());
        assert_eq!(resources.snapshot().consumed.provider_tokens, 5_000);
        let restored =
            TeamResourceController::from_snapshot(&spec, resources.snapshot().clone()).unwrap();
        assert_eq!(restored.snapshot(), resources.snapshot());
    }

    #[test]
    fn token_boundaries_and_provider_circuit_are_closed_and_recoverable() {
        let spec = spec(CampaignExecutionMode::Parallel, 1);
        let task = "23.3.2.5".to_owned();
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let generation = scheduler
            .begin_generation(std::slice::from_ref(&task))
            .unwrap();
        let AdmissionDecision::Admitted(lease) = scheduler
            .admit(
                &spec,
                generation,
                &spec.initial_commit,
                &proposal(&spec, &task, "crates/token", "token-fixture"),
            )
            .unwrap()
        else {
            panic!("lease must be admitted");
        };
        let mut resources = TeamResourceController::new(&spec).unwrap();
        let reservation = resources.implementation_request(&lease, 1_000).unwrap();
        let reservation_id = reservation.reservation_id.clone();
        resources.reserve(reservation).unwrap();
        assert_eq!(
            resources.observe(
                &reservation_id,
                1,
                2_000,
                TaskUtilization {
                    provider_tokens: 100_001,
                    ..TaskUtilization::default()
                },
            ),
            Err(TeamStateError::ResourceCapacity)
        );
        resources
            .observe(
                &reservation_id,
                1,
                2_000,
                TaskUtilization {
                    provider_tokens: 100_000,
                    ..TaskUtilization::default()
                },
            )
            .unwrap();

        for timestamp in [3_000, 4_000, 5_000] {
            resources
                .begin_provider_attempt("claude-implementer", timestamp)
                .unwrap();
            resources
                .finish_provider_attempt("claude-implementer", timestamp, false, true)
                .unwrap();
        }
        assert_eq!(
            resources.begin_provider_attempt("claude-implementer", 6_000),
            Err(TeamStateError::CircuitOpen)
        );
        resources
            .begin_provider_attempt("claude-implementer", 65_000)
            .unwrap();
        assert_eq!(
            resources.snapshot().provider_circuits["claude-implementer"].status,
            ProviderCircuitStatus::HalfOpen
        );
        resources
            .finish_provider_attempt("claude-implementer", 65_001, true, false)
            .unwrap();
        assert_eq!(
            resources.snapshot().provider_circuits["claude-implementer"],
            ProviderCircuit::default()
        );
    }

    #[test]
    fn complete_task_mapping_binds_one_lease_and_rejects_duplicate_remote_identity() {
        let spec = spec(CampaignExecutionMode::Parallel, 2);
        let task_id = "23.2.2.1".to_owned();
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let generation = scheduler
            .begin_generation(std::slice::from_ref(&task_id))
            .unwrap();
        let lease = match scheduler
            .admit(
                &spec,
                generation,
                &spec.initial_commit,
                &proposal(&spec, &task_id, "crates/record", "record"),
            )
            .unwrap()
        {
            AdmissionDecision::Admitted(lease) => lease,
            other @ AdmissionDecision::Deferred { .. } => panic!("unexpected {other:?}"),
        };
        let mut record = CampaignTaskRecord::planned(
            spec.campaign_id.clone(),
            task_id.clone(),
            spec.initial_commit.clone(),
        )
        .unwrap();
        record
            .transition(&CampaignTaskTransition {
                sequence: 0,
                campaign_id: spec.campaign_id.clone(),
                task_id: task_id.clone(),
                generation: 0,
                from: CampaignTaskState::Planned,
                to: CampaignTaskState::Ready,
                evidence_sha256: "1".repeat(64),
            })
            .unwrap();
        record.propose(generation, "2".repeat(64)).unwrap();
        record.bind_lease(&lease, "3".repeat(64)).unwrap();
        record
            .bind_run("run-23-2-2-1".to_owned(), "4".repeat(64))
            .unwrap();
        record
            .bind_worktree(
                "wt-23-2-2-1".to_owned(),
                "codingmage/task-23-2-2-1".to_owned(),
                "5".repeat(64),
            )
            .unwrap();
        record.issue_number = Some(17);
        record.gate_evidence_sha256.push("6".repeat(64));
        let snapshot = TeamCampaignSnapshot {
            version: TEAM_STATE_SCHEMA_VERSION,
            campaign_id: spec.campaign_id.clone(),
            generation,
            campaign_head: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            scheduler: scheduler.snapshot().clone(),
            resources: TeamResourceController::new(&spec)
                .unwrap()
                .snapshot()
                .clone(),
            tasks: BTreeMap::from([(task_id, record)]),
            integration_queue: Vec::new(),
        };
        snapshot.verify().unwrap();
        assert_eq!(snapshot.sha256().unwrap().len(), 64);

        let mut duplicate = CampaignTaskRecord::planned(
            spec.campaign_id.clone(),
            "23.2.2.2".to_owned(),
            spec.initial_commit.clone(),
        )
        .unwrap();
        duplicate.issue_number = Some(17);
        let mut mutated = snapshot;
        mutated.tasks.insert(duplicate.task_id.clone(), duplicate);
        assert_eq!(mutated.verify(), Err(TeamStateError::InvalidRecord));
    }

    #[test]
    fn integration_queue_is_canonical_serial_and_head_bound() {
        let first = "23.4.4.1";
        let second = "23.4.4.2";
        let mut snapshot = publication_ready_snapshot(&[first, second]);
        snapshot
            .enqueue_integration(second, "9".repeat(64))
            .unwrap();
        snapshot.enqueue_integration(first, "a".repeat(64)).unwrap();
        assert_eq!(snapshot.integration_queue, [first, second]);

        let before = snapshot.clone();
        assert_eq!(
            snapshot.mark_merge_ready(second, "b".repeat(64)),
            Err(TeamStateError::InvalidTransition)
        );
        assert_eq!(snapshot, before);
        snapshot.mark_merge_ready(first, "c".repeat(64)).unwrap();
        snapshot.begin_integration(first, "d".repeat(64)).unwrap();
        snapshot
            .bind_integration_commit(first, "d".repeat(40), "e".repeat(64))
            .unwrap();
        let integrating = snapshot.clone();
        assert_eq!(
            snapshot.complete_integration(
                first,
                &"f".repeat(40),
                &"d".repeat(40),
                "e".repeat(40),
                "0".repeat(64),
                "f".repeat(64),
            ),
            Err(TeamStateError::InvalidTransition)
        );
        assert_eq!(snapshot, integrating);

        let previous = snapshot.campaign_head.clone();
        snapshot
            .complete_integration(
                first,
                &previous,
                &"d".repeat(40),
                "e".repeat(40),
                "c".repeat(64),
                "0".repeat(64),
            )
            .unwrap();
        assert_eq!(snapshot.campaign_head, "e".repeat(40));
        assert_eq!(snapshot.integration_queue, [second]);
        assert_eq!(snapshot.tasks[first].state, CampaignTaskState::Merged);
        assert_eq!(
            snapshot.tasks[first].terminal_reason.as_deref(),
            Some("merged")
        );
        assert_eq!(
            snapshot.tasks[first].completion_commit,
            Some("e".repeat(40))
        );
        assert_eq!(
            snapshot.tasks[first].integration_commit,
            Some("d".repeat(40))
        );
        assert!(
            snapshot
                .scheduler
                .active
                .values()
                .all(|lease| lease.task_id != first)
        );
        snapshot.verify().unwrap();
    }

    #[test]
    fn remote_mapping_and_ci_lifecycle_are_exact_idempotent_and_commit_bound() {
        let task_id = "23.4.3.1";
        let mut snapshot = publication_ready_snapshot(&[task_id]);
        let record = snapshot.tasks.get_mut(task_id).unwrap();

        record.bind_issue_number(41, "9".repeat(64)).unwrap();
        let after_issue = record.clone();
        record.bind_issue_number(41, "a".repeat(64)).unwrap();
        assert_eq!(record, &after_issue);
        assert_eq!(
            record.bind_issue_number(42, "a".repeat(64)),
            Err(TeamStateError::InvalidTransition)
        );

        record.bind_pull_request_number(57, "b".repeat(64)).unwrap();
        assert_eq!(record.state, CampaignTaskState::PullRequestOpen);
        let after_pull_request = record.clone();
        record.bind_pull_request_number(57, "c".repeat(64)).unwrap();
        assert_eq!(record, &after_pull_request);

        record.begin_ci_waiting("c".repeat(64)).unwrap();
        let reviewed = record.reviewed_commit.clone().unwrap();
        record.record_ci_pass(&reviewed, "d".repeat(64)).unwrap();
        let after_pass = record.clone();
        record.record_ci_pass(&reviewed, "d".repeat(64)).unwrap();
        assert_eq!(record, &after_pass);
        assert_eq!(
            record.record_ci_pass(&"f".repeat(40), "e".repeat(64)),
            Err(TeamStateError::InvalidTransition)
        );
        snapshot.verify().unwrap();
    }

    #[test]
    fn failing_ci_returns_only_the_matching_task_to_correction() {
        let first = "23.4.3.2";
        let second = "23.4.3.3";
        let mut snapshot = publication_ready_snapshot(&[first, second]);
        for (index, task_id) in [first, second].into_iter().enumerate() {
            let record = snapshot.tasks.get_mut(task_id).unwrap();
            record
                .bind_issue_number(100 + index as u64, format!("{:064x}", 20 + index))
                .unwrap();
            record
                .bind_pull_request_number(200 + index as u64, format!("{:064x}", 30 + index))
                .unwrap();
            record
                .begin_ci_waiting(format!("{:064x}", 40 + index))
                .unwrap();
        }
        let second_before = snapshot.tasks[second].clone();
        let reviewed = snapshot.tasks[first].reviewed_commit.clone().unwrap();
        snapshot
            .tasks
            .get_mut(first)
            .unwrap()
            .record_ci_failure(&reviewed, "f".repeat(64))
            .unwrap();

        assert_eq!(snapshot.tasks[first].state, CampaignTaskState::Correcting);
        assert!(snapshot.tasks[first].reviewed_commit.is_none());
        assert_eq!(snapshot.tasks[second], second_before);
        snapshot.verify().unwrap();
    }

    #[test]
    fn integration_enqueue_order_ignores_completion_permutation() {
        let tasks = ["23.4.4.1", "23.4.4.2", "23.4.4.3"];
        let mut forward = publication_ready_snapshot(&tasks);
        let mut reverse = forward.clone();
        for (index, task) in tasks.iter().enumerate() {
            forward
                .enqueue_integration(task, format!("{:064x}", index.saturating_add(20)))
                .unwrap();
        }
        for (index, task) in tasks.iter().rev().enumerate() {
            reverse
                .enqueue_integration(task, format!("{:064x}", index.saturating_add(30)))
                .unwrap();
        }
        assert_eq!(forward.integration_queue, reverse.integration_queue);
        assert_eq!(forward.integration_queue, tasks);
    }

    #[test]
    fn scheduler_defers_capacity_path_resource_and_duplicate_conflicts() {
        let spec = spec(CampaignExecutionMode::Parallel, 3);
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let tasks = vec![
            "23.3.2.1".to_owned(),
            "23.3.2.2".to_owned(),
            "23.3.2.3".to_owned(),
            "23.3.2.4".to_owned(),
            "23.3.2.5".to_owned(),
            "23.3.2.6".to_owned(),
        ];
        let generation = scheduler.begin_generation(&tasks).unwrap();
        let first = proposal(&spec, &tasks[0], "crates/one", "database-one");
        let first_lease = match scheduler
            .admit(&spec, generation, &spec.initial_commit, &first)
            .unwrap()
        {
            AdmissionDecision::Admitted(lease) => lease,
            other @ AdmissionDecision::Deferred { .. } => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            scheduler
                .admit(&spec, generation, &spec.initial_commit, &first)
                .unwrap(),
            AdmissionDecision::Deferred {
                task_id: tasks[0].clone(),
                reason: AdmissionReason::DuplicateTask,
            }
        );
        assert_eq!(
            scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(&spec, &tasks[1], "crates/one/child", "database-two"),
                )
                .unwrap(),
            AdmissionDecision::Deferred {
                task_id: tasks[1].clone(),
                reason: AdmissionReason::PathConflict,
            }
        );
        assert_eq!(
            scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(&spec, &tasks[2], "docs/two", "database-one"),
                )
                .unwrap(),
            AdmissionDecision::Deferred {
                task_id: tasks[2].clone(),
                reason: AdmissionReason::TestResourceConflict,
            }
        );
        for task in [&tasks[3], &tasks[4]] {
            assert!(matches!(
                scheduler
                    .admit(
                        &spec,
                        generation,
                        &spec.initial_commit,
                        &proposal(&spec, task, &format!("docs/{task}"), task),
                    )
                    .unwrap(),
                AdmissionDecision::Admitted(_)
            ));
        }
        let capacity_task = "23.3.2.6".to_owned();
        assert_eq!(
            scheduler
                .admit(
                    &spec,
                    generation,
                    &spec.initial_commit,
                    &proposal(&spec, &capacity_task, "docs/capacity", "capacity"),
                )
                .unwrap(),
            AdmissionDecision::Deferred {
                task_id: capacity_task,
                reason: AdmissionReason::Capacity,
            }
        );
        scheduler.heartbeat(&first_lease.lease_id, 1, 100).unwrap();
        assert_eq!(
            scheduler.heartbeat(&first_lease.lease_id, 1, 101),
            Err(TeamStateError::InvalidHeartbeat)
        );
        scheduler.release(&first_lease.lease_id).unwrap();
        assert_eq!(
            scheduler.release(&first_lease.lease_id),
            Err(TeamStateError::UnknownLease)
        );
    }

    #[test]
    fn policy_and_snapshot_mutations_fail_closed() {
        let mut invalid_spec = spec(CampaignExecutionMode::Parallel, 5);
        invalid_spec
            .multi_agent
            .as_mut()
            .unwrap()
            .concurrency
            .github_writers = 2;
        assert_eq!(invalid_spec.verify(), Err(CampaignError::InvalidAuthority));

        let spec = spec(CampaignExecutionMode::Parallel, 2);
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        let generation = scheduler
            .begin_generation(&["23.2.2.1".to_owned()])
            .unwrap();
        let lease = match scheduler
            .admit(
                &spec,
                generation,
                &spec.initial_commit,
                &proposal(&spec, "23.2.2.1", "crates/record", "record"),
            )
            .unwrap()
        {
            AdmissionDecision::Admitted(lease) => lease,
            other @ AdmissionDecision::Deferred { .. } => panic!("unexpected {other:?}"),
        };
        let mut snapshot = scheduler.snapshot().clone();
        snapshot.released.insert(lease.lease_id.clone());
        assert_eq!(snapshot.verify(), Err(TeamStateError::InvalidScheduler));
    }
}
