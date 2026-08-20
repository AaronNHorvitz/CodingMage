//! Campaign authority, team-lead proposals, and deterministic pod leases.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_contracts::TaskId;
use codingmage_plan::SelectedWork;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CAMPAIGN_VERSION: u16 = 1;
const MAX_SPEC_BYTES: u64 = 1024 * 1024;
const MAX_PATHS: usize = 256;
const MAX_RESOURCES: usize = 256;
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
    /// Maximum simultaneous implementation pods.
    pub max_parallel_pods: u16,
    /// Maximum accepted units before an operator-authored continuation is required.
    pub max_units: u32,
    /// Maximum aggregate provider spend represented as a decimal string.
    pub maximum_budget_usd: String,
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
            || !(1..=16).contains(&self.max_parallel_pods)
            || self.max_units == 0
            || self.max_units > 100_000
            || !valid_budget(&self.maximum_budget_usd)
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
                !self
                    .allowed_paths
                    .iter()
                    .any(|allowed| path_contains(allowed, denied))
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

/// Deterministic task risk class proposed by the read-only campaign lead.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PodRisk {
    /// Bounded implementation with no shared security or architecture contract.
    Routine,
    /// Shared contract, architecture, security, concurrency, or publication-sensitive work.
    High,
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
    /// Shared gate or integration resources requested by this pod.
    pub test_resources: Vec<String>,
    /// Deterministic risk class subject to coordinator escalation.
    pub risk: PodRisk,
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
        || proposal.test_resources.is_empty()
        || proposal.test_resources.len() > MAX_RESOURCES
        || proposal
            .owned_paths
            .iter()
            .any(|path| !safe_relative(path) || !spec.permits(path))
        || any_overlap(&proposal.owned_paths)
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

fn valid_budget(value: &str) -> bool {
    value.len() <= 16
        && value
            .parse::<f64>()
            .is_ok_and(|budget| budget > 0.0 && budget <= 1_000_000.0)
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
            version: 1,
            campaign_id: "campaign-1".to_owned(),
            max_parallel_pods,
            max_units: 100,
            maximum_budget_usd: "50.00".to_owned(),
            campaign_branch: "codingmage/campaign-1".to_owned(),
            allowed_paths: vec![PathBuf::from("crates"), PathBuf::from("docs")],
            denied_paths: vec![PathBuf::from("docs/private")],
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
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
                test_resources: vec![resource.to_owned()],
                risk: PodRisk::Routine,
                proposal_sha256: ZERO_SHA256.to_owned(),
            },
            &spec(2),
        )
        .unwrap()
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
}
