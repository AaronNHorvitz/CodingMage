//! Durable final campaign pull-request publication and destination promotion.

use std::{fmt::Write as _, fs, path::Path};

use codingmage_campaign::{
    CampaignSpec, DestinationPromotionPolicy, GitHubCampaignPolicy, TeamCampaignSnapshot,
};
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(test)]
use crate::TeamCompletionReconciliation;
use crate::{CiObservation, RuntimeError, TeamCampaignReport};

const PROMOTION_NAME: &str = "team-campaign-promotion.json";
const PROMOTION_VERSION: u16 = 1;

/// Immutable authority for one final campaign pull request and possible destination promotion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignPromotionRequest {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact repository identity.
    pub repository_id: String,
    /// Exact collision-resistant campaign branch.
    pub campaign_branch: String,
    /// Exact protected destination branch.
    pub destination_branch: String,
    /// Final locally verified campaign commit.
    pub final_commit: String,
    /// Current destination commit observed before any promotion.
    pub destination_commit: Option<String>,
    /// Existing final pull request, when already bound.
    pub pull_request_number: Option<u64>,
    /// Final deterministic gate evidence.
    pub final_gate_evidence_sha256: String,
    /// Final independent review evidence.
    pub final_review_evidence_sha256: String,
    /// Integrity digest of the final campaign report.
    pub report_sha256: String,
}

impl CampaignPromotionRequest {
    fn verify(
        &self,
        spec: &CampaignSpec,
        policy: &GitHubCampaignPolicy,
    ) -> Result<(), CampaignPromotionError> {
        if self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || self.campaign_branch == self.destination_branch
            || self.campaign_branch != spec.campaign_branch
                && !self
                    .campaign_branch
                    .strip_prefix(&spec.campaign_branch)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            || self.destination_branch != policy.destination_branch
            || !spec.protected_branches.contains(&self.destination_branch)
            || !valid_commit(&self.final_commit)
            || self
                .destination_commit
                .as_ref()
                .is_some_and(|value| !valid_commit(value))
            || self.pull_request_number == Some(0)
            || !valid_sha256(&self.final_gate_evidence_sha256)
            || !valid_sha256(&self.final_review_evidence_sha256)
            || !valid_sha256(&self.report_sha256)
        {
            return Err(CampaignPromotionError::Authority);
        }
        Ok(())
    }
}

/// Exact final pull-request observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignPullRequestObservation {
    /// Exact pull-request number.
    pub number: u64,
    /// Exact protected base branch.
    pub base_branch: String,
    /// Exact campaign head branch.
    pub head_branch: String,
    /// Exact final campaign commit.
    pub head_commit: String,
    /// Whether the pull request remains a draft.
    pub draft: bool,
    /// Whether the pull request is observed merged into the exact destination.
    pub merged: bool,
    /// Integrity digest of the complete observation.
    pub evidence_sha256: String,
}

/// Exact destination branch and protection observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationObservation {
    /// Exact protected destination branch.
    pub branch: String,
    /// Exact observed destination commit.
    pub commit: String,
    /// Whether configured protection requirements were observed satisfied.
    pub protection_satisfied: bool,
    /// Integrity digest of the complete observation.
    pub evidence_sha256: String,
}

/// Exact destination fast-forward observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationPromotionObservation {
    /// Exact destination branch.
    pub branch: String,
    /// Exact commit observed after promotion.
    pub commit: String,
    /// Exact final pull request observed as merged.
    pub pull_request_number: u64,
    /// Integrity digest of the complete observation.
    pub evidence_sha256: String,
}

/// Human-approval identity that must be matched exactly by an operator decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignPromotionApprovalBinding {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact pull-request number.
    pub pull_request_number: u64,
    /// Exact protected destination branch.
    pub destination_branch: String,
    /// Destination commit observed before approval.
    pub destination_commit: String,
    /// Final campaign commit approved for promotion.
    pub final_commit: String,
    /// Integrity digest of the final campaign report.
    pub report_sha256: String,
}

/// Narrow coordinator-owned final publication and promotion port.
pub trait TeamPromotionPort {
    /// Creates or updates the one final draft pull request idempotently.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn ensure_final_pull_request(
        &mut self,
        request: &CampaignPromotionRequest,
    ) -> Result<CampaignPullRequestObservation, CampaignPromotionError>;

    /// Observes the exact destination head and configured branch protection.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or response failure.
    fn observe_destination(
        &mut self,
        request: &CampaignPromotionRequest,
    ) -> Result<DestinationObservation, CampaignPromotionError>;

    /// Observes required CI checks for the exact final commit.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or response failure.
    fn observe_final_ci(
        &mut self,
        request: &CampaignPromotionRequest,
    ) -> Result<CiObservation, CampaignPromotionError>;

    /// Fast-forwards the exact destination to the exact final commit without force.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority, identity, transport, or reconciliation failure.
    fn promote_destination(
        &mut self,
        request: &CampaignPromotionRequest,
    ) -> Result<DestinationPromotionObservation, CampaignPromotionError>;
}

/// Durable final publication result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignPromotionOutcome {
    /// Required final CI checks remain pending.
    WaitingForCi,
    /// The final pull request is ready and policy forbids automated promotion.
    PullRequestReady,
    /// Exact operator approval is required before promotion.
    ApprovalRequired,
    /// The exact final commit is observed at the protected destination.
    Promoted,
}

/// Stable content-free final publication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignPromotionError {
    /// Authority or policy was missing, malformed, or stale.
    Authority,
    /// Remote or durable identity contradicted the exact request.
    Identity,
    /// A remote effect remains uncertain after reconciliation.
    Uncertain,
    /// Required CI checks failed for the exact final commit.
    CiFailed,
    /// Durable state could not be loaded or advanced exactly.
    State,
}

impl CampaignPromotionError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Authority => "codingmage.team.promotion.authority",
            Self::Identity => "codingmage.team.promotion.identity",
            Self::Uncertain => "codingmage.team.promotion.uncertain",
            Self::CiFailed => "codingmage.team.promotion.ci_failed",
            Self::State => "codingmage.team.promotion.state",
        }
    }
}

impl std::fmt::Display for CampaignPromotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for CampaignPromotionError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CampaignPromotionState {
    version: u16,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    campaign_branch: String,
    destination_branch: String,
    final_commit: String,
    report_sha256: String,
    pull_request_number: Option<u64>,
    destination_commit: Option<String>,
    ci_evidence_sha256: Option<String>,
    promotion_evidence_sha256: Option<String>,
}

impl CampaignPromotionState {
    fn initial(
        spec: &CampaignSpec,
        policy: &GitHubCampaignPolicy,
        campaign_branch: &str,
        report: &TeamCampaignReport,
        authority_sha256: &str,
        report_sha256: &str,
    ) -> Self {
        Self {
            version: PROMOTION_VERSION,
            authority_sha256: authority_sha256.to_owned(),
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            campaign_branch: campaign_branch.to_owned(),
            destination_branch: policy.destination_branch.clone(),
            final_commit: report.final_commit.clone(),
            report_sha256: report_sha256.to_owned(),
            pull_request_number: None,
            destination_commit: None,
            ci_evidence_sha256: None,
            promotion_evidence_sha256: None,
        }
    }

    fn verify(
        &self,
        spec: &CampaignSpec,
        policy: &GitHubCampaignPolicy,
        campaign_branch: &str,
        report: &TeamCampaignReport,
        authority_sha256: &str,
        report_sha256: &str,
    ) -> bool {
        self.version == PROMOTION_VERSION
            && self.authority_sha256 == authority_sha256
            && self.campaign_id == spec.campaign_id
            && self.repository_id == spec.repository_id
            && self.campaign_branch == campaign_branch
            && self.destination_branch == policy.destination_branch
            && self.final_commit == report.final_commit
            && self.report_sha256 == report_sha256
            && self.pull_request_number != Some(0)
            && self
                .destination_commit
                .as_ref()
                .is_none_or(|value| valid_commit(value))
            && self
                .ci_evidence_sha256
                .as_ref()
                .is_none_or(|value| valid_sha256(value))
            && self
                .promotion_evidence_sha256
                .as_ref()
                .is_none_or(|value| valid_sha256(value))
            && (self.promotion_evidence_sha256.is_none()
                || self.pull_request_number.is_some()
                    && self.destination_commit.is_some()
                    && self.ci_evidence_sha256.is_some())
    }

    fn request(&self, report: &TeamCampaignReport) -> CampaignPromotionRequest {
        CampaignPromotionRequest {
            campaign_id: self.campaign_id.clone(),
            repository_id: self.repository_id.clone(),
            campaign_branch: self.campaign_branch.clone(),
            destination_branch: self.destination_branch.clone(),
            final_commit: self.final_commit.clone(),
            destination_commit: self.destination_commit.clone(),
            pull_request_number: self.pull_request_number,
            final_gate_evidence_sha256: report.final_gate_evidence_sha256.clone(),
            final_review_evidence_sha256: report.final_review_evidence_sha256.clone(),
            report_sha256: self.report_sha256.clone(),
        }
    }
}

/// Reconciles the final pull request and applies only an explicitly authorized promotion policy.
///
/// The human-approval callback receives an immutable, commit-bound decision identity. It is never
/// called for automatic or never-promote policy. Every remote write is followed by exact
/// observation and every durable update is integrity-protected.
///
/// # Errors
///
/// Returns a content-free policy, identity, CI, uncertainty, or durable-state failure.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn synchronize_campaign_promotion<T, A>(
    campaign_root: &Path,
    spec: &CampaignSpec,
    campaign_branch: &str,
    snapshot: &TeamCampaignSnapshot,
    report: &TeamCampaignReport,
    authority_sha256: &str,
    destination_merge_allowed: bool,
    port: &mut T,
    mut human_approved: A,
) -> Result<CampaignPromotionOutcome, CampaignPromotionError>
where
    T: TeamPromotionPort,
    A: FnMut(&CampaignPromotionApprovalBinding) -> Result<bool, RuntimeError>,
{
    spec.verify()
        .map_err(|_| CampaignPromotionError::Authority)?;
    snapshot
        .verify()
        .map_err(|_| CampaignPromotionError::State)?;
    let multi = spec
        .multi_agent
        .as_ref()
        .ok_or(CampaignPromotionError::Authority)?;
    let policy = multi
        .github
        .as_ref()
        .ok_or(CampaignPromotionError::Authority)?;
    if report.final_commit != snapshot.campaign_head
        || snapshot
            .tasks
            .values()
            .any(|record| record.state != codingmage_campaign::CampaignTaskState::Merged)
    {
        return Err(CampaignPromotionError::State);
    }
    let report_sha256 = report_digest(report)?;
    let mut state = load_promotion(
        campaign_root,
        spec,
        policy,
        campaign_branch,
        report,
        authority_sha256,
        &report_sha256,
    )?
    .unwrap_or_else(|| {
        CampaignPromotionState::initial(
            spec,
            policy,
            campaign_branch,
            report,
            authority_sha256,
            &report_sha256,
        )
    });
    let mut request = state.request(report);
    request.verify(spec, policy)?;

    let pull_request = port.ensure_final_pull_request(&request)?;
    validate_pull_request(&request, &pull_request)?;
    if state.pull_request_number.is_none() {
        state.pull_request_number = Some(pull_request.number);
        persist_promotion(
            campaign_root,
            &state,
            spec,
            policy,
            campaign_branch,
            report,
            authority_sha256,
            &report_sha256,
        )?;
        request = state.request(report);
    } else if state.pull_request_number != Some(pull_request.number) {
        return Err(CampaignPromotionError::Identity);
    }

    let destination = port.observe_destination(&request)?;
    if state.promotion_evidence_sha256.is_some() {
        if destination.branch == request.destination_branch
            && destination.commit == request.final_commit
            && pull_request.merged
            && valid_sha256(&destination.evidence_sha256)
        {
            return Ok(CampaignPromotionOutcome::Promoted);
        }
        return Err(CampaignPromotionError::Identity);
    }
    let promotion_already_observed =
        destination.commit == request.final_commit && pull_request.merged;
    if !promotion_already_observed {
        validate_destination(&request, &destination)?;
        if let Some(bound) = &state.destination_commit {
            if bound != &destination.commit {
                return Err(CampaignPromotionError::Identity);
            }
        } else {
            state.destination_commit = Some(destination.commit.clone());
            persist_promotion(
                campaign_root,
                &state,
                spec,
                policy,
                campaign_branch,
                report,
                authority_sha256,
                &report_sha256,
            )?;
            request = state.request(report);
        }
    }

    match port.observe_final_ci(&request)? {
        CiObservation::Pending => return Ok(CampaignPromotionOutcome::WaitingForCi),
        CiObservation::Failed { commit, .. } if commit == request.final_commit => {
            return Err(CampaignPromotionError::CiFailed);
        }
        CiObservation::Passed {
            commit,
            evidence_sha256,
        } if commit == request.final_commit && valid_sha256(&evidence_sha256) => {
            if state.ci_evidence_sha256.as_ref() != Some(&evidence_sha256) {
                if state.ci_evidence_sha256.is_some() {
                    return Err(CampaignPromotionError::Identity);
                }
                state.ci_evidence_sha256 = Some(evidence_sha256);
                persist_promotion(
                    campaign_root,
                    &state,
                    spec,
                    policy,
                    campaign_branch,
                    report,
                    authority_sha256,
                    &report_sha256,
                )?;
            }
        }
        _ => return Err(CampaignPromotionError::Identity),
    }

    if multi.destination_promotion_policy == DestinationPromotionPolicy::Never {
        return Ok(CampaignPromotionOutcome::PullRequestReady);
    }
    let approval = CampaignPromotionApprovalBinding {
        campaign_id: spec.campaign_id.clone(),
        pull_request_number: state
            .pull_request_number
            .ok_or(CampaignPromotionError::State)?,
        destination_branch: policy.destination_branch.clone(),
        destination_commit: state
            .destination_commit
            .clone()
            .ok_or(CampaignPromotionError::State)?,
        final_commit: report.final_commit.clone(),
        report_sha256,
    };
    if multi.destination_promotion_policy == DestinationPromotionPolicy::HumanRequired
        && !human_approved(&approval).map_err(|_| CampaignPromotionError::State)?
    {
        return Ok(CampaignPromotionOutcome::ApprovalRequired);
    }
    if !destination_merge_allowed || !destination.protection_satisfied {
        return Err(CampaignPromotionError::Authority);
    }
    let promoted = port.promote_destination(&request)?;
    if promoted.branch != request.destination_branch
        || promoted.commit != request.final_commit
        || promoted.pull_request_number != request.pull_request_number.unwrap_or_default()
        || !valid_sha256(&promoted.evidence_sha256)
    {
        return Err(CampaignPromotionError::Identity);
    }
    state.promotion_evidence_sha256 = Some(promoted.evidence_sha256);
    persist_promotion(
        campaign_root,
        &state,
        spec,
        policy,
        campaign_branch,
        report,
        authority_sha256,
        &approval.report_sha256,
    )?;
    Ok(CampaignPromotionOutcome::Promoted)
}

pub(crate) fn campaign_promotion_approval_binding(
    campaign_root: &Path,
    spec: &CampaignSpec,
    campaign_branch: &str,
    report: &TeamCampaignReport,
    authority_sha256: &str,
) -> Result<Option<CampaignPromotionApprovalBinding>, CampaignPromotionError> {
    let policy = spec
        .multi_agent
        .as_ref()
        .and_then(|value| value.github.as_ref())
        .ok_or(CampaignPromotionError::Authority)?;
    let report_sha256 = report_digest(report)?;
    let Some(state) = load_promotion(
        campaign_root,
        spec,
        policy,
        campaign_branch,
        report,
        authority_sha256,
        &report_sha256,
    )?
    else {
        return Ok(None);
    };
    let (Some(pull_request_number), Some(destination_commit)) =
        (state.pull_request_number, state.destination_commit)
    else {
        return Ok(None);
    };
    Ok(Some(CampaignPromotionApprovalBinding {
        campaign_id: spec.campaign_id.clone(),
        pull_request_number,
        destination_branch: policy.destination_branch.clone(),
        destination_commit,
        final_commit: report.final_commit.clone(),
        report_sha256,
    }))
}

fn load_promotion(
    campaign_root: &Path,
    spec: &CampaignSpec,
    policy: &GitHubCampaignPolicy,
    campaign_branch: &str,
    report: &TeamCampaignReport,
    authority_sha256: &str,
    report_sha256: &str,
) -> Result<Option<CampaignPromotionState>, CampaignPromotionError> {
    if fs::symlink_metadata(campaign_root.join(PROMOTION_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<CampaignPromotionState>::load(campaign_root, PROMOTION_NAME, |value| {
        value.verify(
            spec,
            policy,
            campaign_branch,
            report,
            authority_sha256,
            report_sha256,
        )
    })
    .map(|document| Some(document.payload))
    .map_err(|_| CampaignPromotionError::State)
}

#[allow(clippy::too_many_arguments)]
fn persist_promotion(
    campaign_root: &Path,
    state: &CampaignPromotionState,
    spec: &CampaignSpec,
    policy: &GitHubCampaignPolicy,
    campaign_branch: &str,
    report: &TeamCampaignReport,
    authority_sha256: &str,
    report_sha256: &str,
) -> Result<(), CampaignPromotionError> {
    IntegrityDocument::write_atomic(campaign_root, PROMOTION_NAME, state.clone(), |value| {
        value.verify(
            spec,
            policy,
            campaign_branch,
            report,
            authority_sha256,
            report_sha256,
        )
    })
    .map(|_| ())
    .map_err(|_| CampaignPromotionError::State)
}

fn validate_pull_request(
    request: &CampaignPromotionRequest,
    observation: &CampaignPullRequestObservation,
) -> Result<(), CampaignPromotionError> {
    if observation.number == 0
        || request
            .pull_request_number
            .is_some_and(|number| number != observation.number)
        || observation.base_branch != request.destination_branch
        || observation.head_branch != request.campaign_branch
        || observation.head_commit != request.final_commit
        || !valid_sha256(&observation.evidence_sha256)
    {
        return Err(CampaignPromotionError::Identity);
    }
    Ok(())
}

fn validate_destination(
    request: &CampaignPromotionRequest,
    observation: &DestinationObservation,
) -> Result<(), CampaignPromotionError> {
    if observation.branch != request.destination_branch
        || !valid_commit(&observation.commit)
        || request
            .destination_commit
            .as_ref()
            .is_some_and(|commit| commit != &observation.commit)
        || !valid_sha256(&observation.evidence_sha256)
    {
        return Err(CampaignPromotionError::Identity);
    }
    Ok(())
}

fn report_digest(report: &TeamCampaignReport) -> Result<String, CampaignPromotionError> {
    let bytes = serde_json::to_vec(report).map_err(|_| CampaignPromotionError::State)?;
    Ok(digest(&bytes))
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn digest(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in hash {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use codingmage_campaign::{
        CampaignAuthentication, CampaignConcurrency, CampaignGateTier, CampaignLimits,
        CampaignProvider, CampaignPublication, DurablePodScheduler, MultiAgentPolicy,
        TEAM_STATE_SCHEMA_VERSION, TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode,
        TeamResourceController, TeamResourcePolicy,
    };

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct FakePromotionPort {
        pull_request: Option<u64>,
        pull_request_creations: usize,
        destination: String,
        final_commit: String,
        promotions: usize,
        ci_pending: bool,
    }

    impl TeamPromotionPort for FakePromotionPort {
        fn ensure_final_pull_request(
            &mut self,
            request: &CampaignPromotionRequest,
        ) -> Result<CampaignPullRequestObservation, CampaignPromotionError> {
            let number = *self.pull_request.get_or_insert_with(|| {
                self.pull_request_creations = self.pull_request_creations.saturating_add(1);
                73
            });
            Ok(CampaignPullRequestObservation {
                number,
                base_branch: request.destination_branch.clone(),
                head_branch: request.campaign_branch.clone(),
                head_commit: request.final_commit.clone(),
                draft: self.destination != self.final_commit,
                merged: self.destination == self.final_commit,
                evidence_sha256: "1".repeat(64),
            })
        }

        fn observe_destination(
            &mut self,
            request: &CampaignPromotionRequest,
        ) -> Result<DestinationObservation, CampaignPromotionError> {
            Ok(DestinationObservation {
                branch: request.destination_branch.clone(),
                commit: self.destination.clone(),
                protection_satisfied: true,
                evidence_sha256: "2".repeat(64),
            })
        }

        fn observe_final_ci(
            &mut self,
            request: &CampaignPromotionRequest,
        ) -> Result<CiObservation, CampaignPromotionError> {
            if self.ci_pending {
                Ok(CiObservation::Pending)
            } else {
                Ok(CiObservation::Passed {
                    commit: request.final_commit.clone(),
                    evidence_sha256: "3".repeat(64),
                })
            }
        }

        fn promote_destination(
            &mut self,
            request: &CampaignPromotionRequest,
        ) -> Result<DestinationPromotionObservation, CampaignPromotionError> {
            if request.destination_commit.as_deref() != Some(&self.destination) {
                return Err(CampaignPromotionError::Identity);
            }
            self.promotions = self.promotions.saturating_add(1);
            self.destination.clone_from(&request.final_commit);
            Ok(DestinationPromotionObservation {
                branch: request.destination_branch.clone(),
                commit: request.final_commit.clone(),
                pull_request_number: request.pull_request_number.unwrap_or_default(),
                evidence_sha256: "4".repeat(64),
            })
        }
    }

    #[test]
    fn human_promotion_is_exact_idempotent_and_restart_safe() {
        let (root, spec, snapshot, report) = fixture(DestinationPromotionPolicy::HumanRequired);
        let authority = spec.authority_sha256().unwrap();
        let branch = "codingmage/promotion-fixture/owned";
        let mut port = FakePromotionPort {
            destination: spec.initial_commit.clone(),
            final_commit: report.final_commit.clone(),
            ..FakePromotionPort::default()
        };
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| Ok(false),
            )
            .unwrap(),
            CampaignPromotionOutcome::ApprovalRequired
        );
        assert_eq!(port.pull_request_creations, 1);
        assert_eq!(port.promotions, 0);

        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| Ok(true),
            )
            .unwrap(),
            CampaignPromotionOutcome::Promoted
        );
        assert_eq!(port.promotions, 1);
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| Ok(true),
            )
            .unwrap(),
            CampaignPromotionOutcome::Promoted
        );
        assert_eq!(port.pull_request_creations, 1);
        assert_eq!(port.promotions, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn never_policy_publishes_one_draft_and_cannot_promote() {
        let (root, spec, snapshot, report) = fixture(DestinationPromotionPolicy::Never);
        let authority = spec.authority_sha256().unwrap();
        let mut port = FakePromotionPort {
            destination: spec.initial_commit.clone(),
            final_commit: report.final_commit.clone(),
            ..FakePromotionPort::default()
        };
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                "codingmage/promotion-fixture/owned",
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| panic!("never policy must not request approval"),
            )
            .unwrap(),
            CampaignPromotionOutcome::PullRequestReady
        );
        assert_eq!(port.promotions, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_ci_denied_capability_and_moved_destination_all_fail_closed() {
        let (root, spec, snapshot, report) =
            fixture(DestinationPromotionPolicy::AutoToDefaultBranch);
        let authority = spec.authority_sha256().unwrap();
        let branch = "codingmage/promotion-fixture/owned";
        let mut port = FakePromotionPort {
            destination: spec.initial_commit.clone(),
            final_commit: report.final_commit.clone(),
            ci_pending: true,
            ..FakePromotionPort::default()
        };
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| panic!("automatic policy must not request approval"),
            )
            .unwrap(),
            CampaignPromotionOutcome::WaitingForCi
        );
        port.ci_pending = false;
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                false,
                &mut port,
                |_| panic!("automatic policy must not request approval"),
            ),
            Err(CampaignPromotionError::Authority)
        );
        port.destination = "9".repeat(40);
        assert_eq!(
            synchronize_campaign_promotion(
                &root,
                &spec,
                branch,
                &snapshot,
                &report,
                &authority,
                true,
                &mut port,
                |_| panic!("automatic policy must not request approval"),
            ),
            Err(CampaignPromotionError::Identity)
        );
        assert_eq!(port.promotions, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[allow(clippy::too_many_lines)]
    fn fixture(
        destination_policy: DestinationPromotionPolicy,
    ) -> (
        PathBuf,
        CampaignSpec,
        TeamCampaignSnapshot,
        TeamCampaignReport,
    ) {
        let root = std::env::temp_dir().join(format!(
            "codingmage-promotion-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let provider = |name: &str| CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        let spec = CampaignSpec {
            version: 3,
            campaign_id: "promotion-fixture".to_owned(),
            repository_id: "repo-promotion".to_owned(),
            repository_path: root.join("repository"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
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
                name: "workspace".to_owned(),
                profiles: vec!["workspace".to_owned()],
            }],
            campaign_branch: "codingmage/promotion-fixture".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::DraftStoryPullRequests,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: codingmage_campaign::CampaignExecutionMode::Parallel,
                publication_mode: TaskPublicationMode::CampaignDraftPullRequest,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: destination_policy,
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
                provider_routing: None,
            }),
        };
        spec.verify().unwrap();
        let scheduler = DurablePodScheduler::new(&spec).unwrap();
        let resources = TeamResourceController::new(&spec).unwrap();
        let snapshot = TeamCampaignSnapshot {
            version: TEAM_STATE_SCHEMA_VERSION,
            campaign_id: spec.campaign_id.clone(),
            generation: 0,
            campaign_head: "d".repeat(40),
            task_source_sha256: spec.task_source_sha256.clone(),
            scheduler: scheduler.snapshot().clone(),
            resources: resources.snapshot().clone(),
            tasks: BTreeMap::new(),
            integration_queue: Vec::new(),
        };
        snapshot.verify().unwrap();
        let report = TeamCampaignReport {
            version: 2,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            branch: "codingmage/promotion-fixture/owned".to_owned(),
            initial_commit: spec.initial_commit.clone(),
            final_commit: snapshot.campaign_head.clone(),
            task_source_sha256: snapshot.task_source_sha256.clone(),
            tasks: BTreeMap::new(),
            reconciliation: TeamCompletionReconciliation {
                state_sha256: snapshot.sha256().unwrap(),
                completed_task_ids_sha256: "a".repeat(64),
                removed_worktree_ids_sha256: "b".repeat(64),
                task_evidence_sha256: "c".repeat(64),
                checked_task_count: 0,
                removed_worktree_count: 0,
                process_control_root_count: 1,
                process_control_residue_count: 0,
                active_lease_count: 0,
                active_reservation_count: 0,
                integration_queue_count: 0,
                journal_reconciled: true,
                task_source_reconciled: true,
            },
            final_gate_evidence_sha256: "e".repeat(64),
            final_review_evidence_sha256: "f".repeat(64),
            completed_at_ms: 1,
        };
        (root, spec, snapshot, report)
    }
}
