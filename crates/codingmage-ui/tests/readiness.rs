//! Readiness: local checks that explain preflight's stable codes, and real preflight runs.

mod common;

use std::time::Duration;

use codingmage_ui::{
    Screen,
    backend::{BackendError, CoordinatorBinary},
    readiness::CheckStatus,
};
use common::{
    Fixture, coordinator_binary, git, harness, settle, write_campaign, write_controlled_campaign,
};
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

fn status_of<'a>(checks: &'a [codingmage_ui::readiness::Check], name: &str) -> &'a CheckStatus {
    &checks
        .iter()
        .find(|check| check.name == name)
        .unwrap_or_else(|| panic!("check {name} missing"))
        .status
}

#[test]
fn local_checks_explain_the_preflight_failure_before_it_runs() {
    let fixture = Fixture::new("readiness-fail", 3);
    let spec = write_campaign(&fixture, "not-ready", 2);
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    harness.run_steps(2);
    let checks = harness.state().readiness_checks();
    assert_eq!(
        *status_of(&checks, "repository identity"),
        CheckStatus::Pass
    );
    assert_eq!(*status_of(&checks, "initial commit"), CheckStatus::Pass);
    assert_eq!(*status_of(&checks, "dedicated branch"), CheckStatus::Fail);
    assert_eq!(*status_of(&checks, "open sub-tasks"), CheckStatus::Fail);
    assert_eq!(
        *status_of(&checks, "controlled-target authority"),
        CheckStatus::Fail
    );
    assert_eq!(
        *status_of(&checks, "authorization record"),
        CheckStatus::Unknown
    );
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("dedicated branch: fail");
    harness.get_by_label_contains("3 open sub-tasks (preflight requires at least 10)");
    harness.get_by_label_contains("Preflight admits one pod, exactly ten accepted outcomes");
    // A record that does not match the bound digest is a distinct failure.
    let record = fixture.root.join("wrong-record.txt");
    std::fs::write(&record, b"different bytes\n").unwrap();
    harness.state_mut().set_authorization_record(&record);
    let checks = harness.state().readiness_checks();
    assert_eq!(
        *status_of(&checks, "authorization record"),
        CheckStatus::Fail
    );
    // Running preflight anyway reports the coordinator's stable code with an explanation.
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(2), |app| {
        app.preflight().last_error.is_some() || app.preflight().value.is_some()
    }));
    assert!(matches!(
        harness.state().preflight().last_error,
        Some((_, BackendError::Command { .. }))
    ));
    harness.run_steps(2);
    assert!(
        harness
            .get_all_by_label_contains("codingmage.runtime.authority")
            .count()
            >= 1
    );
    harness.get_by_label_contains("Repository authorization was refused.");
    assert!(!fixture.state.join("campaigns").exists());
}

#[test]
fn real_preflight_passes_on_a_ready_fixture_and_binds_the_report_digest() {
    let fixture = Fixture::new("readiness-pass", 10);
    git(
        &fixture.target,
        &["switch", "-q", "-c", "controlled-target"],
    );
    let (spec, record) = write_controlled_campaign(&fixture, "ready-campaign");
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    harness.state_mut().set_authorization_record(&record);
    let checks = harness.state().readiness_checks();
    assert!(
        checks.iter().all(|check| check.status == CheckStatus::Pass),
        "{checks:?}"
    );
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        app.preflight().last_error.is_some() || app.preflight().value.is_some()
    }));
    let observation = harness
        .state()
        .preflight()
        .value
        .clone()
        .unwrap_or_else(|| {
            panic!(
                "preflight failed: {:?}",
                harness.state().preflight().last_error
            )
        });
    assert_eq!(observation.report.state, "ready");
    assert!(observation.report.source_free);
    assert!(observation.report.repository.dedicated_branch);
    assert_eq!(observation.report.repository.open_subtask_count, 10);
    assert!(
        observation
            .report
            .providers
            .iter()
            .all(|provider| provider.capability_verified)
    );
    assert_eq!(observation.report_sha256.len(), 64);
    let rendered = String::from_utf8(observation.bytes.clone()).unwrap();
    assert!(!rendered.contains(fixture.target.to_str().unwrap()));
    assert!(!rendered.contains("fixture-implementer"));
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains(&format!(
        "Preflight state ready - report sha256 {}",
        observation.report_sha256
    ));
    harness.get_by_label_contains("Local checks: ");
    assert!(!fixture.state.join("campaigns").exists());
}

#[test]
fn provider_probe_failure_is_actionable_and_names_no_substitute() {
    let fixture = Fixture::new("readiness-provider", 10);
    git(
        &fixture.target,
        &["switch", "-q", "-c", "controlled-target"],
    );
    let (spec, record) = write_controlled_campaign(&fixture, "broken-provider");
    // Replace the implementer with an executable that reports no capabilities.
    fixture.executable("fake-claude", "#!/bin/sh\necho 'nothing'\nexit 0\n");
    let mut harness = opened(&fixture);
    harness.state_mut().select_campaign(&spec);
    harness.state_mut().set_authorization_record(&record);
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        app.preflight().last_error.is_some() || app.preflight().value.is_some()
    }));
    let error = harness
        .state()
        .preflight()
        .last_error
        .clone()
        .map(|(_, error)| error)
        .expect("preflight must fail");
    assert!(
        error.code().starts_with("codingmage.provider.claude."),
        "{error}"
    );
    harness.state_mut().select_screen(Screen::Campaign);
    harness.run_steps(2);
    harness.get_by_label_contains("never substitutes another");
    assert!(
        harness
            .query_by_label_contains("Preflight state ready")
            .is_none()
    );
}
