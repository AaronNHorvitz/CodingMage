//! Native shell behavior without a display server: navigation, opening, failure and staleness.

mod common;

use std::{path::PathBuf, time::Duration};

use codingmage_ui::{
    Screen,
    backend::{BackendError, Binding, CoordinatorBinary, Response},
    observed::Freshness,
};
use common::{Fixture, coordinator_binary, harness, settle, tree_digest};
use egui_kittest::kittest::Queryable as _;

#[test]
fn shell_renders_navigation_and_keyboard_switches_screens_without_a_project() {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.run_steps(2);
    for screen in Screen::ALL {
        harness.get_by_role_and_label(egui::accesskit::Role::Button, screen.label());
    }
    harness.get_by_label_contains("No repository opened");
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Work plan")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::WorkPlan);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num6);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Overview);
    assert!(harness.state().project().is_none());
    assert_eq!(
        harness
            .state()
            .diagnosis()
            .freshness(std::time::Instant::now()),
        Freshness::NotRequested
    );
}

#[test]
fn missing_coordinator_is_a_visible_failure_state_and_refuses_requests() {
    let expected = PathBuf::from("/nonexistent/codingmage");
    let mut harness = harness(
        Err(BackendError::BinaryUnavailable {
            expected: expected.clone(),
        }),
        [720.0, 480.0],
    );
    harness.run_steps(2);
    harness.get_by_label_contains("Coordinator executable unavailable");
    assert!(
        harness
            .get_all_by_label_contains("/nonexistent/codingmage")
            .count()
            >= 1
    );
    let fixture = Fixture::new("missing-binary", 3);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    harness.run_steps(2);
    assert!(harness.state().project().is_some());
    let now = std::time::Instant::now();
    assert_eq!(
        harness.state().diagnosis().freshness(now),
        Freshness::Failed
    );
    assert!(matches!(
        harness.state().diagnosis().last_error,
        Some((_, BackendError::BinaryUnavailable { .. }))
    ));
    harness.get_by_label_contains("not installed next to this interface");
}

#[test]
fn opening_a_repository_observes_real_diagnosis_without_side_effects() {
    let fixture = Fixture::new("open", 3);
    let before_target = tree_digest(&fixture.target);
    let before_state = tree_digest(&fixture.state);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some() || app.diagnosis().last_error.is_some()
    }));
    let diagnosis = harness
        .state()
        .diagnosis()
        .value
        .clone()
        .expect("diagnosis observed");
    assert_eq!(diagnosis.state, "ready");
    assert_eq!(diagnosis.head, fixture.head());
    assert_eq!(diagnosis.branch.as_deref(), Some("main"));
    assert!(diagnosis.configuration.capabilities.all_denied());
    harness.run_steps(2);
    harness.get_by_label(diagnosis.repository_id.as_str());
    harness.get_by_label(diagnosis.head.as_str());
    harness.get_by_label_contains("Task source parsed: 1 sprints, 1 stories");
    assert_eq!(tree_digest(&fixture.target), before_target);
    assert_eq!(tree_digest(&fixture.state), before_state);
    assert!(!fixture.scratch.join("worktrees").exists());
    assert_eq!(harness.state().discarded_stale(), 0);
}

#[test]
fn invalid_configuration_is_reported_and_leaves_no_project_open() {
    let fixture = Fixture::new("invalid-config", 3);
    std::fs::write(&fixture.config, "version = 99\n").unwrap();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [900.0, 600.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    harness.run_steps(2);
    assert!(harness.state().project().is_none());
    harness.get_by_label_contains("Configuration could not be opened");
    assert!(
        harness
            .get_all_by_label_contains("schema is invalid")
            .count()
            >= 1
    );
}

#[test]
fn stale_generation_and_cross_project_responses_are_discarded() {
    let first = Fixture::new("stale-a", 3);
    let second = Fixture::new("stale-b", 3);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    let first_config = first.config.clone();
    harness.state_mut().open_project(&first_config);
    let old_generation = harness.state().generation();
    let second_config = second.config.clone();
    harness.state_mut().open_project(&second_config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    // Give the first repository's slow doctor response time to arrive and be discarded.
    std::thread::sleep(Duration::from_millis(500));
    harness.run_steps(2);
    let discarded_before = harness.state().discarded_stale();
    let doctor_json =
        serde_json::to_vec(harness.state().diagnosis().value.as_ref().unwrap()).unwrap();
    let stale = Response {
        generation: old_generation,
        binding: Binding {
            config_path: Some(first_config.clone()),
            repository_id: None,
            campaign_id: None,
        },
        label: "doctor",
        result: Ok(doctor_json.clone()),
    };
    assert!(!harness.state_mut().handle_response(stale));
    let cross_project = Response {
        generation: harness.state().generation(),
        binding: Binding {
            config_path: Some(first_config),
            repository_id: None,
            campaign_id: None,
        },
        label: "doctor",
        result: Ok(doctor_json.clone()),
    };
    assert!(!harness.state_mut().handle_response(cross_project));
    assert_eq!(harness.state().discarded_stale(), discarded_before + 2);
    let current = Response {
        generation: harness.state().generation(),
        binding: harness.state().binding(),
        label: "doctor",
        result: Ok(doctor_json),
    };
    assert!(harness.state_mut().handle_response(current));
    let after_first_open = harness.state().diagnosis().value.as_ref().unwrap().clone();
    assert_ne!(after_first_open.repository_id, "");
}

#[test]
fn malformed_backend_output_is_an_explicit_contract_failure() {
    let fixture = Fixture::new("malformed", 3);
    let fake = fixture.executable(
        "codingmage",
        "#!/bin/sh\ncase \"$1\" in\n  doctor) echo '{\"schema_version\":1,\"command\":\"doctor\",\"surprise\":true}';;\n  *) exit 1;;\nesac\n",
    );
    let binary = CoordinatorBinary::at(&fake);
    let mut harness = harness(binary, [1100.0, 720.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().last_error.is_some()
    }));
    assert!(matches!(
        harness.state().diagnosis().last_error,
        Some((_, BackendError::Contract(_)))
    ));
    harness.run_steps(2);
    harness.get_by_label_contains("does not match the contract");
    drop(harness);
    let unsupported =
        fixture.executable("codingmage", "#!/bin/sh\necho '{\"schema_version\":7}'\n");
    let binary = CoordinatorBinary::at(&unsupported);
    let mut second = common::harness(binary, [1100.0, 720.0]);
    second.state_mut().open_project(&config);
    assert!(settle(&mut second, Duration::from_secs(30), |app| {
        app.diagnosis().last_error.is_some()
    }));
    assert!(matches!(
        second.state().diagnosis().last_error,
        Some((_, BackendError::Contract(_)))
    ));
}

#[test]
fn compact_window_still_exposes_navigation_and_content() {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [720.0, 480.0]);
    harness.run_steps(2);
    for screen in Screen::ALL {
        harness.get_by_role_and_label(egui::accesskit::Role::Button, screen.label());
    }
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Open");
}

#[test]
fn software_renderer_produces_a_non_blank_frame() {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::Vec2::new(1100.0, 720.0))
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_fonts(common::fonts());
            codingmage_ui::App::with_binary(&creation.egui_ctx, binary)
        });
    harness.run_steps(2);
    let image = harness.render().expect("software rendering");
    let distinct = image
        .pixels()
        .map(|pixel| pixel.0)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        distinct.len() > 8,
        "frame had only {} distinct colours",
        distinct.len()
    );
    if let Some(directory) = std::env::var_os("CODINGMAGE_UI_SNAPSHOT_DIR") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image
            .save(directory.join("shell-overview-no-project.png"))
            .unwrap();
    }
}
