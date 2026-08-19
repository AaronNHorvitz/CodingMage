//! Deny-first GitHub synchronization with idempotent, human-preserving writes.

use std::{
    collections::BTreeSet,
    fmt::{self, Write as _},
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
            || !valid_branch(&self.protected_branch)
            || self.branch == self.protected_branch
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
        if self.base_branch != identity.protected_branch
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
                GitHubOperation::ReadIssue | GitHubOperation::ReadPullRequest
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
            base_branch: "main".to_owned(),
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
