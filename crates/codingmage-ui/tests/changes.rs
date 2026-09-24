//! Exact changes, review and test records, and bounded activity from real durable records.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{Screen, backend::CoordinatorBinary};
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
