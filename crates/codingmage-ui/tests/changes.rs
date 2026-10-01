//! Exact changes, review and test records, and bounded activity from real durable records.

mod common;

use std::{fs, process::Command, time::Duration};

use codingmage_contracts::RepositoryId;
use codingmage_state::Journal;
use codingmage_ui::{
    Screen,
    backend::{BackendError, CoordinatorBinary, Response},
    observed::Freshness,
    records::parse_run_records,
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
fn evidence_empty_state_routes_to_setup_then_campaign() {
    let fixture = Fixture::new("evidence-empty", 1);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1100.0, 900.0],
    );
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("Open a repository in Setup");
    harness.get_by_label("Open Setup").click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);

    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("Select a campaign on Campaign");
    harness.get_by_label("Open Campaign").click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Campaign);
}

#[test]
fn evidence_refresh_shows_exact_commands_and_recovers_from_failed_reads() {
    let fixture = Fixture::new("evidence-refresh", 1);
    let spec = write_campaign(&fixture, "evidence-refresh-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some() && app.run_records().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("Publication policy: Local campaign branch only.");
    harness
        .get_by_label("Show command: Refresh changes")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("campaign-changes --config");
    harness
        .get_by_label("Show command: Refresh run evidence")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("campaign-run-records --config");

    harness.get_by_label("Refresh changes").click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some() && !app.changes().loading
    }));
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    harness.state_mut().refresh_change_evidence();
    let request_id = harness
        .state()
        .pending_changes_request_id()
        .unwrap()
        .to_owned();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-changes",
        request_id: Some(request_id),
        result: Ok(b"{".to_vec()),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("No review, completion or delivery is inferred");
    harness.get_by_label("Refresh changes").click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some() && app.changes().last_error.is_none()
    }));

    let generation = harness.state().generation();
    let binding = harness.state().binding();
    harness.state_mut().refresh_run_evidence();
    let request_id = harness
        .state()
        .pending_run_records_request_id()
        .unwrap()
        .to_owned();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-run-records",
        request_id: Some(request_id),
        result: Ok(b"{".to_vec()),
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Malformed backend output");
    harness.get_by_label_contains("No pass or accepted outcome is inferred");
    harness.get_by_label("Refresh run evidence").click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.run_records().value.is_some() && app.run_records().last_error.is_none()
    }));

    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    assert_eq!(
        harness
            .state()
            .status()
            .freshness(std::time::Instant::now()),
        Freshness::Stale
    );
    harness.state_mut().refresh_change_evidence();
    harness.state_mut().refresh_run_evidence();
    assert!(!harness.state().changes().loading);
    assert!(!harness.state().run_records().loading);
    harness.run_steps(2);
    harness.get_by_label_contains("A live campaign status is required");
}

fn records_command(fixture: &Fixture, spec: &std::path::Path) -> std::process::Output {
    Command::new(coordinator_binary())
        .args([
            "campaign-run-records",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn late_valid_evidence_cannot_revive_failed_status() {
    let fixture = Fixture::new("evidence-late-timeout", 1);
    let spec = write_campaign(&fixture, "evidence-late-timeout-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.changes().value.is_some() && app.run_records().value.is_some()
    }));
    let head = harness
        .state()
        .status()
        .value
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .head
        .clone();
    let changes = change_command(&fixture, &spec, &head).stdout;
    let records = records_command(&fixture, &spec).stdout;
    let app = harness.state_mut();
    app.refresh_change_evidence();
    app.refresh_run_evidence();
    let change_id = app.pending_changes_request_id().unwrap().to_owned();
    let record_id = app.pending_run_records_request_id().unwrap().to_owned();
    let generation = app.generation();
    let binding = app.binding();
    assert!(app.handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    for (label, request_id, result) in [
        ("campaign-changes", change_id, changes),
        ("campaign-run-records", record_id, records),
    ] {
        assert!(!app.handle_response(Response {
            generation,
            binding: binding.clone(),
            label,
            request_id: Some(request_id),
            result: Ok(result),
        }));
    }
    assert_eq!(
        app.status().freshness(std::time::Instant::now()),
        Freshness::Stale
    );
    assert!(app.changes().value.is_none());
    assert!(app.run_records().value.is_none());
    assert!(app.pending_changes_request_id().is_none());
    assert!(app.pending_run_records_request_id().is_none());
    assert_ne!(
        app.changes().freshness(std::time::Instant::now()),
        Freshness::Live
    );
    assert_ne!(
        app.run_records().freshness(std::time::Instant::now()),
        Freshness::Live
    );
}

#[test]
fn late_valid_evidence_cannot_cross_head_or_checkpoint_observation() {
    let fixture = Fixture::new("evidence-late-status", 1);
    let spec = write_campaign(&fixture, "evidence-late-status-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    for changed_field in ["head", "updated_at_ms"] {
        harness.state_mut().refresh_campaign();
        assert!(settle(&mut harness, Duration::from_secs(30), |app| {
            app.status().freshness(std::time::Instant::now()) == Freshness::Live
                && app.changes().value.is_some()
                && app.run_records().value.is_some()
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
        let mut changed_status = serde_json::to_value(status).unwrap();
        changed_status[changed_field] = if changed_field == "head" {
            serde_json::json!(fixture.head())
        } else {
            serde_json::json!(status.updated_at_ms + 1)
        };
        let changes = change_command(&fixture, &spec, &head).stdout;
        let records = records_command(&fixture, &spec).stdout;
        let app = harness.state_mut();
        app.refresh_change_evidence();
        app.refresh_run_evidence();
        let change_id = app.pending_changes_request_id().unwrap().to_owned();
        let record_id = app.pending_run_records_request_id().unwrap().to_owned();
        let generation = app.generation();
        let binding = app.binding();
        assert!(app.handle_response(Response {
            generation,
            binding: binding.clone(),
            label: "campaign-status",
            request_id: None,
            result: Ok(serde_json::to_vec(&changed_status).unwrap()),
        }));
        let newer_change_id = app.pending_changes_request_id().unwrap().to_owned();
        let newer_record_id = app.pending_run_records_request_id().unwrap().to_owned();
        assert_ne!(newer_change_id, change_id);
        assert_ne!(newer_record_id, record_id);
        for (label, request_id, result) in [
            ("campaign-changes", change_id, changes),
            ("campaign-run-records", record_id, records),
        ] {
            assert!(!app.handle_response(Response {
                generation,
                binding: binding.clone(),
                label,
                request_id: Some(request_id),
                result: Ok(result),
            }));
        }
        assert!(app.changes().value.is_none());
        assert!(app.run_records().value.is_none());
        assert_eq!(
            app.pending_changes_request_id(),
            Some(newer_change_id.as_str())
        );
        assert_eq!(
            app.pending_run_records_request_id(),
            Some(newer_record_id.as_str())
        );
        assert!(app.changes().loading);
        assert!(app.run_records().loading);
        assert_ne!(
            app.changes().freshness(std::time::Instant::now()),
            Freshness::Live
        );
        assert_ne!(
            app.run_records().freshness(std::time::Instant::now()),
            Freshness::Live
        );
    }
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
                .any(|phase| phase.phase == "review" && phase.outcome == "succeeded"),
            "run evidence: {record:?}"
        );
        assert!(record.journal_problem.is_none());
    }
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("This screen has no verified delivery receipt");
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
        app.refresh_change_evidence();
        let request_id = app.pending_changes_request_id().unwrap().to_owned();
        let accepted = app.handle_response(Response {
            generation: app.generation(),
            binding: app.binding(),
            label: "campaign-changes",
            request_id: Some(request_id),
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
    assert_eq!(records[0].phases.len(), 0);
    assert_eq!(
        records[0].journal_problem.as_deref(),
        Some("events.jsonl is malformed or failed integrity checks")
    );
    assert_eq!(records[0].task_id().as_deref(), Some("0.1.1.1"));
    harness.state_mut().select_screen(Screen::Changes);
    harness.run_steps(2);
    harness.get_by_label_contains("checkpoint.json is absent");
    harness.get_by_label_contains("no checkpoint: review outcome not recorded");
    harness.get_by_label_contains("events.jsonl is malformed or failed integrity checks");
    fs::write(run_dir.join("events.jsonl"), b"").unwrap();
    let empty = records_command(&fixture, &spec);
    assert!(empty.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&empty.stdout).unwrap();
    assert_eq!(
        projection["records"][0]["journal_problem"],
        "events.jsonl is empty"
    );
}

#[test]
fn run_record_projection_uses_bound_runs_and_rejects_foreign_authority() {
    let fixture = Fixture::new("records-bound", 2);
    let spec = write_campaign(&fixture, "records-bound-campaign", 1);
    run_campaign(&fixture, &spec);
    let original = records_command(&fixture, &spec);
    assert!(original.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&original.stdout).unwrap();
    assert_eq!(projection["schema_version"], 1);
    assert_eq!(projection["campaign_id"], "records-bound-campaign");
    assert_eq!(projection["records"].as_array().unwrap().len(), 1);
    assert_eq!(projection["records"][0]["bound_task_id"], "0.1.1.1");
    assert!(!String::from_utf8_lossy(&original.stdout).contains(fixture.state.to_str().unwrap()));

    let unbound = fixture
        .state
        .join("campaigns/records-bound-campaign/state/runs/run-unbound");
    fs::create_dir_all(&unbound).unwrap();
    fs::write(unbound.join("checkpoint.json"), b"malformed").unwrap();
    let again = records_command(&fixture, &spec);
    assert!(again.status.success());
    let again: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(again["records"].as_array().unwrap().len(), 1);

    let run_dir = fixture
        .state
        .join("campaigns/records-bound-campaign/state/runs")
        .join(projection["records"][0]["run_id"].as_str().unwrap());
    let mut journal = Journal::open(&run_dir, "identity-test").unwrap();
    let mut foreign_event = journal.records().last().unwrap().event.clone();
    foreign_event.repository_id = RepositoryId::new("repo-foreign").unwrap();
    journal.append(foreign_event).unwrap();
    drop(journal);
    let mixed = records_command(&fixture, &spec);
    assert!(mixed.status.success());
    let mixed: serde_json::Value = serde_json::from_slice(&mixed.stdout).unwrap();
    assert_eq!(
        mixed["records"][0]["journal_problem"],
        "events.jsonl has invalid identity"
    );
    assert!(mixed["records"][0]["phases"].as_array().unwrap().is_empty());
    let parsed = parse_run_records(&serde_json::to_vec(&mixed).unwrap()).unwrap();
    assert_eq!(
        parsed.records[0].review_label(),
        "journal unavailable: review outcome not corroborated"
    );
    assert_eq!(
        parsed.records[0].gate_label(),
        "journal unavailable: gate evidence not corroborated"
    );

    let other = Fixture::new("records-other", 1);
    let foreign = write_campaign(&other, "records-foreign-campaign", 1);
    let denied = records_command(&fixture, &foreign);
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
}

#[test]
fn linked_or_oversized_run_checkpoint_is_unknown_and_refresh_recovers() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("records-linked", 2);
    let spec = write_campaign(&fixture, "records-linked-campaign", 1);
    run_campaign(&fixture, &spec);
    let runs = fixture
        .state
        .join("campaigns/records-linked-campaign/state/runs");
    let run = fs::read_dir(&runs).unwrap().next().unwrap().unwrap().path();
    let checkpoint = run.join("checkpoint.json");
    let saved = fs::read(&checkpoint).unwrap();
    fs::remove_file(&checkpoint).unwrap();
    let outside = fixture.root.join("outside-checkpoint.json");
    fs::write(&outside, &saved).unwrap();
    symlink(&outside, &checkpoint).unwrap();
    let linked = records_command(&fixture, &spec);
    assert!(linked.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&linked.stdout).unwrap();
    assert!(projection["records"][0]["checkpoint"].is_null());
    assert!(
        projection["records"][0]["checkpoint_problem"]
            .as_str()
            .unwrap()
            .contains("linked")
    );
    fs::remove_file(&checkpoint).unwrap();
    fs::write(&checkpoint, vec![b'x'; 65 * 1024]).unwrap();
    let oversized = records_command(&fixture, &spec);
    assert!(oversized.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&oversized.stdout).unwrap();
    assert!(projection["records"][0]["checkpoint"].is_null());
    assert!(
        projection["records"][0]["checkpoint_problem"]
            .as_str()
            .unwrap()
            .contains("oversized")
    );
    fs::write(&checkpoint, saved).unwrap();
    let recovered = records_command(&fixture, &spec);
    assert!(recovered.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(
        projection["records"][0]["checkpoint"]["review_verdict"],
        "pass"
    );
    let relocated = fixture.root.join("relocated-run");
    fs::rename(&run, &relocated).unwrap();
    symlink(&relocated, &run).unwrap();
    let linked_run = records_command(&fixture, &spec);
    assert!(linked_run.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&linked_run.stdout).unwrap();
    assert!(projection["records"][0]["checkpoint"].is_null());
    assert_eq!(
        projection["records"][0]["checkpoint_problem"],
        "run directory is linked, absent or invalid"
    );
    fs::remove_file(&run).unwrap();
    fs::rename(&relocated, &run).unwrap();
    let journal = run.join("events.jsonl");
    let journal_bytes = fs::read(&journal).unwrap();
    fs::write(&journal, vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    let oversized_journal = records_command(&fixture, &spec);
    assert!(oversized_journal.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&oversized_journal.stdout).unwrap();
    assert_eq!(
        projection["records"][0]["journal_problem"],
        "events.jsonl is oversized"
    );
    assert!(
        projection["records"][0]["phases"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::write(&journal, journal_bytes).unwrap();
}

#[test]
fn forged_run_record_response_clears_previous_evidence_then_recovers() {
    let fixture = Fixture::new("records-forged", 2);
    let spec = write_campaign(&fixture, "records-forged-campaign", 1);
    run_campaign(&fixture, &spec);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.run_records().value.is_some()
    }));
    let result = records_command(&fixture, &spec);
    let projection: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for key in [
        "campaign_id",
        "repository_id",
        "head",
        "updated_at_ms",
        "schema_version",
        "future",
    ] {
        let mut forged = projection.clone();
        forged[key] = match key {
            "updated_at_ms" => serde_json::json!(0),
            "schema_version" => serde_json::json!(2),
            "future" => serde_json::json!(true),
            _ => serde_json::json!("forged"),
        };
        let app = harness.state_mut();
        app.refresh_run_evidence();
        let request_id = app.pending_run_records_request_id().unwrap().to_owned();
        assert!(app.handle_response(Response {
            generation: app.generation(),
            binding: app.binding(),
            label: "campaign-run-records",
            request_id: Some(request_id),
            result: Ok(serde_json::to_vec(&forged).unwrap()),
        }));
        assert!(app.run_records().value.is_none());
    }
    harness.state_mut().refresh_campaign();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.run_records()
            .value
            .as_ref()
            .is_some_and(|records| records.len() == 1)
    }));
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
    let empty_records = records_command(&fixture, &spec);
    assert!(empty_records.status.success());
    let projection: serde_json::Value = serde_json::from_slice(&empty_records.stdout).unwrap();
    assert!(projection["head"].is_null());
    assert_eq!(projection["records"].as_array().unwrap().len(), 0);
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
    harness.get_by_label_contains("No run records are available for this campaign.");
    harness.get_by_label_contains("cannot infer implementation, review or test completion");
    harness.get_by_label_contains("activity lines are not available");
}
