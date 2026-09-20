//! Fake host-client fixtures proving the contract refusal matrix.
//!
//! This file is local preparation only: a simulated host peer drives framed
//! byte streams at the contract layer, and a fake admission predicate mirrors
//! the exact rules the coordinator will enforce for real in a later stage.
//! Nothing here admits a live peer or performs an effect.

use codingmage_contracts::{
    ClientId, HostBlocker, HostCapability, HostContractError, HostControl, HostControlOperation,
    HostOperation, HostRequest, HostRunState, HostStatus, RepositoryId, RequestId, RunId, TaskId,
    TransportLimits, decode_frame, encode_frame,
};

const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TASK_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const POLICY_DIGEST: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// Closed refusal classification mirroring the future coordinator rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeRefusal {
    /// Framing or JSON structure is unusable.
    Malformed,
    /// Protocol version is foreign.
    Version,
    /// Schema, digest, or pairing check failed.
    Schema,
    /// Control or request targets a stale revision.
    Stale,
    /// Client identity is not the admitted one.
    UnauthorizedPeer,
    /// Operation is outside the admitted set.
    WidenedScope,
    /// Repository identity does not match the admitted project.
    CrossProject,
}

/// Fake admitted authority pinned for the whole fixture.
struct FakeAuthority {
    client: ClientId,
    repository: RepositoryId,
    task_digest: String,
    policy_digest: String,
    revision: u64,
    operations: Vec<HostOperation>,
}

impl FakeAuthority {
    fn pinned() -> Self {
        Self {
            client: ClientId::new("fake-host-1").expect("valid fixture"),
            repository: RepositoryId::new("fake-repo-1").expect("valid fixture"),
            task_digest: TASK_DIGEST.to_owned(),
            policy_digest: POLICY_DIGEST.to_owned(),
            revision: 11,
            operations: vec![HostOperation::SubmitJob, HostOperation::ReportStatus],
        }
    }

    fn limits() -> TransportLimits {
        TransportLimits::default_limits()
    }

    fn request(&self, operation: HostOperation) -> HostRequest {
        HostRequest {
            protocol_version: codingmage_contracts::HOST_PROTOCOL_VERSION,
            client_id: self.client.clone(),
            repository: self.repository.clone(),
            run: None,
            task: None,
            source_commit: COMMIT.to_owned(),
            task_source_digest: self.task_digest.clone(),
            authority_policy_digest: self.policy_digest.clone(),
            request_id: RequestId::new("fake-req-1").expect("valid fixture"),
            expected_state_revision: self.revision,
            operation,
        }
    }

    fn admit_bytes(&self, stream: &[u8]) -> Result<HostRequest, FakeRefusal> {
        let (payload, _) =
            decode_frame(stream, &Self::limits()).map_err(|_| FakeRefusal::Malformed)?;
        let request: HostRequest =
            serde_json::from_slice(&payload).map_err(|_| FakeRefusal::Malformed)?;
        match request.verify() {
            Ok(()) => {}
            Err(HostContractError::UnsupportedVersion) => return Err(FakeRefusal::Version),
            Err(_) => return Err(FakeRefusal::Schema),
        }
        if request.client_id != self.client {
            return Err(FakeRefusal::UnauthorizedPeer);
        }
        if request.repository != self.repository {
            return Err(FakeRefusal::CrossProject);
        }
        if request.task_source_digest != self.task_digest
            || request.authority_policy_digest != self.policy_digest
        {
            return Err(FakeRefusal::Schema);
        }
        if !request.is_fresh_against(self.revision) {
            return Err(FakeRefusal::Stale);
        }
        if !self.operations.contains(&request.operation) {
            return Err(FakeRefusal::WidenedScope);
        }
        Ok(request)
    }

    fn admit_control(
        &self,
        control: &HostControl,
        run: &RunId,
    ) -> Result<HostControlOperation, FakeRefusal> {
        if &control.run != run {
            return Err(FakeRefusal::CrossProject);
        }
        match control.verify_against(self.revision) {
            Ok(()) => Ok(control.operation),
            Err(HostContractError::StaleRevision) => Err(FakeRefusal::Stale),
            Err(_) => Err(FakeRefusal::Schema),
        }
    }
}

fn frame_json(value: &serde_json::Value) -> Vec<u8> {
    let payload = serde_json::to_vec(value).expect("valid fixture");
    encode_frame(&payload, &FakeAuthority::limits()).expect("valid fixture")
}

fn request_value(authority: &FakeAuthority, operation: HostOperation) -> serde_json::Value {
    serde_json::to_value(authority.request(operation)).expect("valid fixture")
}

#[test]
fn fake_client_valid_job_is_admitted() {
    let authority = FakeAuthority::pinned();
    let stream = frame_json(&request_value(&authority, HostOperation::SubmitJob));
    let admitted = authority.admit_bytes(&stream).expect("valid fixture");
    assert_eq!(admitted.operation, HostOperation::SubmitJob);
}

#[test]
fn fake_client_foreign_version_is_refused() {
    let authority = FakeAuthority::pinned();
    let mut value = request_value(&authority, HostOperation::SubmitJob);
    value["protocol_version"] = serde_json::Value::from(999);
    assert_eq!(
        authority.admit_bytes(&frame_json(&value)),
        Err(FakeRefusal::Version)
    );
}

#[test]
fn fake_client_malformed_messages_are_refused() {
    let authority = FakeAuthority::pinned();
    let truncated = &frame_json(&request_value(&authority, HostOperation::SubmitJob))[..3];
    assert_eq!(
        authority.admit_bytes(truncated),
        Err(FakeRefusal::Malformed)
    );
    let mut unknown = request_value(&authority, HostOperation::SubmitJob);
    unknown["future_field"] = serde_json::Value::from(true);
    assert_eq!(
        authority.admit_bytes(&frame_json(&unknown)),
        Err(FakeRefusal::Malformed)
    );
    assert_eq!(
        authority.admit_bytes(b"not-json"),
        Err(FakeRefusal::Malformed)
    );
}

#[test]
fn fake_client_unauthorized_peer_is_refused() {
    let authority = FakeAuthority::pinned();
    let mut value = request_value(&authority, HostOperation::SubmitJob);
    value["client_id"] = serde_json::Value::from("intruder-9");
    assert_eq!(
        authority.admit_bytes(&frame_json(&value)),
        Err(FakeRefusal::UnauthorizedPeer)
    );
}

#[test]
fn fake_client_widened_scope_is_refused() {
    let authority = FakeAuthority::pinned();
    let stream = frame_json(&request_value(&authority, HostOperation::Cancel));
    assert_eq!(
        authority.admit_bytes(&stream),
        Err(FakeRefusal::WidenedScope)
    );
}

#[test]
fn fake_client_cross_project_identity_is_refused() {
    let authority = FakeAuthority::pinned();
    let mut value = request_value(&authority, HostOperation::SubmitJob);
    value["repository"] = serde_json::Value::from("other-project-9");
    assert_eq!(
        authority.admit_bytes(&frame_json(&value)),
        Err(FakeRefusal::CrossProject)
    );
    let run = RunId::new("run-owned").expect("valid fixture");
    let other = RunId::new("run-foreign").expect("valid fixture");
    let control = HostControl {
        request_id: RequestId::new("fake-req-2").expect("valid fixture"),
        run: other,
        expected_state_revision: authority.revision,
        operation: HostControlOperation::Cancel,
    };
    assert_eq!(
        authority.admit_control(&control, &run),
        Err(FakeRefusal::CrossProject)
    );
}

#[test]
fn fake_client_stale_controls_are_refused() {
    let authority = FakeAuthority::pinned();
    let run = RunId::new("run-owned").expect("valid fixture");
    let stale = HostControl {
        request_id: RequestId::new("fake-req-3").expect("valid fixture"),
        run: run.clone(),
        expected_state_revision: authority.revision - 1,
        operation: HostControlOperation::Pause,
    };
    assert_eq!(
        authority.admit_control(&stale, &run),
        Err(FakeRefusal::Stale)
    );
    let fresh = HostControl {
        expected_state_revision: authority.revision,
        ..stale
    };
    assert_eq!(
        authority.admit_control(&fresh, &run),
        Ok(HostControlOperation::Pause)
    );
}

#[test]
fn fake_client_stale_request_revision_is_refused() {
    let authority = FakeAuthority::pinned();
    let mut request = authority.request(HostOperation::ReportStatus);
    request.expected_state_revision = authority.revision + 1;
    let payload = serde_json::to_vec(&request).expect("valid fixture");
    let stream = encode_frame(&payload, &FakeAuthority::limits()).expect("valid fixture");
    assert_eq!(authority.admit_bytes(&stream), Err(FakeRefusal::Stale));
}

#[test]
fn fake_client_digest_mismatch_is_refused() {
    let authority = FakeAuthority::pinned();
    let mut value = request_value(&authority, HostOperation::SubmitJob);
    value["task_source_digest"] = serde_json::Value::from(TASK_DIGEST.replace('b', "c"));
    assert_eq!(
        authority.admit_bytes(&frame_json(&value)),
        Err(FakeRefusal::Schema)
    );
}

#[test]
fn fake_client_status_shapes_verify() {
    let status = HostStatus {
        request_id: RequestId::new("fake-req-4").expect("valid fixture"),
        run: RunId::new("run-owned").expect("valid fixture"),
        state: HostRunState::Blocked,
        state_revision: authority_revision(),
        blocker: Some(HostBlocker::Approval),
    };
    assert!(status.verify().is_ok());
    let capability = HostCapability {
        protocol_version: codingmage_contracts::HOST_PROTOCOL_VERSION,
        operations: vec![HostOperation::SubmitJob],
    };
    assert!(capability.verify().is_ok());
}

fn authority_revision() -> u64 {
    FakeAuthority::pinned().revision
}

#[test]
fn fake_client_envelopes_minimize_content() {
    let authority = FakeAuthority::pinned();
    let mut scoped = authority.request(HostOperation::SubmitJob);
    scoped.run = Some(RunId::new("run-owned").expect("valid fixture"));
    scoped.task = Some(TaskId::new("task-owned").expect("valid fixture"));
    let request = serde_json::to_value(scoped).expect("valid fixture");
    let request_keys: Vec<&str> = request
        .as_object()
        .expect("valid fixture")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        request_keys,
        vec![
            "authority_policy_digest",
            "client_id",
            "expected_state_revision",
            "operation",
            "protocol_version",
            "repository",
            "request_id",
            "run",
            "source_commit",
            "task",
            "task_source_digest",
        ]
    );
    let status = serde_json::to_value(HostStatus {
        request_id: RequestId::new("fake-req-5").expect("valid fixture"),
        run: RunId::new("run-owned").expect("valid fixture"),
        state: HostRunState::Active,
        state_revision: 6,
        blocker: None,
    })
    .expect("valid fixture");
    let status_keys: Vec<&str> = status
        .as_object()
        .expect("valid fixture")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        status_keys,
        vec!["request_id", "run", "state", "state_revision"]
    );
    let secret = "super-secret-payload-bytes";
    let encoded = format!(
        "{secret}{}",
        serde_json::to_string(&status).expect("valid fixture")
    );
    for refusal in [
        HostContractError::InvalidRequest,
        HostContractError::UnsupportedVersion,
        HostContractError::StaleRevision,
    ] {
        assert!(!refusal.to_string().contains(secret), "{refusal:?}");
    }
    assert!(!encoded.contains("blocker"));
}
