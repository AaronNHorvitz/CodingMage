//! Exact, restart-safe task publication and CI routing.

use std::path::PathBuf;

use codingmage_campaign::{
    CampaignSpec, CampaignTaskState, TaskPublicationMode, TeamCampaignSnapshot,
};
use codingmage_plan::TaskPlan;

use crate::RuntimeError;

/// Immutable authority for creating or updating one assigned task issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskIssueRequest {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact canonical task identity.
    pub task_id: String,
    /// Bounded canonical task title.
    pub title: String,
    /// Exact assigned pod identity.
    pub pod_id: String,
    /// Existing issue identity, when already bound.
    pub issue_number: Option<u64>,
    /// Existing pull-request identity, when already bound.
    pub pull_request_number: Option<u64>,
    /// Exact task branch.
    pub task_branch: String,
    /// Current closed task-state identifier.
    pub state: String,
    /// Exact campaign integration branch.
    pub campaign_branch: String,
    /// Latest coordinator-created candidate, when one exists.
    pub candidate_commit: Option<String>,
    /// Latest automated review result, when one exists.
    pub review_result: Option<String>,
    /// Exact repository-relative write authority.
    pub owned_paths: Vec<PathBuf>,
    /// Ordered task dependencies.
    pub dependencies: Vec<String>,
    /// Required deterministic gate tiers.
    pub gate_tiers: Vec<String>,
    /// Deterministic gate evidence digests observed so far.
    pub gate_evidence_sha256: Vec<String>,
    /// Independent review evidence digests observed so far.
    pub review_evidence_sha256: Vec<String>,
    /// Stable content-free terminal reason, when one exists.
    pub blocker: Option<String>,
}

impl TaskIssueRequest {
    fn verify(
        &self,
        spec: &CampaignSpec,
        campaign_branch: &str,
    ) -> Result<(), TeamPublicationError> {
        if self.campaign_id != spec.campaign_id
            || self.task_id.is_empty()
            || self.title.is_empty()
            || self.title.len() > 512
            || self.title.contains(['\0', '\n', '\r'])
            || !valid_component(&self.pod_id)
            || self.issue_number == Some(0)
            || self.pull_request_number == Some(0)
            || self.pull_request_number.is_some() && self.issue_number.is_none()
            || self.task_branch.is_empty()
            || !valid_component(&self.state)
            || self.task_branch == self.campaign_branch
            || self.campaign_branch != campaign_branch
            || !is_campaign_branch(&spec.campaign_branch, campaign_branch)
            || spec
                .protected_branches
                .iter()
                .any(|branch| branch == campaign_branch)
            || self
                .candidate_commit
                .as_ref()
                .is_some_and(|value| !valid_commit(value))
            || self
                .review_result
                .as_ref()
                .is_some_and(|value| !valid_component(value))
            || self
                .blocker
                .as_ref()
                .is_some_and(|value| !valid_component(value))
            || self.owned_paths.is_empty()
            || self.gate_tiers.is_empty()
            || self
                .gate_evidence_sha256
                .iter()
                .chain(&self.review_evidence_sha256)
                .any(|value| !valid_sha256(value))
        {
            return Err(TeamPublicationError::Authority);
        }
        Ok(())
    }
}

/// Immutable authority supplied to one task publication adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskPublicationRequest {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact canonical task identity.
    pub task_id: String,
    /// Bounded canonical task title.
    pub title: String,
    /// Exact assigned pod identity.
    pub pod_id: String,
    /// Existing issue identity, when already bound.
    pub issue_number: Option<u64>,
    /// Existing pull-request identity, when already bound.
    pub pull_request_number: Option<u64>,
    /// Exact task branch.
    pub task_branch: String,
    /// Current closed task-state identifier.
    pub state: String,
    /// Exact campaign integration branch.
    pub campaign_branch: String,
    /// Immutable reviewed task commit.
    pub reviewed_commit: String,
    /// Exact repository-relative write authority.
    pub owned_paths: Vec<PathBuf>,
    /// Ordered task dependencies.
    pub dependencies: Vec<String>,
    /// Required deterministic gate tiers.
    pub gate_tiers: Vec<String>,
    /// Deterministic gate evidence digests.
    pub gate_evidence_sha256: Vec<String>,
    /// Independent review evidence digests.
    pub review_evidence_sha256: Vec<String>,
    /// Ordered Claude correction-session identities.
    pub correction_sessions: Vec<String>,
}

impl TaskPublicationRequest {
    fn verify(
        &self,
        spec: &CampaignSpec,
        campaign_branch: &str,
    ) -> Result<(), TeamPublicationError> {
        if self.campaign_id != spec.campaign_id
            || self.task_id.is_empty()
            || self.title.is_empty()
            || self.title.len() > 512
            || self.title.contains(['\0', '\n', '\r'])
            || self.pod_id.is_empty()
            || self.issue_number == Some(0)
            || self.pull_request_number == Some(0)
            || self.pull_request_number.is_some() && self.issue_number.is_none()
            || self.task_branch.is_empty()
            || self.state.is_empty()
            || self.task_branch == self.campaign_branch
            || self.campaign_branch != campaign_branch
            || !is_campaign_branch(&spec.campaign_branch, campaign_branch)
            || spec
                .protected_branches
                .iter()
                .any(|branch| branch == campaign_branch)
            || !valid_commit(&self.reviewed_commit)
            || self.owned_paths.is_empty()
            || self.gate_tiers.is_empty()
            || self.gate_evidence_sha256.is_empty()
            || self.review_evidence_sha256.is_empty()
            || self
                .gate_evidence_sha256
                .iter()
                .chain(&self.review_evidence_sha256)
                .any(|value| !valid_sha256(value))
        {
            return Err(TeamPublicationError::Authority);
        }
        Ok(())
    }

    pub(crate) fn issue_request(&self) -> TaskIssueRequest {
        TaskIssueRequest {
            campaign_id: self.campaign_id.clone(),
            task_id: self.task_id.clone(),
            title: self.title.clone(),
            pod_id: self.pod_id.clone(),
            issue_number: self.issue_number,
            pull_request_number: self.pull_request_number,
            task_branch: self.task_branch.clone(),
            state: self.state.clone(),
            campaign_branch: self.campaign_branch.clone(),
            candidate_commit: Some(self.reviewed_commit.clone()),
            review_result: Some("pass".to_owned()),
            owned_paths: self.owned_paths.clone(),
            dependencies: self.dependencies.clone(),
            gate_tiers: self.gate_tiers.clone(),
            gate_evidence_sha256: self.gate_evidence_sha256.clone(),
            review_evidence_sha256: self.review_evidence_sha256.clone(),
            blocker: None,
        }
    }
}

/// One exact remote issue observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueObservation {
    /// Exact issue number.
    pub number: u64,
    /// Integrity digest of the reconciled remote observation.
    pub evidence_sha256: String,
}

/// One exact remote branch observation after an idempotent push.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushObservation {
    /// Exact task branch.
    pub branch: String,
    /// Exact commit observed at the remote branch.
    pub commit: String,
    /// Integrity digest of the reconciled remote observation.
    pub evidence_sha256: String,
}

/// Immutable authority for publishing one exact coordinator-owned campaign head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignBranchPublicationRequest {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Configured campaign-branch namespace.
    pub campaign_branch_prefix: String,
    /// Exact collision-resistant campaign branch.
    pub campaign_branch: String,
    /// Immutable campaign starting commit.
    pub initial_commit: String,
    /// Exact locally verified campaign head to publish.
    pub campaign_commit: String,
    /// Whether this publication advances accepted task work rather than creating the base branch.
    pub includes_task_integration: bool,
}

impl CampaignBranchPublicationRequest {
    fn verify(&self, spec: &CampaignSpec) -> Result<(), TeamPublicationError> {
        if self.campaign_id != spec.campaign_id
            || self.campaign_branch_prefix != spec.campaign_branch
            || !is_campaign_branch(&self.campaign_branch_prefix, &self.campaign_branch)
            || self.initial_commit != spec.initial_commit
            || !valid_commit(&self.initial_commit)
            || !valid_commit(&self.campaign_commit)
            || (!self.includes_task_integration && self.campaign_commit != self.initial_commit)
            || spec
                .protected_branches
                .iter()
                .any(|branch| branch == &self.campaign_branch)
        {
            return Err(TeamPublicationError::Authority);
        }
        Ok(())
    }
}

/// Exact remote campaign-branch observation after an idempotent non-force push.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignBranchObservation {
    /// Exact remote campaign branch.
    pub branch: String,
    /// Exact coordinator-verified commit observed at the remote branch.
    pub commit: String,
    /// Integrity digest of the reconciled observation.
    pub evidence_sha256: String,
}

/// Exact remote issue and task-PR disposition after local campaign integration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCompletionObservation {
    /// Exact issue updated and closed by the coordinator.
    pub issue_number: u64,
    /// Exact pull request reconciled by the coordinator.
    pub pull_request_number: u64,
    /// True when GitHub observed the task PR as merged rather than closed-as-integrated.
    pub pull_request_merged: bool,
    /// Integrity digest of the complete reconciled observation.
    pub evidence_sha256: String,
}

/// One exact draft task pull-request observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequestObservation {
    /// Exact pull-request number.
    pub number: u64,
    /// Exact task pull-request base.
    pub base_branch: String,
    /// Exact task pull-request head.
    pub head_branch: String,
    /// Exact reviewed commit observed remotely.
    pub head_commit: String,
    /// Integrity digest of the reconciled remote observation.
    pub evidence_sha256: String,
}

/// Commit-bound aggregate CI observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CiObservation {
    /// At least one required check is not terminal.
    Pending,
    /// Every required check passed for the exact reviewed commit.
    Passed {
        /// Exact checked commit.
        commit: String,
        /// Integrity digest of the complete check observation.
        evidence_sha256: String,
    },
    /// At least one required check failed for the exact reviewed commit.
    Failed {
        /// Exact checked commit.
        commit: String,
        /// Integrity digest of the complete check observation.
        evidence_sha256: String,
    },
}

/// Narrow side-effect port owned by the deterministic coordinator.
pub trait TeamPublicationPort {
    /// Publishes one exact verified campaign head without force-pushing or rewriting remote state.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_campaign_branch(
        &mut self,
        request: &CampaignBranchPublicationRequest,
    ) -> Result<CampaignBranchObservation, TeamPublicationError>;

    /// Creates or updates the one exact task issue and reconciles uncertain completion.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_issue(
        &mut self,
        request: &TaskIssueRequest,
    ) -> Result<IssueObservation, TeamPublicationError>;

    /// Pushes only the exact reviewed commit to the exact task branch and reobserves its head.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_reviewed_branch(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<PushObservation, TeamPublicationError>;

    /// Creates or updates the one exact draft task pull request.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_pull_request(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<PullRequestObservation, TeamPublicationError>;

    /// Reads configured checks for the exact pull request and reviewed commit without mutation.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or response failure.
    fn observe_ci(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<CiObservation, TeamPublicationError>;

    /// Reconciles the task issue and PR after exact local campaign integration.
    ///
    /// A squash-integrated task PR may be closed-as-integrated because its reviewed task commit is
    /// intentionally not an ancestor of the campaign branch. Fast-forward task PRs are expected to
    /// appear merged naturally. Both outcomes retain explicit coordinator-owned evidence.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_task_completion(
        &mut self,
        request: &TaskPublicationRequest,
        integration_commit: &str,
        completion_commit: &str,
    ) -> Result<TaskCompletionObservation, TeamPublicationError>;
}

/// Terminal result from one restart-safe publication step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamPublicationOutcome {
    /// Remote checks remain pending; no integration authority exists.
    WaitingForCi,
    /// Exact reviewed commit has passed remote checks and may enter the integration queue.
    ReadyForIntegration,
    /// Failing remote checks returned only this task to its correction lineage.
    CorrectionRequired,
}

/// Stable content-free publication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamPublicationError {
    /// Campaign or task publication authority was malformed or stale.
    Authority,
    /// A remote object identity contradicted durable local state.
    Identity,
    /// A remote effect remains uncertain after reconciliation.
    Uncertain,
    /// Durable state could not be advanced exactly.
    State,
}

impl TeamPublicationError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Authority => "codingmage.team.publication.authority",
            Self::Identity => "codingmage.team.publication.identity",
            Self::Uncertain => "codingmage.team.publication.uncertain",
            Self::State => "codingmage.team.publication.state",
        }
    }
}

impl std::fmt::Display for TeamPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for TeamPublicationError {}

/// Builds exact authority for one assigned task issue before a reviewed candidate exists.
///
/// # Errors
///
/// Returns a content-free authority or state failure unless the task has a durable pod and branch.
pub fn task_issue_publication_request(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
) -> Result<TaskIssueRequest, TeamPublicationError> {
    spec.verify().map_err(|_| TeamPublicationError::Authority)?;
    snapshot.verify().map_err(|_| TeamPublicationError::State)?;
    let policy = spec
        .multi_agent
        .as_ref()
        .ok_or(TeamPublicationError::Authority)?;
    if policy.publication_mode != TaskPublicationMode::PerTaskDraftPullRequest
        || policy.github.is_none()
        || snapshot.campaign_id != spec.campaign_id
    {
        return Err(TeamPublicationError::Authority);
    }
    let record = snapshot
        .tasks
        .get(task_id)
        .ok_or(TeamPublicationError::State)?;
    let selected = plan
        .select_exact(task_id)
        .map_err(|_| TeamPublicationError::Authority)?;
    let request = TaskIssueRequest {
        campaign_id: spec.campaign_id.clone(),
        task_id: task_id.to_owned(),
        title: selected.item.title.clone(),
        pod_id: record.pod_id.clone().ok_or(TeamPublicationError::State)?,
        issue_number: record.issue_number,
        pull_request_number: record.pull_request_number,
        task_branch: record.branch.clone().ok_or(TeamPublicationError::State)?,
        state: task_state_code(record.state).to_owned(),
        campaign_branch: campaign_branch.to_owned(),
        candidate_commit: record.candidate_commit.clone(),
        review_result: record.reviewed_commit.as_ref().map(|_| "pass".to_owned()),
        owned_paths: record.owned_paths.clone(),
        dependencies: selected.item.dependencies.clone(),
        gate_tiers: spec
            .gate_tiers
            .iter()
            .map(|tier| tier.name.clone())
            .collect(),
        gate_evidence_sha256: record.gate_evidence_sha256.clone(),
        review_evidence_sha256: record.review_evidence_sha256.clone(),
        blocker: record.terminal_reason.clone(),
    };
    request.verify(spec, campaign_branch)?;
    Ok(request)
}

/// Creates or updates one assigned task issue and binds its exact identity durably.
///
/// # Errors
///
/// Returns a content-free authority, identity, uncertainty, or state failure. Uncertain writes are
/// reconciled by the adapter's exact campaign/task marker before a caller may retry.
pub fn synchronize_task_issue<T, P>(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    port: &mut T,
    mut persist: P,
) -> Result<IssueObservation, TeamPublicationError>
where
    T: TeamPublicationPort,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    let request = task_issue_publication_request(spec, campaign_branch, plan, snapshot, task_id)?;
    let observation = port.ensure_issue(&request)?;
    if observation.number == 0
        || !valid_sha256(&observation.evidence_sha256)
        || request
            .issue_number
            .is_some_and(|expected| expected != observation.number)
    {
        return Err(TeamPublicationError::Identity);
    }
    if request.issue_number.is_none() {
        snapshot
            .tasks
            .get_mut(task_id)
            .ok_or(TeamPublicationError::State)?
            .bind_issue_number(observation.number, observation.evidence_sha256.clone())
            .map_err(|_| TeamPublicationError::State)?;
        persist(snapshot).map_err(|_| TeamPublicationError::State)?;
    }
    Ok(observation)
}

/// Reconciles one task's issue, branch, pull request, and CI state without duplicate effects.
///
/// # Errors
///
/// Returns a content-free authority, identity, uncertainty, or durable-state failure. The caller
/// must retain the snapshot and retry only through this same reconciliation operation.
#[allow(clippy::too_many_lines)]
pub fn synchronize_task_publication<T, P>(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    port: &mut T,
    mut persist: P,
) -> Result<TeamPublicationOutcome, TeamPublicationError>
where
    T: TeamPublicationPort,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    spec.verify().map_err(|_| TeamPublicationError::Authority)?;
    snapshot.verify().map_err(|_| TeamPublicationError::State)?;
    let policy = spec
        .multi_agent
        .as_ref()
        .ok_or(TeamPublicationError::Authority)?;
    if policy.publication_mode != TaskPublicationMode::PerTaskDraftPullRequest
        || policy.github.is_none()
        || snapshot.campaign_id != spec.campaign_id
    {
        return Err(TeamPublicationError::Authority);
    }
    if snapshot.tasks.get(task_id).is_some_and(|record| {
        record.state == CampaignTaskState::CiWaiting && !record.ci_evidence_sha256.is_empty()
    }) {
        return Ok(TeamPublicationOutcome::ReadyForIntegration);
    }

    if !is_campaign_branch(&spec.campaign_branch, campaign_branch) {
        return Err(TeamPublicationError::Authority);
    }
    let mut request = publication_request(spec, campaign_branch, plan, snapshot, task_id)?;
    let issue = port.ensure_issue(&request.issue_request())?;
    if issue.number == 0
        || !valid_sha256(&issue.evidence_sha256)
        || request
            .issue_number
            .is_some_and(|expected| expected != issue.number)
    {
        return Err(TeamPublicationError::Identity);
    }
    if request.issue_number.is_none() {
        snapshot
            .tasks
            .get_mut(task_id)
            .ok_or(TeamPublicationError::State)?
            .bind_issue_number(issue.number, issue.evidence_sha256)
            .map_err(|_| TeamPublicationError::State)?;
        persist(snapshot).map_err(|_| TeamPublicationError::State)?;
        request = publication_request(spec, campaign_branch, plan, snapshot, task_id)?;
    }

    let pushed = port.ensure_reviewed_branch(&request)?;
    if pushed.branch != request.task_branch
        || pushed.commit != request.reviewed_commit
        || !valid_sha256(&pushed.evidence_sha256)
    {
        return Err(TeamPublicationError::Identity);
    }
    let pull_request = port.ensure_pull_request(&request)?;
    if pull_request.number == 0
        || pull_request.base_branch != request.campaign_branch
        || pull_request.head_branch != request.task_branch
        || pull_request.head_commit != request.reviewed_commit
        || !valid_sha256(&pull_request.evidence_sha256)
        || request
            .pull_request_number
            .is_some_and(|expected| expected != pull_request.number)
    {
        return Err(TeamPublicationError::Identity);
    }
    if request.pull_request_number.is_none() {
        snapshot
            .tasks
            .get_mut(task_id)
            .ok_or(TeamPublicationError::State)?
            .bind_pull_request_number(pull_request.number, pull_request.evidence_sha256)
            .map_err(|_| TeamPublicationError::State)?;
        persist(snapshot).map_err(|_| TeamPublicationError::State)?;
        request = publication_request(spec, campaign_branch, plan, snapshot, task_id)?;
        let updated_issue = port.ensure_issue(&request.issue_request())?;
        if updated_issue.number != issue.number || !valid_sha256(&updated_issue.evidence_sha256) {
            return Err(TeamPublicationError::Identity);
        }
    }
    if snapshot.tasks[task_id].state == CampaignTaskState::PullRequestOpen {
        let evidence = digest(&format!(
            "{}\0{}\0ci-waiting",
            request.task_id, request.reviewed_commit
        ));
        snapshot
            .tasks
            .get_mut(task_id)
            .ok_or(TeamPublicationError::State)?
            .begin_ci_waiting(evidence)
            .map_err(|_| TeamPublicationError::State)?;
        persist(snapshot).map_err(|_| TeamPublicationError::State)?;
    }
    match port.observe_ci(&request)? {
        CiObservation::Pending => Ok(TeamPublicationOutcome::WaitingForCi),
        CiObservation::Passed {
            commit,
            evidence_sha256,
        } => {
            validate_ci(&request, &commit, &evidence_sha256)?;
            snapshot
                .tasks
                .get_mut(task_id)
                .ok_or(TeamPublicationError::State)?
                .record_ci_pass(&commit, evidence_sha256)
                .map_err(|_| TeamPublicationError::State)?;
            persist(snapshot).map_err(|_| TeamPublicationError::State)?;
            Ok(TeamPublicationOutcome::ReadyForIntegration)
        }
        CiObservation::Failed {
            commit,
            evidence_sha256,
        } => {
            validate_ci(&request, &commit, &evidence_sha256)?;
            snapshot
                .tasks
                .get_mut(task_id)
                .ok_or(TeamPublicationError::State)?
                .record_ci_failure(&commit, evidence_sha256)
                .map_err(|_| TeamPublicationError::State)?;
            persist(snapshot).map_err(|_| TeamPublicationError::State)?;
            Ok(TeamPublicationOutcome::CorrectionRequired)
        }
    }
}

/// Builds exact authority for creating or advancing the remotely visible campaign branch.
///
/// # Errors
///
/// Returns a content-free authority failure for a stale branch or campaign identity.
pub fn campaign_branch_publication_request(
    spec: &CampaignSpec,
    campaign_branch: &str,
    snapshot: &TeamCampaignSnapshot,
) -> Result<CampaignBranchPublicationRequest, TeamPublicationError> {
    let request = CampaignBranchPublicationRequest {
        campaign_id: spec.campaign_id.clone(),
        campaign_branch_prefix: spec.campaign_branch.clone(),
        campaign_branch: campaign_branch.to_owned(),
        initial_commit: spec.initial_commit.clone(),
        campaign_commit: snapshot.campaign_head.clone(),
        includes_task_integration: snapshot.campaign_head != spec.initial_commit,
    };
    request.verify(spec)?;
    Ok(request)
}

/// Reconciles one exact campaign branch and validates the complete adapter observation.
///
/// # Errors
///
/// Returns a content-free identity or authority failure without accepting a mismatched response.
pub fn synchronize_campaign_branch<T: TeamPublicationPort>(
    spec: &CampaignSpec,
    campaign_branch: &str,
    snapshot: &TeamCampaignSnapshot,
    port: &mut T,
) -> Result<CampaignBranchObservation, TeamPublicationError> {
    let request = campaign_branch_publication_request(spec, campaign_branch, snapshot)?;
    let observation = port.ensure_campaign_branch(&request)?;
    if observation.branch != request.campaign_branch
        || observation.commit != request.campaign_commit
        || !valid_sha256(&observation.evidence_sha256)
    {
        return Err(TeamPublicationError::Identity);
    }
    Ok(observation)
}

/// Builds the immutable remote reconciliation request for one locally merged task.
///
/// # Errors
///
/// Returns a content-free state or authority failure unless every task identity is complete.
pub fn task_completion_publication_request(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
) -> Result<TaskPublicationRequest, TeamPublicationError> {
    let record = snapshot
        .tasks
        .get(task_id)
        .ok_or(TeamPublicationError::State)?;
    if record.state != CampaignTaskState::Merged
        || record.integration_commit.is_none()
        || record.completion_commit.is_none()
    {
        return Err(TeamPublicationError::State);
    }
    build_publication_request(spec, campaign_branch, plan, snapshot, task_id)
}

/// Reconciles one merged task's remote issue and pull request idempotently.
///
/// # Errors
///
/// Returns a content-free state, authority, or identity failure for incomplete local evidence or
/// a mismatched remote observation.
pub fn synchronize_task_completion<T: TeamPublicationPort>(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
    port: &mut T,
) -> Result<TaskCompletionObservation, TeamPublicationError> {
    let request =
        task_completion_publication_request(spec, campaign_branch, plan, snapshot, task_id)?;
    let record = snapshot
        .tasks
        .get(task_id)
        .ok_or(TeamPublicationError::State)?;
    let observation = port.ensure_task_completion(
        &request,
        record
            .integration_commit
            .as_deref()
            .ok_or(TeamPublicationError::State)?,
        record
            .completion_commit
            .as_deref()
            .ok_or(TeamPublicationError::State)?,
    )?;
    if observation.issue_number != request.issue_number.ok_or(TeamPublicationError::State)?
        || observation.pull_request_number
            != request
                .pull_request_number
                .ok_or(TeamPublicationError::State)?
        || !valid_sha256(&observation.evidence_sha256)
    {
        return Err(TeamPublicationError::Identity);
    }
    Ok(observation)
}

fn publication_request(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
) -> Result<TaskPublicationRequest, TeamPublicationError> {
    let record = snapshot
        .tasks
        .get(task_id)
        .ok_or(TeamPublicationError::State)?;
    if !matches!(
        record.state,
        CampaignTaskState::PublicationReady
            | CampaignTaskState::PullRequestOpen
            | CampaignTaskState::CiWaiting
    ) {
        return Err(TeamPublicationError::State);
    }
    build_publication_request(spec, campaign_branch, plan, snapshot, task_id)
}

fn build_publication_request(
    spec: &CampaignSpec,
    campaign_branch: &str,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
) -> Result<TaskPublicationRequest, TeamPublicationError> {
    let record = snapshot
        .tasks
        .get(task_id)
        .ok_or(TeamPublicationError::State)?;
    let selected = plan
        .select_exact(task_id)
        .map_err(|_| TeamPublicationError::Authority)?;
    let request = TaskPublicationRequest {
        campaign_id: spec.campaign_id.clone(),
        task_id: task_id.to_owned(),
        title: selected.item.title.clone(),
        pod_id: record.pod_id.clone().ok_or(TeamPublicationError::State)?,
        issue_number: record.issue_number,
        pull_request_number: record.pull_request_number,
        task_branch: record.branch.clone().ok_or(TeamPublicationError::State)?,
        state: task_state_code(record.state).to_owned(),
        campaign_branch: campaign_branch.to_owned(),
        reviewed_commit: record
            .reviewed_commit
            .clone()
            .ok_or(TeamPublicationError::State)?,
        owned_paths: record.owned_paths.clone(),
        dependencies: selected.item.dependencies.clone(),
        gate_tiers: spec
            .gate_tiers
            .iter()
            .map(|tier| tier.name.clone())
            .collect(),
        gate_evidence_sha256: record.gate_evidence_sha256.clone(),
        review_evidence_sha256: record.review_evidence_sha256.clone(),
        correction_sessions: record.correction_sessions.clone(),
    };
    request.verify(spec, campaign_branch)?;
    Ok(request)
}

fn validate_ci(
    request: &TaskPublicationRequest,
    commit: &str,
    evidence_sha256: &str,
) -> Result<(), TeamPublicationError> {
    if commit != request.reviewed_commit || !valid_sha256(evidence_sha256) {
        return Err(TeamPublicationError::Identity);
    }
    Ok(())
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_campaign_branch(configured_prefix: &str, actual: &str) -> bool {
    actual == configured_prefix
        || actual
            .strip_prefix(configured_prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

fn digest(value: &str) -> String {
    use sha2::Digest as _;
    let bytes = sha2::Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn task_state_code(state: CampaignTaskState) -> &'static str {
    match state {
        CampaignTaskState::Planned => "planned",
        CampaignTaskState::Ready => "ready",
        CampaignTaskState::Proposed => "proposed",
        CampaignTaskState::Leased => "leased",
        CampaignTaskState::Implementing => "implementing",
        CampaignTaskState::LocalGates => "local_gates",
        CampaignTaskState::Reviewing => "reviewing",
        CampaignTaskState::Correcting => "correcting",
        CampaignTaskState::PublicationReady => "publication_ready",
        CampaignTaskState::PullRequestOpen => "pr_open",
        CampaignTaskState::CiWaiting => "ci_waiting",
        CampaignTaskState::IntegrationQueued => "integration_queued",
        CampaignTaskState::MergeReady => "merge_ready",
        CampaignTaskState::Integrating => "integrating",
        CampaignTaskState::Merged => "merged",
        CampaignTaskState::Blocked => "blocked",
        CampaignTaskState::Disputed => "disputed",
        CampaignTaskState::Failed => "failed",
        CampaignTaskState::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use codingmage_campaign::{
        AdmissionDecision, CampaignAuthentication, CampaignConcurrency, CampaignGateTier,
        CampaignLimits, CampaignProvider, CampaignPublication, CampaignTaskTransition,
        DestinationPromotionPolicy, DurablePodScheduler, GitHubCampaignPolicy, MultiAgentPolicy,
        PodProposal, PodRisk, TaskIntegrationPolicy, TaskMergeStrategy, TeamResourcePolicy,
    };
    use codingmage_plan::TaskPlan;

    use super::*;
    use crate::initialize_team_campaign;

    const TASK_ID: &str = "24.2.1.1";
    const TASKS: &str = "# Tasks\n\n## Sprint 24 - Publication\n\n**Sprint goal:** Publish safely.\n\n### Story 24.2 - Visibility\n\n- [ ] **Task 24.2.1 - Publish task work**\n  - [ ] **Sub-task 24.2.1.1:** Publish one exact task issue and pull request.\n";

    #[derive(Default)]
    struct FakePublicationPort {
        issue: Option<u64>,
        pull_request: Option<u64>,
        branch_commit: Option<String>,
        ci: VecDeque<CiObservation>,
        uncertain_issue_once: bool,
        issue_creations: usize,
        pull_request_creations: usize,
        calls: usize,
        last_campaign_branch: Option<String>,
        campaign_branch_override: Option<String>,
        last_issue_state: Option<String>,
        last_issue_candidate: Option<String>,
    }

    impl TeamPublicationPort for FakePublicationPort {
        fn ensure_campaign_branch(
            &mut self,
            request: &CampaignBranchPublicationRequest,
        ) -> Result<CampaignBranchObservation, TeamPublicationError> {
            Ok(CampaignBranchObservation {
                branch: self
                    .campaign_branch_override
                    .clone()
                    .unwrap_or_else(|| request.campaign_branch.clone()),
                commit: request.campaign_commit.clone(),
                evidence_sha256: "0".repeat(64),
            })
        }

        fn ensure_issue(
            &mut self,
            request: &TaskIssueRequest,
        ) -> Result<IssueObservation, TeamPublicationError> {
            self.calls = self.calls.saturating_add(1);
            self.last_issue_state = Some(request.state.clone());
            self.last_issue_candidate
                .clone_from(&request.candidate_commit);
            let number = *self.issue.get_or_insert_with(|| {
                self.issue_creations = self.issue_creations.saturating_add(1);
                41
            });
            if request.issue_number.is_some_and(|value| value != number) {
                return Err(TeamPublicationError::Identity);
            }
            if self.uncertain_issue_once {
                self.uncertain_issue_once = false;
                return Err(TeamPublicationError::Uncertain);
            }
            Ok(IssueObservation {
                number,
                evidence_sha256: "1".repeat(64),
            })
        }

        fn ensure_reviewed_branch(
            &mut self,
            request: &TaskPublicationRequest,
        ) -> Result<PushObservation, TeamPublicationError> {
            self.calls = self.calls.saturating_add(1);
            if self
                .branch_commit
                .as_ref()
                .is_some_and(|commit| commit != &request.reviewed_commit)
            {
                return Err(TeamPublicationError::Identity);
            }
            self.branch_commit = Some(request.reviewed_commit.clone());
            Ok(PushObservation {
                branch: request.task_branch.clone(),
                commit: request.reviewed_commit.clone(),
                evidence_sha256: "2".repeat(64),
            })
        }

        fn ensure_pull_request(
            &mut self,
            request: &TaskPublicationRequest,
        ) -> Result<PullRequestObservation, TeamPublicationError> {
            self.calls = self.calls.saturating_add(1);
            self.last_campaign_branch = Some(request.campaign_branch.clone());
            let number = *self.pull_request.get_or_insert_with(|| {
                self.pull_request_creations = self.pull_request_creations.saturating_add(1);
                57
            });
            if request
                .pull_request_number
                .is_some_and(|value| value != number)
            {
                return Err(TeamPublicationError::Identity);
            }
            Ok(PullRequestObservation {
                number,
                base_branch: request.campaign_branch.clone(),
                head_branch: request.task_branch.clone(),
                head_commit: request.reviewed_commit.clone(),
                evidence_sha256: "3".repeat(64),
            })
        }

        fn observe_ci(
            &mut self,
            _: &TaskPublicationRequest,
        ) -> Result<CiObservation, TeamPublicationError> {
            self.calls = self.calls.saturating_add(1);
            Ok(self.ci.pop_front().unwrap_or(CiObservation::Pending))
        }

        fn ensure_task_completion(
            &mut self,
            request: &TaskPublicationRequest,
            _: &str,
            _: &str,
        ) -> Result<TaskCompletionObservation, TeamPublicationError> {
            Ok(TaskCompletionObservation {
                issue_number: request.issue_number.ok_or(TeamPublicationError::State)?,
                pull_request_number: request
                    .pull_request_number
                    .ok_or(TeamPublicationError::State)?,
                pull_request_merged: false,
                evidence_sha256: "9".repeat(64),
            })
        }
    }

    #[test]
    fn campaign_branch_publication_rejects_mismatched_adapter_identity() {
        let (spec, _, snapshot) = fixture();
        let mut port = FakePublicationPort::default();
        let observation =
            synchronize_campaign_branch(&spec, &spec.campaign_branch, &snapshot, &mut port)
                .unwrap();
        assert_eq!(observation.commit, snapshot.campaign_head);

        port.campaign_branch_override = Some("codingmage/unowned".to_owned());
        assert_eq!(
            synchronize_campaign_branch(&spec, &spec.campaign_branch, &snapshot, &mut port,),
            Err(TeamPublicationError::Identity)
        );
    }

    #[test]
    fn assigned_task_issue_is_reconciled_before_candidate_or_review() {
        let (spec, plan, mut snapshot) = assigned_fixture();
        let mut port = FakePublicationPort {
            uncertain_issue_once: true,
            ..FakePublicationPort::default()
        };
        assert_eq!(
            snapshot.tasks[TASK_ID].state,
            CampaignTaskState::Implementing
        );
        assert_eq!(snapshot.tasks[TASK_ID].candidate_commit, None);

        assert_eq!(
            synchronize_task_issue(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| Ok(())
            ),
            Err(TeamPublicationError::Uncertain)
        );
        assert_eq!(snapshot.tasks[TASK_ID].issue_number, None);
        assert_eq!(port.issue_creations, 1);

        let observed = synchronize_task_issue(
            &spec,
            &spec.campaign_branch,
            &plan,
            &mut snapshot,
            TASK_ID,
            &mut port,
            |_| Ok(()),
        )
        .expect("reconciled assigned issue");
        assert_eq!(observed.number, 41);
        assert_eq!(snapshot.tasks[TASK_ID].issue_number, Some(41));
        assert_eq!(port.last_issue_state.as_deref(), Some("implementing"));
        assert_eq!(port.last_issue_candidate, None);
        assert_eq!(port.pull_request_creations, 0);

        synchronize_task_issue(
            &spec,
            &spec.campaign_branch,
            &plan,
            &mut snapshot,
            TASK_ID,
            &mut port,
            |_| Ok(()),
        )
        .expect("idempotent assigned issue");
        assert_eq!(port.issue_creations, 1);
    }

    #[test]
    fn publication_reconciles_uncertain_issue_and_never_duplicates_remote_objects() {
        let (spec, plan, mut snapshot) = fixture();
        let reviewed = snapshot.tasks[TASK_ID].reviewed_commit.clone().unwrap();
        let mut port = FakePublicationPort {
            uncertain_issue_once: true,
            ci: VecDeque::from([
                CiObservation::Pending,
                CiObservation::Passed {
                    commit: reviewed,
                    evidence_sha256: "7".repeat(64),
                },
            ]),
            ..FakePublicationPort::default()
        };
        assert_eq!(
            synchronize_task_publication(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| { Ok(()) }
            ),
            Err(TeamPublicationError::Uncertain)
        );
        assert_eq!(port.issue_creations, 1);
        assert_eq!(snapshot.tasks[TASK_ID].issue_number, None);

        assert_eq!(
            synchronize_task_publication(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| { Ok(()) }
            )
            .unwrap(),
            TeamPublicationOutcome::WaitingForCi
        );
        assert_eq!(snapshot.tasks[TASK_ID].issue_number, Some(41));
        assert_eq!(snapshot.tasks[TASK_ID].pull_request_number, Some(57));
        assert_eq!(snapshot.tasks[TASK_ID].state, CampaignTaskState::CiWaiting);

        assert_eq!(
            synchronize_task_publication(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| { Ok(()) }
            )
            .unwrap(),
            TeamPublicationOutcome::ReadyForIntegration
        );
        let calls_after_pass = port.calls;
        assert_eq!(port.issue_creations, 1);
        assert_eq!(port.pull_request_creations, 1);
        assert_eq!(
            snapshot.tasks[TASK_ID].ci_evidence_sha256,
            vec!["7".repeat(64)]
        );
        assert_eq!(
            synchronize_task_publication(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| { Ok(()) }
            )
            .unwrap(),
            TeamPublicationOutcome::ReadyForIntegration
        );
        assert_eq!(port.calls, calls_after_pass);
    }

    #[test]
    fn failing_ci_returns_only_the_exact_task_to_correction() {
        let (spec, plan, mut snapshot) = fixture();
        let reviewed = snapshot.tasks[TASK_ID].reviewed_commit.clone().unwrap();
        let mut port = FakePublicationPort {
            ci: VecDeque::from([CiObservation::Failed {
                commit: reviewed,
                evidence_sha256: "8".repeat(64),
            }]),
            ..FakePublicationPort::default()
        };
        assert_eq!(
            synchronize_task_publication(
                &spec,
                &spec.campaign_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| { Ok(()) }
            )
            .unwrap(),
            TeamPublicationOutcome::CorrectionRequired
        );
        assert_eq!(snapshot.tasks[TASK_ID].state, CampaignTaskState::Correcting);
        assert_eq!(snapshot.tasks[TASK_ID].reviewed_commit, None);
        assert_eq!(snapshot.tasks[TASK_ID].correction_sessions.len(), 0);
    }

    #[test]
    fn publication_targets_the_exact_owned_campaign_branch() {
        let (spec, plan, mut snapshot) = fixture();
        let owned_branch = format!("{}/owned-run", spec.campaign_branch);
        let mut port = FakePublicationPort::default();
        assert_eq!(
            synchronize_task_publication(
                &spec,
                &owned_branch,
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut port,
                |_| Ok(())
            )
            .unwrap(),
            TeamPublicationOutcome::WaitingForCi
        );
        assert_eq!(
            port.last_campaign_branch.as_deref(),
            Some(owned_branch.as_str())
        );

        let (spec, plan, mut snapshot) = fixture();
        let mut denied = FakePublicationPort::default();
        assert_eq!(
            synchronize_task_publication(
                &spec,
                "codingmage/unowned",
                &plan,
                &mut snapshot,
                TASK_ID,
                &mut denied,
                |_| Ok(())
            ),
            Err(TeamPublicationError::Authority)
        );
        assert_eq!(denied.calls, 0);
    }

    fn fixture() -> (CampaignSpec, TaskPlan, TeamCampaignSnapshot) {
        let (spec, plan, mut snapshot) = assigned_fixture();
        let record = snapshot.tasks.get_mut(TASK_ID).unwrap();
        record.candidate_commit = Some("c".repeat(40));
        transition(record, CampaignTaskState::LocalGates, "5");
        record.gate_evidence_sha256.push("6".repeat(64));
        transition(record, CampaignTaskState::Reviewing, "7");
        record.reviewed_commit = Some("c".repeat(40));
        record.review_sessions.push("review-publication".to_owned());
        record.review_evidence_sha256.push("8".repeat(64));
        transition(record, CampaignTaskState::PublicationReady, "9");
        snapshot.verify().unwrap();
        (spec, plan, snapshot)
    }

    #[allow(clippy::too_many_lines)]
    fn assigned_fixture() -> (CampaignSpec, TaskPlan, TeamCampaignSnapshot) {
        let plan = TaskPlan::parse(TASKS.as_bytes()).unwrap();
        let provider = |name: &str| CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        let spec = CampaignSpec {
            version: 3,
            campaign_id: "publication-fixture".to_owned(),
            repository_id: "repo-publication".to_owned(),
            repository_path: PathBuf::from("/tmp/publication-fixture"),
            initial_commit: "a".repeat(40),
            task_source_sha256: plan.source_sha256.clone(),
            operator_authorization_sha256: "b".repeat(64),
            max_parallel_pods: 1,
            max_units: 1,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 10,
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
            campaign_branch: "codingmage/publication-fixture".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::DraftStoryPullRequests,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: codingmage_campaign::CampaignExecutionMode::Parallel,
                publication_mode: TaskPublicationMode::PerTaskDraftPullRequest,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
                task_merge_strategy: TaskMergeStrategy::Squash,
                github: Some(GitHubCampaignPolicy {
                    cli_executable: PathBuf::from("/usr/bin/gh"),
                    account: "fixture-user".to_owned(),
                    host: "github.com".to_owned(),
                    owner: "fixture-owner".to_owned(),
                    repository: "fixture-repository".to_owned(),
                    remote: "origin".to_owned(),
                    destination_branch: "main".to_owned(),
                    required_checks: vec!["workspace".to_owned()],
                }),
                concurrency: CampaignConcurrency::default(),
                resources: TeamResourcePolicy::default(),
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 0,
                integration_validation_interval: 1,
            }),
        };
        let mut snapshot = initialize_team_campaign(&spec, &plan).unwrap();
        let proposal = PodProposal::seal(
            PodProposal {
                version: 3,
                task_id: TASK_ID.to_owned(),
                task_source_sha256: plan.source_sha256.clone(),
                owned_paths: vec![PathBuf::from("src")],
                dependencies: Vec::new(),
                gate_tiers: vec!["focused".to_owned()],
                test_resources: vec!["workspace".to_owned()],
                expected_artifacts: vec![PathBuf::from("src")],
                risk: PodRisk::Routine,
                rationale_summary: "bounded fixture".to_owned(),
                proposal_sha256: "0".repeat(64),
            },
            &spec,
        )
        .unwrap();
        let mut scheduler = DurablePodScheduler::from_snapshot(snapshot.scheduler.clone()).unwrap();
        let generation = scheduler.begin_generation(&[TASK_ID.to_owned()]).unwrap();
        snapshot.generation = generation;
        let record = snapshot.tasks.get_mut(TASK_ID).unwrap();
        record.propose(generation, "1".repeat(64)).unwrap();
        let AdmissionDecision::Admitted(lease) = scheduler
            .admit(&spec, generation, &snapshot.campaign_head, &proposal)
            .unwrap()
        else {
            panic!("fixture task must be admitted");
        };
        record.bind_lease(&lease, "2".repeat(64)).unwrap();
        record
            .bind_run("run-publication".to_owned(), "3".repeat(64))
            .unwrap();
        record
            .bind_worktree(
                "worktree-publication".to_owned(),
                "codingmage/publication-task".to_owned(),
                "4".repeat(64),
            )
            .unwrap();
        snapshot.scheduler = scheduler.snapshot().clone();
        snapshot.verify().unwrap();
        (spec, plan, snapshot)
    }

    fn transition(
        record: &mut codingmage_campaign::CampaignTaskRecord,
        to: CampaignTaskState,
        digit: &str,
    ) {
        record
            .transition(&CampaignTaskTransition {
                sequence: record.next_transition,
                campaign_id: record.campaign_id.clone(),
                task_id: record.task_id.clone(),
                generation: record.generation,
                from: record.state,
                to,
                evidence_sha256: digit.repeat(64),
            })
            .unwrap();
    }
}
