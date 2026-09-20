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

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_contracts::{
    ClientId, HostContractError, HostOperation, HostRequest, RepositoryId, RequestId, RunId,
};
use codingmage_state::{IntegrityDocument, IntegrityDocumentError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::campaign_state::CampaignControlAction;

/// Durable host-disposition document name.
const DISPOSITION_DOCUMENT: &str = "host-dispositions.json";

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
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

/// Terminal outcome recorded for one request identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionOutcome {
    /// Admitted; the effect outcome is still uncertain (crash window).
    Accepted,
    /// Effect completed with the recorded result digest.
    Completed {
        /// Lowercase SHA-256 of the canonical result summary.
        result_digest: String,
    },
    /// Request refused with the stable refusal code.
    Refused {
        /// Stable refusal code, never a prompt or prose.
        code: String,
    },
}

/// Durable record for one request identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostDisposition {
    /// Request identity this record belongs to.
    pub request_id: RequestId,
    /// SHA-256 of the canonical admitted request JSON.
    pub request_digest: String,
    /// Resolved effect kind.
    pub effect: HostEffectKind,
    /// Recorded outcome.
    pub outcome: DispositionOutcome,
}

/// Replay decision for one request identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispositionDecision {
    /// No record exists: the caller may execute the effect once.
    Proceed(HostEffectKind),
    /// An identical retry: return the stored outcome without re-executing.
    Replay(DispositionOutcome),
}

/// Stable disposition-store error.
#[derive(Debug, Eq, PartialEq)]
pub enum HostDispositionError {
    /// Durable document I/O failed.
    Document(IntegrityDocumentError),
    /// A stored or supplied value was malformed.
    Malformed,
    /// The identity is already bound to a different request.
    ConflictingReuse,
    /// The recorded outcome is still uncertain; reconcile first.
    Uncertain,
    /// No record exists for the identity.
    UnknownRequest,
}

impl fmt::Display for HostDispositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Document(_) => "codingmage.host.disposition.document",
            Self::Malformed => "codingmage.host.disposition.malformed",
            Self::ConflictingReuse => "codingmage.host.disposition.conflicting_reuse",
            Self::Uncertain => "codingmage.host.disposition.uncertain",
            Self::UnknownRequest => "codingmage.host.disposition.unknown_request",
        })
    }
}

impl std::error::Error for HostDispositionError {}

/// Durable idempotency store for host request dispositions.
///
/// Identical retries return prior results without re-executing; conflicting
/// reuse of an identity is refused; uncertain (accepted but unfinished)
/// records block replay until reconciled to a terminal outcome.
pub struct HostDispositionStore {
    root: PathBuf,
    records: BTreeMap<String, HostDisposition>,
}

impl HostDispositionStore {
    /// Opens one exact private store root, loading and verifying any record.
    ///
    /// # Errors
    ///
    /// Returns [`HostDispositionError`] for malformed stored records or
    /// durable I/O behavior.
    pub fn open(root: &Path) -> Result<Self, HostDispositionError> {
        let records = Self::load_document(root)?;
        for record in records.values() {
            record
                .verify()
                .map_err(|_| HostDispositionError::Malformed)?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            records,
        })
    }

    /// Records admission or replays the stored disposition.
    ///
    /// # Errors
    ///
    /// Returns [`HostDispositionError::ConflictingReuse`] when the identity
    /// is bound to a different request, [`HostDispositionError::Uncertain`]
    /// when the stored outcome still needs reconciliation, and document
    /// errors for durable I/O behavior.
    pub fn record_admitted(
        &mut self,
        request: &HostRequest,
        effect: HostEffectKind,
    ) -> Result<DispositionDecision, HostDispositionError> {
        let digest = request_digest(request).map_err(|_| HostDispositionError::Malformed)?;
        let key = request.request_id.as_str().to_owned();
        if let Some(stored) = self.records.get(&key) {
            if stored.request_digest != digest {
                return Err(HostDispositionError::ConflictingReuse);
            }
            return match &stored.outcome {
                DispositionOutcome::Accepted => Err(HostDispositionError::Uncertain),
                outcome => Ok(DispositionDecision::Replay(outcome.clone())),
            };
        }
        let record = HostDisposition {
            request_id: request.request_id.clone(),
            request_digest: digest,
            effect,
            outcome: DispositionOutcome::Accepted,
        };
        record
            .verify()
            .map_err(|_| HostDispositionError::Malformed)?;
        self.records.insert(key, record);
        self.persist()?;
        Ok(DispositionDecision::Proceed(effect))
    }

    /// Reconciles one uncertain record to a terminal outcome.
    ///
    /// Repeating the stored outcome is observational. Any other transition
    /// out of a terminal outcome is refused: settled history never changes.
    ///
    /// # Errors
    ///
    /// Returns [`HostDispositionError::UnknownRequest`] for a missing
    /// identity, [`HostDispositionError::Malformed`] for an invalid
    /// transition, and document errors for durable I/O behavior.
    pub fn reconcile(
        &mut self,
        request_id: &RequestId,
        outcome: DispositionOutcome,
    ) -> Result<(), HostDispositionError> {
        if outcome == DispositionOutcome::Accepted {
            return Err(HostDispositionError::Malformed);
        }
        let key = request_id.as_str().to_owned();
        let Some(record) = self.records.get_mut(&key) else {
            return Err(HostDispositionError::UnknownRequest);
        };
        if record.outcome == outcome {
            return Ok(());
        }
        if record.outcome != DispositionOutcome::Accepted {
            return Err(HostDispositionError::Malformed);
        }
        record.outcome = outcome;
        record
            .verify()
            .map_err(|_| HostDispositionError::Malformed)?;
        self.persist()
    }

    fn persist(&self) -> Result<(), HostDispositionError> {
        IntegrityDocument::write_atomic(
            &self.root,
            DISPOSITION_DOCUMENT,
            self.records.clone(),
            |records| records.values().all(|record| record.verify().is_ok()),
        )
        .map_err(HostDispositionError::Document)?;
        Ok(())
    }

    fn load_document(
        root: &Path,
    ) -> Result<BTreeMap<String, HostDisposition>, HostDispositionError> {
        let path = root.join(DISPOSITION_DOCUMENT);
        if fs::symlink_metadata(&path).is_err() {
            return Ok(BTreeMap::new());
        }
        IntegrityDocument::<BTreeMap<String, HostDisposition>>::load(
            root,
            DISPOSITION_DOCUMENT,
            |records| records.values().all(|record| record.verify().is_ok()),
        )
        .map(|document| document.payload)
        .map_err(HostDispositionError::Document)
    }
}

impl HostDisposition {
    /// Validates digest shape and outcome coherence.
    ///
    /// # Errors
    ///
    /// Returns [`HostDispositionError::Malformed`] for a malformed digest, a
    /// completed outcome without a digest, or a refused outcome without a code.
    pub fn verify(&self) -> Result<(), HostDispositionError> {
        if !valid_digest(&self.request_digest) {
            return Err(HostDispositionError::Malformed);
        }
        match &self.outcome {
            DispositionOutcome::Accepted => Ok(()),
            DispositionOutcome::Completed { result_digest } => {
                if valid_digest(result_digest) {
                    Ok(())
                } else {
                    Err(HostDispositionError::Malformed)
                }
            }
            DispositionOutcome::Refused { code } => {
                if code.is_empty() || code.len() > 128 {
                    Err(HostDispositionError::Malformed)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Maximum events returned on one page.
pub const MAX_HOST_EVENT_PAGE: u32 = 1_000;

/// Content-minimized host-visible event kind. Payloads never cross the
/// boundary: observers see kinds, revisions, and content digests only.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostEventKind {
    /// A request was admitted against the pinned grant.
    Admitted,
    /// An effect kind was resolved for an admitted request.
    EffectDecided,
    /// An effect completed with a recorded result digest.
    Completed,
    /// A request was refused with a stable code.
    Refused,
    /// A control was applied to the owned run.
    ControlApplied,
    /// The owned run reached a durable checkpoint.
    Checkpoint,
}

/// One content-minimized host-visible event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostEvent {
    /// Monotonic index within the owned run, starting at zero.
    pub index: u64,
    /// Event kind.
    pub kind: HostEventKind,
    /// Durable state revision at the event.
    pub state_revision: u64,
    /// SHA-256 of the canonical event content held by the coordinator.
    pub digest: String,
}

/// Bounded observation cursor for one owned run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostEventCursor {
    /// Owned run under observation.
    pub run: RunId,
    /// First index wanted; older events are never re-sent.
    pub from_index: u64,
    /// Maximum events wanted, one or more up to [`MAX_HOST_EVENT_PAGE`].
    pub limit: u32,
}

/// One event page with explicit gap signaling.
///
/// A set gap means the cursor pointed past available history (disconnect,
/// compaction, or crash window): the observer must resynchronize from run
/// status at `resync_revision` instead of assuming continuity. Local
/// coordinator controls never depend on this paging and keep working while
/// no host observes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostEventPage {
    /// Events in index order, at most the cursor limit.
    pub events: Vec<HostEvent>,
    /// Cursor index for the next page.
    pub next_from_index: u64,
    /// True when continuity cannot be proven from this page.
    pub gap: bool,
    /// Revision to resynchronize from when `gap` is set.
    pub resync_revision: u64,
}

impl HostEvent {
    /// Validates digest shape. Index and revision ordering is checked by
    /// [`page_host_events`] against the coordinator-held sequence.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::InvalidRequest`] for a malformed digest.
    pub fn verify(&self) -> Result<(), HostContractError> {
        if valid_digest(&self.digest) {
            Ok(())
        } else {
            Err(HostContractError::InvalidRequest)
        }
    }
}

impl HostEventCursor {
    /// Validates the cursor bound.
    ///
    /// # Errors
    ///
    /// Returns [`HostContractError::InvalidRequest`] for a zero limit or a
    /// limit above [`MAX_HOST_EVENT_PAGE`]. Run identity is validated by
    /// construction.
    pub fn verify(&self) -> Result<(), HostContractError> {
        if self.limit == 0 || self.limit > MAX_HOST_EVENT_PAGE {
            return Err(HostContractError::InvalidRequest);
        }
        Ok(())
    }
}

/// Pages coordinator-held events for one cursor with explicit gap behavior.
///
/// # Errors
///
/// Returns [`HostContractError::InvalidRequest`] for an invalid cursor, a
/// run mismatch, a malformed event, or an out-of-order sequence. A cursor
/// past available history is not an error: it returns an empty page with
/// `gap` set so the observer resynchronizes instead of assuming continuity.
pub fn page_host_events(
    events: &[HostEvent],
    run: &RunId,
    cursor: &HostEventCursor,
) -> Result<HostEventPage, HostContractError> {
    cursor.verify()?;
    if &cursor.run != run {
        return Err(HostContractError::InvalidRequest);
    }
    for (position, event) in events.iter().enumerate() {
        event.verify()?;
        let Ok(position) = u64::try_from(position) else {
            return Err(HostContractError::InvalidRequest);
        };
        if event.index != position {
            return Err(HostContractError::InvalidRequest);
        }
    }
    let from = cursor.from_index;
    let len = events.len() as u64;
    if from > len {
        return Ok(HostEventPage {
            events: Vec::new(),
            next_from_index: len,
            gap: true,
            resync_revision: events.last().map_or(0, |event| event.state_revision),
        });
    }
    let Ok(from) = usize::try_from(from) else {
        return Err(HostContractError::InvalidRequest);
    };
    let mut page = Vec::new();
    let mut next = from as u64;
    for event in events.iter().skip(from).take(cursor.limit as usize) {
        page.push(event.clone());
        next = next.saturating_add(1);
    }
    Ok(HostEventPage {
        events: page,
        next_from_index: next,
        gap: false,
        resync_revision: events.last().map_or(0, |event| event.state_revision),
    })
}

/// Returns the SHA-256 of the canonical request JSON.
fn request_digest(request: &HostRequest) -> Result<String, serde_json::Error> {
    let encoded = serde_json::to_vec(request)?;
    Ok(hex_bytes(Sha256::digest(encoded).as_ref()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_contracts::{HOST_PROTOCOL_VERSION, TaskId};

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

    fn store_root() -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("valid fixture")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-host-dispositions-{}-{unique}",
            std::process::id()
        ))
    }

    fn open_store() -> (HostDispositionStore, std::path::PathBuf) {
        let root = store_root();
        let store = HostDispositionStore::open(&root).expect("valid fixture");
        (store, root)
    }

    #[test]
    fn identical_retries_replay_without_reexecuting() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::Pause), 21)
            .expect("valid fixture");
        assert_eq!(
            store
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Proceed(HostEffectKind::Control(CampaignControlAction::Pause))
        );
        store
            .reconcile(
                &admitted.request.request_id,
                DispositionOutcome::Completed {
                    result_digest: TASK_DIGEST.to_owned(),
                },
            )
            .expect("valid fixture");
        assert_eq!(
            store
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(DispositionOutcome::Completed {
                result_digest: TASK_DIGEST.to_owned(),
            })
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn conflicting_reuse_and_uncertain_replay_are_refused() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        let mut conflict = admitted.request.clone();
        conflict.expected_state_revision = 22;
        assert_eq!(
            store.record_admitted(&conflict, admitted.effect),
            Err(HostDispositionError::ConflictingReuse)
        );
        assert_eq!(
            store.record_admitted(&admitted.request, admitted.effect),
            Err(HostDispositionError::Uncertain)
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn reconcile_settles_uncertain_records_exactly_once() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::Cancel), 21)
            .expect("valid fixture");
        let id = admitted.request.request_id.clone();
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        assert_eq!(
            store.reconcile(&id, DispositionOutcome::Accepted),
            Err(HostDispositionError::Malformed)
        );
        assert_eq!(
            store.reconcile(
                &RequestId::new("missing-1").expect("valid fixture"),
                completed()
            ),
            Err(HostDispositionError::UnknownRequest)
        );
        store.reconcile(&id, completed()).expect("valid fixture");
        assert!(store.reconcile(&id, completed()).is_ok());
        assert_eq!(
            store.reconcile(
                &id,
                DispositionOutcome::Refused {
                    code: "other".to_owned(),
                }
            ),
            Err(HostDispositionError::Malformed)
        );
        assert_eq!(
            store
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(completed())
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    fn completed() -> DispositionOutcome {
        DispositionOutcome::Completed {
            result_digest: TASK_DIGEST.to_owned(),
        }
    }

    #[test]
    fn reopened_store_preserves_settled_records() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::ReportStatus), 21)
            .expect("valid fixture");
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        store
            .reconcile(&admitted.request.request_id, completed())
            .expect("valid fixture");
        drop(store);
        let mut reopened = HostDispositionStore::open(&root).expect("valid fixture");
        assert_eq!(
            reopened
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(completed())
        );
        drop(reopened);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn malformed_documents_and_records_are_refused() {
        let root = store_root();
        std::fs::create_dir_all(&root).expect("valid fixture");
        std::fs::write(root.join(DISPOSITION_DOCUMENT), b"{invalid}").expect("valid fixture");
        assert!(matches!(
            HostDispositionStore::open(&root),
            Err(HostDispositionError::Document(_))
        ));
        std::fs::remove_dir_all(&root).expect("valid fixture");
        let bad = HostDisposition {
            request_id: RequestId::new("bad-1").expect("valid fixture"),
            request_digest: "short".to_owned(),
            effect: HostEffectKind::Observe,
            outcome: DispositionOutcome::Accepted,
        };
        assert_eq!(bad.verify(), Err(HostDispositionError::Malformed));
    }

    fn event_sequence(count: u64) -> Vec<super::HostEvent> {
        let mut events = Vec::new();
        for index in 0..count {
            events.push(super::HostEvent {
                index,
                kind: super::HostEventKind::Checkpoint,
                state_revision: 100 + index,
                digest: TASK_DIGEST.to_owned(),
            });
        }
        events
    }

    fn cursor(run: &RunId, from_index: u64, limit: u32) -> super::HostEventCursor {
        super::HostEventCursor {
            run: run.clone(),
            from_index,
            limit,
        }
    }

    #[test]
    fn event_pages_are_bounded_and_chained() {
        let run = RunId::new("cursor-run-1").expect("valid fixture");
        let events = event_sequence(5);
        let first =
            super::page_host_events(&events, &run, &cursor(&run, 0, 2)).expect("valid fixture");
        assert_eq!(first.events.len(), 2);
        assert!(!first.gap);
        let second =
            super::page_host_events(&events, &run, &cursor(&run, first.next_from_index, 10))
                .expect("valid fixture");
        assert_eq!(second.events.len(), 3);
        assert_eq!(second.next_from_index, 5);
        assert!(!second.gap);
        let drained =
            super::page_host_events(&events, &run, &cursor(&run, 5, 10)).expect("valid fixture");
        assert!(drained.events.is_empty());
        assert!(!drained.gap);
    }

    #[test]
    fn past_history_cursors_signal_explicit_gaps() {
        let run = RunId::new("cursor-run-2").expect("valid fixture");
        let events = event_sequence(3);
        let page =
            super::page_host_events(&events, &run, &cursor(&run, 9, 10)).expect("valid fixture");
        assert!(page.events.is_empty());
        assert!(page.gap);
        assert_eq!(page.next_from_index, 3);
        assert_eq!(page.resync_revision, 102);
    }

    #[test]
    fn malformed_cursors_sequences_and_runs_are_refused() {
        let run = RunId::new("cursor-run-3").expect("valid fixture");
        let other = RunId::new("cursor-run-4").expect("valid fixture");
        let events = event_sequence(2);
        assert_eq!(
            super::page_host_events(&events, &run, &cursor(&run, 0, 0)),
            Err(HostContractError::InvalidRequest)
        );
        assert_eq!(
            super::page_host_events(
                &events,
                &run,
                &cursor(&run, 0, super::MAX_HOST_EVENT_PAGE + 1)
            ),
            Err(HostContractError::InvalidRequest)
        );
        assert_eq!(
            super::page_host_events(&events, &other, &cursor(&run, 0, 10)),
            Err(HostContractError::InvalidRequest)
        );
        let mut broken = event_sequence(2);
        broken[1].digest = "short".to_owned();
        assert_eq!(
            super::page_host_events(&broken, &run, &cursor(&run, 0, 10)),
            Err(HostContractError::InvalidRequest)
        );
        let mut unordered = event_sequence(2);
        unordered[1].index = 7;
        assert_eq!(
            super::page_host_events(&unordered, &run, &cursor(&run, 0, 10)),
            Err(HostContractError::InvalidRequest)
        );
    }

    #[test]
    fn local_controls_need_no_host_session() {
        use super::super::campaign_state::{CampaignControlAction, CampaignControlIntent};
        let intent = CampaignControlIntent::new(
            "local-control-1".to_owned(),
            TASK_DIGEST.to_owned(),
            "campaign-1".to_owned(),
            "repo-1".to_owned(),
            RunId::new("run-1").expect("valid fixture"),
            CampaignControlAction::Pause,
            COMMIT.to_owned(),
            1,
        )
        .expect("valid fixture");
        assert_eq!(intent.action.code(), "pause");
        assert_eq!(
            CampaignControlAction::parse("pause"),
            Some(CampaignControlAction::Pause)
        );
    }

    fn effects() -> Vec<super::HostEffectKind> {
        vec![
            super::HostEffectKind::Submit,
            super::HostEffectKind::Observe,
            super::HostEffectKind::Control(super::CampaignControlAction::Cancel),
        ]
    }

    #[test]
    fn crash_window_reopens_as_uncertain_then_replays() {
        for effect in effects() {
            let operation = match effect {
                super::HostEffectKind::Submit => HostOperation::SubmitJob,
                super::HostEffectKind::Observe => HostOperation::ReportStatus,
                super::HostEffectKind::Control(_) => HostOperation::Cancel,
            };
            let (mut store, root) = open_store();
            let admitted = policy()
                .admit(&request(operation), 21)
                .expect("valid fixture");
            assert!(matches!(
                store.record_admitted(&admitted.request, admitted.effect),
                Ok(DispositionDecision::Proceed(_))
            ));
            drop(store);
            let mut restarted = super::HostDispositionStore::open(&root).expect("valid fixture");
            assert_eq!(
                restarted.record_admitted(&admitted.request, admitted.effect),
                Err(HostDispositionError::Uncertain),
                "{effect:?}"
            );
            restarted
                .reconcile(&admitted.request.request_id, completed())
                .expect("valid fixture");
            assert_eq!(
                restarted
                    .record_admitted(&admitted.request, admitted.effect)
                    .expect("valid fixture"),
                DispositionDecision::Replay(completed()),
                "{effect:?}"
            );
            drop(restarted);
            std::fs::remove_dir_all(root).expect("valid fixture");
        }
    }

    #[test]
    fn authority_revocation_narrows_admission() {
        let admitted = policy()
            .admit(&request(HostOperation::Pause), 21)
            .expect("valid fixture");
        let narrowed = HostAdmissionPolicy::new(
            ClientId::new("host-client-1").expect("valid fixture"),
            RepositoryId::new("host-repo-1").expect("valid fixture"),
            TASK_DIGEST.to_owned(),
            POLICY_DIGEST.to_owned(),
            vec![HostOperation::ReportStatus],
        )
        .expect("valid fixture");
        assert_eq!(
            narrowed.admit(&admitted.request, 21),
            Err(HostContractError::WidenedScope)
        );
        let rotated = HostAdmissionPolicy::new(
            ClientId::new("host-client-1").expect("valid fixture"),
            RepositoryId::new("host-repo-1").expect("valid fixture"),
            POLICY_DIGEST.to_owned(),
            POLICY_DIGEST.to_owned(),
            vec![HostOperation::Pause],
        )
        .expect("valid fixture");
        assert_eq!(
            rotated.admit(&admitted.request, 21),
            Err(HostContractError::InvalidRequest)
        );
    }

    #[test]
    fn duplicate_and_conflicting_retries_survive_restart() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        store
            .reconcile(&admitted.request.request_id, completed())
            .expect("valid fixture");
        drop(store);
        let mut restarted = super::HostDispositionStore::open(&root).expect("valid fixture");
        assert_eq!(
            restarted
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(completed())
        );
        let mut conflict = admitted.request.clone();
        conflict.task_source_digest = POLICY_DIGEST.to_owned();
        assert_eq!(
            restarted.record_admitted(&conflict, admitted.effect),
            Err(HostDispositionError::ConflictingReuse)
        );
        drop(restarted);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn stale_cancellation_reconciles_as_refused_not_replayed() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::Cancel), 21)
            .expect("valid fixture");
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        drop(store);
        let mut restarted = super::HostDispositionStore::open(&root).expect("valid fixture");
        let refused = DispositionOutcome::Refused {
            code: "codingmage.host.stale_revision".to_owned(),
        };
        restarted
            .reconcile(&admitted.request.request_id, refused.clone())
            .expect("valid fixture");
        assert_eq!(
            restarted
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(refused)
        );
        drop(restarted);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn unrelated_store_roots_stay_independent() {
        let (mut first, first_root) = open_store();
        let (mut second, second_root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        assert!(matches!(
            first.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        assert!(matches!(
            second.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        first
            .reconcile(&admitted.request.request_id, completed())
            .expect("valid fixture");
        assert_eq!(
            first
                .record_admitted(&admitted.request, admitted.effect)
                .expect("valid fixture"),
            DispositionDecision::Replay(completed())
        );
        assert_eq!(
            second.record_admitted(&admitted.request, admitted.effect),
            Err(HostDispositionError::Uncertain)
        );
        drop(first);
        drop(second);
        std::fs::remove_dir_all(first_root).expect("valid fixture");
        std::fs::remove_dir_all(second_root).expect("valid fixture");
    }

    fn collect_created_paths(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(directory) = stack.pop() {
            let entries = std::fs::read_dir(&directory).expect("valid fixture");
            for entry in entries {
                let path = entry.expect("valid fixture").path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    }

    #[test]
    fn ac_29_1_admission_is_deterministic_and_contained() {
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        let repeated = policy()
            .admit(&request(HostOperation::SubmitJob), 21)
            .expect("valid fixture");
        assert_eq!(admitted, repeated);
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        assert_eq!(
            collect_created_paths(&root),
            vec![root.join(super::DISPOSITION_DOCUMENT)],
            "admission and recording create exactly one known file"
        );
        drop(store);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn ac_29_2_recovery_reconciles_without_duplicates_or_expansion() {
        let narrowed = HostAdmissionPolicy::new(
            ClientId::new("host-client-1").expect("valid fixture"),
            RepositoryId::new("host-repo-1").expect("valid fixture"),
            TASK_DIGEST.to_owned(),
            POLICY_DIGEST.to_owned(),
            vec![HostOperation::ReportStatus],
        )
        .expect("valid fixture");
        let (mut store, root) = open_store();
        let admitted = policy()
            .admit(&request(HostOperation::Cancel), 21)
            .expect("valid fixture");
        assert!(matches!(
            store.record_admitted(&admitted.request, admitted.effect),
            Ok(DispositionDecision::Proceed(_))
        ));
        drop(store);
        let mut restarted = super::HostDispositionStore::open(&root).expect("valid fixture");
        assert_eq!(
            restarted.record_admitted(&admitted.request, admitted.effect),
            Err(HostDispositionError::Uncertain)
        );
        restarted
            .reconcile(&admitted.request.request_id, completed())
            .expect("valid fixture");
        let first = restarted
            .record_admitted(&admitted.request, admitted.effect)
            .expect("valid fixture");
        let second = restarted
            .record_admitted(&admitted.request, admitted.effect)
            .expect("valid fixture");
        assert_eq!(first, second);
        assert_eq!(
            narrowed.admit(&admitted.request, 21),
            Err(HostContractError::WidenedScope)
        );
        drop(restarted);
        std::fs::remove_dir_all(root).expect("valid fixture");
    }

    #[test]
    fn disposition_error_codes_are_stable() {
        assert_eq!(
            HostDispositionError::Malformed.to_string(),
            "codingmage.host.disposition.malformed"
        );
        assert_eq!(
            HostDispositionError::ConflictingReuse.to_string(),
            "codingmage.host.disposition.conflicting_reuse"
        );
        assert_eq!(
            HostDispositionError::Uncertain.to_string(),
            "codingmage.host.disposition.uncertain"
        );
        assert_eq!(
            HostDispositionError::UnknownRequest.to_string(),
            "codingmage.host.disposition.unknown_request"
        );
    }
}
