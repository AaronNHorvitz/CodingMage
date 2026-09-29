//! Campaign and team state over real durable coordinator records.

mod common;

use std::{fs, path::Path, process::Command, time::Duration};

use codingmage_ui::{
    Screen,
    backend::{
        BackendError, CoordinatorBinary, Response,
        models::{ActiveTask, MissionStatus},
    },
    campaign::SelectError,
};
use common::{
    Fixture, coordinator_binary, harness, harness_with_state, run_campaign, settle, write_campaign,
};
use egui_kittest::kittest::Queryable as _;

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
        app.status().value.is_some() && app.mission().value.is_some()
    }));
    assert_eq!(harness.state().status().value, Some(None));
    assert_eq!(
        harness.state().mission().value,
        Some(None),
        "no charter admitted is an explicit absent state, not a failure"
    );
    harness.state_mut().select_screen(Screen::Overview);
    harness.run_steps(2);
    harness.get_by_label_contains("Selected campaign: empty-campaign");
    harness.get_by_label_contains("No durable campaign state was observed");
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
    harness.get_by_label_contains("[ ] Sub-task 0.1.1.1 Complete fixture operation number 1 safely. - dependency-ready [completed at campaign head (verified, not yet in active checkout)]");
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
        .click();
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
fn cross_repository_campaign_is_refused_before_any_backend_request() {
    let first = Fixture::new("campaign-cross-a", 3);
    let second = Fixture::new("campaign-cross-b", 3);
    let foreign = write_campaign(&first, "foreign", 1);
    let mut harness = opened(&second);
    harness.state_mut().select_campaign(&foreign);
    harness.run_steps(2);
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
    harness.run_steps(2);
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
    harness.run_steps(2);
    assert!(harness.state().campaign().is_some());
    drop(harness);
    let mut reopened = open(&fixture);
    reopened.run_steps(2);
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
