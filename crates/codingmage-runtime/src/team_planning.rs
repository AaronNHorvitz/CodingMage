//! Deterministic multi-task planning, readiness, and scheduler admission.

use std::{collections::BTreeMap, fmt::Write as _};

use codingmage_campaign::{
    AdmissionDecision, CampaignSpec, CampaignTaskRecord, CampaignTaskState, CampaignTaskTransition,
    DurablePodScheduler, TeamCampaignSnapshot, TeamLeadOutcome, TeamResourceController,
    validate_team_lead_report,
};
use codingmage_codex::{CodexLeadBinding, CodexLeadTask};
use codingmage_contracts::TeamLeadReport;
use codingmage_plan::{PlanError, PlanItemKind, SelectedWork, TaskPlan};
use sha2::{Digest, Sha256};

use crate::{RuntimeError, TeamBatchJob, generated_run_id};

/// Result of one deterministically validated team-lead planning generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeamPlanningOutcome {
    /// Zero or more nonconflicting proposals were admitted under durable leases.
    Admitted(Vec<TeamBatchJob>),
    /// The lead returned a valid typed nonexecution disposition.
    NoExecution(TeamLeadOutcome),
}

/// Creates the initial durable projection for every open canonical sub-task.
///
/// Existing checked work is not synthesized into unevidenced merged records. Open tasks begin
/// planned, and only tasks whose canonical dependencies are checked become ready.
///
/// # Errors
///
/// Returns [`RuntimeError`] when campaign authority and canonical plan bytes disagree.
pub fn initialize_team_campaign(
    spec: &CampaignSpec,
    plan: &TaskPlan,
) -> Result<TeamCampaignSnapshot, RuntimeError> {
    spec.verify().map_err(RuntimeError::Campaign)?;
    if plan.source_sha256 != spec.task_source_sha256 {
        return Err(RuntimeError::Plan);
    }
    let scheduler = DurablePodScheduler::new(spec).map_err(|_| RuntimeError::State)?;
    let resources = TeamResourceController::new(spec).map_err(|_| RuntimeError::State)?;
    let mut tasks = BTreeMap::new();
    for item in &plan.items {
        if item.kind == PlanItemKind::SubTask && item.state == codingmage_plan::CheckState::Open {
            let record = CampaignTaskRecord::planned(
                spec.campaign_id.clone(),
                item.id.clone(),
                spec.initial_commit.clone(),
            )
            .map_err(|_| RuntimeError::State)?;
            tasks.insert(item.id.clone(), record);
        }
    }
    if tasks.is_empty() {
        return Err(RuntimeError::Plan);
    }
    let mut snapshot = TeamCampaignSnapshot {
        version: codingmage_campaign::TEAM_STATE_SCHEMA_VERSION,
        campaign_id: spec.campaign_id.clone(),
        generation: 0,
        campaign_head: spec.initial_commit.clone(),
        task_source_sha256: plan.source_sha256.clone(),
        scheduler: scheduler.snapshot().clone(),
        resources: resources.snapshot().clone(),
        tasks,
        integration_queue: Vec::new(),
    };
    refresh_team_readiness(plan, &mut snapshot)?;
    Ok(snapshot)
}

/// Promotes newly dependency-ready planned tasks and refreshes unassigned bases atomically.
///
/// # Errors
///
/// Returns [`RuntimeError`] for source drift, missing task identity, or invalid durable state.
pub fn refresh_team_readiness(
    plan: &TaskPlan,
    snapshot: &mut TeamCampaignSnapshot,
) -> Result<(), RuntimeError> {
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    if plan.source_sha256 != snapshot.task_source_sha256 {
        return Err(RuntimeError::Plan);
    }
    let mut candidate = snapshot.clone();
    let task_ids = candidate.tasks.keys().cloned().collect::<Vec<_>>();
    for task_id in task_ids {
        let state = candidate.tasks[&task_id].state;
        if !matches!(state, CampaignTaskState::Planned | CampaignTaskState::Ready) {
            continue;
        }
        let ready = match plan.select_exact(&task_id) {
            Ok(selected) => Some(selected),
            Err(PlanError::InvalidDependency | PlanError::NoReadyWork) => None,
            Err(_) => return Err(RuntimeError::Plan),
        };
        let Some(_) = ready else {
            continue;
        };
        let record = candidate
            .tasks
            .get_mut(&task_id)
            .ok_or(RuntimeError::State)?;
        if record.state == CampaignTaskState::Planned {
            let transition = CampaignTaskTransition {
                sequence: record.next_transition,
                campaign_id: record.campaign_id.clone(),
                task_id: record.task_id.clone(),
                generation: record.generation,
                from: CampaignTaskState::Planned,
                to: CampaignTaskState::Ready,
                evidence_sha256: planning_evidence(record, "ready"),
            };
            record
                .transition(&transition)
                .map_err(|_| RuntimeError::State)?;
        }
        record
            .refresh_ready_base(
                candidate.campaign_head.clone(),
                planning_evidence(record, "refresh-base"),
            )
            .map_err(|_| RuntimeError::State)?;
    }
    candidate.verify().map_err(|_| RuntimeError::State)?;
    *snapshot = candidate;
    Ok(())
}

/// Builds the exact bounded ready-set supplied to one read-only Codex lead turn.
///
/// # Errors
///
/// Returns [`RuntimeError`] when no planning capacity or dependency-ready work exists.
pub fn build_team_lead_binding(
    spec: &CampaignSpec,
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
    campaign_worktree: &std::path::Path,
) -> Result<CodexLeadBinding, RuntimeError> {
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    let ready = ready_work(plan, snapshot)?;
    let active = snapshot.scheduler.active.len();
    let capacity = usize::from(snapshot.scheduler.max_parallel_pods).saturating_sub(active);
    let maximum = capacity.min(ready.len());
    if maximum == 0 {
        return Err(RuntimeError::Plan);
    }
    let ready_tasks = ready
        .into_iter()
        .map(|selected| CodexLeadTask {
            task_id: selected.item.id,
            title: selected.item.title,
            dependencies: selected.item.dependencies,
        })
        .collect();
    Ok(CodexLeadBinding {
        campaign_id: snapshot.campaign_id.clone(),
        repository_id: spec.repository_id.clone(),
        worktree: campaign_worktree.to_path_buf(),
        campaign_head: snapshot.campaign_head.clone(),
        task_source_sha256: snapshot.task_source_sha256.clone(),
        maximum_proposals: u16::try_from(maximum).map_err(|_| RuntimeError::State)?,
        allowed_paths: spec.allowed_paths.clone(),
        denied_paths: spec.denied_paths.clone(),
        gate_tiers: spec
            .gate_tiers
            .iter()
            .map(|tier| tier.name.clone())
            .collect(),
        ready_tasks,
    })
}

/// Validates one untrusted lead report and durably admits every nonconflicting proposal.
///
/// A candidate snapshot is persisted before it replaces caller state. Deferred proposals are
/// returned to ready state and produce no worktree, process, provider, or remote effect.
///
/// # Errors
///
/// Returns [`RuntimeError`] for stale lead output, invalid authority, or persistence failure.
pub fn admit_team_lead_report<P>(
    spec: &CampaignSpec,
    plan: &TaskPlan,
    snapshot: &mut TeamCampaignSnapshot,
    report: TeamLeadReport,
    started_at_ms: u64,
    mut persist: P,
) -> Result<TeamPlanningOutcome, RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    let ready = ready_work(plan, snapshot)?;
    let mut current_spec = spec.clone();
    current_spec
        .initial_commit
        .clone_from(&snapshot.campaign_head);
    current_spec
        .task_source_sha256
        .clone_from(&snapshot.task_source_sha256);
    let outcome =
        validate_team_lead_report(report, &current_spec, &ready).map_err(RuntimeError::Campaign)?;
    let TeamLeadOutcome::Proposals(proposals) = outcome else {
        return Ok(TeamPlanningOutcome::NoExecution(outcome));
    };
    let mut candidate = snapshot.clone();
    let mut scheduler = DurablePodScheduler::from_snapshot(candidate.scheduler.clone())
        .map_err(|_| RuntimeError::State)?;
    let ready_ids = ready
        .iter()
        .map(|selected| selected.item.id.clone())
        .collect::<Vec<_>>();
    let generation = scheduler
        .begin_generation(&ready_ids)
        .map_err(|_| RuntimeError::State)?;
    candidate.generation = generation;
    let resources =
        TeamResourceController::from_snapshot(&current_spec, candidate.resources.clone())
            .map_err(|_| RuntimeError::State)?;
    let mut jobs = Vec::new();
    for (sequence, proposal) in proposals.into_iter().enumerate() {
        let record = candidate
            .tasks
            .get_mut(&proposal.task_id)
            .ok_or(RuntimeError::State)?;
        record
            .propose(generation, planning_evidence(record, "proposed"))
            .map_err(|_| RuntimeError::State)?;
        match scheduler
            .admit(
                &current_spec,
                generation,
                &candidate.campaign_head,
                &proposal,
            )
            .map_err(|_| RuntimeError::State)?
        {
            AdmissionDecision::Admitted(lease) => {
                record
                    .bind_lease(&lease, planning_evidence(record, "leased"))
                    .map_err(|_| RuntimeError::State)?;
                let reservation = resources
                    .implementation_request(&lease, started_at_ms)
                    .map_err(|_| RuntimeError::State)?;
                jobs.push(TeamBatchJob {
                    sequence: u64::try_from(sequence).map_err(|_| RuntimeError::State)?,
                    run_id: generated_run_id()?,
                    lease,
                    reservation,
                });
            }
            AdmissionDecision::Deferred { .. } => {
                let transition = CampaignTaskTransition {
                    sequence: record.next_transition,
                    campaign_id: record.campaign_id.clone(),
                    task_id: record.task_id.clone(),
                    generation: record.generation,
                    from: CampaignTaskState::Proposed,
                    to: CampaignTaskState::Ready,
                    evidence_sha256: planning_evidence(record, "deferred"),
                };
                record
                    .transition(&transition)
                    .map_err(|_| RuntimeError::State)?;
            }
        }
    }
    candidate.scheduler = scheduler.snapshot().clone();
    candidate.verify().map_err(|_| RuntimeError::State)?;
    persist(&candidate)?;
    *snapshot = candidate;
    Ok(TeamPlanningOutcome::Admitted(jobs))
}

fn ready_work(
    plan: &TaskPlan,
    snapshot: &TeamCampaignSnapshot,
) -> Result<Vec<SelectedWork>, RuntimeError> {
    if plan.source_sha256 != snapshot.task_source_sha256 {
        return Err(RuntimeError::Plan);
    }
    let mut ready = Vec::new();
    for item in &plan.items {
        if item.kind == PlanItemKind::SubTask
            && snapshot
                .tasks
                .get(&item.id)
                .is_some_and(|record| record.state == CampaignTaskState::Ready)
        {
            ready.push(
                plan.select_exact(&item.id)
                    .map_err(|_| RuntimeError::Plan)?,
            );
        }
    }
    if ready.is_empty() {
        return Err(RuntimeError::Plan);
    }
    ready.sort_by_key(|selected| {
        std::cmp::Reverse(
            snapshot
                .scheduler
                .ready_age
                .get(&selected.item.id)
                .copied()
                .unwrap_or_default(),
        )
    });
    Ok(ready)
}

fn planning_evidence(record: &CampaignTaskRecord, phase: &str) -> String {
    let material = format!(
        "{}\0{}\0{}\0{}\0{phase}",
        record.campaign_id, record.task_id, record.generation, record.next_transition
    );
    let digest = Sha256::digest(material.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codingmage_campaign::{
        CampaignAuthentication, CampaignConcurrency, CampaignExecutionMode, CampaignGateTier,
        CampaignLimits, CampaignProvider, CampaignPublication, DestinationPromotionPolicy,
        MultiAgentPolicy, PodRisk, TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode,
        TeamResourcePolicy,
    };
    use codingmage_contracts::{LeadDispositionKind, TeamLeadProposal};

    use super::*;

    const PLAN: &str = "# Tasks\n\n## Sprint 24 - Team\n\n**Sprint goal:** Admit independent work.\n\n### Story 24.1 - Scheduling\n\n- [ ] **Task 24.1.1 - Run tasks**\n  - [ ] **Sub-task 24.1.1.1:** Implement the first independent bounded unit.\n  - [ ] **Sub-task 24.1.1.2:** Implement the second independent bounded unit.\n  - [ ] **Sub-task 24.1.1.3:** Implement the third independent bounded unit.\n  - [ ] **Sub-task 24.1.1.4:** Implement the fourth independent bounded unit.\n  - [ ] **Sub-task 24.1.1.5:** Implement the fifth independent bounded unit.\n  - [ ] **Sub-task 24.1.1.6:** Implement the dependency blocked bounded unit.\n<!-- depends-on: 24.1.1.1 -->\n";

    #[test]
    fn five_ready_tasks_are_bound_and_admitted_while_dependency_stays_planned() {
        let plan = TaskPlan::parse(PLAN.as_bytes()).unwrap();
        let spec = spec(&plan, 5);
        let mut snapshot = initialize_team_campaign(&spec, &plan).unwrap();
        assert_eq!(
            snapshot
                .tasks
                .values()
                .filter(|record| record.state == CampaignTaskState::Ready)
                .count(),
            5
        );
        assert_eq!(snapshot.tasks["24.1.1.6"].state, CampaignTaskState::Planned);
        let root = std::env::temp_dir();
        let binding = build_team_lead_binding(&spec, &plan, &snapshot, &root).unwrap();
        assert_eq!(binding.maximum_proposals, 5);
        assert_eq!(binding.ready_tasks.len(), 5);

        let report = report(&snapshot, (1..=5).map(proposal).collect());
        let mut persisted = 0;
        let TeamPlanningOutcome::Admitted(jobs) =
            admit_team_lead_report(&spec, &plan, &mut snapshot, report, 1_000, |_| {
                persisted += 1;
                Ok(())
            })
            .unwrap()
        else {
            panic!("proposals must admit");
        };
        assert_eq!(jobs.len(), 5);
        assert_eq!(persisted, 1);
        assert_eq!(snapshot.scheduler.active.len(), 5);
        assert!(
            jobs.iter()
                .all(|job| snapshot.tasks[&job.lease.task_id].state == CampaignTaskState::Leased)
        );
        snapshot.verify().unwrap();
    }

    #[test]
    fn overlapping_lead_assignments_defer_without_effect_and_persistence_is_atomic() {
        let plan = TaskPlan::parse(PLAN.as_bytes()).unwrap();
        let spec = spec(&plan, 5);
        let mut snapshot = initialize_team_campaign(&spec, &plan).unwrap();
        let mut first = proposal(1);
        let mut second = proposal(2);
        first.owned_paths = vec![PathBuf::from("crates/shared")];
        first.expected_artifacts = first.owned_paths.clone();
        second.owned_paths = vec![PathBuf::from("crates/shared/nested")];
        second.expected_artifacts = second.owned_paths.clone();
        let overlapping = report(&snapshot, vec![first, second]);
        let TeamPlanningOutcome::Admitted(jobs) =
            admit_team_lead_report(&spec, &plan, &mut snapshot, overlapping, 1_000, |_| Ok(()))
                .unwrap()
        else {
            panic!("valid lead proposals expected");
        };
        assert_eq!(jobs.len(), 1);
        assert_eq!(snapshot.tasks["24.1.1.2"].state, CampaignTaskState::Ready);

        let mut fresh = initialize_team_campaign(&spec, &plan).unwrap();
        let before = fresh.clone();
        let result = admit_team_lead_report(
            &spec,
            &plan,
            &mut fresh,
            report(&before, vec![proposal(1)]),
            1_000,
            |_| Err(RuntimeError::State),
        );
        assert_eq!(result, Err(RuntimeError::State));
        assert_eq!(fresh, before);
    }

    fn spec(plan: &TaskPlan, capacity: u16) -> CampaignSpec {
        CampaignSpec {
            version: 3,
            campaign_id: "campaign-planning".to_owned(),
            repository_id: "repo-planning".to_owned(),
            repository_path: std::env::temp_dir(),
            initial_commit: "a".repeat(40),
            task_source_sha256: plan.source_sha256.clone(),
            operator_authorization_sha256: "b".repeat(64),
            max_parallel_pods: capacity,
            max_units: 100,
            limits: CampaignLimits {
                provider_attempts: 100,
                malformed_report_repairs: 10,
                correction_rounds: 20,
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
                name: "focused".to_owned(),
                profiles: vec!["workspace".to_owned()],
            }],
            campaign_branch: "codingmage/campaign-planning".to_owned(),
            allowed_paths: vec![PathBuf::from("crates")],
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: if capacity == 1 {
                    CampaignExecutionMode::Serial
                } else {
                    CampaignExecutionMode::Parallel
                },
                publication_mode: TaskPublicationMode::LocalOnly,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
                task_merge_strategy: TaskMergeStrategy::Squash,
                concurrency: CampaignConcurrency {
                    claude_implementers: capacity,
                    codex_team_leads: 1,
                    codex_reviewers: capacity,
                    test_workers: capacity,
                    github_writers: 1,
                    integration_workers: 1,
                },
                resources: TeamResourcePolicy::default(),
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 10,
            }),
        }
    }

    fn provider(name: &str) -> CampaignProvider {
        CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        }
    }

    fn proposal(index: u8) -> TeamLeadProposal {
        TeamLeadProposal {
            task_id: format!("24.1.1.{index}"),
            dependencies: Vec::new(),
            owned_paths: vec![PathBuf::from(format!("crates/unit-{index}"))],
            gate_tiers: vec!["focused".to_owned()],
            test_resources: vec![format!("fixture-{index}")],
            expected_artifacts: vec![PathBuf::from(format!("crates/unit-{index}"))],
            risk: PodRisk::Routine,
            rationale_summary: "bounded fixture assignment".to_owned(),
        }
    }

    fn report(snapshot: &TeamCampaignSnapshot, proposals: Vec<TeamLeadProposal>) -> TeamLeadReport {
        TeamLeadReport {
            campaign_id: snapshot.campaign_id.clone(),
            campaign_head: snapshot.campaign_head.clone(),
            task_source_sha256: snapshot.task_source_sha256.clone(),
            disposition: LeadDispositionKind::Propose,
            proposals,
            blocked: None,
            deferred: None,
            human_decision: None,
        }
    }
}
