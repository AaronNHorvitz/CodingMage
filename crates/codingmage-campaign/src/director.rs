//! Source-bound planning packets for a proposed director role.
//!
//! These packets are data. Creating or retaining one does not admit work, change a task,
//! satisfy a mission criterion, or authorize a provider. A later coordinator boundary must
//! validate proposals against delegated domains before using any priority suggestion.

use std::{collections::BTreeSet, fmt};

use codingmage_plan::{CheckState, PlanItemKind, TaskPlan};
use serde::{Deserialize, Serialize};

use crate::{
    CampaignSpec, DurableSchedulerSnapshot, MissionCharter, TaskTerminalReason, canonical_sha256,
    valid_commit, valid_sha256,
};

/// Closed schema shared by director input and proposal packets.
pub const DIRECTOR_PACKET_VERSION: u16 = 1;
const MAX_FAILURES: usize = 256;

/// Fresh coordinator-owned observations required to bind or revalidate a planning packet.
#[derive(Clone, Copy)]
pub struct DirectorContext<'a> {
    /// Exact campaign authority.
    pub spec: &'a CampaignSpec,
    /// Exact owner mission charter.
    pub mission: &'a MissionCharter,
    /// Parsed canonical task source.
    pub plan: &'a TaskPlan,
    /// Current durable scheduler projection.
    pub scheduler: &'a DurableSchedulerSnapshot,
    /// Current canonical campaign head.
    pub campaign_head: &'a str,
}

/// Exact source and planning generation used for one director observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorSource {
    /// Campaign identity.
    pub campaign_id: String,
    /// Repository identity.
    pub repository_id: String,
    /// Exact campaign authority digest.
    pub campaign_authority_sha256: String,
    /// Exact owner mission digest.
    pub mission_sha256: String,
    /// Canonical task-source digest.
    pub task_source_sha256: String,
    /// Current immutable scheduler generation.
    pub generation: u64,
    /// Digest of the complete scheduler snapshot.
    pub scheduler_sha256: String,
    /// Observed canonical campaign head.
    pub campaign_head: String,
}

impl DirectorSource {
    /// Binds one planning observation to verified authority, source and scheduler state.
    ///
    /// # Errors
    ///
    /// Refuses an unverified, stale or contradictory source.
    pub fn observe(context: &DirectorContext<'_>) -> Result<Self, DirectorProposalError> {
        let DirectorContext {
            spec,
            mission,
            plan,
            scheduler,
            campaign_head,
        } = context;
        spec.verify().map_err(|_| DirectorProposalError::Source)?;
        let mission_sha256 = mission
            .mission_sha256(spec)
            .map_err(|_| DirectorProposalError::Source)?;
        if plan.source_sha256 != spec.task_source_sha256
            || scheduler.campaign_id != spec.campaign_id
            || scheduler.generation == 0
            || !valid_commit(campaign_head)
        {
            return Err(DirectorProposalError::Source);
        }
        let scheduler_sha256 = scheduler
            .sha256()
            .map_err(|_| DirectorProposalError::Source)?;
        if scheduler.ready_age.keys().any(|id| {
            !plan.items.iter().any(|item| {
                &item.id == id && matches!(item.kind, PlanItemKind::Task | PlanItemKind::SubTask)
            }) && !scheduler.follow_up_bindings.contains_key(id)
        }) {
            return Err(DirectorProposalError::Source);
        }
        Ok(Self {
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            campaign_authority_sha256: spec
                .authority_sha256()
                .map_err(|_| DirectorProposalError::Source)?,
            mission_sha256,
            task_source_sha256: plan.source_sha256.clone(),
            generation: scheduler.generation,
            scheduler_sha256,
            campaign_head: (*campaign_head).to_owned(),
        })
    }

    fn verify(&self, context: &DirectorContext<'_>) -> Result<(), DirectorProposalError> {
        if *self != Self::observe(context)? {
            return Err(DirectorProposalError::Source);
        }
        Ok(())
    }
}

/// Source-checkbox progress for one sprint; this is never verified completion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorMilestone {
    /// Canonical sprint identity.
    pub sprint_id: String,
    /// Count of source-checked items in the sprint.
    pub source_checked_items: u32,
    /// Count of all checklist items in the sprint.
    pub source_total_items: u32,
}

/// Remaining coordinator-enforced limits, as observed by the caller.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemainingDirectorLimits {
    /// Provider attempts remaining.
    pub provider_attempts: u32,
    /// Process invocations remaining.
    pub process_invocations: u32,
    /// Output bytes remaining.
    pub output_bytes: u64,
    /// Retained state bytes remaining.
    pub retained_state_bytes: u64,
    /// Execution elapsed milliseconds remaining.
    pub execution_elapsed_ms: u64,
}

impl RemainingDirectorLimits {
    fn within(self, spec: &CampaignSpec) -> bool {
        self.provider_attempts <= spec.limits.provider_attempts
            && self.process_invocations <= spec.limits.process_invocations
            && self.output_bytes <= spec.limits.output_bytes
            && self.retained_state_bytes <= spec.limits.retained_state_bytes
            && self.execution_elapsed_ms <= spec.limits.execution_elapsed_ms
    }
}

/// Closed, content-free reason for a failed or blocked campaign task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectorFailureCode {
    /// A required prerequisite is unavailable.
    PrerequisiteBlocked,
    /// The implementation provider reported a bounded blocker.
    ProviderBlocked,
    /// The independent reviewer reported a blocker.
    ReviewBlocked,
    /// Review evidence requires a human decision.
    ReviewDisputed,
    /// Required deterministic verification failed.
    GateFailed,
    /// The provider failed beyond the bounded retry policy.
    ProviderFailed,
    /// The owned provider process crashed.
    ProcessCrashed,
    /// The exact task deadline elapsed.
    TimedOut,
    /// A configured task limit was reached.
    LimitExceeded,
    /// Deterministic integration detected a conflict.
    IntegrationConflict,
    /// A repository or work identity became stale.
    StaleIdentity,
    /// Deny-first policy refused an effect.
    PolicyDenied,
    /// An authenticated operator cancelled the task.
    OperatorCancelled,
    /// Required commit-bound remote checks failed.
    CiFailed,
    /// An external prerequisite remains unavailable.
    ExternalBlocked,
}

impl TryFrom<TaskTerminalReason> for DirectorFailureCode {
    type Error = DirectorProposalError;

    fn try_from(reason: TaskTerminalReason) -> Result<Self, Self::Error> {
        Ok(match reason {
            TaskTerminalReason::Merged => return Err(DirectorProposalError::Packet),
            TaskTerminalReason::PrerequisiteBlocked => Self::PrerequisiteBlocked,
            TaskTerminalReason::ProviderBlocked => Self::ProviderBlocked,
            TaskTerminalReason::ReviewBlocked => Self::ReviewBlocked,
            TaskTerminalReason::ReviewDisputed => Self::ReviewDisputed,
            TaskTerminalReason::GateFailed => Self::GateFailed,
            TaskTerminalReason::ProviderFailed => Self::ProviderFailed,
            TaskTerminalReason::ProcessCrashed => Self::ProcessCrashed,
            TaskTerminalReason::TimedOut => Self::TimedOut,
            TaskTerminalReason::LimitExceeded => Self::LimitExceeded,
            TaskTerminalReason::IntegrationConflict => Self::IntegrationConflict,
            TaskTerminalReason::StaleIdentity => Self::StaleIdentity,
            TaskTerminalReason::PolicyDenied => Self::PolicyDenied,
            TaskTerminalReason::OperatorCancelled => Self::OperatorCancelled,
            TaskTerminalReason::CiFailed => Self::CiFailed,
            TaskTerminalReason::ExternalBlocked => Self::ExternalBlocked,
        })
    }
}

/// One content-free failure observation tied to a canonical task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorFailure {
    /// Canonical task identity.
    pub task_id: String,
    /// Closed coordinator failure code, never provider prose.
    pub code: DirectorFailureCode,
}

/// Bounded director input over owner criteria, source progress and ready work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorInput {
    /// Closed packet schema.
    pub version: u16,
    /// Exact source binding.
    pub source: DirectorSource,
    /// Owner-authored criteria in charter order, copied exactly.
    pub mission_criteria: Vec<String>,
    /// Source-checkbox sprint progress; never acceptance evidence.
    pub milestones: Vec<DirectorMilestone>,
    /// Exact scheduler-ready task identities in stable order.
    pub ready_task_ids: Vec<String>,
    /// Bounded, content-free failure observations.
    pub failures: Vec<DirectorFailure>,
    /// Remaining coordinator-enforced limits at observation time.
    pub remaining: RemainingDirectorLimits,
}

impl DirectorInput {
    /// Builds a bounded packet from exact authority and source observations.
    ///
    /// The caller supplies observed failures and remaining limits from coordinator state;
    /// this packet itself cannot update those counters.
    ///
    /// # Errors
    ///
    /// Refuses stale source, unknown failures, malformed codes or impossible limits.
    pub fn build(
        context: &DirectorContext<'_>,
        failures: Vec<DirectorFailure>,
        remaining: RemainingDirectorLimits,
    ) -> Result<Self, DirectorProposalError> {
        let DirectorContext {
            mission,
            plan,
            scheduler,
            ..
        } = context;
        let input = Self {
            version: DIRECTOR_PACKET_VERSION,
            source: DirectorSource::observe(context)?,
            mission_criteria: mission.success_criteria.clone(),
            milestones: milestones(plan)?,
            ready_task_ids: scheduler.ready_age.keys().cloned().collect(),
            failures,
            remaining,
        };
        input.verify(context)?;
        Ok(input)
    }

    /// Rechecks every source-derived field before a packet is offered or persisted.
    ///
    /// # Errors
    ///
    /// Refuses a changed source, generation, authority or malformed observation.
    pub fn verify(&self, context: &DirectorContext<'_>) -> Result<(), DirectorProposalError> {
        let DirectorContext {
            spec,
            mission,
            plan,
            scheduler,
            ..
        } = context;
        self.source.verify(context)?;
        if self.version != DIRECTOR_PACKET_VERSION
            || self.mission_criteria != mission.success_criteria
            || self.milestones != milestones(plan)?
            || self.ready_task_ids != scheduler.ready_age.keys().cloned().collect::<Vec<_>>()
            || self.failures.len() > MAX_FAILURES
            || self
                .failures
                .iter()
                .map(|failure| (&failure.task_id, &failure.code))
                .collect::<BTreeSet<_>>()
                .len()
                != self.failures.len()
            || !self.remaining.within(spec)
            || self.failures.iter().any(|failure| {
                !plan.items.iter().any(|item| {
                    item.id == failure.task_id
                        && matches!(item.kind, PlanItemKind::Task | PlanItemKind::SubTask)
                }) && !scheduler.follow_up_bindings.contains_key(&failure.task_id)
            })
        {
            return Err(DirectorProposalError::Packet);
        }
        Ok(())
    }

    /// Canonical digest after source and semantic checks.
    ///
    /// # Errors
    ///
    /// Refuses a stale or malformed packet.
    pub fn sha256(&self, context: &DirectorContext<'_>) -> Result<String, DirectorProposalError> {
        self.verify(context)?;
        canonical_sha256(self).map_err(|_| DirectorProposalError::Packet)
    }
}

fn milestones(plan: &TaskPlan) -> Result<Vec<DirectorMilestone>, DirectorProposalError> {
    plan.sprints
        .iter()
        .map(|sprint| {
            let prefix = format!("{}.", sprint.id);
            let items = plan
                .items
                .iter()
                .filter(|item| item.id.starts_with(&prefix));
            let mut total = 0_u32;
            let mut checked = 0_u32;
            for item in items {
                total = total.checked_add(1).ok_or(DirectorProposalError::Packet)?;
                if item.state == CheckState::Checked {
                    checked = checked
                        .checked_add(1)
                        .ok_or(DirectorProposalError::Packet)?;
                }
            }
            Ok(DirectorMilestone {
                sprint_id: sprint.id.clone(),
                source_checked_items: checked,
                source_total_items: total,
            })
        })
        .collect()
}

/// Inert priority proposal for one exact director input; this grants no execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorProposal {
    /// Closed packet schema.
    pub version: u16,
    /// Exact observed source and planning generation.
    pub source: DirectorSource,
    /// Canonical digest of the complete director input.
    pub input_sha256: String,
    /// Complete permutation of dependency-ready task IDs; no invented or erased work.
    pub priority_order: Vec<String>,
    /// Zero-based owner-criterion indices needing attention.
    pub attention_criteria: Vec<usize>,
}

impl DirectorProposal {
    /// Checks structural source binding against the exact validated input.
    ///
    /// Delegated-domain and outcome-priority policy belongs to the next admission boundary.
    ///
    /// # Errors
    ///
    /// Refuses stale, invented, omitted or duplicate task and criterion identities.
    pub fn verify(
        &self,
        input: &DirectorInput,
        input_sha256: &str,
    ) -> Result<(), DirectorProposalError> {
        let expected_tasks = input.ready_task_ids.iter().collect::<BTreeSet<_>>();
        let proposed_tasks = self.priority_order.iter().collect::<BTreeSet<_>>();
        let criteria = self.attention_criteria.iter().collect::<BTreeSet<_>>();
        if self.version != DIRECTOR_PACKET_VERSION
            || self.source != input.source
            || !valid_sha256(input_sha256)
            || self.input_sha256 != input_sha256
            || self.priority_order.len() != input.ready_task_ids.len()
            || proposed_tasks != expected_tasks
            || criteria.len() != self.attention_criteria.len()
            || self
                .attention_criteria
                .iter()
                .any(|index| *index >= input.mission_criteria.len())
        {
            return Err(DirectorProposalError::Proposal);
        }
        Ok(())
    }
}

/// Closed director packet or storage failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectorProposalError {
    /// Campaign, mission, task source or scheduler identity changed.
    Source,
    /// Input observation is malformed or contradictory.
    Packet,
    /// Proposed task or criterion identity is malformed, stale or incomplete.
    Proposal,
}

impl fmt::Display for DirectorProposalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Source => "codingmage.director.source",
            Self::Packet => "codingmage.director.packet",
            Self::Proposal => "codingmage.director.proposal",
        })
    }
}

impl std::error::Error for DirectorProposalError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codingmage_plan::TaskPlan;

    use super::*;
    use crate::{
        CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
        CampaignPublication, DurablePodScheduler, InvolvementMode, MISSION_VERSION, MissionBudgets,
    };

    const SOURCE: &str = "# Tasks\n\n## Sprint 1 - First\n\n**Sprint goal:** Finish first.\n\n### Story 1.1 - Build\n\n- [ ] **Task 1.1.1 - Build work**\n  - [x] **Sub-task 1.1.1.1:** Prior source item.\n  - [ ] **Sub-task 1.1.1.2:** Ready work.\n\n- [ ] **AC 1.1:** Acceptance is separate.\n\n- [ ] **Gate 1.1:** Gate is separate.\n";

    fn fixture() -> (
        CampaignSpec,
        MissionCharter,
        TaskPlan,
        DurableSchedulerSnapshot,
    ) {
        let plan = TaskPlan::parse(SOURCE.as_bytes()).unwrap();
        let provider = CampaignProvider {
            executable: PathBuf::from("/bin/true"),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        let spec = CampaignSpec {
            version: 3,
            campaign_id: "director-campaign".to_owned(),
            repository_id: "director-repository".to_owned(),
            repository_path: PathBuf::from("/tmp/director-repository"),
            initial_commit: "a".repeat(40),
            task_source_sha256: plan.source_sha256.clone(),
            operator_authorization_sha256: "b".repeat(64),
            max_parallel_pods: 1,
            max_units: 10,
            limits: CampaignLimits {
                provider_attempts: 10,
                malformed_report_repairs: 1,
                correction_rounds: 2,
                process_invocations: 20,
                output_bytes: 100_000,
                retained_state_bytes: 100_000,
                execution_elapsed_ms: 10_000,
            },
            team_lead: provider.clone(),
            implementer: provider.clone(),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider,
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["unit".to_owned()],
            }],
            campaign_branch: "codingmage/director-campaign".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        };
        let mission = MissionCharter {
            version: MISSION_VERSION,
            mission_id: "director-mission".to_owned(),
            generation: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            initial_commit: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            operator_authorization_sha256: spec.operator_authorization_sha256.clone(),
            campaign_authority_sha256: spec.authority_sha256().unwrap(),
            objective: "Complete approved work".to_owned(),
            success_criteria: vec!["All approved criteria pass".to_owned()],
            exclusions: Vec::new(),
            architecture_invariants: Vec::new(),
            decision_domains: Vec::new(),
            command_registry: vec!["unit".to_owned()],
            involvement: InvolvementMode::HandsOff,
            budgets: MissionBudgets {
                max_decisions: 10,
                max_decision_retries: 2,
                max_no_progress_cycles: 3,
            },
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            revocation_epoch: 0,
        };
        mission.verify(&spec).unwrap();
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        scheduler.begin_generation(&["1.1.1.2".to_owned()]).unwrap();
        (spec, mission, plan, scheduler.snapshot().clone())
    }

    fn remaining(spec: &CampaignSpec) -> RemainingDirectorLimits {
        RemainingDirectorLimits {
            provider_attempts: spec.limits.provider_attempts,
            process_invocations: spec.limits.process_invocations,
            output_bytes: spec.limits.output_bytes,
            retained_state_bytes: spec.limits.retained_state_bytes,
            execution_elapsed_ms: spec.limits.execution_elapsed_ms,
        }
    }

    fn context<'a>(
        spec: &'a CampaignSpec,
        mission: &'a MissionCharter,
        plan: &'a TaskPlan,
        scheduler: &'a DurableSchedulerSnapshot,
    ) -> DirectorContext<'a> {
        DirectorContext {
            spec,
            mission,
            plan,
            scheduler,
            campaign_head: &spec.initial_commit,
        }
    }

    #[test]
    fn exact_source_proposal_remains_inert_and_head_bound() {
        let (spec, mission, plan, scheduler) = fixture();
        let context = context(&spec, &mission, &plan, &scheduler);
        let input = DirectorInput::build(&context, Vec::new(), remaining(&spec)).unwrap();
        assert_eq!(input.mission_criteria, mission.success_criteria);
        assert_eq!(input.milestones[0].source_checked_items, 1);
        assert_eq!(input.milestones[0].source_total_items, 5);
        let proposal = DirectorProposal {
            version: DIRECTOR_PACKET_VERSION,
            source: input.source.clone(),
            input_sha256: input.sha256(&context).unwrap(),
            priority_order: input.ready_task_ids.clone(),
            attention_criteria: vec![0],
        };
        let digest = input.sha256(&context).unwrap();
        assert_eq!(proposal.verify(&input, &digest), Ok(()));

        let moved = DirectorContext {
            campaign_head: &"c".repeat(40),
            ..context
        };
        assert_eq!(input.sha256(&moved), Err(DirectorProposalError::Source));
    }

    #[test]
    fn stale_malformed_and_unknown_field_packets_fail_closed() {
        let (spec, mission, plan, scheduler) = fixture();
        let context = context(&spec, &mission, &plan, &scheduler);
        let input = DirectorInput::build(&context, Vec::new(), remaining(&spec)).unwrap();
        let digest = input.sha256(&context).unwrap();
        let proposal = DirectorProposal {
            version: DIRECTOR_PACKET_VERSION,
            source: input.source.clone(),
            input_sha256: digest.clone(),
            priority_order: input.ready_task_ids.clone(),
            attention_criteria: vec![0],
        };
        let mut bad = proposal.clone();
        bad.priority_order = vec!["1.1.1.1".to_owned()];
        assert_eq!(
            bad.verify(&input, &digest),
            Err(DirectorProposalError::Proposal)
        );
        bad = proposal.clone();
        bad.attention_criteria = vec![0, 0];
        assert_eq!(
            bad.verify(&input, &digest),
            Err(DirectorProposalError::Proposal)
        );
        bad = proposal.clone();
        bad.source.generation += 1;
        assert_eq!(
            bad.verify(&input, &digest),
            Err(DirectorProposalError::Proposal)
        );

        let mut bad_input = input.clone();
        bad_input.remaining.provider_attempts += 1;
        assert_eq!(
            bad_input.verify(&context),
            Err(DirectorProposalError::Packet)
        );
        bad_input = input.clone();
        bad_input.milestones[0].source_checked_items = 4;
        assert_eq!(
            bad_input.verify(&context),
            Err(DirectorProposalError::Packet)
        );
        bad_input = input.clone();
        bad_input.failures.push(DirectorFailure {
            task_id: "1.1".to_owned(),
            code: DirectorFailureCode::GateFailed,
        });
        assert_eq!(
            bad_input.verify(&context),
            Err(DirectorProposalError::Packet)
        );
        bad_input = input.clone();
        bad_input.failures.extend([
            DirectorFailure {
                task_id: "1.1.1.2".to_owned(),
                code: DirectorFailureCode::ProviderBlocked,
            },
            DirectorFailure {
                task_id: "1.1.1.2".to_owned(),
                code: DirectorFailureCode::ProviderBlocked,
            },
        ]);
        assert_eq!(
            bad_input.verify(&context),
            Err(DirectorProposalError::Packet)
        );
        let mut unknown = serde_json::to_value(&proposal).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<DirectorProposal>(unknown).is_err());
    }

    #[test]
    fn only_closed_terminal_failure_codes_enter_director_input() {
        let reasons = [
            TaskTerminalReason::PrerequisiteBlocked,
            TaskTerminalReason::ProviderBlocked,
            TaskTerminalReason::ReviewBlocked,
            TaskTerminalReason::ReviewDisputed,
            TaskTerminalReason::GateFailed,
            TaskTerminalReason::ProviderFailed,
            TaskTerminalReason::ProcessCrashed,
            TaskTerminalReason::TimedOut,
            TaskTerminalReason::LimitExceeded,
            TaskTerminalReason::IntegrationConflict,
            TaskTerminalReason::StaleIdentity,
            TaskTerminalReason::PolicyDenied,
            TaskTerminalReason::OperatorCancelled,
            TaskTerminalReason::CiFailed,
            TaskTerminalReason::ExternalBlocked,
        ];
        let (spec, mission, plan, scheduler) = fixture();
        let context = context(&spec, &mission, &plan, &scheduler);
        let mut input = DirectorInput::build(&context, Vec::new(), remaining(&spec)).unwrap();
        for reason in reasons {
            let code = DirectorFailureCode::try_from(reason).unwrap();
            assert_eq!(serde_json::to_value(code).unwrap(), reason.code());
            input.failures.push(DirectorFailure {
                task_id: "1.1.1.2".to_owned(),
                code,
            });
        }
        assert_eq!(input.verify(&context), Ok(()));
        let encoded = serde_json::to_value(&input).unwrap();
        assert_eq!(
            serde_json::from_value::<DirectorInput>(encoded.clone()).unwrap(),
            input
        );
        for unknown in ["private_token_fragment", "provider prose", "merged"] {
            let mut altered = encoded.clone();
            altered["failures"][0]["code"] = serde_json::json!(unknown);
            assert!(serde_json::from_value::<DirectorInput>(altered).is_err());
        }
        assert_eq!(
            DirectorFailureCode::try_from(TaskTerminalReason::Merged),
            Err(DirectorProposalError::Packet)
        );
    }
}
