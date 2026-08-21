use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Model-proposed pod risk; deterministic policy may only escalate it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PodRisk {
    /// Bounded implementation with no shared security or architecture contract.
    Routine,
    /// Shared contract, architecture, security, concurrency, or publication-sensitive work.
    High,
}

/// Closed campaign-lead disposition. Exactly one payload family is permitted per report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadDispositionKind {
    /// Propose one or more dependency-ready bounded pods.
    Propose,
    /// Record a prerequisite that cannot be resolved inside current authority.
    Blocked,
    /// Delay admission until one supported observable trigger occurs.
    Deferred,
    /// Request an external owner decision without starting implementation.
    HumanDecisionRequired,
}

/// Closed blocker reasons; provider prose cannot create new durable reason classes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadBlockedReason {
    /// Required external data, service output, or approval is absent.
    UnavailableExternalDependency,
    /// Required hardware from the supported matrix is unavailable.
    UnavailableSupportedHardware,
    /// Operator-managed authentication is absent or expired.
    MissingOperatorManagedAuthentication,
    /// A required external service cannot currently be reached.
    UnavailableExternalService,
    /// The task requires a platform outside current support.
    UnsupportedPlatform,
    /// A canonical prerequisite task remains blocked.
    BlockedPrerequisite,
    /// Implementation discovered a condition outside the leased authority.
    ImplementationConditionOutsideAuthority,
}

impl LeadBlockedReason {
    /// Stable content-free reason code for status and durable projections.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnavailableExternalDependency => "unavailable_external_dependency",
            Self::UnavailableSupportedHardware => "unavailable_supported_hardware",
            Self::MissingOperatorManagedAuthentication => "missing_operator_managed_authentication",
            Self::UnavailableExternalService => "unavailable_external_service",
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::BlockedPrerequisite => "blocked_prerequisite",
            Self::ImplementationConditionOutsideAuthority => {
                "implementation_condition_outside_authority"
            }
        }
    }

    /// Whether this reason can coherently apply to a task already proven dependency-ready.
    #[must_use]
    pub const fn valid_for_dependency_ready_task(self) -> bool {
        !matches!(self, Self::BlockedPrerequisite)
    }
}

/// Closed temporary deferral reasons.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadDeferredReason {
    /// The selected provider has temporary capacity constraints.
    TemporaryProviderCapacity,
    /// Another admitted pod owns an overlapping path.
    ActivePathLease,
    /// Another admitted operation owns an exclusive gate resource.
    GateResourceContention,
    /// Canonical ordering requires another accepted outcome first.
    DeterministicDependencyOrder,
    /// The candidate is awaiting a stronger review profile.
    PendingStrongerReview,
    /// An authenticated operator pause prevents admission.
    OperatorPause,
}

/// Closed observable triggers that can make a deferred task eligible again.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadReconsiderationTrigger {
    /// The reviewed campaign head advances.
    CampaignHeadAdvancement,
    /// The conflicting path lease is released.
    LeaseRelease,
    /// The conflicting gate resource is released.
    GateResourceRelease,
    /// Provider capacity is positively revalidated.
    ProviderReset,
    /// The required stronger review completes.
    ReviewCompletion,
    /// The authenticated operator resumes the campaign.
    OperatorResume,
}

impl LeadReconsiderationTrigger {
    /// Stable content-free trigger code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CampaignHeadAdvancement => "campaign_head_advancement",
            Self::LeaseRelease => "lease_release",
            Self::GateResourceRelease => "gate_resource_release",
            Self::ProviderReset => "provider_reset",
            Self::ReviewCompletion => "review_completion",
            Self::OperatorResume => "operator_resume",
        }
    }

    /// Parses one exact stable trigger code.
    #[must_use]
    pub fn parse_code(value: &str) -> Option<Self> {
        match value {
            "campaign_head_advancement" => Some(Self::CampaignHeadAdvancement),
            "lease_release" => Some(Self::LeaseRelease),
            "gate_resource_release" => Some(Self::GateResourceRelease),
            "provider_reset" => Some(Self::ProviderReset),
            "review_completion" => Some(Self::ReviewCompletion),
            "operator_resume" => Some(Self::OperatorResume),
            _ => None,
        }
    }
}

impl LeadDeferredReason {
    /// The only trigger capable of reconsidering this temporary reason.
    #[must_use]
    pub const fn required_trigger(self) -> LeadReconsiderationTrigger {
        match self {
            Self::TemporaryProviderCapacity => LeadReconsiderationTrigger::ProviderReset,
            Self::ActivePathLease => LeadReconsiderationTrigger::LeaseRelease,
            Self::GateResourceContention => LeadReconsiderationTrigger::GateResourceRelease,
            Self::DeterministicDependencyOrder => {
                LeadReconsiderationTrigger::CampaignHeadAdvancement
            }
            Self::PendingStrongerReview => LeadReconsiderationTrigger::ReviewCompletion,
            Self::OperatorPause => LeadReconsiderationTrigger::OperatorResume,
        }
    }

    /// Stable content-free reason code for durable status.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::TemporaryProviderCapacity => "temporary_provider_capacity",
            Self::ActivePathLease => "active_path_lease",
            Self::GateResourceContention => "gate_resource_contention",
            Self::DeterministicDependencyOrder => "deterministic_dependency_order",
            Self::PendingStrongerReview => "pending_stronger_review",
            Self::OperatorPause => "operator_pause",
        }
    }
}

/// Closed reasons for requiring an external owner decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadHumanDecisionReason {
    /// The bounded task scope has more than one material interpretation.
    AmbiguousScope,
    /// A material architecture choice lacks existing policy.
    MaterialArchitectureChoice,
    /// Completion would require authority beyond the campaign specification.
    RequestedAuthorityExpansion,
    /// The requested action could affect a protected branch.
    ProtectedBranchConsequence,
    /// The requested action would change external infrastructure.
    ExternalInfrastructureChange,
    /// Publication or release requires explicit owner authorization.
    ReleaseDecision,
}

/// Exact task and snapshot binding required for every nonproposal disposition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeadTaskBinding {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact campaign head observed by the lead.
    pub campaign_head: String,
    /// Exact canonical task-source digest observed by the lead.
    pub task_source_sha256: String,
    /// Exact dependency-ready task receiving the disposition.
    pub task_id: String,
    /// Exact canonical dependency array for that task.
    pub dependencies: Vec<String>,
}

/// Typed blocked disposition payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeadBlockedDisposition {
    /// Exact immutable task and campaign snapshot.
    pub binding: LeadTaskBinding,
    /// Closed blocker reason.
    pub reason: LeadBlockedReason,
}

/// Typed deferred disposition payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeadDeferredDisposition {
    /// Exact immutable task and campaign snapshot.
    pub binding: LeadTaskBinding,
    /// Closed temporary reason.
    pub reason: LeadDeferredReason,
    /// Exact observation required before reconsideration.
    pub reconsideration_trigger: LeadReconsiderationTrigger,
}

/// Untrusted model-authored proposal before deterministic sealing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamLeadProposal {
    /// Exact dependency-ready task identifier.
    pub task_id: String,
    /// Dependencies claimed from the canonical plan.
    pub dependencies: Vec<String>,
    /// Exact requested write roots.
    pub owned_paths: Vec<PathBuf>,
    /// Required operator-defined gate tiers.
    pub gate_tiers: Vec<String>,
    /// Shared test resources that must be leased exclusively.
    pub test_resources: Vec<String>,
    /// Expected outputs under the requested write roots.
    pub expected_artifacts: Vec<PathBuf>,
    /// Proposed risk, subject to deterministic escalation.
    pub risk: PodRisk,
    /// Concise inspectable summary; never used as authority.
    pub rationale_summary: String,
}

/// Bounded request for an operator decision when no proposal is independently safe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionBlocker {
    /// Exact task and immutable campaign snapshot requiring a decision.
    pub binding: LeadTaskBinding,
    /// Stable closed reason.
    pub reason: LeadHumanDecisionReason,
    /// Concise inspectable question with no hidden reasoning.
    pub summary: String,
}

/// Strict read-only campaign-lead response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamLeadReport {
    /// Exact campaign identity observed by the lead.
    pub campaign_id: String,
    /// Exact starting commit observed by the lead.
    pub campaign_head: String,
    /// Exact canonical task source observed by the lead.
    pub task_source_sha256: String,
    /// Closed mutually exclusive outcome selector.
    pub disposition: LeadDispositionKind,
    /// Bounded proposals selected only from the supplied ready set; nonempty only for `propose`.
    pub proposals: Vec<TeamLeadProposal>,
    /// Present only for `blocked`.
    pub blocked: Option<LeadBlockedDisposition>,
    /// Present only for `deferred`.
    pub deferred: Option<LeadDeferredDisposition>,
    /// Present only for `human_decision_required`.
    pub human_decision: Option<HumanDecisionBlocker>,
}
