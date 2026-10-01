//! Inspectable and exportable reports with privacy and overwrite safeguards.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{
    Screen,
    backend::{BackendError, CoordinatorBinary, Response},
};
use common::{
    Fixture, coordinator_binary, harness, run_campaign, settle, tree_digest, write_campaign,
};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

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
fn report_empty_states_route_to_setup_campaign_and_help() {
    let fixture = Fixture::new("report-empty", 1);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1100.0, 900.0],
    );
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness.get_by_label_contains("Open a repository in Setup");
    harness.get_by_label("Open Help").click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Help);
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness.get_by_label("Open Setup").click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);

    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness.get_by_label_contains("Select a campaign on Campaign");
    harness.get_by_label("Open Campaign").click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Campaign);
}

#[test]
fn report_sources_show_exact_refresh_commands_and_recover_from_malformed_reads() {
    let fixture = Fixture::new("report-source-recovery", 1);
    let spec = write_campaign(&fixture, "report-source-recovery-campaign", 1);
    let before = tree_digest(&fixture.target);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some() && app.explanation().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: Refresh report status")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-status --config");
    harness
        .get_by_label("Show command: Refresh report blockers")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-explain-blocker --config");

    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding: binding.clone(),
        label: "campaign-status",
        request_id: None,
        result: Ok(b"{".to_vec()),
    }));
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-explain-blocker",
        request_id: None,
        result: Ok(b"{".to_vec()),
    }));
    harness.run_steps(2);
    harness.get_by_label("Campaign status refresh failed");
    harness.get_by_label("Blocker explanation refresh failed");
    assert!(
        harness
            .get_all_by_label_contains("codingmage.ui.contract")
            .count()
            >= 2
    );
    harness.get_by_label_contains("retained campaign status may no longer match");
    harness.get_by_label_contains("retained blocker explanation may no longer match");

    harness.get_by_label("Refresh report status").click();
    harness.get_by_label("Refresh report blockers").click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some()
            && app.status().last_error.is_none()
            && app.explanation().value.is_some()
            && app.explanation().last_error.is_none()
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Campaign status observation: live");
    harness.get_by_label_contains("Blocker explanation observation: live");
    assert_eq!(tree_digest(&fixture.target), before);
}

#[test]
fn source_report_inspection_reads_bound_path_free_records_and_clears_on_reselection() {
    let fixture = Fixture::new("source-inspect", 1);
    let first = write_campaign(&fixture, "source-inspect-first", 1);
    let second = write_campaign(&fixture, "source-inspect-second", 1);
    let before = tree_digest(&fixture.target);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&first);
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: Inspect source report now")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-outcome-report --config");
    harness.get_by_label_contains("--include-paths false");
    harness.state_mut().inspect_source_report();
    assert!(harness.state().source_report_pending());
    harness.state_mut().inspect_source_report();
    assert!(harness.state().source_report_pending());
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.source_report_pending()
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Last successful source report snapshot");
    harness.get_by_label_contains("\"campaign_id\": \"source-inspect-first\"");
    assert_eq!(tree_digest(&fixture.target), before);

    harness.state_mut().select_campaign(&second);
    harness.run_steps(2);
    harness.get_by_label_contains("No source report has been requested");
    harness.state_mut().inspect_source_report();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.source_report_pending()
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("\"campaign_id\": \"source-inspect-second\"");
    assert_eq!(tree_digest(&fixture.target), before);
}

fn export_and_settle(harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>) {
    assert!(settle(
        harness,
        Duration::from_secs(30),
        codingmage_ui::App::report_ready
    ));
    harness.state_mut().export_report();
    assert!(harness.state().report_export_pending());
    assert!(settle(harness, Duration::from_secs(30), |app| {
        !app.report_export_pending()
    }));
}

fn reject_duplicate_and_foreign_export_completion(
    harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>,
) {
    assert!(settle(
        harness,
        Duration::from_secs(30),
        codingmage_ui::App::report_ready
    ));
    harness.state_mut().export_report();
    assert!(harness.state().report_export_pending());
    harness.state_mut().export_report();
    assert!(harness.state().report_export_pending());
    assert!(
        harness
            .state()
            .reports_state()
            .message
            .as_ref()
            .is_some_and(|message| message
                .as_ref()
                .is_err_and(|error| error.contains("already pending")))
    );
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(!harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "report-export",
        request_id: Some("other-export".to_owned()),
        result: Ok(Vec::new()),
    }));
    assert!(harness.state().report_export_pending());
    assert!(settle(harness, Duration::from_secs(30), |app| {
        !app.report_export_pending()
    }));
}

#[test]
fn outcome_report_restates_records_and_exports_with_privacy_and_overwrite_safeguards() {
    let fixture = Fixture::new("reports", 2);
    let spec = write_campaign(&fixture, "reports-campaign", 2);
    fs::write(fixture.root.join("block-first-task"), b"block\n").unwrap();
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["completed_units"], 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    let pending = harness.state().assemble_report(false).unwrap();
    assert!(!pending.change_coverage.observed);
    assert_eq!(pending.changed_file_count, None);
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.changes().value.is_some()
            && app.run_records().value.is_some()
            && app.status().value.as_ref().is_some_and(Option::is_some)
    }));
    let report = harness.state().assemble_report(false).unwrap();
    assert_eq!(report.disposition.completed_units, Some(1));
    assert_eq!(report.disposition.blocked, Some(1));
    assert_eq!(report.disposition.accepted_outcomes, Some(2));
    assert_eq!(report.runs.len(), 1);
    assert_eq!(report.runs[0].review_verdict.as_deref(), Some("pass"));
    assert!(report.disposition.delivery.starts_with("withheld"));
    assert_eq!(report.changed_file_count, Some(2));
    assert!(report.change_coverage.observed);
    assert!(report.changed_files.is_none());
    let text = String::from_utf8(report.to_bytes().unwrap()).unwrap();
    assert!(!text.contains("src/lib.rs"));
    assert!(!text.contains(fixture.target.to_str().unwrap()));
    assert!(!text.contains(fixture.config.to_str().unwrap()));
    assert!(!text.contains("fake-claude"));
    assert!(text.contains("unavailable_external_dependency"));
    harness.state_mut().select_screen(Screen::Reports);
    assert!(settle(
        &mut harness,
        Duration::from_secs(30),
        codingmage_ui::App::report_ready
    ));
    harness.run_steps(2);
    harness.get_by_label("1 / 0 / 0");
    harness.get_by_label("2 of 2");
    harness.get_by_label("blocked 0.1.1.1 - unavailable_external_dependency");
    harness.get_by_label_contains("never clears a blocker on its own");
    harness.state_mut().inspect_source_report();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.source_report_pending()
    }));
    harness.run_steps(2);
    harness.get_by_label_contains("Last successful source report snapshot");
    harness.get_by_label_contains("\"campaign_id\": \"reports-campaign\"");
    // Export inside the repository is refused.
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.export_path = fixture.target.join("report.json").display().to_string();
    }
    reject_duplicate_and_foreign_export_completion(&mut harness);
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage.cli.refused");
    assert!(!fixture.target.join("report.json").exists());
    let linked_parent = fixture.root.join("linked-target");
    std::os::unix::fs::symlink(&fixture.target, &linked_parent).unwrap();
    harness.state_mut().reports_state_mut().export_path =
        linked_parent.join("report.json").display().to_string();
    export_and_settle(&mut harness);
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage.cli.refused");
    assert!(!fixture.target.join("report.json").exists());
    // Export outside works once; the second export is refused without overwrite.
    let destination = fixture.root.join("exports/report.json");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.export_path = destination.display().to_string();
    }
    harness.run_steps(2);
    harness.get_by_label("Show command: Export report");
    export_and_settle(&mut harness);
    harness.run_steps(2);
    harness.get_by_label_contains("report exported to");
    let exported = fs::read_to_string(&destination).unwrap();
    let document: serde_json::Value = serde_json::from_str(&exported).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["campaign_id"], "reports-campaign");
    assert_eq!(document["repository_paths_included"], false);
    assert!(!exported.contains("src/lib.rs"));
    export_and_settle(&mut harness);
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage.cli.refused");
    // With paths and overwrite, the file is replaced and marked as containing paths.
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.include_paths = true;
        reports.overwrite = true;
    }
    export_and_settle(&mut harness);
    harness.run_steps(2);
    let exported = fs::read_to_string(&destination).unwrap();
    assert!(exported.contains("src/lib.rs"));
    assert!(exported.contains("\"repository_paths_included\": true"));
    // Viewing and exporting changed neither the target nor the campaign state.
    assert!(
        fs::read_to_string(fixture.target.join("TASKS.md"))
            .unwrap()
            .contains("- [ ] **Sub-task 0.1.1.2:**")
    );
}

#[test]
fn report_and_export_label_retained_status_after_failed_refresh() {
    let fixture = Fixture::new("report-stale", 2);
    let spec = write_campaign(&fixture, "report-stale-campaign", 2);
    let before = tree_digest(&fixture.target);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some()
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
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    assert!(
        harness
            .get_all_by_label_contains("campaign status stale")
            .count()
            >= 1
    );
    harness.get_by_label_contains("Some local observations are loading, missing, stale or failed");
    let report = harness.state().assemble_report(false).unwrap();
    assert!(
        report
            .limits
            .iter()
            .any(|line| line.contains("campaign status stale"))
    );
    let destination = fixture.root.join("stale-report.json");
    harness.state_mut().reports_state_mut().export_path = destination.display().to_string();
    export_and_settle(&mut harness);
    let exported = fs::read_to_string(&destination).unwrap();
    assert!(!exported.contains("campaign status stale"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&exported).unwrap()["schema_version"],
        1
    );
    assert_eq!(tree_digest(&fixture.target), before);
}

#[test]
fn report_assembly_rebinds_after_source_change_without_blocking_fresh_export() {
    let fixture = Fixture::new("report-assembly-rebind", 1);
    let spec = write_campaign(&fixture, "report-assembly-rebind-campaign", 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.status().value.is_some() && !app.status().loading
    }));
    harness.state_mut().select_screen(Screen::Reports);
    assert!(settle(
        &mut harness,
        Duration::from_secs(30),
        codingmage_ui::App::report_ready
    ));
    harness.state_mut().reports_state_mut().show_json = true;
    harness.run_steps(2);

    let destination = fixture.root.join("stale-assembly.json");
    harness.state_mut().reports_state_mut().export_path = destination.display().to_string();
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "campaign-status",
        request_id: None,
        result: Err(BackendError::Timeout),
    }));
    assert!(!harness.state().report_ready());
    harness.state_mut().export_report();
    assert!(harness.state().report_export_pending());
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.report_export_pending()
    }));
    assert!(destination.exists());
    assert!(settle(
        &mut harness,
        Duration::from_secs(30),
        codingmage_ui::App::report_ready
    ));
    let report = harness.state().assemble_report(false).unwrap();
    assert!(
        report
            .limits
            .iter()
            .any(|limit| limit.contains("campaign status stale"))
    );
    let exported = fs::read_to_string(destination).unwrap();
    assert!(!exported.contains("campaign status stale"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&exported).unwrap()["schema_version"],
        1
    );
}

#[test]
fn clicking_a_source_checkbox_changes_nothing() {
    let fixture = Fixture::new("checkbox", 2);
    let before = tree_digest(&fixture.target);
    let mut harness = opened(&fixture);
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    let checkboxes = harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|node| node.accesskit_node().is_disabled())
        .count();
    assert!(checkboxes >= 3, "{checkboxes} disabled source checkboxes");
    for node in harness
        .get_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|node| node.accesskit_node().is_disabled())
    {
        node.click();
    }
    harness.run_steps(2);
    assert_eq!(tree_digest(&fixture.target), before);
    let index = harness.state().plan_index().unwrap();
    assert_eq!(index.counts().checked_subtasks, 0);
}
