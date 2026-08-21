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
const TEAM_STATE_VERSION: u16 = 1;
const MAX_PROVIDER_TOKENS: u64 = 1_000_000_000_000;
const MAX_TASK_RECORDS: usize = 1_000_000;
const MAX_SESSIONS: usize = 1_024;

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
    /// Independent actor and worker ceilings.
    pub concurrency: CampaignConcurrency,
    /// Maximum provider tokens observed for the whole campaign.
    pub max_campaign_tokens: u64,
    /// Maximum provider tokens observed for one task.
    pub max_task_tokens: u64,
    /// Maximum correction cycles for one task.
    pub max_task_correction_cycles: u16,
    /// Maximum follow-up tasks accepted inside the original authority.
    pub max_follow_up_tasks: u16,
}

impl MultiAgentPolicy {
    /// Validates the complete multi-agent authority against the enclosing campaign.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError::InvalidAuthority`] for contradictory or unbounded policy.
    pub fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        self.concurrency.verify(spec)?;
        if self.version != TEAM_POLICY_VERSION
            || self.max_campaign_tokens == 0
            || self.max_campaign_tokens > MAX_PROVIDER_TOKENS
            || self.max_task_tokens == 0
            || self.max_task_tokens > self.max_campaign_tokens
            || self.max_task_correction_cycles == 0
            || self.max_task_correction_cycles > 100
            || self.max_follow_up_tasks > 10_000
            || (self.execution_mode == CampaignExecutionMode::Serial
                && self.concurrency.claude_implementers != 1)
            || (self.publication_mode != TaskPublicationMode::LocalOnly
                && matches!(spec.publication, super::CampaignPublication::LocalOnly))
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
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
    /// Optional task issue number.
    pub issue_number: Option<u64>,
    /// Optional task pull-request number.
    pub pull_request_number: Option<u64>,
    /// Claude implementation session lineage.
    pub implementation_session: Option<String>,
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
            worktree_id: None,
            branch: None,
            base_commit,
            candidate_commit: None,
            reviewed_commit: None,
            issue_number: None,
            pull_request_number: None,
            implementation_session: None,
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
            || self.issue_number == Some(0)
            || self.pull_request_number == Some(0)
            || self.review_sessions.len() > MAX_SESSIONS
            || self.review_sessions.iter().any(|v| !valid_component(v))
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

    /// Applies one ordered legal transition.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for wrong identity, sequence, state, evidence, or transition.
    pub fn transition(
        &mut self,
        transition: &CampaignTaskTransition,
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
        self.state = transition.to;
        self.next_transition = self.next_transition.saturating_add(1);
        self.last_evidence_sha256 = Some(transition.evidence_sha256.clone());
        if transition.to.is_terminal() {
            self.terminal_reason = Some(
                match transition.to {
                    CampaignTaskState::Merged => "merged",
                    CampaignTaskState::Blocked => "blocked",
                    CampaignTaskState::Disputed => "disputed",
                    CampaignTaskState::Failed => "failed",
                    CampaignTaskState::Cancelled => "cancelled",
                    _ => return Err(TeamStateError::InvalidTransition),
                }
                .to_owned(),
            );
        }
        self.verify()
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

    /// Binds one exact scheduler lease to one coordinator-created worktree and branch.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateError`] for stale generation, mismatched identity, or invalid transition.
    pub fn bind_lease(
        &mut self,
        lease: &DurablePodLease,
        worktree_id: String,
        branch: String,
        evidence_sha256: String,
    ) -> Result<(), TeamStateError> {
        self.verify()?;
        lease.verify()?;
        if self.state != CampaignTaskState::Proposed
            || lease.task_id != self.task_id
            || lease.generation != self.generation
            || lease.source_head != self.base_commit
            || !valid_component(&worktree_id)
            || !valid_branch(&branch)
            || !valid_sha256(&evidence_sha256)
            || self.last_evidence_sha256.as_ref() == Some(&evidence_sha256)
        {
            return Err(TeamStateError::InvalidLease);
        }
        let mut candidate = self.clone();
        candidate.pod_id = Some(lease.pod_id.clone());
        candidate.lease_id = Some(lease.lease_id.clone());
        candidate.worktree_id = Some(worktree_id);
        candidate.branch = Some(branch);
        candidate.owned_paths.clone_from(&lease.owned_paths);
        candidate.test_resources.clone_from(&lease.test_resources);
        candidate.state = CampaignTaskState::Leased;
        candidate.next_transition = candidate.next_transition.saturating_add(1);
        candidate.last_evidence_sha256 = Some(evidence_sha256);
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
        let mut pod_ids = BTreeSet::new();
        let mut lease_ids = BTreeSet::new();
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
        if self.integration_queue.iter().any(|task_id| {
            self.tasks
                .get(task_id)
                .is_none_or(|record| record.state != CampaignTaskState::IntegrationQueued)
        }) {
            return Err(TeamStateError::InvalidRecord);
        }
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
            | (Leased, Implementing | Ready | Failed | Cancelled)
            | (Implementing, LocalGates | Blocked | Failed | Cancelled)
            | (LocalGates, Reviewing | Correcting | Failed | Cancelled)
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
                PullRequestOpen | IntegrationQueued | Cancelled
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
        && record.worktree_id.is_some()
        && record.branch.is_some()
        && !record.owned_paths.is_empty();
    let unassigned = record.pod_id.is_none()
        && record.lease_id.is_none()
        && record.worktree_id.is_none()
        && record.branch.is_none()
        && record.owned_paths.is_empty()
        && record.test_resources.is_empty();
    match record.state {
        CampaignTaskState::Planned | CampaignTaskState::Ready | CampaignTaskState::Proposed => {
            unassigned
        }
        CampaignTaskState::Blocked
        | CampaignTaskState::Disputed
        | CampaignTaskState::Failed
        | CampaignTaskState::Cancelled => assigned || unassigned,
        _ => assigned,
    }
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
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
        })
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
                concurrency: CampaignConcurrency {
                    claude_implementers: capacity,
                    codex_team_leads: 1,
                    codex_reviewers: capacity,
                    test_workers: capacity,
                    github_writers: 1,
                    integration_workers: 1,
                },
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 10,
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
        record
            .bind_lease(
                &lease,
                "wt-23-2-2-1".to_owned(),
                "codingmage/task-23-2-2-1".to_owned(),
                "3".repeat(64),
            )
            .unwrap();
        record.issue_number = Some(17);
        record.gate_evidence_sha256.push("4".repeat(64));
        let snapshot = TeamCampaignSnapshot {
            version: 1,
            campaign_id: spec.campaign_id.clone(),
            generation,
            campaign_head: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            scheduler: scheduler.snapshot().clone(),
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
