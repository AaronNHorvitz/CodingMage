//! Parity between the interface's strict models and the real runtime contract types.

use std::collections::BTreeMap;

use codingmage_campaign::CampaignLimits;
use codingmage_runtime::{
    CampaignActiveTaskStatus, CampaignBlockerExplanation, CampaignControlOutcome, CampaignOutcome,
    CampaignPreflightControls, CampaignPreflightGates, CampaignPreflightPolicy,
    CampaignPreflightProvider, CampaignPreflightReport, CampaignPreflightRepository,
    CampaignPreflightStorage, CampaignState, CampaignStatus, CampaignStatusDeferral,
    CampaignStatusOutcomes, CampaignStatusTaskReason, CampaignStatusUtilization,
    CampaignStopReason, TeamCampaignReport, TeamCompletionReconciliation, TeamTaskCompletionReport,
};
use codingmage_ui::backend::models::{
    SUPPORTED_PREFLIGHT_SCHEMA_VERSION, SUPPORTED_REPORT_VERSION, SUPPORTED_SCHEMA_VERSION,
    SUPPORTED_STATUS_SCHEMA_VERSION, parse_blocker_explanation, parse_campaign_outcome,
    parse_campaign_report, parse_campaign_status, parse_control_outcome, parse_preflight,
};

fn limits() -> CampaignLimits {
    CampaignLimits {
        provider_attempts: 10,
        malformed_report_repairs: 2,
        correction_rounds: 3,
        process_invocations: 100,
        output_bytes: 1000,
        retained_state_bytes: 1000,
        execution_elapsed_ms: 1000,
    }
}

#[test]
fn campaign_status_round_trips_through_the_interface_model() {
    let status = CampaignStatus {
        schema_version: SUPPORTED_STATUS_SCHEMA_VERSION,
        campaign_id: "c".to_owned(),
        state: "running_unit".to_owned(),
        actor: "pod".to_owned(),
        model: Some("fixture-implementer".to_owned()),
        branch: "codingmage/c".to_owned(),
        head: "h".to_owned(),
        current_task_id: Some("1.1.1.1".to_owned()),
        current_round: Some(0),
        active_tasks: vec![CampaignActiveTaskStatus {
            task_id: "1.1.1.1".to_owned(),
            pod_id: Some("pod-1".to_owned()),
            state: "implementing".to_owned(),
            actor: "pod".to_owned(),
            model: Some("fixture-implementer".to_owned()),
            correction_round: 0,
            heartbeat_sequence: 3,
        }],
        last_task_id: None,
        completed_units: 0,
        attempt_count: 1,
        planning_generation: 1,
        identical_planning_generations: 0,
        pending_planning_triggers: vec!["initial".to_owned()],
        watchdog_state: "live".to_owned(),
        reconciliation_state: "reconciled".to_owned(),
        outcomes: CampaignStatusOutcomes {
            completed: 0,
            blocked: 1,
            deferred: 1,
            pending_human_decision: 1,
            rejected_proposals: 0,
            accepted: 3,
            max_accepted: 10,
        },
        utilization: CampaignStatusUtilization {
            provider_attempts: 1,
            malformed_report_repairs: 0,
            correction_rounds: 0,
            process_invocations: 3,
            output_bytes: 5,
            retained_state_bytes: 6,
            execution_elapsed_ms: 7,
        },
        limits: limits(),
        blocker_count: 3,
        blocker_code: None,
        blockers: vec![CampaignStatusTaskReason {
            task_id: "1.1.1.2".to_owned(),
            reason_code: "unavailable_external_dependency".to_owned(),
        }],
        deferrals: vec![CampaignStatusDeferral {
            task_id: "1.1.1.3".to_owned(),
            reason_code: "operator_pause".to_owned(),
            trigger_code: "operator_resume".to_owned(),
            trigger_state: "pending".to_owned(),
        }],
        human_decisions: vec![CampaignStatusTaskReason {
            task_id: "1.1.1.4".to_owned(),
            reason_code: "ambiguous_scope".to_owned(),
        }],
        elapsed_ms: 8,
        updated_at_ms: 9,
    };
    let bytes = serde_json::to_vec(&Some(&status)).unwrap();
    let parsed = parse_campaign_status(&bytes).unwrap().unwrap();
    assert_eq!(parsed.active_tasks[0].pod_id.as_deref(), Some("pod-1"));
    assert_eq!(parsed.deferrals[0].trigger_state, "pending");
    assert_eq!(parsed.limits.provider_attempts, 10);
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        serde_json::to_value(&status).unwrap()
    );
}

#[test]
fn explanation_outcome_and_control_round_trip() {
    let explanation = CampaignBlockerExplanation {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        campaign_id: "c".to_owned(),
        state: "blocked".to_owned(),
        blocker_code: Some("codingmage.campaign.no_unblocked_ready_work".to_owned()),
        blockers: Vec::new(),
        deferrals: Vec::new(),
        human_decisions: Vec::new(),
    };
    let parsed = parse_blocker_explanation(&serde_json::to_vec(&explanation).unwrap()).unwrap();
    assert_eq!(
        parsed.blocker_code.as_deref(),
        Some("codingmage.campaign.no_unblocked_ready_work")
    );
    let outcome = CampaignOutcome {
        campaign_id: "c".to_owned(),
        state: CampaignState::Paused,
        branch: "codingmage/c".to_owned(),
        head: "h".to_owned(),
        completed_units: 1,
        stop_reason: CampaignStopReason::OperatorPause,
        last_task_id: Some("1.1.1.1".to_owned()),
        blocker_code: Some("codingmage.campaign.control.paused".to_owned()),
    };
    let parsed = parse_campaign_outcome(&serde_json::to_vec(&outcome).unwrap()).unwrap();
    assert_eq!(parsed.state, "paused");
    assert_eq!(parsed.stop_reason, "operator_pause");
    let control = CampaignControlOutcome {
        campaign_id: "c".to_owned(),
        request_id: "ui-pause-1".to_owned(),
        action: "pause".to_owned(),
        created: true,
    };
    let parsed = parse_control_outcome(&serde_json::to_vec(&control).unwrap()).unwrap();
    assert!(parsed.created);
}

#[test]
fn preflight_report_round_trips() {
    let report = CampaignPreflightReport {
        schema_version: SUPPORTED_PREFLIGHT_SCHEMA_VERSION,
        state: "ready".to_owned(),
        authority_sha256: "a".repeat(64),
        operator_authorization_sha256: "b".repeat(64),
        repository: CampaignPreflightRepository {
            repository_id: "repo-1-2".to_owned(),
            initial_commit: "c".repeat(40),
            branch_sha256: "d".repeat(64),
            task_source_sha256: "e".repeat(64),
            status_sha256: "f".repeat(64),
            references_sha256: "0".repeat(64),
            worktrees_sha256: "1".repeat(64),
            plan_item_count: 12,
            open_subtask_count: 10,
            clean: true,
            dedicated_branch: true,
            checkout_safe: true,
        },
        policy: CampaignPreflightPolicy {
            max_parallel_pods: 1,
            max_accepted_outcomes: 10,
            publication: "local_only".to_owned(),
            default_branch_protected: true,
            allowed_path_count: 1,
            allowed_paths_sha256: "2".repeat(64),
            task_path_authority_count: 0,
            task_path_authority_sha256: "3".repeat(64),
            denied_path_count: 0,
            denied_paths_sha256: "4".repeat(64),
            external_capabilities_denied: true,
        },
        providers: vec![CampaignPreflightProvider {
            role: "implementer".to_owned(),
            executable_sha256: "5".repeat(64),
            profile_sha256: "6".repeat(64),
            capabilities_sha256: "7".repeat(64),
            authentication: "existing_login".to_owned(),
            probe_process_count: 2,
            capability_verified: true,
        }],
        gates: CampaignPreflightGates {
            command_count: 1,
            registry_sha256: "8".repeat(64),
            executable_sha256: vec!["9".repeat(64)],
            tier_count: 1,
            tiers_sha256: "a".repeat(64),
        },
        controls: CampaignPreflightControls {
            process_guard_sha256: "b".repeat(64),
            operator_control_count: 4,
            operator_controls_sha256: "c".repeat(64),
            process_guard_verified: true,
        },
        storage: CampaignPreflightStorage {
            scratch_sufficient: true,
            state_sufficient: true,
            required_available_bytes: 1000,
            sufficient: true,
        },
        source_free: true,
    };
    let parsed = parse_preflight(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(parsed.repository.open_subtask_count, 10);
    assert!(parsed.providers[0].capability_verified);
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        serde_json::to_value(&report).unwrap()
    );
}

#[test]
fn team_report_round_trips() {
    let report = TeamCampaignReport {
        version: SUPPORTED_REPORT_VERSION,
        campaign_id: "c".to_owned(),
        repository_id: "repo-1-2".to_owned(),
        branch: "codingmage/c".to_owned(),
        initial_commit: "a".repeat(40),
        final_commit: "b".repeat(40),
        task_source_sha256: "c".repeat(64),
        tasks: BTreeMap::from([(
            "1.1.1.1".to_owned(),
            TeamTaskCompletionReport {
                reviewed_commit: "d".repeat(40),
                integration_commit: "e".repeat(40),
                completion_commit: "f".repeat(40),
            },
        )]),
        reconciliation: TeamCompletionReconciliation {
            state_sha256: "0".repeat(64),
            completed_task_ids_sha256: "1".repeat(64),
            removed_worktree_ids_sha256: "2".repeat(64),
            task_evidence_sha256: "3".repeat(64),
            checked_task_count: 1,
            removed_worktree_count: 1,
            process_control_root_count: 1,
            process_control_residue_count: 0,
            active_lease_count: 0,
            active_reservation_count: 0,
            integration_queue_count: 0,
            journal_reconciled: true,
            task_source_reconciled: true,
        },
        final_gate_evidence_sha256: "4".repeat(64),
        final_review_evidence_sha256: "5".repeat(64),
        completed_at_ms: 1,
    };
    let parsed = parse_campaign_report(&serde_json::to_vec(&Some(&report)).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.tasks["1.1.1.1"].reviewed_commit, "d".repeat(40));
    assert!(parsed.reconciliation.task_source_reconciled);
}
