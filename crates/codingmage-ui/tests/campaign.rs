//! Campaign and team state over real durable coordinator records.

mod common;

use std::{fs, os::unix::fs::symlink, path::Path, process::Command, time::Duration};

use codingmage_campaign::{
    CampaignConcurrency, CampaignExecutionMode, CampaignSpec, DestinationPromotionPolicy,
    MultiAgentPolicy, TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode,
    TeamResourcePolicy,
};
use codingmage_ui::{
    Screen,
    backend::{
        BackendError, Binding, CoordinatorBinary, Generation, Response,
        models::{
            ActiveTask, BlockerExplanation, CampaignStatus, Deferral, MissionStatus, ModelError,
            TaskReason,
        },
    },
    campaign::{CampaignSelection, SelectError},
};
use common::{
    Fixture, coordinator_binary, git, harness, harness_with_state, run_campaign, settle,
    write_campaign,
};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

fn opened(fixture: &Fixture) -> egui_kittest::Harness<'static, codingmage_ui::App> {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 760.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness
}

#[test]
fn campaign_snapshot_refuses_replaced_picker_file_and_malformed_authority() {
    let fixture = Fixture::new("campaign-snapshot-replacement", 1);
    let selected = write_campaign(&fixture, "selected", 1);
    let other = write_campaign(&fixture, "other", 1);
    let selected_text = selected.to_str().unwrap();
    let listing = Command::new(coordinator_binary())
        .args([
            "directory-list",
            "--directory",
            fixture.root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(listing.status.success());
    let listing: serde_json::Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert!(listing["entries"].as_array().unwrap().iter().any(|entry| {
        entry["name"] == selected.file_name().unwrap().to_str().unwrap() && entry["kind"] == "file"
    }));
    let snapshot = Command::new(coordinator_binary())
        .args(["campaign-select", "--campaign", selected_text])
        .output()
        .unwrap();
    assert!(snapshot.status.success());
    let valid =
        CampaignSelection::from_snapshot(&selected, &fixture.target, None, &snapshot.stdout)
            .unwrap();
    assert_eq!(valid.spec.campaign_id, "selected");
    let mut malformed: serde_json::Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    malformed["unexpected"] = serde_json::json!(true);
    assert_eq!(
        CampaignSelection::from_snapshot(
            &selected,
            &fixture.target,
            None,
            &serde_json::to_vec(&malformed).unwrap(),
        )
        .unwrap_err(),
        SelectError::Contract
    );
    let mut unsupported: serde_json::Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    unsupported["schema_version"] = serde_json::json!(2);
    assert_eq!(
        CampaignSelection::from_snapshot(
            &selected,
            &fixture.target,
            None,
            &serde_json::to_vec(&unsupported).unwrap(),
        )
        .unwrap_err(),
        SelectError::UnsupportedSchema
    );
    let mut changed_digest: serde_json::Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    changed_digest["authority_sha256"] = serde_json::json!("0".repeat(64));
    assert_eq!(
        CampaignSelection::from_snapshot(
            &selected,
            &fixture.target,
            None,
            &serde_json::to_vec(&changed_digest).unwrap(),
        )
        .unwrap_err(),
        SelectError::Contract
    );
    let foreign = CampaignSelection::from_snapshot(&other, &fixture.target, None, &snapshot.stdout);
    assert_eq!(foreign.unwrap_err(), SelectError::Contract);
    fs::remove_file(&selected).unwrap();
    symlink(&other, &selected).unwrap();
    let rejected = Command::new(coordinator_binary())
        .args(["campaign-select", "--campaign", selected_text])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&selected);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.campaign_error().is_some()
    }));
    assert!(harness.state().campaign().is_none());
    assert!(harness.state().status().value.is_none());
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: select campaign specification")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-select --campaign");
}

#[test]
fn selection_snapshot_requires_current_request_and_generation() {
    let fixture = Fixture::new("campaign-selection-binding", 1);
    let selected = write_campaign(&fixture, "selected", 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&selected);
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    let current_id = format!("campaign-select:{}:1", generation.0);
    let response = |request_id: &str, result: Vec<u8>| Response {
        generation,
        binding: binding.clone(),
        label: "campaign-select",
        request_id: Some(request_id.to_owned()),
        result: Ok(result),
    };
    assert!(
        !harness
            .state_mut()
            .handle_response(response("wrong-request", b"{}".to_vec()))
    );
    assert!(harness.state().campaign().is_none());
    assert!(
        harness
            .state_mut()
            .handle_response(response(&current_id, b"{}".to_vec()))
    );
    assert_eq!(
        harness.state().campaign_error(),
        Some(&SelectError::Contract)
    );
    assert!(harness.state().campaign().is_none());
    harness.state_mut().select_campaign(&selected);
    let newer_id = format!("campaign-select:{}:2", harness.state().generation().0);
    harness.state_mut().clear_campaign();
    assert!(
        !harness
            .state_mut()
            .handle_response(response(&newer_id, b"{}".to_vec()))
    );
    assert!(harness.state().campaign().is_none());
    assert!(harness.state().status().value.is_none());
}

#[test]
fn selection_snapshot_cannot_use_a_failed_repository_refresh() {
    let fixture = Fixture::new("campaign-selection-stale-diagnosis", 1);
    let selected = write_campaign(&fixture, "selected", 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&selected);
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "doctor",
        request_id: None,
        result: Err(BackendError::Spawn),
    }));
    assert!(!harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-select",
        request_id: Some(format!("campaign-select:{}:1", generation.0)),
        result: Ok(b"{}".to_vec()),
    }));
    assert_eq!(
        harness.state().campaign_error(),
        Some(&SelectError::DiagnosisUnavailable)
    );
    assert!(harness.state().campaign().is_none());
    assert!(harness.state().status().value.is_none());
}

#[test]
fn stale_diagnosis_clears_the_prior_campaign_before_a_new_selection() {
    let fixture = Fixture::new("campaign-selection-stale-prior", 1);
    let first = write_campaign(&fixture, "first", 1);
    let second = write_campaign(&fixture, "second", 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&first);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.campaign().is_some()
    }));
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "doctor",
        request_id: None,
        result: Err(BackendError::Spawn),
    }));
    harness.state_mut().refresh_diagnosis();
    harness.state_mut().select_campaign(&second);
    assert!(harness.state().campaign().is_none());
    assert!(harness.state().status().value.is_none());
    assert_eq!(
        harness.state().campaign_error(),
        Some(&SelectError::DiagnosisUnavailable)
    );
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().last_error.is_none() && !app.diagnosis().loading
    }));
    assert!(harness.state().campaign().is_none());
    assert!(harness.state().status().value.is_none());
}

fn inject_report(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
    binding: &Binding,
    generation: Generation,
    value: &serde_json::Value,
) {
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-report",
        request_id: None,
        result: Ok(serde_json::to_vec(value).unwrap()),
    }));
}

fn synthetic_final_report(spec: &CampaignSpec, status: &CampaignStatus) -> serde_json::Value {
    serde_json::json!({
        "version": codingmage_ui::backend::models::SUPPORTED_REPORT_VERSION,
        "campaign_id": spec.campaign_id.clone(),
        "repository_id": spec.repository_id.clone(),
        "branch": status.branch.clone(),
        "initial_commit": spec.initial_commit.clone(),
        "final_commit": status.head.clone(),
        "task_source_sha256": "a".repeat(64),
        "tasks": {
            "0.1.1.1": {
                "reviewed_commit": "b".repeat(40),
                "integration_commit": "c".repeat(40),
                "completion_commit": "d".repeat(40)
            }
        },
        "reconciliation": {
            "state_sha256": "e".repeat(64),
            "completed_task_ids_sha256": "f".repeat(64),
            "removed_worktree_ids_sha256": "0".repeat(64),
            "task_evidence_sha256": "1".repeat(64),
            "checked_task_count": 1,
            "removed_worktree_count": 1,
            "process_control_root_count": 1,
            "process_control_residue_count": 0,
            "active_lease_count": 0,
            "active_reservation_count": 0,
            "integration_queue_count": 0,
            "journal_reconciled": true,
            "task_source_reconciled": true
        },
        "final_gate_evidence_sha256": "2".repeat(64),
        "final_review_evidence_sha256": "3".repeat(64),
        "completed_at_ms": 1
    })
}

fn blocker_explanation_fixture() -> BlockerExplanation {
    BlockerExplanation {
        schema_version: 1,
        campaign_id: "bound".to_owned(),
        state: "paused".to_owned(),
        blocker_code: Some("bound-blocker".to_owned()),
        blockers: vec![
            TaskReason {
                task_id: "0.1.1.1".to_owned(),
                reason_code: "task-block".to_owned(),
            },
            TaskReason {
                task_id: "0.1.1.1".to_owned(),
                reason_code: "blocked_prerequisite".to_owned(),
            },
        ],
        deferrals: vec![
            Deferral {
                task_id: "0.1.1.2".to_owned(),
                reason_code: "waiting".to_owned(),
                trigger_code: "source_changed".to_owned(),
                trigger_state: "pending".to_owned(),
            },
            Deferral {
                task_id: "0.1.1.2".to_owned(),
                reason_code: "temporary_provider_capacity".to_owned(),
                trigger_code: "provider_reset".to_owned(),
                trigger_state: "pending".to_owned(),
            },
            Deferral {
                task_id: "0.1.1.2".to_owned(),
                reason_code: "active_path_lease".to_owned(),
                trigger_code: "lease_release".to_owned(),
                trigger_state: "pending".to_owned(),
            },
        ],
        human_decisions: vec![
            TaskReason {
                task_id: "0.1.1.3".to_owned(),
                reason_code: "review_disputed".to_owned(),
            },
            TaskReason {
                task_id: "0.1.1.3".to_owned(),
                reason_code: "release_decision".to_owned(),
            },
        ],
    }
}

fn filter_blockers(harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>, query: &str) {
    let search = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search blockers");
    search.focus();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    search.type_text(query);
    harness.run_steps(2);
}

fn assert_work_plan_status_recovery(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
) {
    // A failed refresh retains the record internally but must not present its prior
    // task outcomes as current verification on the Work plan.
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Coordinator outcomes unavailable");
    assert!(harness.query_by_label("Outcome: completed").is_none());
    harness.get_by_label("Outcome: unknown for 0.1.1.1");
    if let Some(directory) = std::env::var_os("CODINGMAGE_UI_SNAPSHOT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        harness
            .render()
            .expect("software render")
            .save(directory.join("work-plan-stale-outcomes.png"))
            .unwrap();
    }
    harness
        .get_by_label("Show command: Refresh coordinator outcomes")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-status --config");
    harness
        .get_by_label("Show command: Refresh coordinator outcomes")
        .click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::Button,
            "Refresh coordinator outcomes",
        )
        .click();
    assert!(settle(harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some)
            && app.status().last_error.is_none()
            && !app.status().loading
            && app.head_plan().value.as_ref().is_some_and(Option::is_some)
            && !app.head_plan().loading
    }));
    harness.get_by_label("Outcome: completed");
}

#[test]
fn blocker_explanation_rejects_foreign_payload_with_current_request_binding() {
    let fixture = Fixture::new("blocker-payload-binding", 1);
    let spec = write_campaign(&fixture, "bound", 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.explanation().value.is_some()
    }));
    let retained = blocker_explanation_fixture();
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-explain-blocker",
        request_id: None,
        result: Ok(serde_json::to_vec(&retained).unwrap()),
    }));
    assert_eq!(
        harness
            .state()
            .explanation()
            .value
            .as_ref()
            .and_then(Option::as_ref),
        Some(&retained)
    );
    let mut foreign = retained.clone();
    foreign.campaign_id = "other-campaign".to_owned();
    foreign.blocker_code = Some("foreign-blocker".to_owned());
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-explain-blocker",
        request_id: None,
        result: Ok(serde_json::to_vec(&foreign).unwrap()),
    }));
    assert_eq!(
        harness
            .state()
            .explanation()
            .value
            .as_ref()
            .and_then(Option::as_ref),
        Some(&retained)
    );
    assert!(matches!(
        harness.state().explanation().last_error.as_ref(),
        Some((_, BackendError::Contract(ModelError::AuthorityMismatch)))
    ));
    harness.state_mut().select_screen(Screen::Blockers);
    harness.run_steps(2);
    harness.get_by_label_contains("Blocker observation: stale");
    harness.get_by_label_contains("bound-blocker");
    harness.get_by_label_contains("task-block");
    harness.get_by_label_contains("source_changed");
    harness.get_by_label_contains("review_disputed");
    harness.get_by_label_contains("operator-supplied request identity");
    harness
        .get_by_label("Show command: Refresh blockers")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("campaign-explain-blocker");
    filter_blockers(&mut harness, "source_changed");
    harness.get_by_label_contains("Deferred tasks (1)");
    harness.get_by_label_contains("source_changed");
    filter_blockers(&mut harness, "no-such-hold");
    harness.get_by_label_contains("No recorded hold matches this search");
    filter_blockers(&mut harness, "provider_reset");
    harness.get_by_label_contains("an authorized operator can record provider_reset");
    filter_blockers(&mut harness, "lease_release");
    harness.get_by_label_contains("operator evidence command cannot mark this trigger");
    filter_blockers(&mut harness, "blocked_prerequisite");
    harness.get_by_label_contains("canonical outcome must be accepted");
    filter_blockers(&mut harness, "release_decision");
    harness.get_by_label_contains("campaign completion is not release approval");
}

fn checked_head_projection(fixture: &Fixture, spec: &Path, head: &str) -> serde_json::Value {
    let output = Command::new(coordinator_binary())
        .args([
            "campaign-head-plan",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            head,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let projection: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(projection["schema_version"], 1);
    assert_eq!(projection["head"], head);
    assert_eq!(projection["items"][0]["id"], "0.1.1.1");
    assert_eq!(projection["items"][0]["state"], "checked");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Complete fixture operation"));
    let stale = Command::new(coordinator_binary())
        .args([
            "campaign-head-plan",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            &"0".repeat(40),
        ])
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert_eq!(
        String::from_utf8_lossy(&stale.stderr).trim(),
        "codingmage.cli.stale_observation"
    );
    assert_task_detail_projection(fixture, spec, head, &projection);
    projection
}

fn assert_task_detail_projection(
    fixture: &Fixture,
    spec: &Path,
    head: &str,
    projection: &serde_json::Value,
) {
    let detail = Command::new(coordinator_binary())
        .args([
            "campaign-task-detail",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            head,
            "--item",
            "0.1.1.1",
        ])
        .output()
        .unwrap();
    assert!(
        detail.status.success(),
        "{}",
        String::from_utf8_lossy(&detail.stderr)
    );
    let detail: serde_json::Value = serde_json::from_slice(&detail.stdout).unwrap();
    assert_eq!(detail["head"], head);
    assert_eq!(detail["item_id"], "0.1.1.1");
    assert_eq!(detail["source_state"], "checked");
    assert!(
        detail["excerpt"]
            .as_str()
            .unwrap()
            .contains("Complete fixture operation")
    );
    assert!(!detail["truncated"].as_bool().unwrap());
    assert_eq!(
        detail["task_source_sha256"],
        projection["task_source_sha256"]
    );
    let stale_detail = Command::new(coordinator_binary())
        .args([
            "campaign-task-detail",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            &"0".repeat(40),
            "--item",
            "0.1.1.1",
        ])
        .output()
        .unwrap();
    assert!(!stale_detail.status.success());
    assert_eq!(
        String::from_utf8_lossy(&stale_detail.stderr).trim(),
        "codingmage.cli.stale_observation"
    );
    let unknown = Command::new(coordinator_binary())
        .args([
            "campaign-task-detail",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            head,
            "--item",
            "0.1.1.999",
        ])
        .output()
        .unwrap();
    assert!(!unknown.status.success());
    assert_eq!(
        String::from_utf8_lossy(&unknown.stderr).trim(),
        "codingmage.cli.invalid_argument"
    );
}

fn assert_one_identified_pod_for_two_active_tasks(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
) {
    let mut active = harness.state().status().value.clone().flatten().unwrap();
    "implementing".clone_into(&mut active.state);
    let task = ActiveTask {
        task_id: "0.1.1.1".to_owned(),
        pod_id: Some("pod-one".to_owned()),
        state: "implementing".to_owned(),
        actor: "implementer".to_owned(),
        model: None,
        correction_round: 0,
        heartbeat_sequence: 1,
    };
    active.active_tasks = vec![task.clone(), task];
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&active).unwrap()),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("State: implementing");
    harness.get_by_label_contains("Active pods: 1 identified");
    harness.get_by_label_contains("Current gate: not reported by coordinator");
}

fn assert_unknown_bound_codes_are_labelled(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
) {
    let mut status = harness.state().status().value.clone().flatten().unwrap();
    "future_state".clone_into(&mut status.state);
    let mut mission = harness.state().mission().value.clone().flatten().unwrap();
    "future_mode".clone_into(&mut mission.involvement);
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    for (label, bytes) in [
        ("campaign-status", serde_json::to_vec(&status).unwrap()),
        (
            "campaign-mission-status",
            serde_json::to_vec(&mission).unwrap(),
        ),
    ] {
        assert!(harness.state_mut().handle_response(Response {
            generation,
            binding: binding.clone(),
            label,
            request_id: None,
            result: Ok(bytes),
        }));
    }
    harness.run_steps(2);
    harness.get_by_label_contains("State: unknown coordinator value (future_state)");
    harness.get_by_label_contains("Involvement: unknown coordinator value (future_mode)");
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label("unknown coordinator value (future_state)");
    harness.get_by_label("unknown coordinator value (future_mode)");
}

#[test]
fn never_started_campaign_is_an_explicit_empty_state() {
    let fixture = Fixture::new("campaign-empty", 3);
    let spec = write_campaign(&fixture, "empty-campaign", 2);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some()
            && app.mission().value.is_some()
            && app.explanation().value.is_some()
    }));
    assert_eq!(harness.state().status().value, Some(None));
    assert_eq!(harness.state().explanation().value, Some(None));
    assert!(harness.state().explanation().last_error.is_none());
    assert_eq!(
        harness.state().mission().value,
        Some(None),
        "no charter admitted is an explicit absent state, not a failure"
    );
    harness.state_mut().select_screen(Screen::Blockers);
    harness.run_steps(2);
    harness.get_by_label_contains("No durable campaign state was observed");
    harness.get_by_label_contains("Blocker observation: live");
    harness
        .get_by_label("Show command: Refresh blockers")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("campaign-explain-blocker");
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Selected campaign: empty-campaign");
    harness.get_by_label_contains("No durable campaign state was observed");
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    harness.get_by_label_contains("No campaign status has been recorded yet");
    assert!(harness.query_by_label("Outcome: none recorded").is_none());
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("This is an earlier observation");
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open Campaign")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Campaign);
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("Repository: target");
    harness.get_by_label_contains("Campaign: empty-campaign");
    harness.get_by_label_contains("State: not started");
    harness.get_by_label_contains("Involvement: no charter admitted");
    harness.get_by_label_contains("Active pods: 0 (not started)");
    harness.get_by_label_contains("Current gate: not reported by coordinator");
    harness.get_by_label_contains("Last backend update: none (not started)");
    harness.get_by_label_contains("No durable campaign state exists");
    harness.get_by_label_contains("No mission charter is admitted");
    harness.get_by_label_contains("Hands-off: unavailable");
    harness
        .get_by_label_contains("Active checkout matches the campaign's bound repository identity");
    let absent = Command::new(coordinator_binary())
        .args([
            "campaign-head-plan",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            &"0".repeat(40),
        ])
        .output()
        .unwrap();
    assert!(!absent.status.success());
    assert_eq!(
        String::from_utf8_lossy(&absent.stderr).trim(),
        "codingmage.cli.refused"
    );
    assert!(absent.stdout.is_empty());
    assert!(!fixture.state.join("campaigns").exists());
}

#[test]
fn foreign_campaign_status_and_mission_cannot_replace_selected_observations() {
    let fixture = Fixture::new("foreign-projection", 1);
    let spec = write_campaign(&fixture, "bound-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some) && app.mission().value.is_some()
    }));
    let original = harness.state().status().value.clone();
    let selected = harness.state().campaign().unwrap();
    let campaign_id = selected.spec.campaign_id.clone();
    let authority = selected.authority_sha256.clone();
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    let mut foreign_status = original.clone().flatten().unwrap();
    foreign_status.campaign_id = "foreign-campaign".to_owned();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&foreign_status).unwrap()),
    }));
    assert_eq!(harness.state().status().value, original);
    assert!(harness.state().status().last_error.is_some());
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("This is an earlier observation");
    harness.get_by_label_contains(&format!(
        "State: {} (stale)",
        original.as_ref().unwrap().as_ref().unwrap().state
    ));
    harness.get_by_label_contains("Last backend update:");

    let mut mission = MissionStatus {
        schema_version: 1,
        campaign_id: "foreign-campaign".to_owned(),
        mission_id: "test-mission".to_owned(),
        mission_sha256: "1".repeat(64),
        authority_sha256: authority.clone(),
        generation: 1,
        involvement: "supervised".to_owned(),
        issued_at_ms: 1,
        expires_at_ms: 2,
        expired: false,
        revocation_epoch: 0,
        revoked: false,
        decisions_recorded: 0,
        permitted_choices: 0,
        held_decisions: 0,
        pending_owner_decisions: 0,
        owner_answers: 0,
        observed_at_ms: 1,
    };
    for (id, digest) in [
        ("foreign-campaign", authority.as_str()),
        (campaign_id.as_str(), "wrong-authority"),
    ] {
        mission.campaign_id = id.to_owned();
        mission.authority_sha256 = digest.to_owned();
        assert!(harness.state_mut().handle_response(Response {
            generation,
            binding: binding.clone(),
            label: "campaign-mission-status",
            request_id: None,
            result: Ok(serde_json::to_vec(&mission).unwrap()),
        }));
        assert_eq!(harness.state().mission().value, Some(None));
        assert!(harness.state().mission().last_error.is_some());
    }
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("Stale mission observation");
    harness.get_by_label_contains("Involvement: no charter admitted (stale)");
    mission.authority_sha256 = authority;
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-mission-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&mission).unwrap()),
    }));
    assert_eq!(harness.state().mission().value, Some(Some(mission)));
    assert!(harness.state().mission().last_error.is_none());
    harness.run_steps(2);
    harness.get_by_label_contains("Involvement: Supervised");
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&original.unwrap()).unwrap()),
    }));
    assert!(harness.state().status().last_error.is_none());
    assert_one_identified_pod_for_two_active_tasks(&mut harness);
    assert_unknown_bound_codes_are_labelled(&mut harness);
}

#[test]
fn malformed_status_has_an_exact_read_only_recovery_action() {
    let fixture = Fixture::new("status-recovery", 1);
    let spec = write_campaign(&fixture, "bound-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some)
    }));
    let expected = harness.state().status().value.clone();
    let mut malformed = expected.clone().flatten().unwrap();
    malformed.campaign_id = "another-campaign".to_owned();
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&malformed).unwrap()),
    }));
    assert_eq!(harness.state().status().value, expected);
    assert!(harness.state().status().last_error.is_some());
    let prior_observation = harness.state().status().observed_at;
    let original = expected.clone().flatten().unwrap();
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("Observation: stale");
    harness.get_by_label_contains("Current campaign progress cannot be confirmed");
    harness
        .get_by_label("Show command: Refresh campaign status")
        .focus();
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-status --config");
    harness.get_by_label("Refresh campaign status").focus();
    harness.key_press(egui::Key::Enter);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status()
            .value
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|status| {
                status.campaign_id == original.campaign_id
                    && status.head == original.head
                    && status.state == original.state
            })
            && app.status().last_error.is_none()
            && app.status().observed_at != prior_observation
    }));
    assert_eq!(harness.state().generation(), generation);
    harness.get_by_label_contains("Observation: live");
}

#[test]
fn completed_unit_is_distinct_from_the_source_checkbox_and_counts_agree() {
    let fixture = Fixture::new("campaign-complete", 2);
    let spec = write_campaign(&fixture, "one-unit", 1);
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["state"], "paused");
    assert_eq!(outcome["completed_units"], 1);
    let source_after = fs::read_to_string(fixture.target.join("TASKS.md")).unwrap();
    assert!(source_after.contains("- [ ] **Sub-task 0.1.1.1:**"));
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some) && app.head_plan().value.is_some()
    }));
    let status = harness
        .state()
        .status()
        .value
        .clone()
        .flatten()
        .expect("status");
    assert_eq!(status.completed_units, 1);
    assert_eq!(status.outcomes.accepted, 1);
    assert_eq!(status.outcomes.max_accepted, 1);
    assert_eq!(status.state, "paused");
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Recorded outcomes: 1 completed, 0 blocked");
    harness.get_by_label_contains("Attention: coordinator blocker code");
    let projection = checked_head_projection(&fixture, &spec, &status.head);
    let overlay = harness.state().task_overlay();
    let first = overlay.get("0.1.1.1").unwrap().labels(true);
    assert_eq!(
        first,
        vec![
            "open in source",
            "completed at campaign head (verified, not yet in active checkout)"
        ]
    );
    let second = overlay.get("0.1.1.2").unwrap().labels(true);
    assert_eq!(second, vec!["open in source"]);
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label("paused");
    harness.get_by_label("1 of 1");
    harness.get_by_label("No unit is active.");
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    harness.get_by_label("Outcome: completed");
    harness.get_by_label_contains(
        "[ ] Sub-task 0.1.1.1 Complete fixture operation number 1 safely. - dependency-ready",
    );
    harness.get_by_label_contains(
        "[ ] Sub-task 0.1.1.2 Complete fixture operation number 2 safely. - dependency-ready",
    );
    assert!(
        harness
            .query_by_label_contains(
                "0.1.1.2 Complete fixture operation number 2 safely. - dependency-ready ["
            )
            .is_none()
    );
    assert_work_plan_status_recovery(&mut harness);
    assert_task_detail_workflow(&mut harness, &status.head);
    let mut wrong_head = projection;
    wrong_head["head"] = serde_json::json!("0".repeat(40));
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-head-plan",
        request_id: None,
        result: Ok(serde_json::to_vec(&wrong_head).unwrap()),
    }));
    assert!(harness.state().head_plan().last_error.is_some());
    assert!(
        harness.state().task_overlay()["0.1.1.1"]
            .campaign_head
            .is_none()
    );
    let mut hold = status;
    hold.blocker_code = None;
    hold.blocker_count = 1;
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&hold).unwrap()),
    }));
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Attention: the coordinator reports a hold");
}

#[test]
fn final_report_outcomes_require_bound_payload_and_current_status() {
    let fixture = Fixture::new("report-payload-binding", 1);
    let spec = write_campaign(&fixture, "report-bound", 1);
    assert_eq!(run_campaign(&fixture, &spec)["completed_units"], 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some)
    }));
    let selected = &harness.state().campaign().unwrap().spec;
    let status = harness.state().status().value.clone().flatten().unwrap();
    assert_eq!(status.state, "complete");
    let report = synthetic_final_report(selected, &status);
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    inject_report(&mut harness, &binding, generation, &report);
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_some());
    let mut paused = status.clone();
    paused.state = "paused".to_owned();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&paused).unwrap()),
    }));
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-status",
        request_id: None,
        result: Ok(serde_json::to_vec(&status).unwrap()),
    }));
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_some());

    for field in ["campaign_id", "repository_id", "initial_commit", "branch"] {
        let mut forged = report.clone();
        forged[field] = serde_json::json!("foreign-value");
        inject_report(&mut harness, &binding, generation, &forged);
        assert!(matches!(
            harness.state().report().last_error,
            Some((_, BackendError::Contract(ModelError::AuthorityMismatch)))
        ));
        assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
        inject_report(&mut harness, &binding, generation, &report);
        assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_some());
    }

    let mut wrong_head = report.clone();
    wrong_head["final_commit"] = serde_json::json!("9".repeat(40));
    inject_report(&mut harness, &binding, generation, &wrong_head);
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    inject_report(&mut harness, &binding, generation, &serde_json::Value::Null);
    assert!(
        harness
            .state()
            .report()
            .value
            .as_ref()
            .is_some_and(Option::is_none)
    );
    assert!(harness.state().report().last_error.is_none());
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    inject_report(&mut harness, &binding, generation, &report);
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
}

#[test]
fn work_plan_final_report_failure_has_exact_refresh_and_recovers() {
    let fixture = Fixture::new("report-workplan-recovery", 1);
    let spec = write_campaign(&fixture, "report-recovery", 1);
    let mut policy_spec = CampaignSpec::load(&spec).unwrap();
    policy_spec.multi_agent = Some(MultiAgentPolicy {
        version: 1,
        execution_mode: CampaignExecutionMode::Parallel,
        publication_mode: TaskPublicationMode::LocalOnly,
        task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
        destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
        task_merge_strategy: TaskMergeStrategy::Squash,
        github: None,
        concurrency: CampaignConcurrency {
            claude_implementers: 1,
            codex_team_leads: 1,
            codex_reviewers: 1,
            test_workers: 1,
            github_writers: 1,
            integration_workers: 1,
        },
        resources: TeamResourcePolicy::default(),
        max_campaign_tokens: 1_000_000,
        max_task_tokens: 500_000,
        max_task_correction_cycles: 3,
        max_follow_up_tasks: 0,
        integration_validation_interval: 1,
        provider_routing: None,
    });
    policy_spec.verify().unwrap();
    fs::write(&spec, toml::to_string(&policy_spec).unwrap()).unwrap();
    assert_eq!(run_campaign(&fixture, &spec)["completed_units"], 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some)
    }));
    let selected = &harness.state().campaign().unwrap().spec;
    let status = harness.state().status().value.clone().flatten().unwrap();
    let report = synthetic_final_report(selected, &status);
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    inject_report(&mut harness, &binding, generation, &serde_json::Value::Null);
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    harness.get_by_label_contains("No final report is recorded yet");
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    inject_report(&mut harness, &binding, generation, &report);
    harness.run_steps(2);
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_some());

    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-report",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    harness.run_steps(2);
    harness.get_by_label("Final report unavailable");
    harness.get_by_label_contains("Accepted outcomes from the retained report are hidden");
    assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    harness
        .get_by_label("Show command: Refresh final report")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-report --config");
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Refresh final report")
        .click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.report().last_error.is_none() && app.report().value.is_some() && !app.report().loading
    }));
    if harness
        .state()
        .report()
        .value
        .as_ref()
        .is_some_and(Option::is_some)
    {
        assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_some());
    } else {
        harness.get_by_label_contains("No final report is recorded yet");
        assert!(harness.state().task_overlay()["0.1.1.1"].accepted.is_none());
    }
    assert!(harness.query_by_label("Final report unavailable").is_none());
}

#[test]
fn work_plan_long_title_preserves_head_outcome_at_minimum_window() {
    let fixture = Fixture::new("long-outcome", 1);
    let source = fs::read_to_string(fixture.target.join("TASKS.md")).unwrap();
    let long_title = "L".repeat(codingmage_ui::content::MAX_LIST_CHARS);
    let source = source.replace("Complete fixture operation number 1 safely.", &long_title);
    fs::write(fixture.target.join("TASKS.md"), source).unwrap();
    git(&fixture.target, &["add", "TASKS.md"]);
    git(&fixture.target, &["commit", "-q", "-m", "long title"]);
    let spec = write_campaign(&fixture, "long-outcome", 1);
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["completed_units"], 1);

    let binary = CoordinatorBinary::at(&coordinator_binary());
    let state_dir = fixture.root.join("ui-state");
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(1024.0, 640.0))
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_fonts(common::fonts());
            codingmage_ui::App::with_state_dir(&creation.egui_ctx, binary, Ok(state_dir))
        });
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some) && app.head_plan().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);

    let badge = harness.get_by_label("Outcome: completed");
    let item = harness.get_by_label_contains("[ ] Sub-task 0.1.1.1");
    assert!(badge.accesskit_node().has_bounds());
    assert!(item.accesskit_node().has_bounds());
    assert!(item.accesskit_node().label().unwrap().contains(&long_title));
    let image = harness.render().expect("software render");
    assert_eq!((image.width(), image.height()), (1024, 640));
    if let Some(directory) = std::env::var_os("CODINGMAGE_UI_SNAPSHOT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        image
            .save(directory.join("work-plan-long-completed-outcome.png"))
            .unwrap();
    }
    harness
        .get_by_label_contains("[ ] Sub-task 0.1.1.1")
        .focus();
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label("Coordinator outcome");
    harness.get_by_label("completed at campaign head (verified, not yet in active checkout)");
    harness.get_by_label("Source checkbox");
}

fn assert_task_detail_workflow(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
    head: &str,
) {
    harness
        .get_by_label_contains("[ ] Sub-task 0.1.1.1 Complete fixture operation number 1 safely.")
        .click();
    harness.run_steps(2);
    harness
        .get_by_label("Show command: Load source detail")
        .focus();
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label_contains("campaign-task-detail");
    harness.get_by_label("Load source detail").click();
    assert!(settle(harness, Duration::from_secs(30), |app| {
        app.task_detail().value.is_some()
    }));
    let first_detail = harness.state().task_detail().value.clone().unwrap();
    assert_eq!(first_detail.head, head);
    assert_eq!(first_detail.item_id, "0.1.1.1");
    assert!(settle(harness, Duration::from_secs(30), |app| {
        app.run_records().value.is_some()
    }));
    harness.get_by_label("Source excerpt").click();
    harness.run_steps(2);
    harness.get_by_label("Complete fixture operation number 1 safely.");
    harness.get_by_label("Task run evidence").focus();
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label("Show command: Inspect task runs");
    let first_run = harness
        .state()
        .run_records()
        .value
        .as_ref()
        .unwrap()
        .iter()
        .find(|record| record.bound_task_id == first_detail.item_id)
        .expect("bound task run");
    harness.get_by_label(&format!("Run {}", first_run.run_id));
    harness.get_by_label_contains("Candidate commit");
    let mut malformed = serde_json::to_value(&first_detail).unwrap();
    malformed["story_criteria"] = serde_json::json!([{
        "id":"\u{202e}0.1.AC1", "title":"spoofed", "title_truncated":false,
        "source_state":"open"
    }]);
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-task-detail",
        request_id: Some(first_detail.item_id.clone()),
        result: Ok(serde_json::to_vec(&malformed).unwrap()),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Source detail unavailable");
    assert!(harness.query_by_label("spoofed").is_none());
    harness
        .get_by_label_contains("[ ] Sub-task 0.1.1.2 Complete fixture operation number 2 safely.")
        .click();
    harness.run_steps(2);
    assert!(harness.state().task_detail().value.is_none());
    let binding = harness.state().binding();
    let generation = harness.state().generation();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-task-detail",
        request_id: Some("0.1.1.1".to_owned()),
        result: Ok(serde_json::to_vec(&first_detail).unwrap()),
    }));
    assert!(harness.state().task_detail().value.is_none());
}

#[test]
fn blocked_task_shows_its_closed_reason_and_independent_progress() {
    let fixture = Fixture::new("campaign-blocked", 2);
    let spec = write_campaign(&fixture, "blocked-campaign", 2);
    fs::write(fixture.root.join("block-first-task"), b"block\n").unwrap();
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["completed_units"], 1, "{outcome}");
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some) && app.head_plan().value.is_some()
    }));
    let status = harness.state().status().value.clone().flatten().unwrap();
    assert_eq!(status.outcomes.blocked, 1);
    assert_eq!(status.outcomes.completed, 1);
    assert_eq!(status.blockers[0].task_id, "0.1.1.1");
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Recorded outcomes: 1 completed, 1 blocked");
    harness.get_by_label_contains("Attention:");
    let overlay = harness.state().task_overlay();
    assert_eq!(
        overlay.get("0.1.1.1").unwrap().labels(true),
        vec!["open in source", "blocked: unavailable_external_dependency"]
    );
    assert_eq!(
        overlay.get("0.1.1.2").unwrap().labels(true),
        vec![
            "open in source",
            "completed at campaign head (verified, not yet in active checkout)"
        ]
    );
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label("0.1.1.1 - unavailable_external_dependency");
}

#[test]
fn cross_repository_campaign_snapshot_is_refused_before_status_request() {
    let first = Fixture::new("campaign-cross-a", 3);
    let second = Fixture::new("campaign-cross-b", 3);
    let foreign = write_campaign(&first, "foreign", 1);
    let mut harness = opened(&second);
    harness.state_mut().select_campaign(&foreign);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        matches!(
            app.campaign_error(),
            Some(SelectError::DifferentRepositoryPath { .. })
        )
    }));
    assert!(harness.state().campaign().is_none());
    assert!(matches!(
        harness.state().campaign_error(),
        Some(SelectError::DifferentRepositoryPath { .. })
    ));
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("Campaign specification refused");
    let foreign_projection = Command::new(coordinator_binary())
        .args([
            "campaign-head-plan",
            "--config",
            second.config.to_str().unwrap(),
            "--campaign",
            foreign.to_str().unwrap(),
            "--head",
            &"0".repeat(40),
        ])
        .output()
        .unwrap();
    assert!(!foreign_projection.status.success());
    assert!(foreign_projection.stdout.is_empty());
    let mut tampered = fs::read_to_string(&foreign).unwrap();
    tampered = tampered.replace(
        &format!("repository_path = \"{}\"", first.target.display()),
        &format!("repository_path = \"{}\"", second.target.display()),
    );
    let tampered_path = first.root.join("tampered.toml");
    fs::write(&tampered_path, tampered).unwrap();
    harness.state_mut().select_campaign(&tampered_path);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        matches!(
            app.campaign_error(),
            Some(SelectError::DifferentRepositoryId { .. })
        )
    }));
    assert!(matches!(
        harness.state().campaign_error(),
        Some(SelectError::DifferentRepositoryId { .. })
    ));
    assert!(harness.state().status().value.is_none());
}

#[test]
fn campaign_selection_is_remembered_per_configuration() {
    let fixture = Fixture::new("campaign-memory", 3);
    let spec = write_campaign(&fixture, "remembered", 1);
    let state_home = fixture.root.join("ui-state");
    let open = |fixture: &Fixture| {
        let binary = CoordinatorBinary::at(&coordinator_binary());
        let mut harness = harness_with_state(binary, [1100.0, 760.0], state_home.clone());
        let config = fixture.config.clone();
        harness.state_mut().open_project(&config);
        assert!(settle(&mut harness, Duration::from_secs(30), |app| {
            app.diagnosis().value.is_some()
        }));
        harness
    };
    let mut harness = open(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.campaign().is_some()
    }));
    assert!(harness.state().campaign().is_some());
    drop(harness);
    let mut reopened = open(&fixture);
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.campaign().is_some()
    }));
    assert_eq!(
        reopened
            .state()
            .campaign()
            .map(|campaign| campaign.spec_path.clone()),
        Some(spec.clone())
    );
    reopened.state_mut().clear_campaign();
    drop(reopened);
    let third = open(&fixture);
    assert!(third.state().campaign().is_none());
}
