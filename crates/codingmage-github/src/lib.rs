//! Deny-first GitHub synchronization with idempotent, human-preserving writes.

use std::{
    collections::BTreeSet,
    fmt::{self, Write as _},
    path::{Path, PathBuf},
};

use codingmage_contracts::{EvidenceId, RepositoryId, RunId, TaskId};
use codingmage_state::{EventKind, EventOutcome, Journal, JournalEvent};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MAX_BODY_BYTES: usize = 256 * 1024;

/// Exact authenticated GitHub endpoint and repository binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubIdentity {
    /// Expected authenticated account.
    pub account: String,
    /// Exact API host.
    pub host: String,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repository: String,
    /// Exact publication branch.
    pub branch: String,
    /// Isolated campaign integration branch targeted by task pull requests.
    pub campaign_branch: String,
    /// Default or protected branch that publication must not target.
    pub protected_branch: String,
}

impl GitHubIdentity {
    /// Validates all authority-bearing identity fields.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::InvalidIdentity`] for malformed or overlapping identities.
    pub fn validate(&self) -> Result<(), GitHubError> {
        if !valid_component(&self.account)
            || !valid_host(&self.host)
            || !valid_component(&self.owner)
            || !valid_component(&self.repository)
            || !valid_branch(&self.branch)
            || !valid_branch(&self.campaign_branch)
            || !valid_branch(&self.protected_branch)
            || self.branch == self.protected_branch
            || self.branch == self.campaign_branch
            || self.campaign_branch == self.protected_branch
        {
            return Err(GitHubError::InvalidIdentity);
        }
        Ok(())
    }

    /// Returns the exact repository selector for `gh --repo`.
    #[must_use]
    pub fn repository_selector(&self) -> String {
        format!("{}/{}", self.owner, self.repository)
    }
}

/// Separately configurable GitHub capabilities. Every field defaults to denied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct GitHubPermissions {
    /// Read issues.
    pub issue_read: bool,
    /// Create or update owned issue sections.
    pub issue_write: bool,
    /// Read pull requests.
    pub pull_request_read: bool,
    /// Create or update draft pull requests.
    pub pull_request_write: bool,
    /// Add explicitly labeled automated comments.
    pub comments: bool,
    /// Push the exact configured feature branch.
    pub branch_push: bool,
    /// Read commit-bound check status.
    pub checks_read: bool,
    /// Merge an exact reviewed task branch into the campaign branch.
    pub task_merge: bool,
    /// Promote the exact completed campaign branch to the protected destination.
    pub destination_merge: bool,
}

/// Operations exposed by the adapter. Destructive administration is intentionally absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubOperation {
    /// Read an issue.
    ReadIssue,
    /// Create or update a story issue.
    WriteIssue,
    /// Read a pull request.
    ReadPullRequest,
    /// Create or update a draft pull request.
    WriteDraftPullRequest,
    /// Add an automated review comment.
    Comment,
    /// Push the exact feature branch.
    PushBranch,
    /// Read checks for an exact commit.
    ReadChecks,
    /// Merge an exact task pull request to the campaign branch.
    MergeTaskPullRequest,
    /// Merge an exact final pull request to the protected destination.
    MergeDestinationPullRequest,
}

impl GitHubPermissions {
    /// Returns whether the exact operation was explicitly granted.
    #[must_use]
    pub const fn allows(self, operation: GitHubOperation) -> bool {
        match operation {
            GitHubOperation::ReadIssue => self.issue_read,
            GitHubOperation::WriteIssue => self.issue_write,
            GitHubOperation::ReadPullRequest => self.pull_request_read,
            GitHubOperation::WriteDraftPullRequest => self.pull_request_write,
            GitHubOperation::Comment => self.comments,
            GitHubOperation::PushBranch => self.branch_push,
            GitHubOperation::ReadChecks => self.checks_read,
            GitHubOperation::MergeTaskPullRequest => self.task_merge,
            GitHubOperation::MergeDestinationPullRequest => self.destination_merge,
        }
    }
}

/// Token-blind `gh` capability probe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthProbe {
    /// Authenticated account from `gh api user`.
    pub login: String,
    /// Exact host selected by configuration.
    pub host: String,
}

impl AuthProbe {
    /// Parses bounded structured probe output and validates account and host.
    ///
    /// # Errors
    ///
    /// Returns a schema, identity, or size error. No token field is accepted.
    pub fn parse(bytes: &[u8], identity: &GitHubIdentity) -> Result<Self, GitHubError> {
        if bytes.len() > 16 * 1024 {
            return Err(GitHubError::InvalidResponse);
        }
        let probe: Self =
            serde_json::from_slice(bytes).map_err(|_| GitHubError::InvalidResponse)?;
        if probe.login != identity.account || probe.host != identity.host {
            return Err(GitHubError::IdentityChanged);
        }
        Ok(probe)
    }

    /// Returns exact token-blind argument vectors for the capability probe.
    #[must_use]
    pub fn command_plans(identity: &GitHubIdentity) -> [Vec<String>; 2] {
        [
            vec![
                "auth".to_owned(),
                "status".to_owned(),
                "--hostname".to_owned(),
                identity.host.clone(),
            ],
            vec![
                "api".to_owned(),
                "--hostname".to_owned(),
                identity.host.clone(),
                "user".to_owned(),
                "--jq".to_owned(),
                "{login:.login,host:\"".to_owned() + &identity.host + "\"}",
            ],
        ]
    }
}

/// Canonical story content owned by `CodingMage`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryIssue {
    /// Canonical story task identity.
    pub story_id: TaskId,
    /// Bounded title.
    pub title: String,
    /// Canonical source anchor.
    pub source_anchor: String,
    /// Sub-task lines controlled by local planning state.
    pub subtasks: Vec<(TaskId, String, bool)>,
}

impl StoryIssue {
    /// Renders one marker-bounded owned section.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::InvalidContent`] for malformed or oversized fields.
    pub fn render_owned_section(&self) -> Result<String, GitHubError> {
        if self.title.is_empty()
            || self.title.len() > 256
            || self.title.contains(['\n', '\r', '\0'])
            || !valid_anchor(&self.source_anchor)
            || self.subtasks.len() > 1_000
        {
            return Err(GitHubError::InvalidContent);
        }
        let mut body = format!(
            "<!-- codingmage:start {} -->\nSource: `{}`\n\n",
            self.story_id, self.source_anchor
        );
        for (task_id, title, complete) in &self.subtasks {
            if title.is_empty() || title.len() > 512 || title.contains(['\n', '\r', '\0']) {
                return Err(GitHubError::InvalidContent);
            }
            body.push_str(if *complete { "- [x] " } else { "- [ ] " });
            let _ = writeln!(body, "`{task_id}` {title}");
        }
        let _ = write!(body, "<!-- codingmage:end {} -->", self.story_id);
        if body.len() > MAX_BODY_BYTES {
            return Err(GitHubError::InvalidContent);
        }
        Ok(body)
    }

    /// Replaces only the matching CodingMage-owned section and preserves all other bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched, duplicated, or unclosed ownership markers.
    pub fn merge_into(&self, existing: &str) -> Result<String, GitHubError> {
        let replacement = self.render_owned_section()?;
        let start = format!("<!-- codingmage:start {} -->", self.story_id);
        let end = format!("<!-- codingmage:end {} -->", self.story_id);
        let starts: Vec<_> = existing.match_indices(&start).collect();
        let ends: Vec<_> = existing.match_indices(&end).collect();
        match (starts.as_slice(), ends.as_slice()) {
            ([], []) => {
                let separator = if existing.is_empty() { "" } else { "\n\n" };
                Ok(format!("{existing}{separator}{replacement}"))
            }
            ([(start_at, _)], [(end_at, _)]) if start_at < end_at => {
                let after = end_at + end.len();
                Ok(format!(
                    "{}{}{}",
                    &existing[..*start_at],
                    replacement,
                    &existing[after..]
                ))
            }
            _ => Err(GitHubError::OwnershipMarkers),
        }
    }
}

/// Complete coordinator-owned section for one campaign task issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskIssue {
    /// Immutable campaign identity.
    pub campaign_id: String,
    /// Canonical task identity.
    pub task_id: TaskId,
    /// Optional parent story or task.
    pub parent: Option<TaskId>,
    /// Ordered dependency identities.
    pub dependencies: Vec<TaskId>,
    /// Exact assigned pod identity.
    pub pod_id: String,
    /// Exact repository-relative write authority.
    pub authorized_paths: Vec<PathBuf>,
    /// Ordered required gate profile identifiers.
    pub required_gates: Vec<String>,
    /// Current closed task-state identifier.
    pub state: String,
    /// Exact task branch.
    pub branch: String,
    /// Bound task pull request, when created.
    pub pull_request: Option<u64>,
    /// Latest immutable candidate commit, when present.
    pub candidate_commit: Option<String>,
    /// Latest structured automated-review result.
    pub review_result: Option<String>,
    /// Stable content-free blocker code.
    pub blocker: Option<String>,
    /// Ordered immutable completion evidence.
    pub evidence: Vec<EvidenceId>,
}

impl TaskIssue {
    /// Renders the complete marker-owned task section.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::InvalidContent`] when any authority field is unsafe or unbounded.
    pub fn render_owned_section(&self) -> Result<String, GitHubError> {
        self.validate()?;
        let key = self.marker_key();
        let dependencies = join_task_ids(&self.dependencies);
        let parent = self.parent.as_ref().map_or("none", TaskId::as_str);
        let paths = self
            .authorized_paths
            .iter()
            .map(|path| format!("`{}`", path.display()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut body = format!(
            "<!-- codingmage:task:start {key} -->\nAutomated task record; human-authored text outside this section is preserved.\n\nCampaign: `{}`\nTask: `{}`\nParent: `{parent}`\nDependencies: {dependencies}\nAssigned pod: `{}`\nAuthorized paths: {paths}\nRequired gates: {}\nState: `{}`\nBranch: `{}`\nPull request: {}\nCandidate: {}\nAutomated review: {}\nBlocker: {}\nEvidence: {}\n<!-- codingmage:task:end {key} -->",
            self.campaign_id,
            self.task_id,
            self.pod_id,
            join_or_none(&self.required_gates),
            self.state,
            self.branch,
            self.pull_request
                .map_or_else(|| "none".to_owned(), |value| format!("#{value}")),
            self.candidate_commit.as_deref().unwrap_or("none"),
            self.review_result.as_deref().unwrap_or("none"),
            self.blocker.as_deref().unwrap_or("none"),
            join_evidence(&self.evidence),
        );
        if self.evidence.is_empty() {
            body = body.replace("Evidence: \n", "Evidence: none\n");
        }
        if body.len() > MAX_BODY_BYTES {
            return Err(GitHubError::InvalidContent);
        }
        Ok(body)
    }

    /// Replaces only this task's exact owned section and preserves all surrounding bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, duplicated, or crossing ownership markers.
    pub fn merge_into(&self, existing: &str) -> Result<String, GitHubError> {
        let replacement = self.render_owned_section()?;
        replace_owned_section(
            existing,
            &format!("<!-- codingmage:task:start {} -->", self.marker_key()),
            &format!("<!-- codingmage:task:end {} -->", self.marker_key()),
            &replacement,
        )
    }

    fn marker_key(&self) -> String {
        format!("{}:{}", self.campaign_id, self.task_id)
    }

    fn validate(&self) -> Result<(), GitHubError> {
        if !valid_component(&self.campaign_id)
            || !valid_component(&self.pod_id)
            || self.dependencies.len() > 1_000
            || self.authorized_paths.is_empty()
            || self.authorized_paths.len() > 1_000
            || self
                .authorized_paths
                .iter()
                .any(|path| !safe_relative(path))
            || self.required_gates.is_empty()
            || self.required_gates.len() > 256
            || self
                .required_gates
                .iter()
                .any(|value| !valid_component(value))
            || !valid_component(&self.state)
            || !valid_branch(&self.branch)
            || self.pull_request == Some(0)
            || self
                .candidate_commit
                .as_ref()
                .is_some_and(|commit| !valid_commit(commit))
            || self
                .review_result
                .as_ref()
                .is_some_and(|value| !valid_component(value))
            || self
                .blocker
                .as_ref()
                .is_some_and(|value| !valid_component(value))
            || self.evidence.len() > 1_024
        {
            return Err(GitHubError::InvalidContent);
        }
        Ok(())
    }
}

/// Draft task pull-request content bound to one campaign branch and immutable reviewed commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskPullRequest {
    /// Immutable campaign identity.
    pub campaign_id: String,
    /// Canonical task identity.
    pub task_id: TaskId,
    /// Linked task issue.
    pub issue_number: u64,
    /// Exact campaign integration branch.
    pub base_branch: String,
    /// Exact task branch.
    pub head_branch: String,
    /// Immutable cumulative reviewed candidate.
    pub reviewed_commit: String,
    /// Stable task requirement identifiers.
    pub requirements: Vec<String>,
    /// Immutable deterministic test evidence.
    pub tests: Vec<EvidenceId>,
    /// Structured automated-review result.
    pub review_result: String,
    /// Ordered correction-session identities.
    pub correction_history: Vec<String>,
    /// Current integration-state identifier.
    pub integration_status: String,
    /// Stable risk identifiers.
    pub risk_notes: Vec<String>,
}

impl TaskPullRequest {
    /// Renders a marker-owned draft body that never represents human approval.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::InvalidContent`] for an unsafe identity or unbound evidence.
    pub fn render_owned_section(&self, identity: &GitHubIdentity) -> Result<String, GitHubError> {
        if !valid_component(&self.campaign_id)
            || self.issue_number == 0
            || self.base_branch != identity.campaign_branch
            || self.head_branch != identity.branch
            || self.head_branch == identity.protected_branch
            || !valid_commit(&self.reviewed_commit)
            || self.requirements.is_empty()
            || self.tests.is_empty()
            || self.requirements.len() > 1_000
            || self.tests.len() > 1_024
            || self.correction_history.len() > 1_024
            || self.risk_notes.len() > 1_024
            || self
                .requirements
                .iter()
                .chain(&self.correction_history)
                .chain(&self.risk_notes)
                .any(|value| !valid_component(value))
            || !valid_component(&self.review_result)
            || !valid_component(&self.integration_status)
        {
            return Err(GitHubError::InvalidContent);
        }
        let key = format!("{}:{}", self.campaign_id, self.task_id);
        let body = format!(
            "<!-- codingmage:pr:start {key} -->\nAutomated development record; this is not human approval.\n\nCampaign: `{}`\nTask: `{}`\nLinked issue: #{}\nBase: `{}`\nHead: `{}`\nReviewed commit: `{}`\nRequirements: {}\nTests: {}\nAutomated review: `{}`\nCorrection history: {}\nIntegration status: `{}`\nRisk notes: {}\n<!-- codingmage:pr:end {key} -->",
            self.campaign_id,
            self.task_id,
            self.issue_number,
            self.base_branch,
            self.head_branch,
            self.reviewed_commit,
            self.requirements.join(", "),
            join_evidence(&self.tests),
            self.review_result,
            join_or_none(&self.correction_history),
            self.integration_status,
            join_or_none(&self.risk_notes),
        );
        if body.len() > MAX_BODY_BYTES {
            return Err(GitHubError::InvalidContent);
        }
        Ok(body)
    }

    /// Replaces only this task PR's exact owned section.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed ownership markers or invalid content.
    pub fn merge_into(
        &self,
        identity: &GitHubIdentity,
        existing: &str,
    ) -> Result<String, GitHubError> {
        let replacement = self.render_owned_section(identity)?;
        let key = format!("{}:{}", self.campaign_id, self.task_id);
        replace_owned_section(
            existing,
            &format!("<!-- codingmage:pr:start {key} -->"),
            &format!("<!-- codingmage:pr:end {key} -->"),
            &replacement,
        )
    }
}

/// Draft pull-request content tied to exact local evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPullRequest {
    /// Story scope.
    pub story_id: TaskId,
    /// Exact source and head branches.
    pub base_branch: String,
    /// Exact authorized feature branch.
    pub head_branch: String,
    /// Ordered full commit identities.
    pub commits: Vec<String>,
    /// Immutable test evidence.
    pub tests: Vec<EvidenceId>,
    /// Structured finding identities.
    pub findings: Vec<String>,
    /// Stable limitation codes.
    pub limitations: Vec<String>,
    /// Stable blocker codes.
    pub blockers: Vec<String>,
}

impl DraftPullRequest {
    /// Validates draft-only publication and renders its owned body.
    ///
    /// # Errors
    ///
    /// Returns an identity or content error when scope is not exact and locally evidenced.
    pub fn render(&self, identity: &GitHubIdentity) -> Result<String, GitHubError> {
        if self.base_branch != identity.campaign_branch
            || self.head_branch != identity.branch
            || self.commits.is_empty()
            || self.commits.iter().any(|commit| !valid_commit(commit))
            || self.tests.is_empty()
            || self
                .findings
                .iter()
                .chain(&self.limitations)
                .chain(&self.blockers)
                .any(|value| !valid_component(value))
        {
            return Err(GitHubError::InvalidContent);
        }
        let body = format!(
            "<!-- codingmage:draft {} -->\nAutomated development record; not human approval.\n\nScope: `{}`\nCommits: {}\nTests: {}\nFindings: {}\nLimitations: {}\nBlockers: {}\n",
            self.story_id,
            self.story_id,
            self.commits.join(", "),
            join_evidence(&self.tests),
            join_or_none(&self.findings),
            join_or_none(&self.limitations),
            join_or_none(&self.blockers),
        );
        if body.len() > MAX_BODY_BYTES {
            return Err(GitHubError::InvalidContent);
        }
        Ok(body)
    }
}

/// Automated review comment that cannot represent human approval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomatedReviewComment {
    /// Exact reviewed commit.
    pub reviewed_commit: String,
    /// Stable structured finding identities.
    pub finding_ids: Vec<String>,
}

impl AutomatedReviewComment {
    /// Renders a bounded explicitly automated comment.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubError::InvalidContent`] for invalid commit or finding identities.
    pub fn render(&self) -> Result<String, GitHubError> {
        if !valid_commit(&self.reviewed_commit)
            || self.finding_ids.is_empty()
            || self.finding_ids.iter().any(|value| !valid_component(value))
        {
            return Err(GitHubError::InvalidContent);
        }
        Ok(format!(
            "CodingMage automated review output; this is not human approval.\nReviewed commit: `{}`\nFindings: {}",
            self.reviewed_commit,
            self.finding_ids.join(", ")
        ))
    }
}

/// Durably records a refused GitHub identity, redirect, or permission change.
///
/// # Errors
///
/// Returns a journal error for persistence failure, or [`GitHubError::InvalidContent`] when the
/// supplied error is not an external-boundary change.
pub fn record_boundary_change(
    error: GitHubError,
    repository_id: RepositoryId,
    run_id: RunId,
    task_id: TaskId,
    timestamp_ms: u64,
    journal: &mut Journal,
) -> Result<(), GitHubError> {
    let change = match error {
        GitHubError::Redirect => "redirect",
        GitHubError::IdentityChanged => "identity_changed",
        GitHubError::PermissionChanged => "permission_changed",
        _ => return Err(GitHubError::InvalidContent),
    };
    journal
        .append(JournalEvent {
            timestamp_ms,
            run_id,
            task_id,
            repository_id,
            identities: codingmage_state::DurableIdentities::default(),
            kind: EventKind::ExternalBoundaryChanged {
                system: "github".to_owned(),
                change: change.to_owned(),
            },
            outcome: EventOutcome::Blocked,
            evidence: Vec::new(),
            redactions: Vec::new(),
        })
        .map_err(|_| GitHubError::Journal)?;
    journal.write_snapshot().map_err(|_| GitHubError::Journal)?;
    Ok(())
}

/// One idempotent remote write request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteRequest {
    /// Stable operation key used for timeout reconciliation.
    pub idempotency_key: String,
    /// Expected prior remote version, absent on create.
    pub expected_version: Option<u64>,
    /// Complete desired body after preserving human content.
    pub body: String,
    /// Requested operation.
    pub operation: GitHubOperation,
}

impl WriteRequest {
    /// Creates a content-derived idempotency key.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized content or a read-only operation.
    pub fn new(
        operation: GitHubOperation,
        expected_version: Option<u64>,
        body: String,
    ) -> Result<Self, GitHubError> {
        if body.len() > MAX_BODY_BYTES
            || matches!(
                operation,
                GitHubOperation::ReadIssue
                    | GitHubOperation::ReadPullRequest
                    | GitHubOperation::ReadChecks
            )
        {
            return Err(GitHubError::InvalidContent);
        }
        let canonical = format!("{operation:?}\0{expected_version:?}\0{body}");
        Ok(Self {
            idempotency_key: sha256_hex(canonical.as_bytes()),
            expected_version,
            body,
            operation,
        })
    }
}

/// Remote object metadata needed for idempotent synchronization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteRecord {
    /// Stable remote object identity.
    pub object_id: u64,
    /// Optimistic-concurrency version.
    pub version: u64,
    /// Current body including human-owned content.
    pub body: String,
    /// Previously accepted idempotency keys.
    pub applied_keys: BTreeSet<String>,
    /// Remote remains a draft.
    pub draft: bool,
}

/// Transport result that distinguishes unknown completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportResult {
    /// Write response was observed.
    Applied(RemoteRecord),
    /// Response was lost and completion must be reconciled.
    TimedOut,
}

/// Exact destination class for one coordinator-authorized merge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeTarget {
    /// Task branch into the isolated campaign branch.
    CampaignBranch,
    /// Completed campaign branch into the protected destination.
    ProtectedDestination,
}

/// Closed coordinator policy used to authorize one exact merge effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeMode {
    /// No merge effect is permitted.
    Never,
    /// Exact operator approval is required in addition to all automated evidence.
    HumanRequired,
    /// Automated merge is permitted after every deterministic prerequisite passes.
    Automatic,
}

/// Complete commit-bound evidence required before one merge effect may be attempted.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MergeAuthorization {
    /// Destination class.
    pub target: MergeTarget,
    /// Exact pull-request number.
    pub pull_request_number: u64,
    /// Exact base branch observed remotely.
    pub base_branch: String,
    /// Exact head branch observed remotely.
    pub head_branch: String,
    /// Commit accepted by deterministic gates and independent review.
    pub reviewed_commit: String,
    /// Current remote head observed immediately before authorization.
    pub observed_head: String,
    /// Every required deterministic gate passed for `reviewed_commit`.
    pub deterministic_gates_passed: bool,
    /// Required CI checks passed for `reviewed_commit`.
    pub ci_passed: bool,
    /// Fresh automated review returned PASS for `reviewed_commit`.
    pub automated_review_passed: bool,
    /// Branch-protection preconditions were observed satisfied.
    pub branch_protection_satisfied: bool,
    /// Every authorized plan task is terminally complete.
    pub campaign_complete: bool,
    /// No blocked or disputed task remains.
    pub no_blocking_findings: bool,
    /// Exact operator decision for a human-required destination promotion.
    pub human_approval: bool,
}

/// Narrow transport used by production `gh` and deterministic fake servers.
pub trait GitHubTransport {
    /// Reads the current object without mutating it.
    ///
    /// # Errors
    ///
    /// Returns a content-free identity, permission, network, or response error.
    fn read(&mut self) -> Result<Option<RemoteRecord>, GitHubError>;
    /// Sends one request with no retry hidden inside the transport.
    ///
    /// # Errors
    ///
    /// Returns a content-free identity, permission, network, conflict, or response error.
    fn write(&mut self, request: &WriteRequest) -> Result<TransportResult, GitHubError>;
    /// Looks up an exact idempotency key after uncertain completion.
    ///
    /// # Errors
    ///
    /// Returns a content-free identity, permission, network, or response error.
    fn reconcile(&mut self, idempotency_key: &str) -> Result<Option<RemoteRecord>, GitHubError>;
}

/// Idempotent synchronizer bound to exact identity and permissions.
#[derive(Debug)]
pub struct GitHubSynchronizer<T> {
    identity: GitHubIdentity,
    permissions: GitHubPermissions,
    transport: T,
}

impl<T: GitHubTransport> GitHubSynchronizer<T> {
    /// Creates a disabled-or-explicitly-authorized synchronizer.
    ///
    /// # Errors
    ///
    /// Returns an identity error before retaining the transport.
    pub fn new(
        identity: GitHubIdentity,
        permissions: GitHubPermissions,
        transport: T,
    ) -> Result<Self, GitHubError> {
        identity.validate()?;
        Ok(Self {
            identity,
            permissions,
            transport,
        })
    }

    /// Applies one permitted idempotent write or reconciles it after timeout without replay.
    ///
    /// # Errors
    ///
    /// Returns denied, identity, redirect, permission, conflict, or uncertain errors exactly.
    pub fn synchronize(&mut self, request: &WriteRequest) -> Result<RemoteRecord, GitHubError> {
        if !self.permissions.allows(request.operation) {
            return Err(GitHubError::Denied);
        }
        match self.transport.write(request)? {
            TransportResult::Applied(record) => Ok(record),
            TransportResult::TimedOut => self
                .transport
                .reconcile(&request.idempotency_key)?
                .ok_or(GitHubError::Uncertain),
        }
    }

    /// Synchronizes one local-authoritative story section while preserving concurrent human text.
    ///
    /// A single optimistic conflict is reconciled by refetching and rebuilding the owned section;
    /// network timeout is reconciled by idempotency-key lookup and is never blindly replayed.
    ///
    /// # Errors
    ///
    /// Returns the exact policy, transport, ownership, conflict, or uncertainty error.
    pub fn synchronize_story(&mut self, story: &StoryIssue) -> Result<RemoteRecord, GitHubError> {
        if !self.permissions.issue_read || !self.permissions.issue_write {
            return Err(GitHubError::Denied);
        }
        for attempt in 0..2 {
            let current = self.transport.read()?;
            let body = story.merge_into(current.as_ref().map_or("", |record| &record.body))?;
            let request = WriteRequest::new(
                GitHubOperation::WriteIssue,
                current.as_ref().map(|record| record.version),
                body,
            )?;
            match self.synchronize(&request) {
                Err(GitHubError::Conflict) if attempt == 0 => {}
                result => return result,
            }
        }
        Err(GitHubError::Conflict)
    }

    /// Authorizes only the exact nonprotected feature branch after local gates pass.
    ///
    /// # Errors
    ///
    /// Returns denied or identity errors; no force or protected-branch mode exists.
    pub fn authorize_push(
        &self,
        branch: &str,
        local_gates_passed: bool,
    ) -> Result<(), GitHubError> {
        if !self.permissions.branch_push || !local_gates_passed {
            return Err(GitHubError::Denied);
        }
        if branch != self.identity.branch || branch == self.identity.protected_branch {
            return Err(GitHubError::IdentityChanged);
        }
        Ok(())
    }

    /// Authorizes one exact commit-bound merge without performing the remote effect.
    ///
    /// Task merges may target only the campaign branch. Destination promotion additionally
    /// requires complete-plan and no-blocker evidence. Human-required policy cannot be satisfied
    /// by automated-review output alone.
    ///
    /// # Errors
    ///
    /// Returns denied or identity errors before any transport write can occur.
    pub fn authorize_merge(
        &self,
        request: &MergeAuthorization,
        mode: MergeMode,
    ) -> Result<GitHubOperation, GitHubError> {
        if mode == MergeMode::Never
            || request.pull_request_number == 0
            || !valid_commit(&request.reviewed_commit)
            || request.reviewed_commit != request.observed_head
            || !request.deterministic_gates_passed
            || !request.ci_passed
            || !request.automated_review_passed
            || !request.branch_protection_satisfied
            || (mode == MergeMode::HumanRequired && !request.human_approval)
        {
            return Err(GitHubError::Denied);
        }
        let operation = match request.target {
            MergeTarget::CampaignBranch => {
                if !self.permissions.task_merge
                    || request.base_branch != self.identity.campaign_branch
                    || request.head_branch != self.identity.branch
                {
                    return Err(GitHubError::IdentityChanged);
                }
                GitHubOperation::MergeTaskPullRequest
            }
            MergeTarget::ProtectedDestination => {
                if !self.permissions.destination_merge
                    || request.base_branch != self.identity.protected_branch
                    || request.head_branch != self.identity.campaign_branch
                    || !request.campaign_complete
                    || !request.no_blocking_findings
                {
                    return Err(GitHubError::Denied);
                }
                GitHubOperation::MergeDestinationPullRequest
            }
        };
        Ok(operation)
    }

    /// Consumes the synchronizer and returns its transport for test inspection.
    #[must_use]
    pub fn into_transport(self) -> T {
        self.transport
    }
}

/// Content-free GitHub adapter failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubError {
    /// Account, host, owner, repository, or branch is invalid.
    InvalidIdentity,
    /// Authenticated or remote identity changed.
    IdentityChanged,
    /// Capability was not explicitly granted.
    Denied,
    /// Structured response was malformed or oversized.
    InvalidResponse,
    /// Owned or publication content is invalid.
    InvalidContent,
    /// Ownership markers were malformed or duplicated.
    OwnershipMarkers,
    /// Remote version changed concurrently.
    Conflict,
    /// Network effect remains uncertain after lookup.
    Uncertain,
    /// Host redirected outside exact identity.
    Redirect,
    /// Remote permission was reduced or revoked.
    PermissionChanged,
    /// Durable boundary-change recording failed.
    Journal,
}

impl fmt::Display for GitHubError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIdentity => "codingmage.github.invalid_identity",
            Self::IdentityChanged => "codingmage.github.identity_changed",
            Self::Denied => "codingmage.github.denied",
            Self::InvalidResponse => "codingmage.github.invalid_response",
            Self::InvalidContent => "codingmage.github.invalid_content",
            Self::OwnershipMarkers => "codingmage.github.ownership_markers",
            Self::Conflict => "codingmage.github.conflict",
            Self::Uncertain => "codingmage.github.uncertain",
            Self::Redirect => "codingmage.github.redirect",
            Self::PermissionChanged => "codingmage.github.permission_changed",
            Self::Journal => "codingmage.github.journal",
        })
    }
}

impl std::error::Error for GitHubError {}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_host(value: &str) -> bool {
    valid_component(value) && value.contains('.') && !value.starts_with('.')
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(component, std::path::Component::Normal(_))
                && !component
                    .as_os_str()
                    .to_string_lossy()
                    .contains(['\0', '\n', '\r'])
        })
}

fn valid_anchor(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'#'))
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn join_evidence(values: &[EvidenceId]) -> String {
    values
        .iter()
        .map(EvidenceId::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_task_ids(values: &[TaskId]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
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

fn replace_owned_section(
    existing: &str,
    start: &str,
    end: &str,
    replacement: &str,
) -> Result<String, GitHubError> {
    let starts = existing.match_indices(start).collect::<Vec<_>>();
    let ends = existing.match_indices(end).collect::<Vec<_>>();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => {
            let separator = if existing.is_empty() { "" } else { "\n\n" };
            Ok(format!("{existing}{separator}{replacement}"))
        }
        ([(start_at, _)], [(end_at, _)]) if start_at < end_at => {
            let after = end_at.saturating_add(end.len());
            Ok(format!(
                "{}{}{}",
                &existing[..*start_at],
                replacement,
                &existing[after..]
            ))
        }
        _ => Err(GitHubError::OwnershipMarkers),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone, Copy, Debug)]
    enum Behavior {
        Apply,
        TimeoutApplied,
        TimeoutLost,
        ConflictWithHuman,
        Redirect,
        PermissionChanged,
    }

    #[derive(Debug, Default)]
    struct FakeGitHub {
        current: Option<RemoteRecord>,
        behaviors: VecDeque<Behavior>,
        writes: usize,
        reads: usize,
    }

    impl FakeGitHub {
        fn with_body(body: &str) -> Self {
            Self {
                current: Some(RemoteRecord {
                    object_id: 17,
                    version: 1,
                    body: body.to_owned(),
                    applied_keys: BTreeSet::new(),
                    draft: false,
                }),
                ..Self::default()
            }
        }

        fn apply(&mut self, request: &WriteRequest) -> RemoteRecord {
            if let Some(current) = &self.current
                && current.applied_keys.contains(&request.idempotency_key)
            {
                return current.clone();
            }
            let mut keys = self
                .current
                .as_ref()
                .map_or_else(BTreeSet::new, |record| record.applied_keys.clone());
            keys.insert(request.idempotency_key.clone());
            let record = RemoteRecord {
                object_id: self.current.as_ref().map_or(17, |record| record.object_id),
                version: self
                    .current
                    .as_ref()
                    .map_or(1, |record| record.version.saturating_add(1)),
                body: request.body.clone(),
                applied_keys: keys,
                draft: request.operation == GitHubOperation::WriteDraftPullRequest,
            };
            self.current = Some(record.clone());
            record
        }
    }

    impl GitHubTransport for FakeGitHub {
        fn read(&mut self) -> Result<Option<RemoteRecord>, GitHubError> {
            self.reads += 1;
            Ok(self.current.clone())
        }

        fn write(&mut self, request: &WriteRequest) -> Result<TransportResult, GitHubError> {
            self.writes += 1;
            if self.current.as_ref().map(|record| record.version) != request.expected_version {
                return Err(GitHubError::Conflict);
            }
            match self.behaviors.pop_front().unwrap_or(Behavior::Apply) {
                Behavior::Apply => Ok(TransportResult::Applied(self.apply(request))),
                Behavior::TimeoutApplied => {
                    self.apply(request);
                    Ok(TransportResult::TimedOut)
                }
                Behavior::TimeoutLost => Ok(TransportResult::TimedOut),
                Behavior::ConflictWithHuman => {
                    let current = self.current.as_mut().expect("fixture has current record");
                    current.body.push_str("\nHuman concurrent edit.");
                    current.version = current.version.saturating_add(1);
                    Err(GitHubError::Conflict)
                }
                Behavior::Redirect => Err(GitHubError::Redirect),
                Behavior::PermissionChanged => Err(GitHubError::PermissionChanged),
            }
        }

        fn reconcile(
            &mut self,
            idempotency_key: &str,
        ) -> Result<Option<RemoteRecord>, GitHubError> {
            Ok(self.current.as_ref().and_then(|record| {
                record
                    .applied_keys
                    .contains(idempotency_key)
                    .then(|| record.clone())
            }))
        }
    }

    fn identity() -> GitHubIdentity {
        GitHubIdentity {
            account: "AaronNHorvitz".to_owned(),
            host: "github.com".to_owned(),
            owner: "AaronNHorvitz".to_owned(),
            repository: "CodingMage".to_owned(),
            branch: "codingmage/story-15".to_owned(),
            campaign_branch: "codingmage/campaign-15".to_owned(),
            protected_branch: "main".to_owned(),
        }
    }

    fn permissions() -> GitHubPermissions {
        GitHubPermissions {
            issue_read: true,
            issue_write: true,
            pull_request_read: true,
            pull_request_write: true,
            comments: true,
            branch_push: true,
            checks_read: true,
            task_merge: true,
            destination_merge: true,
        }
    }

    fn story(complete: bool) -> StoryIssue {
        StoryIssue {
            story_id: TaskId::new("15.2").unwrap(),
            title: "GitHub publication".to_owned(),
            source_anchor: "TASKS.md#story-15.2".to_owned(),
            subtasks: vec![(
                TaskId::new("15.2.1").unwrap(),
                "Synchronize issue".to_owned(),
                complete,
            )],
        }
    }

    #[test]
    fn auth_probe_is_token_blind_and_binds_account_and_host() {
        let identity = identity();
        let probe = AuthProbe::parse(
            br#"{"login":"AaronNHorvitz","host":"github.com"}"#,
            &identity,
        )
        .unwrap();
        assert_eq!(probe.login, "AaronNHorvitz");
        assert!(
            AuthProbe::parse(
                br#"{"login":"AaronNHorvitz","host":"github.com","token":"secret"}"#,
                &identity
            )
            .is_err()
        );
        assert!(AuthProbe::parse(br#"{"login":"other","host":"github.com"}"#, &identity).is_err());
        let plans = AuthProbe::command_plans(&identity);
        assert!(
            plans
                .iter()
                .flatten()
                .all(|argument| !argument.contains("token"))
        );
    }

    #[test]
    fn owned_story_section_preserves_human_content_and_local_authority() {
        let remote = "Human introduction.\n\n<!-- codingmage:start 15.2 -->\n- [x] remote-only claim\n<!-- codingmage:end 15.2 -->\n\nHuman footer.";
        let merged = story(false).merge_into(remote).unwrap();
        assert!(merged.starts_with("Human introduction."));
        assert!(merged.ends_with("Human footer."));
        assert!(merged.contains("- [ ] `15.2.1` Synchronize issue"));
        assert!(!merged.contains("remote-only claim"));
    }

    #[test]
    fn issue_fields_cannot_inject_ownership_markers() {
        let mut hostile = story(false);
        hostile.subtasks[0].1 = "text\n<!-- codingmage:end 15.2 -->\nInjected authority".to_owned();
        assert_eq!(
            hostile.render_owned_section().unwrap_err(),
            GitHubError::InvalidContent
        );
        hostile = story(false);
        hostile.title = "title\n<!-- codingmage:start other -->".to_owned();
        assert_eq!(
            hostile.render_owned_section().unwrap_err(),
            GitHubError::InvalidContent
        );
    }

    #[test]
    fn timeout_is_reconciled_by_key_and_never_blindly_replayed() {
        let request = WriteRequest::new(
            GitHubOperation::WriteIssue,
            None,
            story(false).render_owned_section().unwrap(),
        )
        .unwrap();
        let mut applied = FakeGitHub::default();
        applied.behaviors.push_back(Behavior::TimeoutApplied);
        let mut sync = GitHubSynchronizer::new(identity(), permissions(), applied).unwrap();
        assert!(sync.synchronize(&request).is_ok());
        assert_eq!(sync.into_transport().writes, 1);

        let mut lost = FakeGitHub::default();
        lost.behaviors.push_back(Behavior::TimeoutLost);
        let mut sync = GitHubSynchronizer::new(identity(), permissions(), lost).unwrap();
        assert_eq!(
            sync.synchronize(&request).unwrap_err(),
            GitHubError::Uncertain
        );
        assert_eq!(sync.into_transport().writes, 1);
    }

    #[test]
    fn concurrent_human_edit_is_refetched_and_preserved() {
        let mut fake = FakeGitHub::with_body("Human initial text.");
        fake.behaviors.push_back(Behavior::ConflictWithHuman);
        fake.behaviors.push_back(Behavior::Apply);
        let mut sync = GitHubSynchronizer::new(identity(), permissions(), fake).unwrap();
        let result = sync.synchronize_story(&story(false)).unwrap();
        assert!(result.body.contains("Human initial text."));
        assert!(result.body.contains("Human concurrent edit."));
        assert!(result.body.contains("codingmage:start 15.2"));
        let fake = sync.into_transport();
        assert_eq!(fake.reads, 2);
        assert_eq!(fake.writes, 2);
    }

    #[test]
    fn redirect_permission_loss_and_disabled_adapter_fail_without_effect() {
        for (behavior, expected) in [
            (Behavior::Redirect, GitHubError::Redirect),
            (Behavior::PermissionChanged, GitHubError::PermissionChanged),
        ] {
            let mut fake = FakeGitHub::default();
            fake.behaviors.push_back(behavior);
            let request =
                WriteRequest::new(GitHubOperation::WriteIssue, None, "owned".to_owned()).unwrap();
            let mut sync = GitHubSynchronizer::new(identity(), permissions(), fake).unwrap();
            assert_eq!(sync.synchronize(&request).unwrap_err(), expected);
        }
        let fake = FakeGitHub::default();
        let request =
            WriteRequest::new(GitHubOperation::WriteIssue, None, "owned".to_owned()).unwrap();
        let mut sync =
            GitHubSynchronizer::new(identity(), GitHubPermissions::default(), fake).unwrap();
        assert_eq!(sync.synchronize(&request).unwrap_err(), GitHubError::Denied);
        assert_eq!(sync.into_transport().writes, 0);
    }

    #[test]
    fn draft_pr_and_push_remain_exact_nonapproval_operations() {
        let identity = identity();
        let draft = DraftPullRequest {
            story_id: TaskId::new("15.2").unwrap(),
            base_branch: "codingmage/campaign-15".to_owned(),
            head_branch: "codingmage/story-15".to_owned(),
            commits: vec!["a".repeat(40)],
            tests: vec![EvidenceId::new("evidence-15").unwrap()],
            findings: vec!["finding-1".to_owned()],
            limitations: vec!["none-recorded".to_owned()],
            blockers: Vec::new(),
        };
        let body = draft.render(&identity).unwrap();
        assert!(body.contains("not human approval"));
        let comment = AutomatedReviewComment {
            reviewed_commit: "a".repeat(40),
            finding_ids: vec!["finding-1".to_owned()],
        }
        .render()
        .unwrap();
        assert!(comment.contains("not human approval"));
        let sync = GitHubSynchronizer::new(identity, permissions(), FakeGitHub::default()).unwrap();
        assert!(sync.authorize_push("codingmage/story-15", true).is_ok());
        assert_eq!(
            sync.authorize_push("main", true).unwrap_err(),
            GitHubError::IdentityChanged
        );
        assert_eq!(
            sync.authorize_push("codingmage/story-15", false)
                .unwrap_err(),
            GitHubError::Denied
        );
    }

    #[test]
    fn task_issue_and_pr_preserve_human_content_and_bind_campaign_base() {
        let issue = TaskIssue {
            campaign_id: "campaign-15".to_owned(),
            task_id: TaskId::new("15.2.1").unwrap(),
            parent: Some(TaskId::new("15.2").unwrap()),
            dependencies: vec![TaskId::new("15.1.1").unwrap()],
            pod_id: "pod-15".to_owned(),
            authorized_paths: vec![PathBuf::from("crates/codingmage-github")],
            required_gates: vec!["github-tests".to_owned()],
            state: "publication_ready".to_owned(),
            branch: "codingmage/story-15".to_owned(),
            pull_request: Some(23),
            candidate_commit: Some("a".repeat(40)),
            review_result: Some("pass".to_owned()),
            blocker: None,
            evidence: vec![EvidenceId::new("github-evidence-15").unwrap()],
        };
        let existing = "Human issue context.\n\nHuman checklist.";
        let merged = issue.merge_into(existing).unwrap();
        assert!(merged.starts_with(existing));
        assert!(merged.contains("Assigned pod: `pod-15`"));
        assert!(merged.contains("Pull request: #23"));

        let task_pr = TaskPullRequest {
            campaign_id: "campaign-15".to_owned(),
            task_id: TaskId::new("15.2.1").unwrap(),
            issue_number: 19,
            base_branch: "codingmage/campaign-15".to_owned(),
            head_branch: "codingmage/story-15".to_owned(),
            reviewed_commit: "a".repeat(40),
            requirements: vec!["requirement-15".to_owned()],
            tests: vec![EvidenceId::new("github-evidence-15").unwrap()],
            review_result: "pass".to_owned(),
            correction_history: vec!["correction-1".to_owned()],
            integration_status: "queued".to_owned(),
            risk_notes: vec!["bounded".to_owned()],
        };
        let pr_body = task_pr
            .merge_into(&identity(), "Human PR context.")
            .unwrap();
        assert!(pr_body.starts_with("Human PR context."));
        assert!(pr_body.contains("Base: `codingmage/campaign-15`"));
        assert!(pr_body.contains("this is not human approval"));

        let mut wrong_base = task_pr;
        wrong_base.base_branch = "main".to_owned();
        assert_eq!(
            wrong_base.render_owned_section(&identity()).unwrap_err(),
            GitHubError::InvalidContent
        );
    }

    #[test]
    fn merge_authority_enforces_exact_sha_policy_and_safe_destination_default() {
        let sync =
            GitHubSynchronizer::new(identity(), permissions(), FakeGitHub::default()).unwrap();
        let task = MergeAuthorization {
            target: MergeTarget::CampaignBranch,
            pull_request_number: 23,
            base_branch: "codingmage/campaign-15".to_owned(),
            head_branch: "codingmage/story-15".to_owned(),
            reviewed_commit: "a".repeat(40),
            observed_head: "a".repeat(40),
            deterministic_gates_passed: true,
            ci_passed: true,
            automated_review_passed: true,
            branch_protection_satisfied: true,
            campaign_complete: false,
            no_blocking_findings: true,
            human_approval: false,
        };
        assert_eq!(
            sync.authorize_merge(&task, MergeMode::Automatic).unwrap(),
            GitHubOperation::MergeTaskPullRequest
        );
        assert_eq!(
            sync.authorize_merge(&task, MergeMode::HumanRequired)
                .unwrap_err(),
            GitHubError::Denied
        );
        let stale = MergeAuthorization {
            observed_head: "b".repeat(40),
            ..task.clone()
        };
        assert_eq!(
            sync.authorize_merge(&stale, MergeMode::Automatic)
                .unwrap_err(),
            GitHubError::Denied
        );

        let destination = MergeAuthorization {
            target: MergeTarget::ProtectedDestination,
            base_branch: "main".to_owned(),
            head_branch: "codingmage/campaign-15".to_owned(),
            campaign_complete: true,
            ..task
        };
        assert_eq!(
            sync.authorize_merge(&destination, MergeMode::HumanRequired)
                .unwrap_err(),
            GitHubError::Denied
        );
        assert_eq!(
            sync.authorize_merge(&destination, MergeMode::Automatic)
                .unwrap(),
            GitHubOperation::MergeDestinationPullRequest
        );
    }

    #[test]
    fn duplicate_delivery_returns_one_remote_object() {
        let request = WriteRequest::new(
            GitHubOperation::WriteDraftPullRequest,
            None,
            "draft".to_owned(),
        )
        .unwrap();
        let mut fake = FakeGitHub::default();
        let first = fake.apply(&request);
        let second = fake.apply(&request);
        assert_eq!(first, second);
        assert_eq!(second.applied_keys.len(), 1);
        assert!(second.draft);
    }

    #[test]
    fn boundary_changes_are_durably_recorded_without_remote_content() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-github-boundary-{}-{unique}",
            std::process::id()
        ));
        let mut journal = Journal::open(&root, "owner").unwrap();
        for (index, error) in [
            GitHubError::Redirect,
            GitHubError::IdentityChanged,
            GitHubError::PermissionChanged,
        ]
        .into_iter()
        .enumerate()
        {
            record_boundary_change(
                error,
                RepositoryId::new("repo-1").unwrap(),
                RunId::new("run-1").unwrap(),
                TaskId::new("task-15").unwrap(),
                index as u64,
                &mut journal,
            )
            .unwrap();
        }
        assert_eq!(journal.records().len(), 3);
        let persisted = std::fs::read_to_string(root.join("events.jsonl")).unwrap();
        assert!(!persisted.contains("remote body"));
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
}
