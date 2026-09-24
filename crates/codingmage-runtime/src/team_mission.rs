//! Durable mission authority: charter admission, involvement, decisions, expiry and revocation.
//!
//! The mission document lives beside the campaign's other integrity-protected controls and is
//! bound to the exact campaign authority digest, mission digest and identities. Both campaign
//! engines re-read it before effects; a revoked or expired mission stops new effects the same way
//! an authenticated cancellation does, and nothing in this module starts a process or touches
//! the target repository.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use codingmage_campaign::{
    CampaignSpec, DecisionObservation, DecisionProposal, InvolvementMode, MissionCharter,
    MissionDecisionOutcome, evaluate_decision,
};
use codingmage_codex::{CodexLeadBinding, CodexLeadDecision, CodexLeadDomain};
use codingmage_contracts::{HumanDecisionBlocker, RunId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_service::CoordinatorLock;
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{RuntimeError, canonical_file, generated_run_id, private_directory};

const MISSION_NAME: &str = "mission.json";
const MISSION_STATE_VERSION: u16 = 1;
const MAX_MISSION_REQUESTS: usize = 10_000;
const MISSION_ROOT: &str = "missions";
const MISSION_LOCK_ROOT: &str = "mission-control-locks";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MissionRevocationRequest {
    request_id: String,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MissionGenerationRecord {
    generation: u64,
    mission_sha256: String,
    involvement: InvolvementMode,
    issued_at_ms: u64,
    expires_at_ms: u64,
    admitted_at_ms: u64,
}

/// One retained decision outcome bound to the mission generation that produced it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MissionDecisionRecord {
    decision_id: String,
    domain_id: String,
    generation: u64,
    revocation_epoch: u64,
    proposal_sha256: String,
    outcome: MissionDecisionOutcome,
    recorded_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingOwnerDecision {
    decision_id: String,
    domain_id: String,
    generation: u64,
    requested_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OwnerDecisionAnswer {
    request_id: String,
    decision_id: String,
    generation: u64,
    alternative: Option<String>,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MissionState {
    version: u16,
    mission_id: String,
    mission_sha256: String,
    authority_sha256: String,
    campaign_id: String,
    repository_id: String,
    generation: u64,
    involvement: InvolvementMode,
    issued_at_ms: u64,
    expires_at_ms: u64,
    revocation_epoch: u64,
    revoked: bool,
    created_at_ms: u64,
    updated_at_ms: u64,
    charter: MissionCharter,
    generations: Vec<MissionGenerationRecord>,
    revocations: Vec<MissionRevocationRequest>,
    decisions: Vec<MissionDecisionRecord>,
    pending: Vec<PendingOwnerDecision>,
    answers: Vec<OwnerDecisionAnswer>,
}

impl MissionState {
    fn initial(
        spec: &CampaignSpec,
        charter: &MissionCharter,
        mission_sha256: &str,
        authority_sha256: &str,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            version: MISSION_STATE_VERSION,
            mission_id: charter.mission_id.clone(),
            mission_sha256: mission_sha256.to_owned(),
            authority_sha256: authority_sha256.to_owned(),
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            generation: charter.generation,
            involvement: charter.involvement,
            issued_at_ms: charter.issued_at_ms,
            expires_at_ms: charter.expires_at_ms,
            revocation_epoch: charter.revocation_epoch,
            revoked: false,
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
            charter: charter.clone(),
            generations: vec![MissionGenerationRecord {
                generation: charter.generation,
                mission_sha256: mission_sha256.to_owned(),
                involvement: charter.involvement,
                issued_at_ms: charter.issued_at_ms,
                expires_at_ms: charter.expires_at_ms,
                admitted_at_ms: timestamp_ms,
            }],
            revocations: Vec::new(),
            decisions: Vec::new(),
            pending: Vec::new(),
            answers: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn verify(&self, spec: &CampaignSpec, authority_sha256: &str) -> bool {
        if self.version != MISSION_STATE_VERSION
            || self.authority_sha256 != authority_sha256
            || self.campaign_id != spec.campaign_id
            || self.repository_id != spec.repository_id
            || self.mission_id.is_empty()
            || !valid_sha256(&self.mission_sha256)
            || self.generation == 0
            || self.expires_at_ms <= self.issued_at_ms
            || self.created_at_ms > self.updated_at_ms
            || self.generations.is_empty()
            || self.generations.len() > MAX_MISSION_REQUESTS
            || self.revocations.len() > MAX_MISSION_REQUESTS
            || self.decisions.len() > MAX_MISSION_REQUESTS
            || self.pending.len() > MAX_MISSION_REQUESTS
            || self.answers.len() > MAX_MISSION_REQUESTS
            || self.revoked == self.revocations.is_empty()
        {
            return false;
        }
        let mut prior_generation = 0;
        for record in &self.generations {
            if record.generation <= prior_generation
                || !valid_sha256(&record.mission_sha256)
                || record.expires_at_ms <= record.issued_at_ms
                || record.admitted_at_ms < self.created_at_ms
                || record.admitted_at_ms > self.updated_at_ms
            {
                return false;
            }
            prior_generation = record.generation;
        }
        let Some(latest) = self.generations.last() else {
            return false;
        };
        if latest.generation != self.generation
            || latest.mission_sha256 != self.mission_sha256
            || latest.involvement != self.involvement
            || latest.issued_at_ms != self.issued_at_ms
            || latest.expires_at_ms != self.expires_at_ms
            || self.charter.generation != self.generation
            || self.charter.involvement != self.involvement
            || self.charter.mission_id != self.mission_id
            || self.charter.mission_sha256(spec).ok().as_deref()
                != Some(self.mission_sha256.as_str())
        {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut prior_timestamp = self.created_at_ms;
        for request in &self.revocations {
            if RunId::new(request.request_id.clone()).is_err()
                || !seen.insert(request.request_id.clone())
                || request.timestamp_ms < prior_timestamp
                || request.timestamp_ms > self.updated_at_ms
            {
                return false;
            }
            prior_timestamp = request.timestamp_ms;
        }
        for record in &self.decisions {
            if record.decision_id.is_empty()
                || record.domain_id.is_empty()
                || record.generation == 0
                || record.generation > self.generation
                || !valid_sha256(&record.proposal_sha256)
                || record.recorded_at_ms < self.created_at_ms
                || record.recorded_at_ms > self.updated_at_ms
            {
                return false;
            }
        }
        let mut pending_ids = BTreeSet::new();
        for pending in &self.pending {
            if pending.decision_id.is_empty()
                || !pending_ids.insert(pending.decision_id.clone())
                || pending.generation == 0
                || pending.generation > self.generation
                || pending.requested_at_ms < self.created_at_ms
                || pending.requested_at_ms > self.updated_at_ms
            {
                return false;
            }
        }
        if self.involvement == InvolvementMode::HandsOff && !self.pending.is_empty() {
            return false;
        }
        let mut answer_ids = BTreeSet::new();
        for answer in &self.answers {
            if RunId::new(answer.request_id.clone()).is_err()
                || !answer_ids.insert(answer.request_id.clone())
                || seen.contains(&answer.request_id)
                || answer.decision_id.is_empty()
                || pending_ids.contains(&answer.decision_id)
                || answer.generation == 0
                || answer.generation > self.generation
                || answer.timestamp_ms < self.created_at_ms
                || answer.timestamp_ms > self.updated_at_ms
                || answer.alternative.as_ref().is_some_and(String::is_empty)
            {
                return false;
            }
        }
        true
    }

    fn revoke(&mut self, request_id: &str, timestamp_ms: u64) -> Result<bool, RuntimeError> {
        if self
            .revocations
            .iter()
            .any(|request| request.request_id == request_id)
        {
            return Ok(false);
        }
        if self.revocations.len() >= MAX_MISSION_REQUESTS || timestamp_ms < self.updated_at_ms {
            return Err(RuntimeError::State);
        }
        if !self.revoked {
            self.revoked = true;
            self.revocation_epoch = self
                .revocation_epoch
                .checked_add(1)
                .ok_or(RuntimeError::State)?;
        }
        self.updated_at_ms = timestamp_ms;
        self.revocations.push(MissionRevocationRequest {
            request_id: request_id.to_owned(),
            timestamp_ms,
        });
        Ok(true)
    }

    fn reissue(
        &mut self,
        charter: &MissionCharter,
        mission_sha256: &str,
        timestamp_ms: u64,
    ) -> Result<bool, RuntimeError> {
        if mission_sha256 == self.mission_sha256 {
            return Ok(false);
        }
        if self.revoked
            || charter.mission_id != self.mission_id
            || charter.generation <= self.generation
            || charter.revocation_epoch != self.revocation_epoch
            || timestamp_ms < self.updated_at_ms
            || self.generations.len() >= MAX_MISSION_REQUESTS
        {
            return Err(RuntimeError::Authority);
        }
        self.generation = charter.generation;
        mission_sha256.clone_into(&mut self.mission_sha256);
        self.charter = charter.clone();
        self.involvement = charter.involvement;
        self.issued_at_ms = charter.issued_at_ms;
        self.expires_at_ms = charter.expires_at_ms;
        self.updated_at_ms = timestamp_ms;
        if charter.involvement == InvolvementMode::HandsOff {
            self.pending.clear();
        }
        self.generations.push(MissionGenerationRecord {
            generation: charter.generation,
            mission_sha256: mission_sha256.to_owned(),
            involvement: charter.involvement,
            issued_at_ms: charter.issued_at_ms,
            expires_at_ms: charter.expires_at_ms,
            admitted_at_ms: timestamp_ms,
        });
        Ok(true)
    }

    fn prior_attempts(&self, decision_id: &str) -> u16 {
        u16::try_from(
            self.decisions
                .iter()
                .filter(|record| record.decision_id == decision_id)
                .count(),
        )
        .unwrap_or(u16::MAX)
    }

    fn decisions_recorded(&self) -> u32 {
        u32::try_from(self.decisions.len()).unwrap_or(u32::MAX)
    }

    fn observation(&self, now_ms: u64) -> MissionAuthorityObservation {
        MissionAuthorityObservation {
            mission_id: self.mission_id.clone(),
            generation: self.generation,
            involvement: self.involvement,
            revocation_epoch: self.revocation_epoch,
            revoked: self.revoked,
            expired: now_ms >= self.expires_at_ms || now_ms < self.issued_at_ms,
            pending_owner_decisions: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
        }
    }
}

/// Mission authority as observed by an engine immediately before an effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionAuthorityObservation {
    /// Stable mission identity.
    pub mission_id: String,
    /// Current authority generation.
    pub generation: u64,
    /// Involvement mode of the current generation.
    pub involvement: InvolvementMode,
    /// Current revocation epoch.
    pub revocation_epoch: u64,
    /// True once an authenticated revocation was recorded; irreversible.
    pub revoked: bool,
    /// True when the current time lies outside the charter validity window.
    pub expired: bool,
    /// Owner decisions awaiting an answer; always zero under hands-off.
    pub pending_owner_decisions: u32,
}

impl MissionAuthorityObservation {
    /// True only when new effects may start under this mission.
    #[must_use]
    pub const fn permits_new_effects(&self) -> bool {
        !self.revoked && !self.expired
    }

    /// Stable hold code when new effects may not start.
    #[must_use]
    pub const fn hold_code(&self) -> Option<&'static str> {
        if self.revoked {
            Some("codingmage.mission.revoked")
        } else if self.expired {
            Some("codingmage.mission.expired")
        } else {
            None
        }
    }
}

/// Source-free report of the durable mission state for one exact campaign.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionStatusReport {
    /// Report schema version.
    pub schema_version: u16,
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Stable mission identity.
    pub mission_id: String,
    /// Digest of the admitted charter generation.
    pub mission_sha256: String,
    /// Digest of the bound campaign authority.
    pub authority_sha256: String,
    /// Current authority generation.
    pub generation: u64,
    /// Involvement mode code.
    pub involvement: String,
    /// Charter issue time in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Charter expiry in Unix milliseconds.
    pub expires_at_ms: u64,
    /// True when the observation time is outside the validity window.
    pub expired: bool,
    /// Current revocation epoch.
    pub revocation_epoch: u64,
    /// True once revoked.
    pub revoked: bool,
    /// Retained decision records.
    pub decisions_recorded: u32,
    /// Decisions the coordinator applied as permitted choices.
    pub permitted_choices: u32,
    /// Decisions retained as holds (decision needed, deferred or blocked).
    pub held_decisions: u32,
    /// Owner decisions awaiting an answer.
    pub pending_owner_decisions: u32,
    /// Owner answers recorded.
    pub owner_answers: u32,
    /// Observation time in Unix milliseconds.
    pub observed_at_ms: u64,
}

/// Outcome of one authenticated mission control request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionControlOutcome {
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Stable mission identity.
    pub mission_id: String,
    /// Caller-generated idempotency identity.
    pub request_id: String,
    /// Closed action code.
    pub action: String,
    /// Authority generation after the request.
    pub generation: u64,
    /// Revocation epoch after the request.
    pub revocation_epoch: u64,
    /// True only when this invocation created the durable request.
    pub created: bool,
}

/// One role profile as observed by mission preflight, without model names or paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPreflightRole {
    /// Closed role name.
    pub role: String,
    /// True when the configured executable is an absolute regular nonsymlink file.
    pub executable_available: bool,
    /// Digest of the executable bytes, or an empty string when unavailable.
    pub executable_sha256: String,
    /// Credential boundary code for the role.
    pub authentication: String,
}

/// Source-free result of validating one charter against its campaign before any work starts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPreflightReport {
    /// Report schema version.
    pub schema_version: u16,
    /// Stable mission identity.
    pub mission_id: String,
    /// Digest of the verified charter.
    pub mission_sha256: String,
    /// Charter generation.
    pub generation: u64,
    /// Involvement mode code.
    pub involvement: String,
    /// Charter expiry in Unix milliseconds.
    pub expires_at_ms: u64,
    /// True when the observation time lies outside the validity window.
    pub expired: bool,
    /// Number of delegated decision domains.
    pub decision_domains: u32,
    /// Role availability and credential boundaries.
    pub roles: Vec<MissionPreflightRole>,
    /// Effects that will hold for owner authority under this campaign policy.
    pub retained_holds: Vec<String>,
    /// Channels the involvement mode needs during the campaign; empty under hands-off.
    pub interactive_prerequisites: Vec<String>,
    /// Exact reasons a no-intervention run is invalid; empty when valid.
    pub no_intervention_defects: Vec<String>,
    /// True when hands-off execution would be admitted with this configuration.
    pub no_intervention_valid: bool,
    /// `absent`, `bound` or `mismatch` relative to the durable mission state.
    pub durable_state: String,
}

/// Validates one charter against its campaign, providers and durable state before any work.
///
/// This never starts a process, admits the mission or touches the target. A hands-off charter
/// whose configuration would need an interactive step is refused with
/// [`RuntimeError::Authority`] so an invalid no-intervention campaign cannot start.
///
/// # Errors
///
/// Returns [`RuntimeError`] for unbound charters, invalid no-intervention configurations or
/// unverifiable durable state.
#[allow(clippy::too_many_lines)]
pub fn campaign_mission_preflight(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    charter_path: &Path,
) -> Result<MissionPreflightReport, RuntimeError> {
    let (authority_sha256, mission_root) =
        validate_mission_authority(config, spec, codingmage_binary)?;
    let charter = MissionCharter::load(charter_path, spec).map_err(RuntimeError::Campaign)?;
    let mission_sha256 = charter
        .mission_sha256(spec)
        .map_err(RuntimeError::Campaign)?;
    let observed_at_ms = now_ms()?;
    let authentication = match spec.implementer_authentication {
        codingmage_campaign::CampaignAuthentication::Bare => "bare",
        codingmage_campaign::CampaignAuthentication::ExistingLogin => "existing_login",
    };
    let mut roles = vec![
        role_report("team_lead", &spec.team_lead.executable, "read_only"),
        role_report("implementer", &spec.implementer.executable, authentication),
        role_report("reviewer", &spec.reviewer.executable, "read_only"),
    ];
    if let Some(routing) = spec
        .multi_agent
        .as_ref()
        .and_then(|policy| policy.provider_routing.as_ref())
    {
        if let Some(elevated) = routing.elevated_implementer.as_ref() {
            roles.push(role_report(
                "elevated_implementer",
                &elevated.executable,
                authentication,
            ));
        }
        if let Some(elevated) = routing.elevated_reviewer.as_ref() {
            roles.push(role_report(
                "elevated_reviewer",
                &elevated.executable,
                "read_only",
            ));
        }
    }
    let mut retained_holds = Vec::new();
    if let Some(policy) = spec.multi_agent.as_ref() {
        match policy.task_integration_policy {
            codingmage_campaign::TaskIntegrationPolicy::HumanRequired => {
                retained_holds.push("task_integration_human_required".to_owned());
            }
            codingmage_campaign::TaskIntegrationPolicy::Never => {
                retained_holds.push("task_integration_never".to_owned());
            }
            codingmage_campaign::TaskIntegrationPolicy::AutoToCampaignBranch => {}
        }
        match policy.destination_promotion_policy {
            codingmage_campaign::DestinationPromotionPolicy::HumanRequired => {
                retained_holds.push("destination_promotion_human_required".to_owned());
            }
            codingmage_campaign::DestinationPromotionPolicy::Never => {
                retained_holds.push("destination_promotion_never".to_owned());
            }
            codingmage_campaign::DestinationPromotionPolicy::AutoToDefaultBranch => {}
        }
        if policy.publication_mode != codingmage_campaign::TaskPublicationMode::LocalOnly {
            retained_holds.push("remote_publication_separately_granted".to_owned());
        }
    } else {
        retained_holds.push("task_integration_human_required".to_owned());
        retained_holds.push("destination_promotion_human_required".to_owned());
    }
    let interactive_prerequisites = match charter.involvement {
        InvolvementMode::Supervised => vec!["owner_checkpoint_channel".to_owned()],
        InvolvementMode::ExceptionOnly => vec!["owner_exception_channel".to_owned()],
        InvolvementMode::HandsOff => Vec::new(),
    };
    let expired = !charter.valid_at(observed_at_ms);
    let mut defects = Vec::new();
    if expired {
        defects.push("mission_expired".to_owned());
    }
    if spec.implementer_authentication != codingmage_campaign::CampaignAuthentication::ExistingLogin
    {
        defects.push("implementer_login_discovery_required".to_owned());
    }
    for role in &roles {
        if !role.executable_available {
            defects.push(format!("{}_executable_unavailable", role.role));
        }
    }
    if charter
        .decision_domains
        .iter()
        .any(|domain| domain.escalation == codingmage_campaign::EscalationDisposition::AskOwner)
    {
        defects.push("owner_prompt_configured".to_owned());
    }
    let durable_state = match load_mission(&mission_root, spec, &authority_sha256)? {
        None => "absent",
        Some(state) if state.mission_sha256 == mission_sha256 && !state.revoked => "bound",
        Some(_) => "mismatch",
    }
    .to_owned();
    let no_intervention_valid = defects.is_empty();
    if charter.involvement == InvolvementMode::HandsOff && !no_intervention_valid {
        return Err(RuntimeError::Authority);
    }
    Ok(MissionPreflightReport {
        schema_version: 1,
        mission_id: charter.mission_id.clone(),
        mission_sha256,
        generation: charter.generation,
        involvement: charter.involvement.code().to_owned(),
        expires_at_ms: charter.expires_at_ms,
        expired,
        decision_domains: u32::try_from(charter.decision_domains.len()).unwrap_or(u32::MAX),
        roles,
        retained_holds,
        interactive_prerequisites,
        no_intervention_defects: defects,
        no_intervention_valid,
        durable_state,
    })
}

fn role_report(role: &str, executable: &Path, authentication: &str) -> MissionPreflightRole {
    let available = executable.is_absolute()
        && fs::symlink_metadata(executable)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
    let executable_sha256 = if available {
        fs::read(executable)
            .map(|bytes| hex(&Sha256::digest(bytes)))
            .unwrap_or_default()
    } else {
        String::new()
    };
    MissionPreflightRole {
        role: role.to_owned(),
        executable_available: available && !executable_sha256.is_empty(),
        executable_sha256,
        authentication: authentication.to_owned(),
    }
}

/// Admits or re-issues one charter for one exact campaign without starting any work.
///
/// A charter whose digest already matches the durable state is idempotent. A charter with the
/// same mission identity and a higher generation re-issues the mission; a lower or equal
/// generation, a different mission identity, a revoked mission or a different revocation epoch
/// is refused and changes nothing.
///
/// # Errors
///
/// Returns [`RuntimeError`] for unbound charters, stale generations or durable-state failures.
pub fn admit_campaign_mission(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    charter_path: &Path,
) -> Result<MissionControlOutcome, RuntimeError> {
    let (authority_sha256, mission_root) =
        validate_mission_authority(config, spec, codingmage_binary)?;
    let charter = MissionCharter::load(charter_path, spec).map_err(RuntimeError::Campaign)?;
    let mission_sha256 = charter
        .mission_sha256(spec)
        .map_err(RuntimeError::Campaign)?;
    private_directory(&mission_root)?;
    let _lock = mission_lock(config, spec)?;
    let timestamp_ms = now_ms()?;
    let (mut state, created) = match load_mission(&mission_root, spec, &authority_sha256)? {
        Some(state) => (state, false),
        None => (
            MissionState::initial(
                spec,
                &charter,
                &mission_sha256,
                &authority_sha256,
                timestamp_ms,
            ),
            true,
        ),
    };
    let changed = if created {
        true
    } else {
        state.reissue(&charter, &mission_sha256, timestamp_ms)?
    };
    if changed {
        IntegrityDocument::write_atomic(&mission_root, MISSION_NAME, state.clone(), |value| {
            value.verify(spec, &authority_sha256)
        })
        .map_err(|_| RuntimeError::State)?;
    }
    Ok(MissionControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        mission_id: state.mission_id.clone(),
        request_id: mission_sha256,
        action: if created { "admit" } else { "reissue" }.to_owned(),
        generation: state.generation,
        revocation_epoch: state.revocation_epoch,
        created: changed,
    })
}

/// Records one authenticated, irreversible mission revocation.
///
/// # Errors
///
/// Returns [`RuntimeError`] when no mission is bound, the request identity is malformed or the
/// durable state cannot be written.
pub fn revoke_campaign_mission(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    request_id: &str,
) -> Result<MissionControlOutcome, RuntimeError> {
    let request_id = RunId::new(request_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    let (authority_sha256, mission_root) =
        validate_mission_authority(config, spec, codingmage_binary)?;
    let _lock = mission_lock(config, spec)?;
    let mut state =
        load_mission(&mission_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    let created = state.revoke(request_id.as_str(), now_ms()?)?;
    if created {
        IntegrityDocument::write_atomic(&mission_root, MISSION_NAME, state.clone(), |value| {
            value.verify(spec, &authority_sha256)
        })
        .map_err(|_| RuntimeError::State)?;
    }
    Ok(MissionControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        mission_id: state.mission_id.clone(),
        request_id: request_id.as_str().to_owned(),
        action: "revoke".to_owned(),
        generation: state.generation,
        revocation_epoch: state.revocation_epoch,
        created,
    })
}

/// Records the owner's answer to one pending decision in supervised or exception-only mode.
///
/// `alternative` must be one of the domain's approved alternatives; `None` retains the work as
/// blocked. Hands-off missions never have pending decisions, so this call is refused there.
///
/// # Errors
///
/// Returns [`RuntimeError`] when the decision is not pending, the alternative is not approved,
/// the charter is unavailable or the durable state cannot be written.
pub fn answer_campaign_decision(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    decision_id: &str,
    request_id: &str,
    alternative: Option<&str>,
) -> Result<MissionControlOutcome, RuntimeError> {
    let request_id = RunId::new(request_id.to_owned()).map_err(|_| RuntimeError::Spec)?;
    let (authority_sha256, mission_root) =
        validate_mission_authority(config, spec, codingmage_binary)?;
    let _lock = mission_lock(config, spec)?;
    let mut state =
        load_mission(&mission_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    if state.revoked {
        return Err(RuntimeError::Authority);
    }
    let charter = state.charter.clone();
    if let Some(existing) = state
        .answers
        .iter()
        .find(|answer| answer.request_id == request_id.as_str())
    {
        return if existing.decision_id == decision_id
            && existing.alternative.as_deref() == alternative
        {
            Ok(MissionControlOutcome {
                campaign_id: spec.campaign_id.clone(),
                mission_id: state.mission_id.clone(),
                request_id: request_id.as_str().to_owned(),
                action: "answer".to_owned(),
                generation: state.generation,
                revocation_epoch: state.revocation_epoch,
                created: false,
            })
        } else {
            Err(RuntimeError::Authority)
        };
    }
    let Some(position) = state
        .pending
        .iter()
        .position(|pending| pending.decision_id == decision_id)
    else {
        return Err(RuntimeError::Authority);
    };
    let pending = state.pending[position].clone();
    if let Some(alternative) = alternative {
        let approved = charter
            .domain(&pending.domain_id)
            .is_some_and(|domain| domain.alternatives.iter().any(|known| known == alternative));
        if !approved {
            return Err(RuntimeError::Authority);
        }
    }
    let timestamp_ms = now_ms()?;
    if state.answers.len() >= MAX_MISSION_REQUESTS || timestamp_ms < state.updated_at_ms {
        return Err(RuntimeError::State);
    }
    state.pending.remove(position);
    state.answers.push(OwnerDecisionAnswer {
        request_id: request_id.as_str().to_owned(),
        decision_id: decision_id.to_owned(),
        generation: state.generation,
        alternative: alternative.map(str::to_owned),
        timestamp_ms,
    });
    state.updated_at_ms = timestamp_ms;
    IntegrityDocument::write_atomic(&mission_root, MISSION_NAME, state.clone(), |value| {
        value.verify(spec, &authority_sha256)
    })
    .map_err(|_| RuntimeError::State)?;
    Ok(MissionControlOutcome {
        campaign_id: spec.campaign_id.clone(),
        mission_id: state.mission_id,
        request_id: request_id.as_str().to_owned(),
        action: "answer".to_owned(),
        generation: state.generation,
        revocation_epoch: state.revocation_epoch,
        created: true,
    })
}

/// Reads the durable mission state for one exact campaign.
///
/// # Errors
///
/// Returns [`RuntimeError::State`] when no mission is bound or the document fails verification.
pub fn campaign_mission_status(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
) -> Result<MissionStatusReport, RuntimeError> {
    let (authority_sha256, mission_root) =
        validate_mission_authority(config, spec, codingmage_binary)?;
    let state = load_mission(&mission_root, spec, &authority_sha256)?.ok_or(RuntimeError::State)?;
    let observed_at_ms = now_ms()?;
    let permitted = state
        .decisions
        .iter()
        .filter(|record| record.outcome.permits_effect())
        .count();
    Ok(MissionStatusReport {
        schema_version: 1,
        campaign_id: spec.campaign_id.clone(),
        mission_id: state.mission_id.clone(),
        mission_sha256: state.mission_sha256.clone(),
        authority_sha256,
        generation: state.generation,
        involvement: state.involvement.code().to_owned(),
        issued_at_ms: state.issued_at_ms,
        expires_at_ms: state.expires_at_ms,
        expired: observed_at_ms >= state.expires_at_ms || observed_at_ms < state.issued_at_ms,
        revocation_epoch: state.revocation_epoch,
        revoked: state.revoked,
        decisions_recorded: state.decisions_recorded(),
        permitted_choices: u32::try_from(permitted).unwrap_or(u32::MAX),
        held_decisions: u32::try_from(state.decisions.len() - permitted).unwrap_or(u32::MAX),
        pending_owner_decisions: u32::try_from(state.pending.len()).unwrap_or(u32::MAX),
        owner_answers: u32::try_from(state.answers.len()).unwrap_or(u32::MAX),
        observed_at_ms,
    })
}

/// Observes mission authority for an engine immediately before an effect.
///
/// Returns `Ok(None)` when no mission is bound to the campaign, which preserves legacy behavior.
///
/// # Errors
///
/// Returns [`RuntimeError::State`] when a bound mission document fails verification; callers
/// must treat that as a hold, never as absence.
pub(crate) fn observe_mission_authority(
    state_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
    now_ms: u64,
) -> Result<Option<MissionAuthorityObservation>, RuntimeError> {
    let mission_root = mission_root(state_root, spec);
    Ok(load_mission(&mission_root, spec, authority_sha256)?.map(|state| state.observation(now_ms)))
}

/// Evaluates and durably records one decision proposal against the bound mission.
///
/// Under supervised and exception-only involvement, a `decision_needed` outcome registers one
/// pending owner decision per decision identity; a repeated identical proposal is deduplicated
/// and does not re-request. A previously recorded owner answer selecting an approved alternative
/// turns the same decision identity into a permitted choice for that alternative.
///
/// # Errors
///
/// Returns [`RuntimeError`] when no mission is bound, the proposal is malformed or the durable
/// state cannot be written.
pub(crate) fn record_mission_decision(
    state_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
    proposal: &DecisionProposal,
    now_ms: u64,
) -> Result<MissionDecisionOutcome, RuntimeError> {
    let mission_root = mission_root(state_root, spec);
    let mut state =
        load_mission(&mission_root, spec, authority_sha256)?.ok_or(RuntimeError::State)?;
    let charter = state.charter.clone();
    let charter = &charter;
    let proposal_sha256 = serializable_sha256(proposal)?;
    if let Some(existing) = state.decisions.iter().rev().find(|record| {
        record.decision_id == proposal.decision_id
            && record.proposal_sha256 == proposal_sha256
            && record.generation == state.generation
            && matches!(
                record.outcome,
                MissionDecisionOutcome::DecisionNeeded { .. }
            )
    }) && state
        .pending
        .iter()
        .any(|pending| pending.decision_id == proposal.decision_id)
    {
        return Ok(existing.outcome.clone());
    }
    let answered = state
        .answers
        .iter()
        .rev()
        .find(|answer| {
            answer.decision_id == proposal.decision_id && answer.generation == state.generation
        })
        .cloned();
    let mut evaluated = proposal.clone();
    if let Some(answer) = &answered
        && let Some(alternative) = &answer.alternative
    {
        evaluated.selected_alternative.clone_from(alternative);
    }
    let observation = DecisionObservation {
        now_ms,
        revocation_epoch: state.revocation_epoch,
        revoked: state.revoked,
        decisions_recorded: state.decisions_recorded(),
        prior_attempts: state.prior_attempts(&proposal.decision_id),
    };
    let mut outcome = evaluate_decision(charter, spec, &evaluated, &observation)
        .map_err(RuntimeError::Campaign)?;
    if answered.is_some_and(|answer| answer.alternative.is_none()) {
        outcome = MissionDecisionOutcome::Blocked {
            reason: codingmage_campaign::DecisionHoldReason::UndelegatedDecision,
        };
    }
    if now_ms < state.updated_at_ms || state.decisions.len() >= MAX_MISSION_REQUESTS {
        return Err(RuntimeError::State);
    }
    if matches!(outcome, MissionDecisionOutcome::DecisionNeeded { .. })
        && state.involvement.may_request_owner_decision()
        && !state
            .pending
            .iter()
            .any(|pending| pending.decision_id == proposal.decision_id)
    {
        state.pending.push(PendingOwnerDecision {
            decision_id: proposal.decision_id.clone(),
            domain_id: proposal.domain_id.clone(),
            generation: state.generation,
            requested_at_ms: now_ms,
        });
    }
    state.decisions.push(MissionDecisionRecord {
        decision_id: proposal.decision_id.clone(),
        domain_id: proposal.domain_id.clone(),
        generation: state.generation,
        revocation_epoch: state.revocation_epoch,
        proposal_sha256,
        outcome: outcome.clone(),
        recorded_at_ms: now_ms,
    });
    state.updated_at_ms = now_ms;
    IntegrityDocument::write_atomic(&mission_root, MISSION_NAME, state, |value| {
        value.verify(spec, authority_sha256)
    })
    .map_err(|_| RuntimeError::State)?;
    Ok(outcome)
}

/// Outcome of routing one lead human-decision blocker through the bound mission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LeadDecisionResolution {
    /// Deterministic mission outcome recorded for the proposal.
    pub(crate) outcome: MissionDecisionOutcome,
    /// Charter bound on consecutive re-plans without admitted work.
    pub(crate) max_no_progress_cycles: u16,
}

/// Routes a lead human-decision blocker through the bound mission, if any.
///
/// Returns `Ok(None)` when no mission is bound or the blocker carries no typed proposal, so the
/// legacy human-decision path applies unchanged.
///
/// # Errors
///
/// Returns [`RuntimeError`] when the bound mission document fails verification, the proposal is
/// malformed or the durable state cannot be written.
pub(crate) fn resolve_lead_decision(
    state_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
    blocker: &HumanDecisionBlocker,
    now_ms: u64,
) -> Result<Option<LeadDecisionResolution>, RuntimeError> {
    let Some(proposal) = blocker.decision.as_ref() else {
        return Ok(None);
    };
    let mission_root = mission_root(state_root, spec);
    let Some(state) = load_mission(&mission_root, spec, authority_sha256)? else {
        return Ok(None);
    };
    let max_no_progress_cycles = state.charter.budgets.max_no_progress_cycles;
    let outcome = record_mission_decision(state_root, spec, authority_sha256, proposal, now_ms)?;
    Ok(Some(LeadDecisionResolution {
        outcome,
        max_no_progress_cycles,
    }))
}

/// Stable engine blocker code for a mission outcome that did not permit an effect.
#[must_use]
pub(crate) const fn lead_hold_code(outcome: &MissionDecisionOutcome) -> &'static str {
    match outcome {
        MissionDecisionOutcome::PermittedChoice { .. } => "codingmage.mission.permitted_choice",
        MissionDecisionOutcome::DecisionNeeded { .. } => "codingmage.mission.decision_needed",
        MissionDecisionOutcome::Deferred { .. } => "codingmage.mission.decision_deferred",
        MissionDecisionOutcome::Blocked { .. } => "codingmage.mission.decision_blocked",
    }
}

/// Adds the bound mission's delegated domains and accepted decisions to a lead binding.
///
/// Without a bound mission the binding is left unchanged.
///
/// # Errors
///
/// Returns [`RuntimeError::State`] when a bound mission document fails verification.
pub(crate) fn attach_lead_mission_context(
    state_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
    binding: &mut CodexLeadBinding,
) -> Result<(), RuntimeError> {
    let mission_root = mission_root(state_root, spec);
    let Some(state) = load_mission(&mission_root, spec, authority_sha256)? else {
        return Ok(());
    };
    binding.decision_domains = state
        .charter
        .decision_domains
        .iter()
        .map(|domain| CodexLeadDomain {
            domain_id: domain.domain_id.clone(),
            class: domain.class.code().to_owned(),
            alternatives: domain.alternatives.clone(),
        })
        .collect();
    let mut accepted: Vec<CodexLeadDecision> = Vec::new();
    for record in state.decisions.iter().rev() {
        if record.generation != state.generation
            || accepted
                .iter()
                .any(|decision| decision.decision_id == record.decision_id)
        {
            continue;
        }
        if let MissionDecisionOutcome::PermittedChoice {
            domain_id,
            alternative,
            ..
        } = &record.outcome
        {
            accepted.push(CodexLeadDecision {
                decision_id: record.decision_id.clone(),
                domain_id: domain_id.clone(),
                alternative: alternative.clone(),
            });
        }
    }
    accepted.reverse();
    binding.accepted_decisions = accepted;
    Ok(())
}

/// Current Unix time in milliseconds for mission observations.
///
/// # Errors
///
/// Returns [`RuntimeError::State`] when the clock is unavailable.
pub(crate) fn current_time_ms() -> Result<u64, RuntimeError> {
    now_ms()
}

/// Returns the termination an engine must apply when the bound mission forbids new effects.
///
/// `Ok(None)` means either no mission is bound or the mission permits new effects.
///
/// # Errors
///
/// Returns [`RuntimeError::State`] when a bound mission document fails verification.
pub(crate) fn mission_hold_termination(
    state_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
) -> Result<Option<crate::CampaignTermination>, RuntimeError> {
    let Some(observation) =
        observe_mission_authority(state_root, spec, authority_sha256, now_ms()?)?
    else {
        return Ok(None);
    };
    Ok(observation.hold_code().map(|code| {
        if observation.revoked {
            crate::CampaignTermination::new(
                crate::CampaignState::Cancelled,
                crate::CampaignStopReason::MissionRevoked,
                Some(code.to_owned()),
            )
        } else {
            crate::CampaignTermination::new(
                crate::CampaignState::Blocked,
                crate::CampaignStopReason::MissionExpired,
                Some(code.to_owned()),
            )
        }
    }))
}

fn validate_mission_authority(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
) -> Result<(String, PathBuf), RuntimeError> {
    spec.verify().map_err(RuntimeError::Campaign)?;
    let authority_sha256 = spec.authority_sha256().map_err(RuntimeError::Campaign)?;
    let binary = canonical_file(codingmage_binary)?;
    let source_root = binary.parent().ok_or(RuntimeError::Authority)?;
    let authorization = RepositoryAuthorization::authorize(config, source_root)
        .map_err(|_| RuntimeError::Authority)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id
        || fs::canonicalize(&config.target_path).map_err(|_| RuntimeError::Authority)?
            != spec.repository_path
    {
        return Err(RuntimeError::Authority);
    }
    Ok((authority_sha256, mission_root(&config.state_root, spec)))
}

fn mission_root(state_root: &Path, spec: &CampaignSpec) -> PathBuf {
    state_root.join(MISSION_ROOT).join(&spec.campaign_id)
}

fn mission_lock(config: &Config, spec: &CampaignSpec) -> Result<CoordinatorLock, RuntimeError> {
    let lock_id = generated_run_id()?;
    let repository_id = codingmage_contracts::RepositoryId::new(spec.repository_id.clone())
        .map_err(|_| RuntimeError::Spec)?;
    CoordinatorLock::acquire(
        &config
            .state_root
            .join(MISSION_LOCK_ROOT)
            .join(&spec.campaign_id),
        &repository_id,
        lock_id.as_str(),
    )
    .map_err(|_| RuntimeError::Orchestration)
}

fn load_mission(
    mission_root: &Path,
    spec: &CampaignSpec,
    authority_sha256: &str,
) -> Result<Option<MissionState>, RuntimeError> {
    if fs::symlink_metadata(mission_root.join(MISSION_NAME)).is_err() {
        return Ok(None);
    }
    IntegrityDocument::<MissionState>::load(mission_root, MISSION_NAME, |value| {
        value.verify(spec, authority_sha256)
    })
    .map(|document| Some(document.payload))
    .map_err(|_| RuntimeError::State)
}

fn serializable_sha256(value: &impl Serialize) -> Result<String, RuntimeError> {
    let bytes = serde_json::to_vec(value).map_err(|_| RuntimeError::State)?;
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

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn now_ms() -> Result<u64, RuntimeError> {
    let elapsed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| RuntimeError::State)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| RuntimeError::State)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codingmage_campaign::{
        CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
        CampaignPublication, DecisionClass, DecisionDomainGrant, DecisionHoldReason,
        EscalationDisposition, MissionBudgets, PodRisk,
    };

    use super::*;

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
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["unit".to_owned()],
            }],
            campaign_branch: "codingmage/fixture-mission".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: vec![],
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        }
    }

    fn charter(spec: &CampaignSpec, involvement: InvolvementMode) -> MissionCharter {
        MissionCharter {
            version: codingmage_campaign::MISSION_VERSION,
            mission_id: "mission-1".to_owned(),
            generation: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            initial_commit: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            operator_authorization_sha256: spec.operator_authorization_sha256.clone(),
            campaign_authority_sha256: spec.authority_sha256().unwrap(),
            objective: "Ship the storage layer".to_owned(),
            success_criteria: vec!["storage sub-tasks accepted".to_owned()],
            exclusions: Vec::new(),
            architecture_invariants: Vec::new(),
            decision_domains: vec![DecisionDomainGrant {
                domain_id: "storage-layout".to_owned(),
                class: DecisionClass::Architecture,
                alternatives: vec!["single-file".to_owned(), "per-task-file".to_owned()],
                scope_paths: vec![PathBuf::from("src/storage")],
                max_risk: PodRisk::Routine,
                required_gate_tiers: vec!["focused".to_owned()],
                escalation: EscalationDisposition::Block,
            }],
            command_registry: vec!["unit".to_owned()],
            involvement,
            budgets: MissionBudgets {
                max_decisions: 10,
                max_decision_retries: 3,
                max_no_progress_cycles: 3,
            },
            issued_at_ms: 1,
            expires_at_ms: 31_536_000_000,
            revocation_epoch: 0,
        }
    }

    fn proposal(domain_id: &str, alternative: &str) -> DecisionProposal {
        DecisionProposal {
            decision_id: "decision-1".to_owned(),
            domain_id: domain_id.to_owned(),
            class: DecisionClass::Architecture,
            selected_alternative: alternative.to_owned(),
            affected_paths: vec![PathBuf::from("src/storage/journal.rs")],
            risk: PodRisk::Routine,
            gate_tiers: vec!["focused".to_owned()],
            rationale_summary: "smaller documents".to_owned(),
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codingmage-mission-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_state(root: &Path, spec: &CampaignSpec, state: &MissionState) {
        let authority = spec.authority_sha256().unwrap();
        IntegrityDocument::write_atomic(
            &mission_root(root, spec),
            MISSION_NAME,
            state.clone(),
            |value| value.verify(spec, &authority),
        )
        .unwrap();
    }

    type Mutation = fn(&mut MissionState);

    fn initial(
        spec: &CampaignSpec,
        involvement: InvolvementMode,
    ) -> (MissionCharter, MissionState) {
        let charter = charter(spec, involvement);
        let digest = charter.mission_sha256(spec).unwrap();
        let state = MissionState::initial(
            spec,
            &charter,
            &digest,
            &spec.authority_sha256().unwrap(),
            100,
        );
        (charter, state)
    }

    #[test]
    fn initial_state_verifies_and_rejects_identity_and_projection_mutation() {
        let spec = spec();
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::HandsOff);
        assert!(state.verify(&spec, &authority));
        let mutations: Vec<(&str, Mutation)> = vec![
            ("version", |state| state.version = 2),
            ("campaign", |state| state.campaign_id = "other".to_owned()),
            ("repository", |state| {
                state.repository_id = "other".to_owned();
            }),
            ("generation", |state| state.generation = 2),
            ("revoked-flag", |state| state.revoked = true),
            ("expiry", |state| state.expires_at_ms = state.issued_at_ms),
            ("digest", |state| state.mission_sha256 = "zz".repeat(32)),
            ("pending-under-hands-off", |state| {
                state.pending.push(PendingOwnerDecision {
                    decision_id: "d".to_owned(),
                    domain_id: "x".to_owned(),
                    generation: 1,
                    requested_at_ms: 100,
                });
            }),
        ];
        for (name, mutate) in mutations {
            let mut changed = state.clone();
            mutate(&mut changed);
            assert!(!changed.verify(&spec, &authority), "{name}");
        }
        assert!(!state.verify(&spec, &"0".repeat(64)));
    }

    #[test]
    fn revocation_is_idempotent_irreversible_and_advances_the_epoch() {
        let spec = spec();
        let (_, mut state) = initial(&spec, InvolvementMode::HandsOff);
        assert_eq!(state.revoke("revoke-1", 200), Ok(true));
        assert!(state.revoked);
        assert_eq!(state.revocation_epoch, 1);
        assert_eq!(state.revoke("revoke-1", 300), Ok(false));
        assert_eq!(state.revoke("revoke-2", 300), Ok(true));
        assert_eq!(state.revocation_epoch, 1);
        assert_eq!(state.revoke("revoke-3", 250), Err(RuntimeError::State));
        assert!(state.verify(&spec, &spec.authority_sha256().unwrap()));
        let observation = state.observation(1_000);
        assert!(observation.revoked);
        assert!(!observation.permits_new_effects());
        assert_eq!(observation.hold_code(), Some("codingmage.mission.revoked"));
    }

    #[test]
    fn expiry_is_observed_without_a_control_request() {
        let spec = spec();
        let (_, state) = initial(&spec, InvolvementMode::Supervised);
        let live = state.observation(1_000);
        assert!(live.permits_new_effects());
        assert_eq!(live.hold_code(), None);
        let expired = state.observation(u64::MAX / 2);
        assert!(expired.expired);
        assert_eq!(expired.hold_code(), Some("codingmage.mission.expired"));
        assert!(state.observation(0).expired);
    }

    #[test]
    fn reissue_requires_same_mission_higher_generation_and_no_revocation() {
        let spec = spec();
        let (charter, mut state) = initial(&spec, InvolvementMode::Supervised);
        let same = charter.mission_sha256(&spec).unwrap();
        assert_eq!(state.reissue(&charter, &same, 200), Ok(false));

        let mut lower = charter.clone();
        lower.objective = "changed".to_owned();
        let lower_digest = lower.mission_sha256(&spec).unwrap();
        assert_eq!(
            state.reissue(&lower, &lower_digest, 200),
            Err(RuntimeError::Authority)
        );

        let mut other = charter.clone();
        other.mission_id = "mission-2".to_owned();
        other.generation = 2;
        let other_digest = other.mission_sha256(&spec).unwrap();
        assert_eq!(
            state.reissue(&other, &other_digest, 200),
            Err(RuntimeError::Authority)
        );

        let mut next = charter.clone();
        next.generation = 2;
        next.involvement = InvolvementMode::HandsOff;
        let next_digest = next.mission_sha256(&spec).unwrap();
        assert_eq!(state.reissue(&next, &next_digest, 200), Ok(true));
        assert_eq!(state.generation, 2);
        assert_eq!(state.involvement, InvolvementMode::HandsOff);
        assert!(state.verify(&spec, &spec.authority_sha256().unwrap()));

        state.revoke("revoke-1", 300).unwrap();
        let mut after_revocation = next.clone();
        after_revocation.generation = 3;
        after_revocation.revocation_epoch = 1;
        let digest = after_revocation.mission_sha256(&spec).unwrap();
        assert_eq!(
            state.reissue(&after_revocation, &digest, 400),
            Err(RuntimeError::Authority)
        );
    }

    #[test]
    fn recorded_decisions_are_bound_to_generation_and_deduplicated() {
        let spec = spec();
        let root = temp_root("decisions");
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::Supervised);
        write_state(&root, &spec, &state);

        let permitted = record_mission_decision(
            &root,
            &spec,
            &authority,
            &proposal("storage-layout", "per-task-file"),
            500,
        )
        .unwrap();
        assert!(permitted.permits_effect());

        let mut undelegated = proposal("unknown", "anything");
        undelegated.decision_id = "decision-2".to_owned();
        let needed = record_mission_decision(&root, &spec, &authority, &undelegated, 600).unwrap();
        assert_eq!(
            needed,
            MissionDecisionOutcome::DecisionNeeded {
                reason: DecisionHoldReason::UndelegatedDecision
            }
        );
        let repeated =
            record_mission_decision(&root, &spec, &authority, &undelegated, 700).unwrap();
        assert_eq!(repeated, needed);
        let state = load_mission(&mission_root(&root, &spec), &spec, &authority)
            .unwrap()
            .unwrap();
        assert_eq!(state.pending.len(), 1);
        assert_eq!(
            state.decisions.len(),
            2,
            "a repeated identical request is not re-recorded"
        );

        let mut later = proposal("storage-layout", "single-file");
        later.decision_id = "decision-3".to_owned();
        assert_eq!(
            record_mission_decision(&root, &spec, &authority, &later, 100),
            Err(RuntimeError::State),
            "time cannot move backwards"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hands_off_never_registers_a_pending_owner_decision() {
        let spec = spec();
        let root = temp_root("hands-off");
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::HandsOff);
        write_state(&root, &spec, &state);
        let outcome = record_mission_decision(
            &root,
            &spec,
            &authority,
            &proposal("unknown", "anything"),
            500,
        )
        .unwrap();
        assert_eq!(
            outcome,
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::UndelegatedDecision
            }
        );
        let state = load_mission(&mission_root(&root, &spec), &spec, &authority)
            .unwrap()
            .unwrap();
        assert!(state.pending.is_empty());
        assert_eq!(state.decisions.len(), 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn revoked_mission_records_only_holds_and_no_progress_is_bounded() {
        let spec = spec();
        let root = temp_root("revoked");
        let authority = spec.authority_sha256().unwrap();
        let (_, mut state) = initial(&spec, InvolvementMode::HandsOff);
        let mut unapproved = proposal("storage-layout", "sqlite");
        for attempt in 0..3 {
            write_state(&root, &spec, &state);
            unapproved.decision_id = "decision-loop".to_owned();
            let outcome =
                record_mission_decision(&root, &spec, &authority, &unapproved, 500 + attempt)
                    .unwrap();
            assert_eq!(
                outcome,
                MissionDecisionOutcome::Blocked {
                    reason: DecisionHoldReason::UnapprovedAlternative
                }
            );
            state = load_mission(&mission_root(&root, &spec), &spec, &authority)
                .unwrap()
                .unwrap();
        }
        let exhausted =
            record_mission_decision(&root, &spec, &authority, &unapproved, 900).unwrap();
        assert_eq!(
            exhausted,
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::NoProgress
            }
        );

        state = load_mission(&mission_root(&root, &spec), &spec, &authority)
            .unwrap()
            .unwrap();
        state.revoke("revoke-1", 1_000).unwrap();
        write_state(&root, &spec, &state);
        let revoked = record_mission_decision(
            &root,
            &spec,
            &authority,
            &proposal("storage-layout", "per-task-file"),
            1_100,
        )
        .unwrap();
        assert_eq!(
            revoked,
            MissionDecisionOutcome::Blocked {
                reason: DecisionHoldReason::MissionRevoked
            }
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn incompatible_state_version_is_refused_not_migrated() {
        let spec = spec();
        let root = temp_root("version");
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::HandsOff);
        write_state(&root, &spec, &state);
        let path = mission_root(&root, &spec).join(MISSION_NAME);
        let text = fs::read_to_string(&path).unwrap();
        let upgraded = text.replacen(
            "\"version\":1,\"mission_id\"",
            "\"version\":2,\"mission_id\"",
            1,
        );
        assert_ne!(upgraded, text);
        fs::write(&path, upgraded).unwrap();
        assert_eq!(
            observe_mission_authority(&root, &spec, &authority, 500),
            Err(RuntimeError::State)
        );
        assert_eq!(
            mission_hold_termination(&root, &spec, &authority),
            Err(RuntimeError::State)
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn restart_cannot_resurrect_revoked_authority() {
        let spec = spec();
        let root = temp_root("restart");
        let authority = spec.authority_sha256().unwrap();
        let (charter, mut state) = initial(&spec, InvolvementMode::Supervised);
        state.revoke("revoke-1", 200).unwrap();
        write_state(&root, &spec, &state);

        let reloaded = load_mission(&mission_root(&root, &spec), &spec, &authority)
            .unwrap()
            .unwrap();
        assert!(reloaded.revoked);
        assert_eq!(reloaded.revocation_epoch, 1);
        let observation = observe_mission_authority(&root, &spec, &authority, 300)
            .unwrap()
            .unwrap();
        assert!(!observation.permits_new_effects());
        let termination = mission_hold_termination(&root, &spec, &authority)
            .unwrap()
            .unwrap();
        assert_eq!(termination.state, crate::CampaignState::Cancelled);
        assert_eq!(
            termination.reason,
            crate::CampaignStopReason::MissionRevoked
        );

        let mut next = charter.clone();
        next.generation = 2;
        let digest = next.mission_sha256(&spec).unwrap();
        let mut after_restart = reloaded;
        assert_eq!(
            after_restart.reissue(&next, &digest, 400),
            Err(RuntimeError::Authority)
        );
        let blocked = record_mission_decision(
            &root,
            &spec,
            &authority,
            &proposal("storage-layout", "per-task-file"),
            500,
        )
        .unwrap();
        assert!(!blocked.permits_effect());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lead_decisions_route_through_the_mission_and_feed_the_next_binding() {
        let spec = spec();
        let root = temp_root("lead");
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::HandsOff);
        write_state(&root, &spec, &state);
        let blocker = |decision: Option<DecisionProposal>| HumanDecisionBlocker {
            binding: codingmage_contracts::LeadTaskBinding {
                campaign_id: spec.campaign_id.clone(),
                campaign_head: spec.initial_commit.clone(),
                task_source_sha256: spec.task_source_sha256.clone(),
                task_id: "1.1.1.1".to_owned(),
                dependencies: Vec::new(),
            },
            reason: codingmage_contracts::LeadHumanDecisionReason::MaterialArchitectureChoice,
            summary: "layout".to_owned(),
            decision,
        };
        assert_eq!(
            resolve_lead_decision(&root, &spec, &authority, &blocker(None), 500),
            Ok(None),
            "no typed proposal keeps the legacy human-decision path"
        );
        let resolution = resolve_lead_decision(
            &root,
            &spec,
            &authority,
            &blocker(Some(proposal("storage-layout", "per-task-file"))),
            600,
        )
        .unwrap()
        .unwrap();
        assert!(resolution.outcome.permits_effect());
        assert_eq!(resolution.max_no_progress_cycles, 3);

        let mut binding = CodexLeadBinding {
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            worktree: root.clone(),
            campaign_head: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            readiness_census_sha256: "d".repeat(64),
            maximum_proposals: 1,
            allowed_paths: spec.allowed_paths.clone(),
            denied_paths: Vec::new(),
            gate_tiers: vec!["focused".to_owned()],
            ready_tasks: Vec::new(),
            decision_domains: Vec::new(),
            accepted_decisions: Vec::new(),
        };
        attach_lead_mission_context(&root, &spec, &authority, &mut binding).unwrap();
        assert_eq!(binding.decision_domains.len(), 1);
        assert_eq!(binding.decision_domains[0].domain_id, "storage-layout");
        assert_eq!(binding.decision_domains[0].class, "architecture");
        assert_eq!(
            binding.accepted_decisions,
            vec![CodexLeadDecision {
                decision_id: "decision-1".to_owned(),
                domain_id: "storage-layout".to_owned(),
                alternative: "per-task-file".to_owned(),
            }]
        );

        let held = resolve_lead_decision(
            &root,
            &spec,
            &authority,
            &blocker(Some(proposal("unknown", "anything"))),
            700,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            lead_hold_code(&held.outcome),
            "codingmage.mission.decision_blocked"
        );
        let mut unbound = spec.clone();
        unbound.campaign_id = "no-mission".to_owned();
        let unbound_authority = unbound.authority_sha256().unwrap();
        assert_eq!(
            resolve_lead_decision(
                &root,
                &unbound,
                &unbound_authority,
                &blocker(Some(proposal("storage-layout", "per-task-file"))),
                800
            ),
            Ok(None)
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tampered_or_cross_project_document_is_a_hold_not_absence() {
        let spec = spec();
        let root = temp_root("tampered");
        let authority = spec.authority_sha256().unwrap();
        let (_, state) = initial(&spec, InvolvementMode::HandsOff);
        write_state(&root, &spec, &state);
        assert!(
            observe_mission_authority(&root, &spec, &authority, 500)
                .unwrap()
                .is_some()
        );
        let mut other = spec.clone();
        other.campaign_id = "fixture-mission".to_owned();
        other.repository_id = "other-repo".to_owned();
        assert_eq!(
            observe_mission_authority(&root, &other, &other.authority_sha256().unwrap(), 500),
            Err(RuntimeError::State)
        );
        let path = mission_root(&root, &spec).join(MISSION_NAME);
        let mut text = fs::read_to_string(&path).unwrap();
        text = text.replace("\"revoked\":false", "\"revoked\":true");
        fs::write(&path, text).unwrap();
        assert_eq!(
            observe_mission_authority(&root, &spec, &authority, 500),
            Err(RuntimeError::State)
        );
        let mut unbound = spec.clone();
        unbound.campaign_id = "no-mission".to_owned();
        assert_eq!(
            observe_mission_authority(&root, &unbound, &unbound.authority_sha256().unwrap(), 500),
            Ok(None)
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
