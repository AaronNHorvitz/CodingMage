//! Exact changes, review and test records, and bounded activity from real durable records.

mod common;

use std::{fs, process::Command, time::Duration};

use codingmage_ui::{
    Screen,
    backend::{CoordinatorBinary, Response},
};
use common::{Fixture, coordinator_binary, harness, run_campaign, settle, write_campaign};
use egui_kittest::kittest::Queryable as _;

fn opened(fixture: &Fixture) -> egui_kittest::Harness<'static, codingmage_ui::App> {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 900.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness
}

fn change_command(fixture: &Fixture, spec: &std::path::Path, head: &str) -> std::process::Output {
    Command::new(coordinator_binary())
        .args([
            "campaign-changes",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
            "--head",
            head,
        ])
        .output()
        .unwrap()
}

#[test]
fn accepted_units_show_exact_commits_files_verdicts_and_gate_evidence() {
    let fixture = Fixture::new("changes", 2);
    let spec = write_campaign(&fixture, "changes-campaign", 2);
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["completed_units"], 2);
    let initial = fixture.head();
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.changes().value.is_some() && app.run_records().value.is_some()
    }));
    let changes = harness.state().changes().value.clone().unwrap();
    assert_eq!(changes.base, initial);
    assert_eq!(
        changes.commits.len(),
        4,
        "candidate and completion commit per unit"
    );
    assert!(
        changes
            .commits
            .iter()
            .all(|commit| commit.subject.starts_with("codingmage: complete 0.1.1."))
    );
    let paths = changes
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    assert!(
        paths.contains(&"src/lib.rs") && paths.contains(&"TASKS.md"),
        "{paths:?}"
    );
    let records = harness.state().run_records().value.clone().unwrap();
    assert_eq!(records.len(), 2);
    for record in &records {
        let checkpoint = record.checkpoint.as_ref().expect("checkpoint");
        assert_eq!(checkpoint.review_verdict.as_deref(), Some("pass"));
        // Two configured gates plus the baseline comparison receipt bound to the candidate.
        assert_eq!(checkpoint.gate_evidence.len(), 3);
        assert!(
            record
                .phases
                .iter()
                .any(|phase| phase.phase == "review" && phase.outcome == "succeeded")
        );
        assert_eq!(record.malformed_journal_lines, 0);
    }
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("engineering completion below is not delivery");
    harness.get_by_label_contains("2 changed file(s)");
    harness.get_by_label("src/lib.rs");
    assert_eq!(
        harness
            .get_all_by_label_contains("review verdict: pass")
            .count(),
        2
    );
    assert_eq!(
        harness
            .get_all_by_label_contains("3 gate evidence record(s)")
            .count(),
        2
    );
    assert_eq!(
        harness
            .get_all_by_label_contains("Journaled phases: claim (succeeded)")
            .count(),
        2
    );
    harness.get_by_label_contains("Reviewer finding text is not retained by the backend");
    // Source in the active checkout is still untouched: the changes live on the campaign branch.
    assert_eq!(fixture.head(), initial);
}

#[test]
fn coordinator_change_projection_binds_head_and_withholds_stale_or_cross_project_reads() {
    let fixture = Fixture::new("changes-bound", 2);
    let spec = write_campaign(&fixture, "changes-bound-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some()
    }));
    let status = harness
        .state()
        .status()
        .value
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    let head = status.head.clone();
    let output = change_command(&fixture, &spec, &head);
    assert!(output.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(projection["schema_version"], 1);
    assert_eq!(projection["head"], head);
    assert_eq!(projection["base"], fixture.head());
    assert_eq!(projection["campaign_id"], "changes-bound-campaign");
    assert_eq!(projection["commits"][0]["id"], head);
    assert_eq!(projection["commits_truncated"], false);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Complete fixture operation"));

    let stale = change_command(&fixture, &spec, &"0".repeat(40));
    assert!(!stale.status.success());
    assert!(stale.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&stale.stderr).trim(),
        "codingmage.cli.stale_observation"
    );

    let other = Fixture::new("changes-other", 1);
    let foreign = write_campaign(&other, "foreign-campaign", 1);
    let wrong_repository = change_command(&fixture, &foreign, &head);
    assert!(!wrong_repository.status.success());
    assert!(wrong_repository.stdout.is_empty());
}

#[test]
fn forged_change_projection_is_not_rendered_and_refresh_recovers() {
    let fixture = Fixture::new("changes-forged", 2);
    let spec = write_campaign(&fixture, "changes-forged-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some()
    }));
    let head = harness
        .state()
        .changes()
        .value
        .as_ref()
        .unwrap()
        .head
        .clone();
    let output = change_command(&fixture, &spec, &head);
    let projection: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for mutation in ["repository_id", "head", "schema_version", "unexpected"] {
        let mut forged = projection.clone();
        forged[mutation] = match mutation {
            "repository_id" => serde_json::json!("other-repository"),
            "head" => serde_json::json!("0".repeat(40)),
            "schema_version" => serde_json::json!(2),
            _ => serde_json::json!(true),
        };
        let app = harness.state_mut();
        let accepted = app.handle_response(Response {
            generation: app.generation(),
            binding: app.binding(),
            label: "campaign-changes",
            request_id: None,
            result: Ok(serde_json::to_vec(&forged).unwrap()),
        });
        assert!(accepted);
        assert!(app.changes().value.is_none());
    }
    harness.state_mut().refresh_campaign();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes()
            .value
            .as_ref()
            .is_some_and(|changes| changes.head == head)
    }));
}

#[test]
fn missing_and_malformed_records_are_reported_not_passed() {
    let fixture = Fixture::new("changes-malformed", 2);
    let spec = write_campaign(&fixture, "malformed-campaign", 1);
    run_campaign(&fixture, &spec);
    let runs = fixture
        .state
        .join("campaigns/malformed-campaign/state/runs");
    let run_dir = fs::read_dir(&runs).unwrap().next().unwrap().unwrap().path();
    let mut events = fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    events.push_str("this line is not a journal record\n");
    fs::write(run_dir.join("events.jsonl"), events).unwrap();
    fs::remove_file(run_dir.join("checkpoint.json")).unwrap();
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.run_records().value.is_some()
    }));
    let records = harness.state().run_records().value.clone().unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].checkpoint.is_none());
    assert_eq!(records[0].malformed_journal_lines, 1);
    assert_eq!(records[0].task_id().as_deref(), Some("0.1.1.1"));
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("checkpoint.json is absent");
    harness.get_by_label_contains("no checkpoint: review outcome not recorded");
    harness.get_by_label_contains("1 malformed journal line(s) ignored");
}

#[test]
fn never_started_campaign_has_no_changes_or_records() {
    let fixture = Fixture::new("changes-empty", 2);
    let spec = write_campaign(&fixture, "empty-campaign", 1);
    let absent = change_command(&fixture, &spec, &"0".repeat(40));
    assert!(!absent.status.success());
    assert!(absent.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&absent.stderr).trim(),
        "codingmage.cli.refused"
    );
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.status().value.is_some() && app.run_records().value.is_some()
    }));
    assert!(
        harness
            .state()
            .run_records()
            .value
            .as_ref()
            .unwrap()
            .is_empty()
    );
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness
        .get_by_label_contains("The campaign has never started; there are no candidate changes.");
    harness.get_by_label_contains("No run records exist for this campaign.");
    harness.get_by_label_contains("activity lines are not available");
}
