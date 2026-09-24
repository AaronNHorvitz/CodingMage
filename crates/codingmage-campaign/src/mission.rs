//! Mission charter, decision-domain grants and owner-involvement contracts (Sprint 32).
//!
//! A mission charter is an immutable owner-authored document that binds one exact campaign
//! authority to measurable outcomes, explicit exclusions, named decision domains the coordinator
//! may resolve without the owner, an involvement mode, budgets, expiry and a revocation epoch.
//! The charter never widens the campaign specification: every scope, gate and risk grant must be
//! contained in the campaign authority it references. Role-authored decision proposals are data;
//! [`evaluate_decision`] derives the only permitted outcomes deterministically.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use codingmage_contracts::{DecisionClass, DecisionProposal, PodRisk};
use serde::{Deserialize, Serialize};

use super::{
    CampaignError, CampaignSpec, MAX_SPEC_BYTES, MAX_SUMMARY_BYTES, any_overlap, canonical_sha256,
    path_contains, safe_relative, valid_commit, valid_component, valid_sha256,
};

/// Closed mission charter schema version.
pub const MISSION_VERSION: u16 = 1;
const MAX_TEXT_BYTES: usize = MAX_SUMMARY_BYTES;
const MAX_TEXT_ITEMS: usize = 256;
const MAX_DOMAINS: usize = 64;
const MAX_ALTERNATIVES: usize = 32;
const MAX_MISSION_ELAPSED_MS: u64 = 365 * 24 * 60 * 60 * 1000;
const MAX_MISSION_COUNTER: u32 = 1_000_000;

/// Owner involvement during one admitted mission. The mode never changes delivery authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvolvementMode {
    /// Undelegated choices are routed to the owner as exact checkpoint requests.
    Supervised,
    /// Undelegated choices produce one deduplicated exception request each; work continues.
    ExceptionOnly,
    /// No mid-campaign request is issued; undelegated choices are retained as blockers.
    HandsOff,
}

impl InvolvementMode {
    /// Stable content-free mode code for status and durable projections.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Supervised => "supervised",
            Self::ExceptionOnly => "exception_only",
            Self::HandsOff => "hands_off",
        }
    }

    /// True when the mode may issue a request for an owner answer.
    #[must_use]
    pub const fn may_request_owner_decision(self) -> bool {
        !matches!(self, Self::HandsOff)
    }
}

/// What the coordinator does with an in-domain proposal that selects no approved alternative.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationDisposition {
    /// Ask the owner through the exact control channel; invalid under hands-off.
    AskOwner,
    /// Defer until a declared observation changes.
    Defer,
    /// Retain the affected work as blocked.
    Block,
}

/// One named decision domain the owner delegates to the coordinator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionDomainGrant {
    /// Stable domain identity referenced by proposals and decision records.
    pub domain_id: String,
    /// Closed class of choice delegated in this domain.
    pub class: DecisionClass,
    /// Closed approved alternatives; a proposal must select exactly one.
    pub alternatives: Vec<String>,
    /// Repository-relative roots this domain may affect; each must be campaign-permitted.
    pub scope_paths: Vec<PathBuf>,
    /// Highest risk a proposal in this domain may carry.
    pub max_risk: PodRisk,
    /// Gate tiers every proposal in this domain must request; each must exist in the campaign.
    pub required_gate_tiers: Vec<String>,
    /// Disposition for an in-domain proposal outside the approved alternatives.
    pub escalation: EscalationDisposition,
}

/// Independent mission budgets that bound decision handling and no-progress loops.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionBudgets {
    /// Maximum decision records the mission may accumulate.
    pub max_decisions: u32,
    /// Maximum times one decision identity may be re-proposed without a new outcome.
    pub max_decision_retries: u16,
    /// Maximum consecutive planning cycles without progress before the campaign holds.
    pub max_no_progress_cycles: u16,
}

impl MissionBudgets {
    fn verify(self) -> Result<(), CampaignError> {
        if self.max_decisions == 0
            || self.max_decisions > MAX_MISSION_COUNTER
            || self.max_decision_retries == 0
            || self.max_decision_retries > 1_000
            || self.max_no_progress_cycles == 0
            || self.max_no_progress_cycles > 1_000
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }
}

/// Versioned immutable owner mission charter bound to one exact campaign authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionCharter {
    /// Closed schema version.
    pub version: u16,
    /// Stable mission identity.
    pub mission_id: String,
    /// Monotonic authority generation; a re-issued charter must increase it.
    pub generation: u64,
    /// Exact campaign this charter governs.
    pub campaign_id: String,
    /// Exact repository identity from the campaign authority.
    pub repository_id: String,
    /// Exact campaign starting commit.
    pub initial_commit: String,
    /// Exact canonical task source observed at the starting commit.
    pub task_source_sha256: String,
    /// Digest of the external operator authorization record.
    pub operator_authorization_sha256: String,
    /// Digest of the complete campaign authority this charter binds to.
    pub campaign_authority_sha256: String,
    /// Bounded owner objective; inspectable text, never authority.
    pub objective: String,
    /// Measurable outcomes required before the objective can be reported satisfied.
    pub success_criteria: Vec<String>,
    /// Work the mission must not perform even if a task or provider suggests it.
    pub exclusions: Vec<String>,
    /// Constraints every delegated choice must preserve.
    pub architecture_invariants: Vec<String>,
    /// Delegated decision domains; absence means every material choice needs the owner.
    pub decision_domains: Vec<DecisionDomainGrant>,
    /// Gate profile names the mission may request; each must exist in a campaign gate tier.
    pub command_registry: Vec<String>,
    /// Owner involvement mode.
    pub involvement: InvolvementMode,
    /// Independent decision and no-progress budgets.
    pub budgets: MissionBudgets,
    /// Charter issue time in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Charter expiry in Unix milliseconds; no effect may start after it.
    pub expires_at_ms: u64,
    /// Revocation epoch this charter was issued under; durable state may only move forward.
    pub revocation_epoch: u64,
}

impl MissionCharter {
    /// Loads one absolute, regular, nonsymlink charter and validates it against the campaign.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] for unavailable, oversized, malformed or unbound charters.
    pub fn load(path: &Path, spec: &CampaignSpec) -> Result<Self, CampaignError> {
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
        let charter: Self = toml::from_str(&source).map_err(|_| CampaignError::InvalidSpec)?;
        charter.verify(spec)?;
        Ok(charter)
    }

    /// Revalidates every authority-bearing field against the exact campaign authority.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError::InvalidAuthority`] when the charter is unbound, contradictory,
    /// wider than the campaign or names an involvement-incompatible escalation.
    #[allow(clippy::too_many_lines)]
    pub fn verify(&self, spec: &CampaignSpec) -> Result<(), CampaignError> {
        let authority_sha256 = spec.authority_sha256()?;
        if self.version != MISSION_VERSION
            || !valid_component(&self.mission_id)
            || self.generation == 0
            || self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || !valid_commit(&self.initial_commit)
            || self.initial_commit != spec.initial_commit
            || !valid_sha256(&self.task_source_sha256)
            || self.task_source_sha256 != spec.task_source_sha256
            || !valid_sha256(&self.operator_authorization_sha256)
            || self.operator_authorization_sha256 != spec.operator_authorization_sha256
            || !valid_sha256(&self.campaign_authority_sha256)
            || self.campaign_authority_sha256 != authority_sha256
            || !bounded_text(&self.objective)
            || self.success_criteria.is_empty()
            || !bounded_texts(&self.success_criteria)
            || !bounded_texts(&self.exclusions)
            || !bounded_texts(&self.architecture_invariants)
            || self.decision_domains.len() > MAX_DOMAINS
            || self.command_registry.is_empty()
            || self.command_registry.len() > MAX_TEXT_ITEMS
            || !unique(&self.command_registry)
            || self
                .command_registry
                .iter()
                .any(|profile| !known_gate_profile(spec, profile))
            || self.budgets.verify().is_err()
            || self.issued_at_ms == 0
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms - self.issued_at_ms > MAX_MISSION_ELAPSED_MS
            || !unique(
                &self
                    .decision_domains
                    .iter()
                    .map(|domain| domain.domain_id.clone())
                    .collect::<Vec<_>>(),
            )
            || self
                .decision_domains
                .iter()
                .any(|domain| domain.verify(spec, self.involvement).is_err())
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }

    /// Returns the canonical digest of the complete verified charter.
    ///
    /// # Errors
    ///
    /// Returns [`CampaignError`] when the charter or serialization is invalid.
    pub fn mission_sha256(&self, spec: &CampaignSpec) -> Result<String, CampaignError> {
        self.verify(spec)?;
        canonical_sha256(self)
    }

    /// Returns the delegated domain with the exact identity, if any.
    #[must_use]
    pub fn domain(&self, domain_id: &str) -> Option<&DecisionDomainGrant> {
        self.decision_domains
            .iter()
            .find(|domain| domain.domain_id == domain_id)
    }

    /// True when `now_ms` lies inside the charter's validity window.
    #[must_use]
    pub const fn valid_at(&self, now_ms: u64) -> bool {
        now_ms >= self.issued_at_ms && now_ms < self.expires_at_ms
    }
}

impl DecisionDomainGrant {
    fn verify(
        &self,
        spec: &CampaignSpec,
        involvement: InvolvementMode,
    ) -> Result<(), CampaignError> {
        if !valid_component(&self.domain_id)
            || self.alternatives.is_empty()
            || self.alternatives.len() > MAX_ALTERNATIVES
            || !unique(&self.alternatives)
            || self
                .alternatives
                .iter()
                .any(|alternative| !valid_component(alternative))
            || self.scope_paths.is_empty()
            || self.scope_paths.len() > MAX_TEXT_ITEMS
            || self
                .scope_paths
                .iter()
                .any(|path| !safe_relative(path) || !spec.permits(path))
            || any_overlap(&self.scope_paths)
            || self.required_gate_tiers.is_empty()
            || !unique(&self.required_gate_tiers)
            || self
                .required_gate_tiers
                .iter()
                .any(|tier| !spec.gate_tiers.iter().any(|known| &known.name == tier))
            || (self.escalation == EscalationDisposition::AskOwner
                && !involvement.may_request_owner_decision())
        {
            return Err(CampaignError::InvalidAuthority);
        }
        Ok(())
    }

    /// True when every affected path is inside this domain's scope.
    #[must_use]
    pub fn covers(&self, paths: &[PathBuf]) -> bool {
        !paths.is_empty()
            && paths.iter().all(|path| {
                safe_relative(path)
                    && self
                        .scope_paths
                        .iter()
                        .any(|scope| path_contains(scope, path))
            })
    }
}

fn well_formed(proposal: &DecisionProposal) -> bool {
    valid_component(&proposal.decision_id)
        && valid_component(&proposal.domain_id)
        && valid_component(&proposal.selected_alternative)
        && proposal.affected_paths.len() <= MAX_TEXT_ITEMS
        && proposal.gate_tiers.len() <= MAX_TEXT_ITEMS
        && unique(&proposal.gate_tiers)
        && bounded_text(&proposal.rationale_summary)
}

/// Coordinator-observed facts a decision is evaluated against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionObservation {
    /// Current Unix time in milliseconds.
    pub now_ms: u64,
    /// Current durable revocation epoch; must not be older than the charter's.
    pub revocation_epoch: u64,
    /// True when the mission has been revoked.
    pub revoked: bool,
    /// Decision records already retained for this mission.
    pub decisions_recorded: u32,
    /// Prior outcomes recorded for this exact decision identity.
    pub prior_attempts: u16,
}

/// Closed reasons why a proposal did not become a permitted choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionHoldReason {
    /// The charter expired or is not yet valid.
    MissionExpired,
    /// The mission was revoked.
    MissionRevoked,
    /// No delegated domain covers this decision.
    UndelegatedDecision,
    /// The proposal claims a class, path, risk or gate outside the domain grant.
    AuthorityExpansion,
    /// The selected alternative is not an approved one.
    UnapprovedAlternative,
    /// The mission decision budget is exhausted.
    DecisionBudgetExhausted,
    /// The same decision identity was re-proposed beyond its retry budget.
    NoProgress,
}

impl DecisionHoldReason {
    /// Stable content-free reason code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::MissionExpired => "mission_expired",
            Self::MissionRevoked => "mission_revoked",
            Self::UndelegatedDecision => "undelegated_decision",
            Self::AuthorityExpansion => "authority_expansion",
            Self::UnapprovedAlternative => "unapproved_alternative",
            Self::DecisionBudgetExhausted => "decision_budget_exhausted",
            Self::NoProgress => "no_progress",
        }
    }
}

/// Deterministic outcome of evaluating one proposal against the charter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MissionDecisionOutcome {
    /// The choice lies inside a delegated domain; the coordinator may apply it.
    PermittedChoice {
        /// Exact domain that permitted the choice.
        domain_id: String,
        /// Exact approved alternative selected.
        alternative: String,
        /// Risk after deterministic escalation.
        risk: PodRisk,
    },
    /// The owner must answer through the exact control channel; no effect may proceed.
    DecisionNeeded {
        /// Why the owner is needed.
        reason: DecisionHoldReason,
    },
    /// Retain the work until a declared observation changes; no effect may proceed.
    Deferred {
        /// Why the work is deferred.
        reason: DecisionHoldReason,
    },
    /// Retain the affected work as blocked; no effect may proceed.
    Blocked {
        /// Why the work is blocked.
        reason: DecisionHoldReason,
    },
}

impl MissionDecisionOutcome {
    /// Stable content-free outcome code for durable records and status.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PermittedChoice { .. } => "permitted_choice",
            Self::DecisionNeeded { .. } => "decision_needed",
            Self::Deferred { .. } => "deferred",
            Self::Blocked { .. } => "blocked",
        }
    }

    /// True only for a permitted choice; every other outcome creates no authority.
    #[must_use]
    pub const fn permits_effect(&self) -> bool {
        matches!(self, Self::PermittedChoice { .. })
    }
}

/// Evaluates one untrusted proposal against the verified charter and observed state.
///
/// Every path other than [`MissionDecisionOutcome::PermittedChoice`] creates no lease, process,
/// Git or external effect. An expired or revoked mission never permits a choice, hands-off never
/// asks the owner, and a proposal that widens class, paths, risk or gates beyond its domain is
/// blocked regardless of the domain's configured escalation.
///
/// # Errors
///
/// Returns [`CampaignError::InvalidProposal`] for malformed proposals and
/// [`CampaignError::InvalidAuthority`] for an unverifiable charter or a stale revocation epoch.
pub fn evaluate_decision(
    charter: &MissionCharter,
    spec: &CampaignSpec,
    proposal: &DecisionProposal,
    observation: &DecisionObservation,
) -> Result<MissionDecisionOutcome, CampaignError> {
    charter.verify(spec)?;
    if observation.revocation_epoch < charter.revocation_epoch {
        return Err(CampaignError::InvalidAuthority);
    }
    if !well_formed(proposal) {
        return Err(CampaignError::InvalidProposal);
    }
    if observation.revoked || observation.revocation_epoch > charter.revocation_epoch {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::MissionRevoked,
        });
    }
    if !charter.valid_at(observation.now_ms) {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::MissionExpired,
        });
    }
    if observation.decisions_recorded >= charter.budgets.max_decisions {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::DecisionBudgetExhausted,
        });
    }
    if observation.prior_attempts >= charter.budgets.max_decision_retries {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::NoProgress,
        });
    }
    let Some(domain) = charter.domain(&proposal.domain_id) else {
        return Ok(undelegated(charter.involvement));
    };
    let expands_authority = domain.class != proposal.class
        || !domain.covers(&proposal.affected_paths)
        || (domain.max_risk == PodRisk::Routine && proposal.risk == PodRisk::High)
        || !domain
            .required_gate_tiers
            .iter()
            .all(|tier| proposal.gate_tiers.contains(tier))
        || proposal
            .gate_tiers
            .iter()
            .any(|tier| !spec.gate_tiers.iter().any(|known| &known.name == tier));
    if expands_authority {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::AuthorityExpansion,
        });
    }
    if !domain
        .alternatives
        .iter()
        .any(|alternative| alternative == &proposal.selected_alternative)
    {
        return Ok(match domain.escalation {
            EscalationDisposition::AskOwner if charter.involvement.may_request_owner_decision() => {
                MissionDecisionOutcome::DecisionNeeded {
                    reason: DecisionHoldReason::UnapprovedAlternative,
                }
            }
            EscalationDisposition::Defer => MissionDecisionOutcome::Deferred {
                reason: DecisionHoldReason::UnapprovedAlternative,
            },
            EscalationDisposition::AskOwner | EscalationDisposition::Block => {
                MissionDecisionOutcome::Blocked {
                    reason: DecisionHoldReason::UnapprovedAlternative,
                }
            }
        });
    }
    let risk = if proposal
        .affected_paths
        .iter()
        .any(|path| super::sensitive_review_path(path))
    {
        PodRisk::High
    } else {
        proposal.risk
    };
    if risk == PodRisk::High && domain.max_risk == PodRisk::Routine {
        return Ok(MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::AuthorityExpansion,
        });
    }
    Ok(MissionDecisionOutcome::PermittedChoice {
        domain_id: domain.domain_id.clone(),
        alternative: proposal.selected_alternative.clone(),
        risk,
    })
}

const fn undelegated(involvement: InvolvementMode) -> MissionDecisionOutcome {
    if involvement.may_request_owner_decision() {
        MissionDecisionOutcome::DecisionNeeded {
            reason: DecisionHoldReason::UndelegatedDecision,
        }
    } else {
        MissionDecisionOutcome::Blocked {
            reason: DecisionHoldReason::UndelegatedDecision,
        }
    }
}

fn bounded_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_TEXT_BYTES
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\n')
}

fn bounded_texts(values: &[String]) -> bool {
    values.len() <= MAX_TEXT_ITEMS && values.iter().all(|value| bounded_text(value))
}

fn unique<T: Ord>(values: &[T]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn known_gate_profile(spec: &CampaignSpec, profile: &str) -> bool {
    spec.gate_tiers
        .iter()
        .any(|tier| tier.profiles.iter().any(|known| known == profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    type CharterMutation = fn(&mut MissionCharter);
    type ProposalMutation = fn(&mut DecisionProposal);
    use crate::{
        CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
        CampaignPublication,
    };

    fn spec() -> CampaignSpec {
        let provider = CampaignProvider {
            executable: PathBuf::from("/bin/true"),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        CampaignSpec {
            version: 3,
            campaign_id: "fixture-mission".to_owned(),
            repository_id: "repo-fixture".to_owned(),
            repository_path: PathBuf::from("/tmp/fixture-mission"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            operator_authorization_sha256: "c".repeat(64),
            max_parallel_pods: 1,
            max_units: 10,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 10,
                process_invocations: 1_000,
                output_bytes: 1_000_000,
                retained_state_bytes: 1_000_000,
                execution_elapsed_ms: 100_000,
            },
            team_lead: provider.clone(),
            implementer: provider.clone(),
            implementer_authentication: CampaignAuthentication::ExistingLogin,
            reviewer: provider,
            gate_tiers: vec![
                CampaignGateTier {
                    name: "focused".to_owned(),
                    profiles: vec!["unit".to_owned()],
                },
                CampaignGateTier {
                    name: "full".to_owned(),
                    profiles: vec!["unit".to_owned(), "integration".to_owned()],
                },
            ],
            campaign_branch: "codingmage/fixture-mission".to_owned(),
            allowed_paths: vec![PathBuf::from("src"), PathBuf::from("docs")],
            task_path_authority: Vec::new(),
            denied_paths: vec![PathBuf::from("secrets")],
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        }
    }

    fn domain() -> DecisionDomainGrant {
        DecisionDomainGrant {
            domain_id: "storage-layout".to_owned(),
            class: DecisionClass::Architecture,
            alternatives: vec!["single-file".to_owned(), "per-task-file".to_owned()],
            scope_paths: vec![PathBuf::from("src/storage")],
            max_risk: PodRisk::Routine,
            required_gate_tiers: vec!["focused".to_owned()],
            escalation: EscalationDisposition::Block,
        }
    }

    fn charter(involvement: InvolvementMode) -> MissionCharter {
        let spec = spec();
        MissionCharter {
            version: MISSION_VERSION,
            mission_id: "mission-1".to_owned(),
            generation: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            initial_commit: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            operator_authorization_sha256: spec.operator_authorization_sha256.clone(),
            campaign_authority_sha256: spec.authority_sha256().unwrap(),
            objective: "Ship the storage layer".to_owned(),
            success_criteria: vec!["all storage sub-tasks accepted".to_owned()],
            exclusions: vec!["no network access".to_owned()],
            architecture_invariants: vec!["one writer per checkout".to_owned()],
            decision_domains: vec![domain()],
            command_registry: vec!["unit".to_owned()],
            involvement,
            budgets: MissionBudgets {
                max_decisions: 10,
                max_decision_retries: 2,
                max_no_progress_cycles: 3,
            },
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            revocation_epoch: 0,
        }
    }

    fn proposal() -> DecisionProposal {
        DecisionProposal {
            decision_id: "decision-1".to_owned(),
            domain_id: "storage-layout".to_owned(),
            class: DecisionClass::Architecture,
            selected_alternative: "per-task-file".to_owned(),
            affected_paths: vec![PathBuf::from("src/storage/journal.rs")],
            risk: PodRisk::Routine,
            gate_tiers: vec!["focused".to_owned()],
            rationale_summary: "smaller documents".to_owned(),
        }
    }

    fn observation() -> DecisionObservation {
        DecisionObservation {
            now_ms: 5_000,
            revocation_epoch: 0,
            revoked: false,
            decisions_recorded: 0,
            prior_attempts: 0,
        }
    }

    #[test]
    fn verified_charter_has_stable_digest_and_round_trips_through_toml() {
        let spec = spec();
        let charter = charter(InvolvementMode::HandsOff);
        assert_eq!(charter.verify(&spec), Ok(()));
        let digest = charter.mission_sha256(&spec).unwrap();
        let encoded = toml::to_string(&charter).unwrap();
        let decoded: MissionCharter = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, charter);
        assert_eq!(decoded.mission_sha256(&spec).unwrap(), digest);
    }

    #[test]
    fn charter_rejects_unknown_fields_and_bad_schema() {
        let spec = spec();
        let mut encoded = toml::to_string(&charter(InvolvementMode::Supervised)).unwrap();
        encoded.push_str("\nunexpected = true\n");
        assert!(toml::from_str::<MissionCharter>(&encoded).is_err());
        let mut charter = charter(InvolvementMode::Supervised);
        charter.version = 2;
        assert_eq!(charter.verify(&spec), Err(CampaignError::InvalidAuthority));
    }

    #[test]
    fn charter_must_bind_the_exact_campaign_authority() {
        let spec = spec();
        let mutations: Vec<(&str, CharterMutation)> = vec![
            ("campaign", |charter| {
                charter.campaign_id = "other".to_owned();
            }),
            ("repository", |charter| {
                charter.repository_id = "other-repo".to_owned();
            }),
            ("commit", |charter| charter.initial_commit = "d".repeat(40)),
            ("source", |charter| {
                charter.task_source_sha256 = "e".repeat(64);
            }),
            ("authorization", |charter| {
                charter.operator_authorization_sha256 = "f".repeat(64);
            }),
            ("authority", |charter| {
                charter.campaign_authority_sha256 = "0".repeat(64);
            }),
            ("generation", |charter| charter.generation = 0),
            ("expiry", |charter| {
                charter.expires_at_ms = charter.issued_at_ms;
            }),
            ("criteria", |charter| charter.success_criteria.clear()),
            ("registry", |charter| {
                charter.command_registry = vec!["shell".to_owned()];
            }),
            ("budget", |charter| charter.budgets.max_decisions = 0),
            ("scope", |charter| {
                charter.decision_domains[0].scope_paths = vec![PathBuf::from("secrets")];
            }),
            ("escape", |charter| {
                charter.decision_domains[0].scope_paths = vec![PathBuf::from("../src")];
            }),
            ("tier", |charter| {
                charter.decision_domains[0].required_gate_tiers = vec!["missing".to_owned()];
            }),
            ("alternatives", |charter| {
                charter.decision_domains[0].alternatives.clear();
            }),
            ("duplicate-domain", |charter| {
                let duplicate = charter.decision_domains[0].clone();
                charter.decision_domains.push(duplicate);
            }),
        ];
        for (name, mutate) in mutations {
            let mut charter = charter(InvolvementMode::Supervised);
            mutate(&mut charter);
            assert_eq!(
                charter.verify(&spec),
                Err(CampaignError::InvalidAuthority),
                "{name}"
            );
        }
    }

    #[test]
    fn hands_off_charter_cannot_configure_owner_prompts() {
        let spec = spec();
        let mut charter = charter(InvolvementMode::HandsOff);
        charter.decision_domains[0].escalation = EscalationDisposition::AskOwner;
        assert_eq!(charter.verify(&spec), Err(CampaignError::InvalidAuthority));
        let mut supervised = charter.clone();
        supervised.involvement = InvolvementMode::ExceptionOnly;
        assert_eq!(supervised.verify(&spec), Ok(()));
    }

    #[test]
    fn delegated_choice_is_permitted_without_owner_in_every_mode() {
        let spec = spec();
        for involvement in [
            InvolvementMode::Supervised,
            InvolvementMode::ExceptionOnly,
            InvolvementMode::HandsOff,
        ] {
            let outcome =
                evaluate_decision(&charter(involvement), &spec, &proposal(), &observation())
                    .unwrap();
            assert_eq!(
                outcome,
                MissionDecisionOutcome::PermittedChoice {
                    domain_id: "storage-layout".to_owned(),
                    alternative: "per-task-file".to_owned(),
                    risk: PodRisk::Routine,
                }
            );
            assert!(outcome.permits_effect());
        }
    }

    #[test]
    fn undelegated_choice_asks_owner_only_when_the_mode_allows_it() {
        let spec = spec();
        let mut proposal = proposal();
        proposal.domain_id = "unknown-domain".to_owned();
        let supervised = evaluate_decision(
            &charter(InvolvementMode::Supervised),
            &spec,
            &proposal,
            &observation(),
        )
        .unwrap();
        assert_eq!(
            supervised,
            MissionDecisionOutcome::DecisionNeeded {
                reason: DecisionHoldReason::UndelegatedDecision
            }
        );
        let hands_off = evaluate_decision(
            &charter(InvolvementMode::HandsOff),
            &spec,
            &proposal,
            &observation(),
        )
        .unwrap();
        assert_eq!(
            hands_off,
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::UndelegatedDecision
            }
        );
        assert!(!hands_off.permits_effect());
    }

    #[test]
    fn scope_class_risk_and_gate_expansion_are_blocked_in_every_mode() {
        let spec = spec();
        let mutations: Vec<(&str, ProposalMutation)> = vec![
            ("class", |proposal| {
                proposal.class = DecisionClass::Dependency;
            }),
            ("path", |proposal| {
                proposal.affected_paths = vec![PathBuf::from("src/network/client.rs")];
            }),
            ("escape", |proposal| {
                proposal.affected_paths = vec![PathBuf::from("../src/storage/x.rs")];
            }),
            ("empty-paths", |proposal| proposal.affected_paths.clear()),
            ("risk", |proposal| proposal.risk = PodRisk::High),
            ("gates", |proposal| proposal.gate_tiers.clear()),
            ("unknown-gate", |proposal| {
                proposal.gate_tiers = vec!["focused".to_owned(), "nonexistent".to_owned()];
            }),
            ("sensitive", |proposal| {
                proposal.affected_paths = vec![PathBuf::from("src/storage/schemas/x.json")];
            }),
        ];
        for involvement in [
            InvolvementMode::Supervised,
            InvolvementMode::ExceptionOnly,
            InvolvementMode::HandsOff,
        ] {
            for (name, mutate) in &mutations {
                let mut proposal = proposal();
                mutate(&mut proposal);
                let outcome =
                    evaluate_decision(&charter(involvement), &spec, &proposal, &observation())
                        .unwrap();
                assert_eq!(
                    outcome,
                    MissionDecisionOutcome::Blocked {
                        reason: DecisionHoldReason::AuthorityExpansion
                    },
                    "{name} under {}",
                    involvement.code()
                );
            }
        }
    }

    #[test]
    fn unapproved_alternative_follows_the_domain_escalation_without_widening_it() {
        let spec = spec();
        let mut proposal = proposal();
        proposal.selected_alternative = "sqlite".to_owned();
        let cases = [
            (
                InvolvementMode::Supervised,
                EscalationDisposition::AskOwner,
                MissionDecisionOutcome::DecisionNeeded {
                    reason: DecisionHoldReason::UnapprovedAlternative,
                },
            ),
            (
                InvolvementMode::ExceptionOnly,
                EscalationDisposition::Defer,
                MissionDecisionOutcome::Deferred {
                    reason: DecisionHoldReason::UnapprovedAlternative,
                },
            ),
            (
                InvolvementMode::HandsOff,
                EscalationDisposition::Block,
                MissionDecisionOutcome::Blocked {
                    reason: DecisionHoldReason::UnapprovedAlternative,
                },
            ),
        ];
        for (involvement, escalation, expected) in cases {
            let mut charter = charter(involvement);
            charter.decision_domains[0].escalation = escalation;
            assert_eq!(
                evaluate_decision(&charter, &spec, &proposal, &observation()).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn expired_revoked_and_exhausted_missions_never_permit_a_choice() {
        let spec = spec();
        let charter = charter(InvolvementMode::HandsOff);
        let mut expired = observation();
        expired.now_ms = 10_000;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &expired).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::MissionExpired
            }
        );
        let mut early = observation();
        early.now_ms = 999;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &early).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::MissionExpired
            }
        );
        let mut revoked = observation();
        revoked.revoked = true;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &revoked).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::MissionRevoked
            }
        );
        let mut newer_epoch = observation();
        newer_epoch.revocation_epoch = 1;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &newer_epoch).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::MissionRevoked
            }
        );
        let mut exhausted = observation();
        exhausted.decisions_recorded = 10;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &exhausted).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::DecisionBudgetExhausted
            }
        );
        let mut looping = observation();
        looping.prior_attempts = 2;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &looping).unwrap(),
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::NoProgress
            }
        );
    }

    #[test]
    fn stale_charter_epoch_and_malformed_proposal_are_errors_not_outcomes() {
        let spec = spec();
        let mut charter = charter(InvolvementMode::HandsOff);
        charter.revocation_epoch = 3;
        assert_eq!(
            evaluate_decision(&charter, &spec, &proposal(), &observation()),
            Err(CampaignError::InvalidAuthority)
        );
        let mut malformed = proposal();
        malformed.decision_id = "bad id".to_owned();
        assert_eq!(
            evaluate_decision(
                &self::charter(InvolvementMode::HandsOff),
                &spec,
                &malformed,
                &observation()
            ),
            Err(CampaignError::InvalidProposal)
        );
        let mut unbound = self::charter(InvolvementMode::HandsOff);
        unbound.campaign_authority_sha256 = "0".repeat(64);
        assert_eq!(
            evaluate_decision(&unbound, &spec, &proposal(), &observation()),
            Err(CampaignError::InvalidAuthority)
        );
    }

    #[test]
    fn charter_load_requires_absolute_regular_bounded_file() {
        let spec = spec();
        assert_eq!(
            MissionCharter::load(Path::new("relative.toml"), &spec),
            Err(CampaignError::InvalidSpec)
        );
        let root = std::env::temp_dir().join(format!(
            "codingmage-mission-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("mission.toml");
        fs::write(
            &path,
            toml::to_string(&charter(InvolvementMode::HandsOff)).unwrap(),
        )
        .unwrap();
        let loaded = MissionCharter::load(&path, &spec).unwrap();
        assert_eq!(loaded.involvement, InvolvementMode::HandsOff);
        fs::write(&path, "version = 1\n").unwrap();
        assert_eq!(
            MissionCharter::load(&path, &spec),
            Err(CampaignError::InvalidSpec)
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
