//! Campaign authority, team-lead proposals, and deterministic pod leases.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_contracts::TaskId;
pub use codingmage_contracts::{
    HumanDecisionBlocker, LeadBlockedDisposition, LeadBlockedReason, LeadDeferredDisposition,
    LeadDeferredReason, LeadDispositionKind, LeadHumanDecisionReason, LeadReconsiderationTrigger,
    LeadTaskBinding, PodRisk, TeamLeadProposal, TeamLeadReport,
};
use codingmage_plan::SelectedWork;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CAMPAIGN_VERSION: u16 = 2;
const MAX_SPEC_BYTES: u64 = 1024 * 1024;
const MAX_PATHS: usize = 256;
const MAX_RESOURCES: usize = 256;
const MAX_SUMMARY_BYTES: usize = 4 * 1024;
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Exact operator-selected provider profile. Provider output never changes this authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignProvider {
    /// Absolute provider executable selected outside model context.
    pub executable: PathBuf,
    /// Exact model selector.
    pub model: String,
    /// Exact effort selector.
    pub effort: String,
}

/// Credential discovery available to the implementation provider.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignAuthentication {
    /// Provider runs without inherited login discovery.
    Bare,
    /// Provider may discover its existing local login; `CodingMage` never reads the credential.
    ExistingLogin,
}

/// Named deterministic gate tier available to pod proposals.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignGateTier {
    /// Stable tier name referenced by proposals.
    pub name: String,
    /// Operator-authored gate profile names. These are data, never shell commands.
    pub profiles: Vec<String>,
}

/// Highest GitHub visibility permitted for one campaign.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignPublication {
    /// Keep campaign branches and review state local.
    LocalOnly,
    /// Permit configured story branches and draft pull requests, never merges.
    DraftStoryPullRequests,
}

/// Versioned operator-approved authority for one campaign.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignSpec {
    /// Closed schema version.
    pub version: u16,
    /// Stable campaign identity.
    pub campaign_id: String,
    /// Stable repository identity obtained from the validated target.
    pub repository_id: String,
    /// Absolute repository root; model output cannot replace it.
    pub repository_path: PathBuf,
    /// Exact campaign starting commit.
    pub initial_commit: String,
    /// Exact canonical task source observed at the starting commit.
    pub task_source_sha256: String,
    /// Digest of the external operator authorization record.
    pub operator_authorization_sha256: String,
    /// Maximum simultaneous implementation pods.
    #[serde(default = "default_parallel_pods")]
    pub max_parallel_pods: u16,
    /// Maximum accepted units before an operator-authored continuation is required.
    pub max_units: u32,
    /// Read-only campaign planning profile.
    pub team_lead: CampaignProvider,
    /// Pod implementation profile.
    pub implementer: CampaignProvider,
    /// Implementation-provider login boundary.
    pub implementer_authentication: CampaignAuthentication,
    /// Independent pod review profile.
    pub reviewer: CampaignProvider,
    /// Closed deterministic gate tiers proposals may request.
    pub gate_tiers: Vec<CampaignGateTier>,
    /// Coordinator-owned campaign branch prefix.
    pub campaign_branch: String,
    /// Relative repository roots from which exact pod paths may be leased.
    pub allowed_paths: Vec<PathBuf>,
    /// Relative roots that no pod proposal may equal, contain, or enter.
    #[serde(default)]
    pub denied_paths: Vec<PathBuf>,
    /// Protected branches that cannot be campaign publication targets.
    pub protected_branches: Vec<String>,
    /// Highest configured remote visibility.
    pub publication: CampaignPublication,
}

impl CampaignSpec {
    /// Loads one absolute, regular, nonsymlink campaign specification.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] for unavailable, oversized, malformed, or unsafe authority.
    pub fn load(path: &Path) -> Result<Self, CampaignError> {
        if !path.is_absolute() {
            return Err(CampaignError::InvalidSpec);
        }
        let metadata = fs::symlink_metadata(path).map_err(|_| CampaignError::InvalidSpec)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_SPEC_BYTES
        {
            return Err(CampaignError::InvalidSpec);
        }
        let source = fs::read_to_string(path).map_err(|_| CampaignError::InvalidSpec)?;
        let spec: Self = toml::from_str(&source).map_err(|_| CampaignError::InvalidSpec)?;
        spec.verify()?;
        Ok(spec)
    }

    /// Revalidates every authority-bearing field.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError::InvalidAuthority`] for broad, ambiguous, or contradictory policy.
    pub fn verify(&self) -> Result<(), CampaignError> {
        if self.version != CAMPAIGN_VERSION
            || !valid_component(&self.campaign_id)
            || !valid_component(&self.repository_id)
            || !self.repository_path.is_absolute()
            || !valid_commit(&self.initial_commit)
            || !valid_sha256(&self.task_source_sha256)
            || !valid_sha256(&self.operator_authorization_sha256)
            || !(1..=16).contains(&self.max_parallel_pods)
            || self.max_units == 0
            || self.max_units > 100_000
            || !valid_provider(&self.team_lead)
            || !valid_provider(&self.implementer)
            || !valid_provider(&self.reviewer)
            || self.gate_tiers.is_empty()
            || self.gate_tiers.len() > MAX_RESOURCES
            || self.gate_tiers.iter().any(|tier| {
                !valid_component(&tier.name)
                    || tier.profiles.is_empty()
                    || tier.profiles.len() > MAX_RESOURCES
                    || tier
                        .profiles
                        .iter()
                        .any(|profile| !valid_component(profile))
                    || tier.profiles.iter().collect::<BTreeSet<_>>().len() != tier.profiles.len()
            })
            || self
                .gate_tiers
                .iter()
                .map(|tier| &tier.name)
                .collect::<BTreeSet<_>>()
                .len()
                != self.gate_tiers.len()
            || !valid_branch(&self.campaign_branch)
            || !self.campaign_branch.starts_with("codingmage/")
            || self.allowed_paths.is_empty()
            || self.allowed_paths.len() > MAX_PATHS
            || self.denied_paths.len() > MAX_PATHS
            || self.protected_branches.is_empty()
            || self
                .protected_branches
                .iter()
                .any(|branch| !valid_branch(branch) || branch == &self.campaign_branch)
            || self
                .allowed_paths
                .iter()
                .chain(&self.denied_paths)
                .any(|path| !safe_relative(path))
            || any_overlap(&self.allowed_paths)
            || any_overlap(&self.denied_paths)
            || self.denied_paths.iter().any(|denied| {
                self.allowed_paths
                    .iter()
                    .any(|allowed| paths_overlap(allowed, denied))
            })
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }

    /// Returns a canonical digest of the complete operator-approved campaign authority.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] when authority or serialization is invalid.
    pub fn authority_sha256(&self) -> Result<String, CampaignError> {
        self.verify()?;
        canonical_sha256(self)
    }

    fn permits(&self, path: &Path) -> bool {
        self.allowed_paths
            .iter()
            .any(|allowed| path_contains(allowed, path))
            && !self
                .denied_paths
                .iter()
                .any(|denied| paths_overlap(denied, path))
    }
}

/// Deterministically validated lead outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeamLeadOutcome {
    /// Sealed proposals eligible for scheduler admission.
    Proposals(Vec<PodProposal>),
    /// Typed prerequisite blocker with no implementation authority.
    Blocked(LeadBlockedDisposition),
    /// Typed temporary deferral with no implementation authority.
    Deferred(LeadDeferredDisposition),
    /// Recorded question requiring external authority.
    HumanDecision(HumanDecisionBlocker),
}

/// Validates a lead response against immutable campaign authority and the coordinator's ready set.
///
/// # Errors
///
/// Returns [`CampaignError`] for stale, contradictory, duplicate, escaping, or non-ready output.
pub fn validate_team_lead_report(
    report: TeamLeadReport,
    spec: &CampaignSpec,
    ready: &[SelectedWork],
) -> Result<TeamLeadOutcome, CampaignError> {
    spec.verify()?;
    if report.campaign_id != spec.campaign_id
        || report.campaign_head != spec.initial_commit
        || report.task_source_sha256 != spec.task_source_sha256
        || report.proposals.len() > usize::from(spec.max_parallel_pods)
    {
        return Err(CampaignError::InvalidProposal);
    }
    match report.disposition {
        LeadDispositionKind::Blocked => {
            let blocker = report
                .blocked
                .clone()
                .ok_or(CampaignError::InvalidProposal)?;
            if !report.proposals.is_empty()
                || report.deferred.is_some()
                || report.human_decision.is_some()
                || !valid_lead_binding(&blocker.binding, &report, ready)
            {
                return Err(CampaignError::InvalidProposal);
            }
            return Ok(TeamLeadOutcome::Blocked(blocker));
        }
        LeadDispositionKind::Deferred => {
            let deferral = report
                .deferred
                .clone()
                .ok_or(CampaignError::InvalidProposal)?;
            if !report.proposals.is_empty()
                || report.blocked.is_some()
                || report.human_decision.is_some()
                || !valid_lead_binding(&deferral.binding, &report, ready)
            {
                return Err(CampaignError::InvalidProposal);
            }
            return Ok(TeamLeadOutcome::Deferred(deferral));
        }
        LeadDispositionKind::HumanDecisionRequired => {
            let blocker = report
                .human_decision
                .clone()
                .ok_or(CampaignError::InvalidProposal)?;
            if !report.proposals.is_empty()
                || report.blocked.is_some()
                || report.deferred.is_some()
                || !valid_lead_binding(&blocker.binding, &report, ready)
                || blocker.summary.is_empty()
                || blocker.summary.len() > MAX_SUMMARY_BYTES
                || blocker.summary.chars().any(char::is_control)
            {
                return Err(CampaignError::InvalidProposal);
            }
            return Ok(TeamLeadOutcome::HumanDecision(blocker));
        }
        LeadDispositionKind::Propose => {}
    }
    if report.proposals.is_empty()
        || report.blocked.is_some()
        || report.deferred.is_some()
        || report.human_decision.is_some()
    {
        return Err(CampaignError::InvalidProposal);
    }

    let ready_by_id = ready
        .iter()
        .map(|selected| (selected.item.id.as_str(), selected))
        .collect::<BTreeMap<_, _>>();
    let mut task_ids = BTreeSet::new();
    let mut sealed = Vec::with_capacity(report.proposals.len());
    for proposal in report.proposals {
        if !task_ids.insert(proposal.task_id.clone()) {
            return Err(CampaignError::InvalidProposal);
        }
        let selected = ready_by_id
            .get(proposal.task_id.as_str())
            .ok_or(CampaignError::InvalidProposal)?;
        if proposal.dependencies != selected.item.dependencies {
            return Err(CampaignError::InvalidProposal);
        }
        sealed.push(PodProposal::seal(
            PodProposal {
                version: CAMPAIGN_VERSION,
                task_id: proposal.task_id,
                task_source_sha256: report.task_source_sha256.clone(),
                owned_paths: proposal.owned_paths,
                dependencies: proposal.dependencies,
                gate_tiers: proposal.gate_tiers,
                test_resources: proposal.test_resources,
                expected_artifacts: proposal.expected_artifacts,
                risk: proposal.risk,
                rationale_summary: proposal.rationale_summary,
                proposal_sha256: ZERO_SHA256.to_owned(),
            },
            spec,
        )?);
    }
    Ok(TeamLeadOutcome::Proposals(sealed))
}

fn valid_lead_binding(
    binding: &LeadTaskBinding,
    report: &TeamLeadReport,
    ready: &[SelectedWork],
) -> bool {
    binding.campaign_id == report.campaign_id
        && binding.campaign_head == report.campaign_head
        && binding.task_source_sha256 == report.task_source_sha256
        && ready.iter().any(|selected| {
            selected.item.id == binding.task_id
                && selected.item.dependencies == binding.dependencies
        })
}

/// Hash-bound, model-proposed pod packet with no authority until admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PodProposal {
    /// Closed proposal schema version.
    pub version: u16,
    /// Exact dependency-ready task.
    pub task_id: String,
    /// Whole canonical task-source digest observed by the lead.
    pub task_source_sha256: String,
    /// Exact paths requested for this pod.
    pub owned_paths: Vec<PathBuf>,
    /// Exact dependencies copied from the canonical task item.
    pub dependencies: Vec<String>,
    /// Operator-defined gate tiers required for this pod.
    pub gate_tiers: Vec<String>,
    /// Shared gate or integration resources requested by this pod.
    pub test_resources: Vec<String>,
    /// Expected relative artifacts; these do not grant additional write authority.
    pub expected_artifacts: Vec<PathBuf>,
    /// Deterministic risk class subject to coordinator escalation.
    pub risk: PodRisk,
    /// Bounded lead summary retained for human inspection, never authority.
    pub rationale_summary: String,
    /// SHA-256 over all preceding fields.
    pub proposal_sha256: String,
}

impl PodProposal {
    /// Validates and seals one team-lead proposal as data.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] for malformed identity, paths, resources, or authority.
    pub fn seal(mut proposal: Self, spec: &CampaignSpec) -> Result<Self, CampaignError> {
        proposal.version = CAMPAIGN_VERSION;
        ZERO_SHA256.clone_into(&mut proposal.proposal_sha256);
        validate_proposal_shape(&proposal, spec)?;
        proposal.proposal_sha256 = canonical_sha256(&proposal)?;
        Ok(proposal)
    }

    /// Revalidates shape, campaign authority, and canonical identity.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] after any proposal mutation or authority mismatch.
    pub fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        validate_proposal_shape(self, spec)?;
        let mut candidate = self.clone();
        ZERO_SHA256.clone_into(&mut candidate.proposal_sha256);
        if canonical_sha256(&candidate)? != self.proposal_sha256 {
            return Err(CampaignError::StaleProposal);
        }
        Ok(())
    }
}

/// Exact active pod lease returned only after deterministic admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PodLease {
    /// Deterministic campaign-local pod identity.
    pub pod_id: String,
    /// Exact leased task.
    pub task_id: String,
    /// Exact nonoverlapping path authority.
    pub owned_paths: Vec<PathBuf>,
    /// Exact nonoverlapping test-resource authority.
    pub test_resources: Vec<String>,
    /// Sealed proposal identity.
    pub proposal_sha256: String,
}

/// In-memory deterministic admission controller for one campaign generation.
#[derive(Clone, Debug)]
pub struct PodScheduler {
    max_parallel_pods: u16,
    next_sequence: u64,
    active: BTreeMap<String, PodLease>,
}

impl PodScheduler {
    /// Creates an empty scheduler from verified campaign authority.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] when the campaign specification is invalid.
    pub fn new(spec: &CampaignSpec) -> Result<Self, CampaignError> {
        spec.verify()?;
        Ok(Self {
            max_parallel_pods: spec.max_parallel_pods,
            next_sequence: 0,
            active: BTreeMap::new(),
        })
    }

    /// Admits one exact ready task when paths, resources, source, and capacity remain valid.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] before mutation for stale or conflicting proposals.
    pub fn admit(
        &mut self,
        spec: &CampaignSpec,
        selected: &SelectedWork,
        proposal: PodProposal,
    ) -> Result<PodLease, CampaignError> {
        proposal.verify(spec)?;
        if self.active.len() >= usize::from(self.max_parallel_pods) {
            return Err(CampaignError::Capacity);
        }
        if proposal.task_id != selected.item.id
            || proposal.task_source_sha256 != selected.source_sha256
            || proposal.dependencies != selected.item.dependencies
            || self
                .active
                .values()
                .any(|lease| lease.task_id == proposal.task_id)
            || self.active.values().any(|lease| {
                lease.owned_paths.iter().any(|left| {
                    proposal
                        .owned_paths
                        .iter()
                        .any(|right| paths_overlap(left, right))
                }) || lease.test_resources.iter().any(|resource| {
                    proposal
                        .test_resources
                        .iter()
                        .any(|right| right == resource)
                })
            })
        {
            return Err(CampaignError::Conflict);
        }
        let identity = format!(
            "pod-{}-{}",
            self.next_sequence,
            &proposal.proposal_sha256[..12]
        );
        self.next_sequence = self.next_sequence.saturating_add(1);
        let lease = PodLease {
            pod_id: identity.clone(),
            task_id: proposal.task_id,
            owned_paths: proposal.owned_paths,
            test_resources: proposal.test_resources,
            proposal_sha256: proposal.proposal_sha256,
        };
        self.active.insert(identity, lease.clone());
        Ok(lease)
    }

    /// Releases one exact pod lease.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError::UnknownLease`] when no active lease matches.
    pub fn release(&mut self, pod_id: &str) -> Result<PodLease, CampaignError> {
        self.active
            .remove(pod_id)
            .ok_or(CampaignError::UnknownLease)
    }

    /// Returns active leases in stable pod-identity order.
    #[must_use]
    pub fn active(&self) -> Vec<&PodLease> {
        self.active.values().collect()
    }
}

fn validate_proposal_shape(
    proposal: &PodProposal,
    spec: &CampaignSpec,
) -> Result<(), CampaignError> {
    spec.verify()?;
    TaskId::new(proposal.task_id.clone()).map_err(|_| CampaignError::InvalidProposal)?;
    if proposal.version != CAMPAIGN_VERSION
        || !valid_sha256(&proposal.task_source_sha256)
        || proposal.owned_paths.is_empty()
        || proposal.owned_paths.len() > MAX_PATHS
        || proposal.dependencies.len() > MAX_RESOURCES
        || proposal.gate_tiers.is_empty()
        || proposal.gate_tiers.len() > MAX_RESOURCES
        || proposal.test_resources.is_empty()
        || proposal.test_resources.len() > MAX_RESOURCES
        || proposal
            .owned_paths
            .iter()
            .any(|path| !safe_relative(path) || !spec.permits(path))
        || any_overlap(&proposal.owned_paths)
        || proposal.dependencies.iter().any(|dependency| {
            TaskId::new(dependency.clone()).is_err() || dependency == &proposal.task_id
        })
        || proposal.dependencies.iter().collect::<BTreeSet<_>>().len()
            != proposal.dependencies.len()
        || proposal
            .gate_tiers
            .iter()
            .any(|requested| !spec.gate_tiers.iter().any(|tier| &tier.name == requested))
        || proposal.gate_tiers.iter().collect::<BTreeSet<_>>().len() != proposal.gate_tiers.len()
        || proposal
            .test_resources
            .iter()
            .any(|resource| !valid_component(resource))
        || proposal
            .test_resources
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != proposal.test_resources.len()
        || proposal.expected_artifacts.len() > MAX_PATHS
        || proposal.expected_artifacts.iter().any(|path| {
            !safe_relative(path)
                || !proposal
                    .owned_paths
                    .iter()
                    .any(|root| path_contains(root, path))
        })
        || proposal.rationale_summary.is_empty()
        || proposal.rationale_summary.len() > MAX_SUMMARY_BYTES
        || proposal.rationale_summary.chars().any(char::is_control)
        || !valid_sha256(&proposal.proposal_sha256)
    {
        return Err(CampaignError::InvalidProposal);
    }
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

const fn default_parallel_pods() -> u16 {
    1
}

fn any_overlap(paths: &[PathBuf]) -> bool {
    paths.iter().enumerate().any(|(index, left)| {
        paths
            .iter()
            .skip(index + 1)
            .any(|right| paths_overlap(left, right))
    })
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    path_contains(left, right) || path_contains(right, left)
}

fn path_contains(parent: &Path, child: &Path) -> bool {
    parent == child || child.starts_with(parent)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with(['-', '.', '/'])
        && !value.ends_with(['.', '/'])
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
}

fn valid_provider(provider: &CampaignProvider) -> bool {
    provider.executable.is_absolute()
        && valid_component(&provider.model)
        && matches!(
            provider.effort.as_str(),
            "low" | "medium" | "high" | "xhigh" | "max"
        )
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn canonical_sha256(value: &impl Serialize) -> Result<String, CampaignError> {
    let bytes = serde_json::to_vec(value).map_err(|_| CampaignError::Serialization)?;
    Ok(hex(&Sha256::digest(bytes)))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

/// Stable campaign refusal without source, path, provider, or credential content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignError {
    /// Campaign specification is unavailable or malformed.
    InvalidSpec,
    /// Campaign authority is broad, contradictory, or ambiguous.
    InvalidAuthority,
    /// Team-lead proposal is malformed or outside campaign authority.
    InvalidProposal,
    /// Proposal identity or canonical source is stale.
    StaleProposal,
    /// Pod path, task, or resource authority conflicts with an active lease.
    Conflict,
    /// Configured pod capacity is exhausted.
    Capacity,
    /// Selected pod lease does not exist.
    UnknownLease,
    /// Canonical serialization failed.
    Serialization,
}

impl fmt::Display for CampaignError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSpec => "codingmage.campaign.spec",
            Self::InvalidAuthority => "codingmage.campaign.authority",
            Self::InvalidProposal => "codingmage.campaign.proposal",
            Self::StaleProposal => "codingmage.campaign.stale_proposal",
            Self::Conflict => "codingmage.campaign.conflict",
            Self::Capacity => "codingmage.campaign.capacity",
            Self::UnknownLease => "codingmage.campaign.unknown_lease",
            Self::Serialization => "codingmage.campaign.serialization",
        })
    }
}

impl std::error::Error for CampaignError {}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_plan::TaskPlan;

    fn spec(max_parallel_pods: u16) -> CampaignSpec {
        CampaignSpec {
            version: CAMPAIGN_VERSION,
            campaign_id: "campaign-1".to_owned(),
            repository_id: "repo-1".to_owned(),
            repository_path: PathBuf::from("/tmp/codingmage-campaign-target"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
            max_parallel_pods,
            max_units: 100,
            team_lead: provider("gpt-lead", "high"),
            implementer: provider("claude-implementer", "high"),
            implementer_authentication: CampaignAuthentication::ExistingLogin,
            reviewer: provider("gpt-reviewer", "xhigh"),
            gate_tiers: vec![CampaignGateTier {
                name: "rust-focused".to_owned(),
                profiles: vec!["rust-test".to_owned(), "rust-clippy".to_owned()],
            }],
            campaign_branch: "codingmage/campaign-1".to_owned(),
            allowed_paths: vec![PathBuf::from("crates"), PathBuf::from("docs/public")],
            denied_paths: vec![PathBuf::from("docs/private")],
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
        }
    }

    fn provider(model: &str, effort: &str) -> CampaignProvider {
        CampaignProvider {
            executable: PathBuf::from("/usr/bin/provider"),
            model: model.to_owned(),
            effort: effort.to_owned(),
        }
    }

    fn plan() -> TaskPlan {
        TaskPlan::parse(b"# Tasks\n\n## Sprint 1 - Build\n\n**Sprint goal:** Build safely.\n\n### Story 1.1 - Units\n\n- [ ] **Task 1.1.1 - Work**\n  - [ ] **Sub-task 1.1.1.1:** Complete the first independent bounded unit.\n  - [ ] **Sub-task 1.1.1.2:** Complete the second independent bounded unit.\n  - [ ] **Sub-task 1.1.1.3:** Complete the third independent bounded unit.\n").unwrap()
    }

    fn proposal(selected: &SelectedWork, path: &str, resource: &str) -> PodProposal {
        PodProposal::seal(
            PodProposal {
                version: 0,
                task_id: selected.item.id.clone(),
                task_source_sha256: selected.source_sha256.clone(),
                owned_paths: vec![PathBuf::from(path)],
                dependencies: selected.item.dependencies.clone(),
                gate_tiers: vec!["rust-focused".to_owned()],
                test_resources: vec![resource.to_owned()],
                expected_artifacts: vec![PathBuf::from(path).join("artifact")],
                risk: PodRisk::Routine,
                rationale_summary: "Bounded independent fixture work.".to_owned(),
                proposal_sha256: ZERO_SHA256.to_owned(),
            },
            &spec(2),
        )
        .unwrap()
    }

    fn lead_proposal(selected: &SelectedWork, path: &str) -> TeamLeadProposal {
        TeamLeadProposal {
            task_id: selected.item.id.clone(),
            dependencies: selected.item.dependencies.clone(),
            owned_paths: vec![PathBuf::from(path)],
            gate_tiers: vec!["rust-focused".to_owned()],
            test_resources: vec![format!("resource-{}", selected.item.id.replace('.', "-"))],
            expected_artifacts: vec![PathBuf::from(path).join("artifact")],
            risk: PodRisk::Routine,
            rationale_summary: "Dependency-ready and path-bounded.".to_owned(),
        }
    }

    #[test]
    fn campaign_authority_rejects_overlap_escape_and_denied_roots() {
        let valid = spec(2);
        valid.verify().unwrap();
        assert!(valid.authority_sha256().is_ok());

        let mut overlap = valid.clone();
        overlap.allowed_paths.push(PathBuf::from("crates/engine"));
        assert_eq!(overlap.verify(), Err(CampaignError::InvalidAuthority));

        let selected = plan().select_exact("1.1.1.1").expect("ready selection");
        let escaping = PodProposal {
            owned_paths: vec![PathBuf::from("../escape")],
            ..proposal(&selected, "crates/engine", "rust")
        };
        assert_eq!(
            PodProposal::seal(escaping, &valid),
            Err(CampaignError::InvalidProposal)
        );
        let denied = PodProposal {
            owned_paths: vec![PathBuf::from("docs/private/file.md")],
            ..proposal(&selected, "crates/engine", "rust")
        };
        assert_eq!(
            PodProposal::seal(denied, &valid),
            Err(CampaignError::InvalidProposal)
        );
    }

    #[test]
    fn scheduler_admits_only_disjoint_ready_capacity() {
        let authority = spec(2);
        let ready = plan()
            .select_ready(&BTreeSet::new(), &BTreeSet::new(), 3)
            .unwrap();
        let mut scheduler = PodScheduler::new(&authority).unwrap();
        let first = scheduler
            .admit(
                &authority,
                &ready[0],
                proposal(&ready[0], "crates/engine", "engine-tests"),
            )
            .unwrap();
        assert_eq!(scheduler.active().len(), 1);

        let overlapping = proposal(&ready[1], "crates/engine/src", "other-tests");
        assert_eq!(
            scheduler.admit(&authority, &ready[1], overlapping),
            Err(CampaignError::Conflict)
        );
        let shared_resource = proposal(&ready[1], "docs/public", "engine-tests");
        assert_eq!(
            scheduler.admit(&authority, &ready[1], shared_resource),
            Err(CampaignError::Conflict)
        );
        scheduler
            .admit(
                &authority,
                &ready[1],
                proposal(&ready[1], "docs/public", "docs-tests"),
            )
            .unwrap();
        assert_eq!(
            scheduler.admit(
                &authority,
                &ready[2],
                proposal(&ready[2], "crates/cli", "cli-tests"),
            ),
            Err(CampaignError::Capacity)
        );
        assert_eq!(scheduler.release(&first.pod_id).unwrap(), first);
        assert_eq!(scheduler.active().len(), 1);
    }

    #[test]
    fn proposal_mutation_and_stale_source_are_rejected() {
        let authority = spec(1);
        let selected = plan().select_exact("1.1.1.1").unwrap();
        let mut proposed = proposal(&selected, "crates/engine", "engine-tests");
        proposed.risk = PodRisk::High;
        assert_eq!(
            proposed.verify(&authority),
            Err(CampaignError::StaleProposal)
        );

        let valid = proposal(&selected, "crates/engine", "engine-tests");
        let mut stale = selected;
        stale.source_sha256 = "f".repeat(64);
        assert_eq!(
            PodScheduler::new(&authority)
                .unwrap()
                .admit(&authority, &stale, valid),
            Err(CampaignError::Conflict)
        );
    }

    fn binding(authority: &CampaignSpec, selected: &SelectedWork) -> LeadTaskBinding {
        LeadTaskBinding {
            campaign_id: authority.campaign_id.clone(),
            campaign_head: authority.initial_commit.clone(),
            task_source_sha256: authority.task_source_sha256.clone(),
            task_id: selected.item.id.clone(),
            dependencies: selected.item.dependencies.clone(),
        }
    }

    #[test]
    fn team_lead_output_is_only_untrusted_bounded_input() {
        let authority = spec(2);
        let ready = plan()
            .select_ready(&BTreeSet::new(), &BTreeSet::new(), 3)
            .unwrap();
        let mut matching = authority.clone();
        matching.task_source_sha256 = ready[0].source_sha256.clone();
        let report = TeamLeadReport {
            campaign_id: matching.campaign_id.clone(),
            campaign_head: authority.initial_commit.clone(),
            task_source_sha256: ready[0].source_sha256.clone(),
            disposition: LeadDispositionKind::Propose,
            proposals: vec![lead_proposal(&ready[0], "crates/engine")],
            blocked: None,
            deferred: None,
            human_decision: None,
        };
        let TeamLeadOutcome::Proposals(sealed) =
            validate_team_lead_report(report, &matching, &ready).unwrap()
        else {
            panic!("expected sealed proposals");
        };
        assert_eq!(sealed.len(), 1);
        assert!(sealed[0].verify(&matching).is_ok());

        let mut invented = lead_proposal(&ready[0], "crates/engine");
        invented.dependencies.push("9.9.9.9".to_owned());
        let hostile = TeamLeadReport {
            campaign_id: matching.campaign_id.clone(),
            campaign_head: matching.initial_commit.clone(),
            task_source_sha256: matching.task_source_sha256.clone(),
            disposition: LeadDispositionKind::Propose,
            proposals: vec![invented],
            blocked: None,
            deferred: None,
            human_decision: None,
        };
        assert_eq!(
            validate_team_lead_report(hostile, &matching, &ready),
            Err(CampaignError::InvalidProposal)
        );
    }

    #[test]
    fn team_lead_human_decision_is_exclusive_and_bounded() {
        let authority = spec(1);
        let selected = plan().select_exact("1.1.1.1").unwrap();
        let report = TeamLeadReport {
            campaign_id: authority.campaign_id.clone(),
            campaign_head: authority.initial_commit.clone(),
            task_source_sha256: authority.task_source_sha256.clone(),
            disposition: LeadDispositionKind::HumanDecisionRequired,
            proposals: Vec::new(),
            blocked: None,
            deferred: None,
            human_decision: Some(HumanDecisionBlocker {
                binding: binding(&authority, &selected),
                reason: LeadHumanDecisionReason::MaterialArchitectureChoice,
                summary: "Select the public compatibility boundary.".to_owned(),
            }),
        };
        assert!(matches!(
            validate_team_lead_report(report, &authority, &[selected]).unwrap(),
            TeamLeadOutcome::HumanDecision(_)
        ));
    }

    #[test]
    fn closed_dispositions_are_exclusive_typed_and_snapshot_bound() {
        let mut authority = spec(1);
        let selected = plan().select_exact("1.1.1.1").unwrap();
        authority.task_source_sha256 = selected.source_sha256.clone();
        let bound = binding(&authority, &selected);

        let blocked = TeamLeadReport {
            campaign_id: authority.campaign_id.clone(),
            campaign_head: authority.initial_commit.clone(),
            task_source_sha256: authority.task_source_sha256.clone(),
            disposition: LeadDispositionKind::Blocked,
            proposals: Vec::new(),
            blocked: Some(LeadBlockedDisposition {
                binding: bound.clone(),
                reason: LeadBlockedReason::UnavailableExternalDependency,
            }),
            deferred: None,
            human_decision: None,
        };
        assert!(matches!(
            validate_team_lead_report(blocked.clone(), &authority, std::slice::from_ref(&selected))
                .unwrap(),
            TeamLeadOutcome::Blocked(_)
        ));

        let deferred = TeamLeadReport {
            disposition: LeadDispositionKind::Deferred,
            blocked: None,
            deferred: Some(LeadDeferredDisposition {
                binding: bound.clone(),
                reason: LeadDeferredReason::GateResourceContention,
                reconsideration_trigger: LeadReconsiderationTrigger::GateResourceRelease,
            }),
            ..blocked.clone()
        };
        assert!(matches!(
            validate_team_lead_report(deferred, &authority, std::slice::from_ref(&selected))
                .unwrap(),
            TeamLeadOutcome::Deferred(_)
        ));

        let mut mixed = blocked.clone();
        mixed.proposals = vec![lead_proposal(&selected, "crates/engine")];
        assert_eq!(
            validate_team_lead_report(mixed, &authority, std::slice::from_ref(&selected)),
            Err(CampaignError::InvalidProposal)
        );

        for field in 0..5 {
            let mut stale = blocked.clone();
            let binding = &mut stale.blocked.as_mut().unwrap().binding;
            match field {
                0 => binding.campaign_id.push_str("-other"),
                1 => binding.campaign_head = "f".repeat(40),
                2 => binding.task_source_sha256 = "f".repeat(64),
                3 => binding.task_id.push_str(".9"),
                _ => binding.dependencies.push("9.9.9.9".to_owned()),
            }
            assert_eq!(
                validate_team_lead_report(stale, &authority, std::slice::from_ref(&selected)),
                Err(CampaignError::InvalidProposal)
            );
        }

        let mut unknown = serde_json::to_value(blocked).unwrap();
        unknown["blocked"]["reason"] = serde_json::Value::String("invented_reason".to_owned());
        assert!(serde_json::from_value::<TeamLeadReport>(unknown).is_err());
    }
}
