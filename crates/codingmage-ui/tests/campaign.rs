//! Campaign and team state over real durable coordinator records.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{Screen, backend::CoordinatorBinary, campaign::SelectError};
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

#[test]
fn never_started_campaign_is_an_explicit_empty_state() {
    let fixture = Fixture::new("campaign-empty", 3);
    let spec = write_campaign(&fixture, "empty-campaign", 2);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some()
    }));
    assert_eq!(harness.state().status().value, Some(None));
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("No durable campaign state exists");
    harness.get_by_label_contains("Hands-off: unavailable");
    harness
        .get_by_label_contains("Active checkout matches the campaign's bound repository identity");
    assert!(!fixture.state.join("campaigns").exists());
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
