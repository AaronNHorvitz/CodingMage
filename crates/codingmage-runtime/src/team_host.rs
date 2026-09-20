//! Typed host admission boundary (local preparation).
//!
//! Admitted host requests resolve to exactly three effect kinds against the
//! existing coordinator machinery: job submission descriptors, read-only
//! observation, and the shared [`CampaignControlAction`] set already backing
//! operator controls and recovery. Standalone CLI behavior is untouched: this
//! module only classifies validated requests against an explicit pinned
//! operator grant and performs no state-changing effect itself. Durable
//! request dispositions, event cursors, and restart reconciliation attach in
//! later stages without changing these rules.

use codingmage_contracts::{ClientId, HostContractError, HostOperation, HostRequest, RepositoryId};

use super::campaign_state::CampaignControlAction;

/// Pinned operator grant a host request is admitted against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostAdmissionPolicy {
    client: ClientId,
    repository: RepositoryId,
    task_source_digest: String,
    authority_policy_digest: String,
    operations: Vec<HostOperation>,
}

/// Effect kind resolved for one admitted host request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostEffectKind {
    /// Bounded job submission descriptor; execution needs later stages.
    Submit,
    /// Read-only status or event observation.
    Observe,
    /// One shared coordinator control action.
    Control(CampaignControlAction),
}

/// A host request admitted against the pinned policy with its effect kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedHostRequest {
    /// The validated admitted request.
    pub request: HostRequest,
    /// The resolved effect kind.
    pub effect: HostEffectKind,
}

impl HostAdmissionPolicy {
    /// Pins an operator grant, refusing malformed digests and operation sets.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::InvalidRequest`] for a malformed digest,
    /// an empty operation set, or a duplicated operation.
    pub fn new(
        client: ClientId,
        repository: RepositoryId,
        task_source_digest: String,
        authority_policy_digest: String,
        operations: Vec<HostOperation>,
    ) -> Result<Self, HostContractError> {
        if !valid_digest(&task_source_digest) || !valid_digest(&authority_policy_digest) {
            return Err(HostContractError::InvalidRequest);
        }
        if operations.is_empty() {
            return Err(HostContractError::InvalidRequest);
        }
        let mut seen: Vec<HostOperation> = Vec::with_capacity(operations.len());
        for operation in &operations {
            if seen.contains(operation) {
                return Err(HostContractError::InvalidRequest);
            }
            seen.push(*operation);
        }
        Ok(Self {
            client,
            repository,
            task_source_digest,
            authority_policy_digest,
            operations,
        })
    }

    /// Admits one request against the pinned grant and resolves its effect.
    ///
    /// Source-commit shape is validated; exact matching against live
    /// coordinator source attaches with the coordinator connection in a later
    /// stage. No state changes and no process, worktree, or remote effects
    /// occur here.
    ///
    /// # Errors
    ///
    /// Returns the exact refusal for foreign versions, unauthorized peers,
    /// cross-project identities, digest mismatches, stale revisions, and
    /// operations outside the grant.
    pub fn admit(
        &self,
        request: &HostRequest,
        current_revision: u64,
    ) -> Result<AdmittedHostRequest, HostContractError> {
        request.verify()?;
        if request.client_id != self.client {
            return Err(HostContractError::UnauthorizedPeer);
        }
        if request.repository != self.repository {
            return Err(HostContractError::CrossProject);
        }
        if request.task_source_digest != self.task_source_digest
            || request.authority_policy_digest != self.authority_policy_digest
        {
            return Err(HostContractError::InvalidRequest);
        }
        if !request.is_fresh_against(current_revision) {
            return Err(HostContractError::StaleRevision);
        }
        if !self.operations.contains(&request.operation) {
            return Err(HostContractError::WidenedScope);
        }
        let effect = match request.operation {
            HostOperation::SubmitJob => HostEffectKind::Submit,
            HostOperation::ReportStatus | HostOperation::ObserveEvents => HostEffectKind::Observe,
            HostOperation::Pause => HostEffectKind::Control(CampaignControlAction::Pause),
            HostOperation::Resume => HostEffectKind::Control(CampaignControlAction::Resume),
            HostOperation::StopAfterUnit => {
                HostEffectKind::Control(CampaignControlAction::StopAfterUnit)
            }
            HostOperation::Cancel => HostEffectKind::Control(CampaignControlAction::Cancel),
        };
        Ok(AdmittedHostRequest {
            request: request.clone(),
            effect,
        })
    }
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
    use codingmage_contracts::{HOST_PROTOCOL_VERSION, RequestId, RunId, TaskId};

    const COMMIT: &str = "dddddddddddddddddddddddddddddddddddddddd";
    const TASK_DIGEST: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    const POLICY_DIGEST: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    fn policy() -> HostAdmissionPolicy {
        HostAdmissionPolicy::new(
            ClientId::new("host-client-1").expect("valid fixture"),
            RepositoryId::new("host-repo-1").expect("valid fixture"),
            TASK_DIGEST.to_owned(),
            POLICY_DIGEST.to_owned(),
            vec![
                HostOperation::SubmitJob,
                HostOperation::ReportStatus,
                HostOperation::ObserveEvents,
                HostOperation::Pause,
                HostOperation::Resume,
                HostOperation::StopAfterUnit,
                HostOperation::Cancel,
            ],
        )
        .expect("valid fixture")
    }

    fn request(operation: HostOperation) -> HostRequest {
        HostRequest {
            protocol_version: HOST_PROTOCOL_VERSION,
            client_id: ClientId::new("host-client-1").expect("valid fixture"),
            repository: RepositoryId::new("host-repo-1").expect("valid fixture"),
            run: Some(RunId::new("host-run-1").expect("valid fixture")),
            task: Some(TaskId::new("host-task-1").expect("valid fixture")),
            source_commit: COMMIT.to_owned(),
            task_source_digest: TASK_DIGEST.to_owned(),
            authority_policy_digest: POLICY_DIGEST.to_owned(),
            request_id: RequestId::new("host-req-1").expect("valid fixture"),
            expected_state_revision: 21,
            operation,
        }
    }

    #[test]
    fn admits_submit_observe_and_every_control() {
        let policy = policy();
        assert_eq!(
            policy
                .admit(&request(HostOperation::SubmitJob), 21)
                .expect("valid fixture")
                .effect,
            HostEffectKind::Submit
        );
        assert_eq!(
            policy
                .admit(&request(HostOperation::ReportStatus), 21)
                .expect("valid fixture")
                .effect,
            HostEffectKind::Observe
        );
        assert_eq!(
            policy
                .admit(&request(HostOperation::ObserveEvents), 21)
                .expect("valid fixture")
                .effect,
            HostEffectKind::Observe
        );
        for (operation, action) in [
            (HostOperation::Pause, CampaignControlAction::Pause),
            (HostOperation::Resume, CampaignControlAction::Resume),
            (
                HostOperation::StopAfterUnit,
                CampaignControlAction::StopAfterUnit,
            ),
            (HostOperation::Cancel, CampaignControlAction::Cancel),
        ] {
            assert_eq!(
                policy
                    .admit(&request(operation), 21)
                    .expect("valid fixture")
                    .effect,
                HostEffectKind::Control(action),
                "{operation:?}"
            );
        }
    }

    #[test]
    fn refuses_foreign_version_unauthorized_peer_and_cross_project() {
        let policy = policy();
        let mut version = request(HostOperation::SubmitJob);
        version.protocol_version = HOST_PROTOCOL_VERSION + 1;
        assert_eq!(
            policy.admit(&version, 21),
            Err(HostContractError::UnsupportedVersion)
        );
        let mut peer = request(HostOperation::SubmitJob);
        peer.client_id = ClientId::new("intruder-1").expect("valid fixture");
        assert_eq!(
            policy.admit(&peer, 21),
            Err(HostContractError::UnauthorizedPeer)
        );
        let mut project = request(HostOperation::SubmitJob);
        project.repository = RepositoryId::new("other-repo-1").expect("valid fixture");
        assert_eq!(
            policy.admit(&project, 21),
            Err(HostContractError::CrossProject)
        );
    }

    #[test]
    fn refuses_digest_mismatch_stale_revision_and_widened_scope() {
        let policy = policy();
        let mut digests = request(HostOperation::SubmitJob);
        digests.task_source_digest = POLICY_DIGEST.to_owned();
        assert_eq!(
            policy.admit(&digests, 21),
            Err(HostContractError::InvalidRequest)
        );
        assert_eq!(
            policy.admit(&request(HostOperation::SubmitJob), 22),
            Err(HostContractError::StaleRevision)
        );
        let narrow = HostAdmissionPolicy::new(
            ClientId::new("host-client-1").expect("valid fixture"),
            RepositoryId::new("host-repo-1").expect("valid fixture"),
            TASK_DIGEST.to_owned(),
            POLICY_DIGEST.to_owned(),
            vec![HostOperation::ReportStatus],
        )
        .expect("valid fixture");
        assert_eq!(
            narrow.admit(&request(HostOperation::Cancel), 21),
            Err(HostContractError::WidenedScope)
        );
    }

    #[test]
    fn refuses_malformed_policy_grants() {
        let client = ClientId::new("host-client-1").expect("valid fixture");
        let repository = RepositoryId::new("host-repo-1").expect("valid fixture");
        assert_eq!(
            HostAdmissionPolicy::new(
                client.clone(),
                repository.clone(),
                "short".to_owned(),
                POLICY_DIGEST.to_owned(),
                vec![HostOperation::Pause],
            ),
            Err(HostContractError::InvalidRequest)
        );
        assert_eq!(
            HostAdmissionPolicy::new(
                client.clone(),
                repository.clone(),
                TASK_DIGEST.to_owned(),
                POLICY_DIGEST.to_owned(),
                Vec::new(),
            ),
            Err(HostContractError::InvalidRequest)
        );
        assert_eq!(
            HostAdmissionPolicy::new(
                client,
                repository,
                TASK_DIGEST.to_owned(),
                POLICY_DIGEST.to_owned(),
                vec![HostOperation::Pause, HostOperation::Pause],
            ),
            Err(HostContractError::InvalidRequest)
        );
    }

    #[test]
    fn admitted_request_preserves_exact_identities() {
        let admitted = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        assert_eq!(admitted.request.request_id.as_str(), "host-req-1");
        assert_eq!(
            admitted.request.run.as_ref().map(RunId::as_str),
            Some("host-run-1")
        );
    }
}
