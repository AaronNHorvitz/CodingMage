//! Inspectable and exportable reports with privacy and overwrite safeguards.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{Screen, backend::CoordinatorBinary};
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
fn outcome_report_restates_records_and_exports_with_privacy_and_overwrite_safeguards() {
    let fixture = Fixture::new("reports", 2);
    let spec = write_campaign(&fixture, "reports-campaign", 2);
    fs::write(fixture.root.join("block-first-task"), b"block\n").unwrap();
    let outcome = run_campaign(&fixture, &spec);
    assert_eq!(outcome["completed_units"], 1);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
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
    assert_eq!(report.changed_file_count, 2);
    assert!(report.changed_files.is_none());
    let text = String::from_utf8(report.to_bytes().unwrap()).unwrap();
    assert!(!text.contains("src/lib.rs"));
    assert!(!text.contains(fixture.target.to_str().unwrap()));
    assert!(!text.contains(fixture.config.to_str().unwrap()));
    assert!(!text.contains("fake-claude"));
    assert!(text.contains("unavailable_external_dependency"));
    harness.state_mut().select_screen(Screen::Reports);
    harness.run_steps(2);
    harness.get_by_label("1 / 0 / 0");
    harness.get_by_label("2 of 2");
    harness.get_by_label("blocked 0.1.1.1 - unavailable_external_dependency");
    harness.get_by_label_contains("never clears a blocker on its own");
    // Export inside the repository is refused.
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.export_path = fixture.target.join("report.json").display().to_string();
    }
    harness.state_mut().export_report();
    harness.run_steps(2);
    harness.get_by_label_contains("is inside the target repository");
    assert!(!fixture.target.join("report.json").exists());
    // Export outside works once; the second export is refused without overwrite.
    let destination = fixture.root.join("exports/report.json");
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.export_path = destination.display().to_string();
    }
    harness.state_mut().export_report();
    harness.run_steps(2);
    harness.get_by_label_contains("report exported to");
    let exported = fs::read_to_string(&destination).unwrap();
    assert!(!exported.contains("src/lib.rs"));
    harness.state_mut().export_report();
    harness.run_steps(2);
    harness.get_by_label_contains("already exists");
    // With paths and overwrite, the file is replaced and marked as containing paths.
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.include_paths = true;
        reports.overwrite = true;
    }
    harness.state_mut().export_report();
    harness.run_steps(2);
    let exported = fs::read_to_string(&destination).unwrap();
    assert!(exported.contains("src/lib.rs"));
    assert!(exported.contains("\"contains_repository_paths\": true"));
    // Viewing and exporting changed neither the target nor the campaign state.
    assert!(
        fs::read_to_string(fixture.target.join("TASKS.md"))
            .unwrap()
            .contains("- [ ] **Sub-task 0.1.1.2:**")
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
