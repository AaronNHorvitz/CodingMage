//! Versioned host-application job and control contract (local preparation).
//!
//! These are closed schemas for a separately authorized host to request and
//! observe bounded coding work. They carry identities and digests only: a
//! host request never becomes executable authority by itself, and every
//! admitted unit remains subject to coordinator policy, operator grants, and
//! human-only approval requirements. Unknown fields, incompatible versions,
//! and mismatched identities fail before any lease or process exists. Live
//! cross-product use additionally needs counterpart admission and pinned
//! consumer qualification; fakes prove the contract only.

use core::fmt;

use serde::{Deserialize, Serialize};

use super::{ClientId, RepositoryId, RequestId, RunId, TaskId};

/// Host protocol version admitted by this contract.
pub const HOST_PROTOCOL_VERSION: u16 = 1;

/// Maximum operations advertised in one [`HostCapability`].
pub const MAX_HOST_OPERATIONS: usize = 16;

/// Requested host operation. Control operations form the closed subset in
/// [`HostControlOperation`]; parsing a control from a job operation is
/// refused by type, not by convention.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOperation {
    /// Submit one bounded job against preauthorized configuration.
    SubmitJob,
    /// Observe the read-only status report of an owned run.
    ReportStatus,
    /// Resume observation from a bounded event cursor.
    ObserveEvents,
    /// Pause an owned run at the next unit boundary.
    Pause,
    /// Resume a paused owned run.
    Resume,
    /// Let the current unit finish, then stop without admitting more work.
    StopAfterUnit,
    /// Cancel an owned run without adopting newer state.
    Cancel,
}

/// Closed control subset. Only these operations may ride a [`HostControl`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostControlOperation {
    /// Pause an owned run at the next unit boundary.
    Pause,
    /// Resume a paused owned run.
    Resume,
    /// Let the current unit finish, then stop without admitting more work.
    StopAfterUnit,
    /// Cancel an owned run without adopting newer state.
    Cancel,
}

/// Truthful lifecycle of one host-visible run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostRunState {
    /// Admitted but not yet executing.
    Submitted,
    /// Executing under coordinator authority.
    Active,
    /// Paused at a unit boundary by an exact control.
    Paused,
    /// Stopped truthfully on a blocker below.
    Blocked,
    /// Cancelled by an exact control; never resumed implicitly.
    Cancelled,
    /// Finished with a reviewed candidate and bound evidence.
    Completed,
    /// Stopped on failure without claiming completion.
    Failed,
}

/// Closed blocker category carried in status without prompts or prose.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostBlocker {
    /// An external prerequisite is unavailable.
    External,
    /// A human-only approval is missing.
    Approval,
    /// The request exceeds granted authority.
    Authority,
    /// A dependency is not ready.
    Dependency,
    /// The operation is unsupported on this coordinator.
    Unsupported,
}

/// Closed versioned host request envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostRequest {
    /// Host protocol version; must equal [`HOST_PROTOCOL_VERSION`].
    pub protocol_version: u16,
    /// Admitted client identity.
    pub client_id: ClientId,
    /// Target repository identity.
    pub repository: RepositoryId,
    /// Campaign run identity, absent before the run exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Task identity, absent for run-scoped operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
    /// Exact source commit the request was authorized against.
    pub source_commit: String,
    /// Canonical task-source digest the request was authorized against.
    pub task_source_digest: String,
    /// Authority-policy digest the request was authorized against.
    pub authority_policy_digest: String,
    /// Unique request identity; identical retries return prior results.
    pub request_id: RequestId,
    /// State revision the sender last observed.
    pub expected_state_revision: u64,
    /// Requested operation.
    pub operation: HostOperation,
}

/// Closed versioned capability advertisement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostCapability {
    /// Host protocol version; must equal [`HOST_PROTOCOL_VERSION`].
    pub protocol_version: u16,
    /// Supported operations, non-empty and duplicate-free.
    pub operations: Vec<HostOperation>,
}

/// Closed versioned host control bound to one exact revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostControl {
    /// Unique request identity of the control itself.
    pub request_id: RequestId,
    /// Owned run the control targets.
    pub run: RunId,
    /// Revision the control was issued against; stale controls fail.
    pub expected_state_revision: u64,
    /// Control operation.
    pub operation: HostControlOperation,
}

/// Closed versioned host-visible run status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostStatus {
    /// Unique request identity this status answers.
    pub request_id: RequestId,
    /// Owned run under observation.
    pub run: RunId,
    /// Current lifecycle.
    pub state: HostRunState,
    /// Current durable state revision.
    pub state_revision: u64,
    /// Blocker category, present only when `state` is [`HostRunState::Blocked`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<HostBlocker>,
}

/// Stable host-contract error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostContractError {
    /// A request, capability, control, or status value was malformed.
    InvalidRequest,
    /// The protocol version is not the admitted one.
    UnsupportedVersion,
    /// The control targets a revision that is no longer current.
    StaleRevision,
}

impl fmt::Display for HostContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "codingmage.host.invalid_request",
            Self::UnsupportedVersion => "codingmage.host.unsupported_version",
            Self::StaleRevision => "codingmage.host.stale_revision",
        })
    }
}

impl std::error::Error for HostContractError {}

impl HostRequest {
    /// Validates the version and the bound commit and authority digests.
    ///
    /// Client, repository, run, task, and request identities are validated
    /// by their constructors and deserializers before this check runs.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::UnsupportedVersion`] for a foreign
    /// protocol version and [`HostContractError::InvalidRequest`] for any
    /// malformed commit or digest.
    pub fn verify(&self) -> Result<(), HostContractError> {
        if self.protocol_version != HOST_PROTOCOL_VERSION {
            return Err(HostContractError::UnsupportedVersion);
        }
        if !valid_commit(&self.source_commit)
            || !valid_digest(&self.task_source_digest)
            || !valid_digest(&self.authority_policy_digest)
        {
            return Err(HostContractError::InvalidRequest);
        }
        Ok(())
    }

    /// Returns true when `current_revision` still matches the requester's view.
    ///
    /// A request issued against an older revision must be retried against
    /// fresh state rather than admitted blindly.
    #[must_use]
    pub fn is_fresh_against(&self, current_revision: u64) -> bool {
        self.expected_state_revision == current_revision
    }
}

impl HostCapability {
    /// Validates the version and the advertised operation set.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::UnsupportedVersion`] for a foreign
    /// protocol version and [`HostContractError::InvalidRequest`] for an
    /// empty, oversized, or duplicated operation set.
    pub fn verify(&self) -> Result<(), HostContractError> {
        if self.protocol_version != HOST_PROTOCOL_VERSION {
            return Err(HostContractError::UnsupportedVersion);
        }
        if self.operations.is_empty() || self.operations.len() > MAX_HOST_OPERATIONS {
            return Err(HostContractError::InvalidRequest);
        }
        let mut seen: Vec<HostOperation> = Vec::with_capacity(self.operations.len());
        for operation in &self.operations {
            if seen.contains(operation) {
                return Err(HostContractError::InvalidRequest);
            }
            seen.push(*operation);
        }
        Ok(())
    }
}

impl HostControl {
    /// Validates the control and its freshness against `current_revision`.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::StaleRevision`] when the control targets
    /// a revision that is no longer current. Request and run identities are
    /// validated by their constructors and deserializers.
    pub fn verify_against(&self, current_revision: u64) -> Result<(), HostContractError> {
        if self.expected_state_revision != current_revision {
            return Err(HostContractError::StaleRevision);
        }
        Ok(())
    }
}

impl HostStatus {
    /// Validates status coherence: a blocker is present exactly when the
    /// lifecycle is blocked.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::InvalidRequest`] for a blocker attached
    /// to a non-blocked lifecycle, or a blocked lifecycle without one.
    pub fn verify(&self) -> Result<(), HostContractError> {
        let blocked = self.state == HostRunState::Blocked;
        if blocked != self.blocker.is_some() {
            return Err(HostContractError::InvalidRequest);
        }
        Ok(())
    }
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const DIGEST_A: &str = "ab0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd";
    const DIGEST_B: &str = "cd0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd";

    fn sample_request() -> HostRequest {
        HostRequest {
            protocol_version: HOST_PROTOCOL_VERSION,
            client_id: ClientId::new("host-app-1").expect("valid fixture"),
            repository: RepositoryId::new("repo-1").expect("valid fixture"),
            run: Some(RunId::new("run-1").expect("valid fixture")),
            task: Some(TaskId::new("task-1").expect("valid fixture")),
            source_commit: COMMIT.to_owned(),
            task_source_digest: DIGEST_A.to_owned(),
            authority_policy_digest: DIGEST_B.to_owned(),
            request_id: RequestId::new("req-1").expect("valid fixture"),
            expected_state_revision: 7,
            operation: HostOperation::SubmitJob,
        }
    }

    #[test]
    fn valid_request_verifies_and_round_trips() {
        let request = sample_request();
        assert!(request.verify().is_ok());
        assert!(request.is_fresh_against(7));
        assert!(!request.is_fresh_against(8));
        let encoded = serde_json::to_string(&request).expect("valid fixture");
        let decoded: HostRequest = serde_json::from_str(&encoded).expect("valid fixture");
        assert_eq!(decoded, request);
    }

    #[test]
    fn rejects_unknown_fields_before_any_effect() {
        let encoded = format!(
            "{},\"unknown\":1}}",
            serde_json::to_string(&sample_request())
                .expect("valid fixture")
                .trim_end_matches('}')
        );
        let decoded = serde_json::from_str::<HostRequest>(&encoded);
        assert!(decoded.is_err());
    }

    #[test]
    fn rejects_foreign_protocol_version() {
        let mut request = sample_request();
        request.protocol_version = HOST_PROTOCOL_VERSION + 1;
        assert_eq!(request.verify(), Err(HostContractError::UnsupportedVersion));
        let capability = HostCapability {
            protocol_version: HOST_PROTOCOL_VERSION + 1,
            operations: vec![HostOperation::Pause],
        };
        assert_eq!(
            capability.verify(),
            Err(HostContractError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_malformed_commit_and_digests() {
        for (commit, source, policy) in [
            ("short".to_owned(), DIGEST_A.to_owned(), DIGEST_B.to_owned()),
            (COMMIT.to_owned(), "short".to_owned(), DIGEST_B.to_owned()),
            (COMMIT.to_owned(), DIGEST_A.to_owned(), "short".to_owned()),
            (
                COMMIT.to_owned(),
                DIGEST_A.to_ascii_uppercase(),
                DIGEST_B.to_owned(),
            ),
            (
                COMMIT.to_owned(),
                format!("{DIGEST_A}x"),
                DIGEST_B.to_owned(),
            ),
        ] {
            let mut request = sample_request();
            request.source_commit = commit.clone();
            request.task_source_digest = source.clone();
            request.authority_policy_digest = policy.clone();
            assert_eq!(
                request.verify(),
                Err(HostContractError::InvalidRequest),
                "{commit}/{source}/{policy}"
            );
        }
    }

    #[test]
    fn rejects_malformed_identity_text() {
        assert!(ClientId::new("../escape").is_err());
        assert!(RequestId::new("").is_err());
        let encoded = serde_json::to_string(&sample_request()).expect("valid fixture");
        let hostile = encoded.replace("repo-1", "repo/../escape");
        assert!(serde_json::from_str::<HostRequest>(&hostile).is_err());
    }

    #[test]
    fn capability_accepts_distinct_operations() {
        let capability = HostCapability {
            protocol_version: HOST_PROTOCOL_VERSION,
            operations: vec![HostOperation::SubmitJob, HostOperation::Cancel],
        };
        assert!(capability.verify().is_ok());
        let encoded = serde_json::to_string(&capability).expect("valid fixture");
        let decoded: HostCapability = serde_json::from_str(&encoded).expect("valid fixture");
        assert_eq!(decoded, capability);
    }

    #[test]
    fn rejects_empty_duplicate_and_oversized_capabilities() {
        let empty = HostCapability {
            protocol_version: HOST_PROTOCOL_VERSION,
            operations: Vec::new(),
        };
        assert_eq!(empty.verify(), Err(HostContractError::InvalidRequest));
        let duplicate = HostCapability {
            protocol_version: HOST_PROTOCOL_VERSION,
            operations: vec![HostOperation::Pause, HostOperation::Pause],
        };
        assert_eq!(duplicate.verify(), Err(HostContractError::InvalidRequest));
        let oversized = HostCapability {
            protocol_version: HOST_PROTOCOL_VERSION,
            operations: vec![HostOperation::Pause; MAX_HOST_OPERATIONS + 1],
        };
        assert_eq!(oversized.verify(), Err(HostContractError::InvalidRequest));
    }

    #[test]
    fn control_refuses_stale_revision() {
        let control = HostControl {
            request_id: RequestId::new("req-9").expect("valid fixture"),
            run: RunId::new("run-9").expect("valid fixture"),
            expected_state_revision: 3,
            operation: HostControlOperation::Cancel,
        };
        assert!(control.verify_against(3).is_ok());
        assert_eq!(
            control.verify_against(4),
            Err(HostContractError::StaleRevision)
        );
    }

    #[test]
    fn status_requires_blocker_pairing() {
        let blocked = HostStatus {
            request_id: RequestId::new("req-2").expect("valid fixture"),
            run: RunId::new("run-2").expect("valid fixture"),
            state: HostRunState::Blocked,
            state_revision: 5,
            blocker: Some(HostBlocker::External),
        };
        assert!(blocked.verify().is_ok());
        let missing = HostStatus {
            blocker: None,
            ..blocked.clone()
        };
        assert_eq!(missing.verify(), Err(HostContractError::InvalidRequest));
        let stray = HostStatus {
            state: HostRunState::Active,
            ..blocked.clone()
        };
        assert_eq!(stray.verify(), Err(HostContractError::InvalidRequest));
    }

    #[test]
    fn state_revision_freshness_is_deterministic() {
        let request = sample_request();
        assert!(request.is_fresh_against(request.expected_state_revision));
        assert!(!request.is_fresh_against(request.expected_state_revision + 1));
    }

    #[test]
    fn host_error_codes_are_stable() {
        assert_eq!(
            HostContractError::InvalidRequest.to_string(),
            "codingmage.host.invalid_request"
        );
        assert_eq!(
            HostContractError::UnsupportedVersion.to_string(),
            "codingmage.host.unsupported_version"
        );
        assert_eq!(
            HostContractError::StaleRevision.to_string(),
            "codingmage.host.stale_revision"
        );
    }
}
