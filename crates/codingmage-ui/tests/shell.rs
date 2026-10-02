//! Native shell behavior without a display server: navigation, opening, failure and staleness.

mod common;

use std::{fs, path::PathBuf, process::Command, time::Duration};

use codingmage_ui::{
    Screen,
    backend::{BackendError, Binding, CoordinatorBinary, Response},
    observed::Freshness,
};
use common::{Fixture, coordinator_binary, harness, settle, tree_digest};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

#[test]
fn shell_renders_navigation_and_keyboard_switches_screens_without_a_project() {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.run_steps(2);
    for screen in Screen::ALL {
        harness.get_by_role_and_label(egui::accesskit::Role::Button, screen.label());
    }
    harness.get_by_label_contains("No repository opened");
    harness.get_by_label_contains("Repository: none opened");
    harness.get_by_label_contains("Campaign: none selected");
    harness.get_by_label_contains("State: no campaign selected");
    harness.get_by_label_contains("Active pods: unknown");
    harness.get_by_label_contains("Last backend update: none");
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
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num9);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Blockers);
    harness.get_by_label_contains("Open a repository in Setup before inspecting");
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open Setup")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);
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
fn offline_help_is_reachable_and_does_not_start_a_coordinator() {
    let mut harness = harness(
        Err(BackendError::BinaryUnavailable {
            expected: PathBuf::from("/nonexistent/codingmage"),
        }),
        [1024.0, 640.0],
    );
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num8);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Help);
    harness.get_by_label_contains("without a repository, coordinator connection or network");
    harness.get_by_label("Glossary");
    harness.get_by_label_contains("Source checkbox:");
    harness.get_by_label_contains("CodingMage native interface");
    harness.get_by_label("Copy diagnostic summary");
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Return to Overview")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Overview);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num8);
    harness.run_steps(2);
    let mut licence_reached = false;
    for _ in 0..128 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(1);
        if harness
            .get_by_label("CodingMage source licence (Apache-2.0)")
            .accesskit_node()
            .is_focused()
        {
            licence_reached = true;
            break;
        }
    }
    assert!(
        licence_reached,
        "keyboard focus never reached the source licence"
    );
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label_contains("TERMS AND CONDITIONS FOR USE");
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
fn command_palette_navigates_offline_and_refuses_unavailable_diagnosis() {
    let mut harness = harness(
        Err(BackendError::BinaryUnavailable {
            expected: PathBuf::from("/nonexistent/codingmage"),
        }),
        [1024.0, 640.0],
    );
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run_steps(2);
    harness.get_by_label("Command palette");
    harness.get_by_role_and_label(
        egui::accesskit::Role::TextInput,
        "Search destinations and actions",
    );
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::WorkPlan);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run_steps(2);
    let search = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("palette search field");
    search.type_text("diagnosis");
    harness.run_steps(2);
    harness.get_by_label("Show command: Refresh diagnosis");
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label("Command palette");
    assert_eq!(
        harness
            .state()
            .diagnosis()
            .freshness(std::time::Instant::now()),
        Freshness::NotRequested
    );
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::WorkPlan);
    assert!(harness.state().project().is_none());
}

#[test]
fn command_palette_refreshes_only_the_opened_project_with_exact_preview() {
    let fixture = Fixture::new("palette-doctor", 3);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1024.0, 640.0],
    );
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    let first_observation = harness.state().diagnosis().observed_at.unwrap();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run_steps(2);
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("palette search field")
        .type_text("diagnosis");
    harness.run_steps(2);
    harness
        .get_by_label("Show command: Refresh diagnosis")
        .click();
    harness.run_steps(2);
    let exact = codingmage_ui::command::format_command(
        &coordinator_binary(),
        &[
            "doctor".to_owned(),
            "--config".to_owned(),
            fixture.config.display().to_string(),
        ],
    )
    .unwrap();
    harness.get_by_label(exact.as_str());
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    assert_eq!(
        harness.state().diagnosis().observed_at,
        Some(first_observation),
        "Enter on Show command must only toggle its disclosure"
    );
    harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .expect("palette search field")
        .focus();
    harness.key_press(egui::Key::Enter);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .observed_at
        .is_some_and(|observed| observed > first_observation)));
    assert_eq!(harness.state().screen(), Screen::Overview);
    assert_eq!(
        harness.state().diagnosis().value.as_ref().unwrap().head,
        fixture.head()
    );
}

#[test]
fn first_run_explains_storage_sign_in_and_next_step_without_a_coordinator() {
    let mut harness = harness(
        Err(BackendError::BinaryUnavailable {
            expected: PathBuf::from("/nonexistent/codingmage"),
        }),
        [1024.0, 640.0],
    );
    harness.run_steps(2);
    harness.get_by_label_contains("What should the team do?");
    harness.get_by_label_contains("configured local state directory");
    harness.get_by_label_contains("provider uses its own existing sign-in");
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::Button,
            "Create configuration in Setup",
        )
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Open Help and About")
        .click();
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Help);
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
    assert!(harness.state().project().is_none());
    assert!(matches!(
        harness.state().open_error(),
        Some(codingmage_ui::project::OpenError::Backend(
            BackendError::BinaryUnavailable { .. }
        ))
    ));
    harness.get_by_label_contains("Install the sibling codingmage executable");
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
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: open configuration")
        .click();
    harness.run_steps(2);
    let open_command = codingmage_ui::command::format_command(
        &coordinator_binary(),
        &[
            "project-open".to_owned(),
            "--config".to_owned(),
            fixture.config.display().to_string(),
        ],
    )
    .unwrap();
    harness.get_by_label(open_command.as_str());
    assert_eq!(tree_digest(&fixture.target), before_target);
    assert_eq!(tree_digest(&fixture.state), before_state);
    assert!(!fixture.scratch.join("worktrees").exists());
    assert_eq!(harness.state().discarded_stale(), 0);
}

#[test]
fn changed_source_after_open_clears_the_old_plan_and_requires_reopen() {
    let fixture = Fixture::new("source-changed-after-open", 3);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1100.0, 720.0],
    );
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    assert!(harness.state().plan_index().is_some());
    let source = fixture.target.join("TASKS.md");
    let mut changed = fs::read(&source).unwrap();
    changed.extend_from_slice(b"\n");
    fs::write(&source, changed).unwrap();
    let observed = Command::new(coordinator_binary())
        .args(["doctor", "--config", fixture.config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(observed.status.success());
    let response = Response {
        generation: harness.state().generation(),
        binding: harness.state().binding(),
        label: "doctor",
        request_id: None,
        result: Ok(observed.stdout),
    };
    assert!(harness.state_mut().handle_response(response));
    assert!(harness.state().plan_index().is_none());
    assert!(matches!(
        &harness.state().project().unwrap().plan,
        Err(codingmage_ui::project::PlanLoadError::Stale)
    ));
    harness.state_mut().select_screen(Screen::WorkPlan);
    harness.run_steps(2);
    harness.get_by_label("Task source changed");
    harness.get_by_label_contains("Reopen the configuration");
}

#[test]
fn diagnosis_for_a_different_configuration_target_cannot_confirm_the_open_project() {
    let fixture = Fixture::new("target-changed-after-open", 3);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1100.0, 720.0],
    );
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    let earlier = harness.state().diagnosis().value.clone();
    let mut foreign =
        serde_json::to_value(harness.state().diagnosis().value.as_ref().unwrap()).unwrap();
    foreign["configuration"]["target_path_sha256"] = serde_json::json!("0".repeat(64));
    let response = Response {
        generation: harness.state().generation(),
        binding: harness.state().binding(),
        label: "doctor",
        request_id: None,
        result: Ok(serde_json::to_vec(&foreign).unwrap()),
    };
    assert!(harness.state_mut().handle_response(response));
    assert!(matches!(
        harness.state().diagnosis().last_error,
        Some((
            _,
            BackendError::Contract(codingmage_ui::backend::models::ModelError::AuthorityMismatch)
        ))
    ));
    assert_eq!(harness.state().diagnosis().value, earlier);
    assert!(harness.state().plan_index().is_none());
    assert!(matches!(
        &harness.state().project().unwrap().plan,
        Err(codingmage_ui::project::PlanLoadError::Stale)
    ));
}

#[test]
fn slow_project_snapshot_keeps_the_window_responsive_and_shows_loading() {
    let fixture = Fixture::new("slow-open", 3);
    let wrapper = fixture.executable(
        "slow-open-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'project-open' ]; then sleep 1; fi\nexec {} \"$@\"\n",
            codingmage_ui::command::format_command(&coordinator_binary(), &[]).unwrap()
        ),
    );
    let mut harness = harness(CoordinatorBinary::at(&wrapper), [1100.0, 720.0]);
    let started = std::time::Instant::now();
    harness.state_mut().open_project(&fixture.config);
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(harness.state().project().is_none());
    harness.run_steps(2);
    harness.get_by_label_contains("Opening configuration through the coordinator");
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
}

#[test]
fn refresh_shows_the_exact_doctor_command_without_running_it() {
    let fixture = Fixture::new("show-doctor", 3);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let before = tree_digest(&fixture.target);
    harness.run_steps(2);
    harness.get_by_label("Show command").click();
    harness.run_steps(2);
    let expected = codingmage_ui::command::format_command(
        &coordinator_binary(),
        &[
            "doctor".to_owned(),
            "--config".to_owned(),
            fixture.config.display().to_string(),
        ],
    )
    .unwrap();
    harness.get_by_label(expected.as_str());
    assert_eq!(tree_digest(&fixture.target), before);
}

#[test]
fn invalid_configuration_is_reported_and_leaves_no_project_open() {
    let fixture = Fixture::new("invalid-config", 3);
    std::fs::write(&fixture.config, "version = 99\n").unwrap();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [900.0, 600.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .open_error()
        .is_some()));
    harness.run_steps(2);
    assert!(harness.state().project().is_none());
    harness.get_by_label_contains("Configuration could not be opened");
    assert!(
        harness
            .get_all_by_label_contains("codingmage.cli.config")
            .count()
            >= 1
    );
}

#[test]
fn project_snapshot_refuses_unknown_fields_versions_identity_and_digest() {
    let fixture = Fixture::new("snapshot-contract", 3);
    let output = std::process::Command::new(coordinator_binary())
        .args(["project-open", "--config", fixture.config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let original: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut changed = original.clone();
    changed["unexpected"] = serde_json::json!(true);
    assert!(matches!(
        codingmage_ui::project::Project::from_snapshot(
            &fixture.config,
            &serde_json::to_vec(&changed).unwrap()
        ),
        Err(codingmage_ui::project::OpenError::Contract)
    ));
    changed = original.clone();
    changed["schema_version"] = serde_json::json!(7);
    assert!(matches!(
        codingmage_ui::project::Project::from_snapshot(
            &fixture.config,
            &serde_json::to_vec(&changed).unwrap()
        ),
        Err(codingmage_ui::project::OpenError::UnsupportedSchema)
    ));
    changed = original.clone();
    changed["config_path"] = serde_json::json!("/synthetic/other.toml");
    assert!(matches!(
        codingmage_ui::project::Project::from_snapshot(
            &fixture.config,
            &serde_json::to_vec(&changed).unwrap()
        ),
        Err(codingmage_ui::project::OpenError::Contract)
    ));
    changed = original;
    changed["plan"]["source_sha256"] = serde_json::json!("0".repeat(64));
    assert!(matches!(
        codingmage_ui::project::Project::from_snapshot(
            &fixture.config,
            &serde_json::to_vec(&changed).unwrap()
        ),
        Err(codingmage_ui::project::OpenError::Contract)
    ));
    changed["plan"] =
        serde_json::json!({"state": "invalid", "code": "codingmage.plan.unrecognized"});
    assert!(matches!(
        codingmage_ui::project::Project::from_snapshot(
            &fixture.config,
            &serde_json::to_vec(&changed).unwrap()
        ),
        Err(codingmage_ui::project::OpenError::Contract)
    ));
    changed["plan"] =
        serde_json::json!({"state": "invalid", "code": "codingmage.plan.duplicate_id"});
    let parsed = codingmage_ui::project::Project::from_snapshot(
        &fixture.config,
        &serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        parsed.plan,
        Err(codingmage_ui::project::PlanLoadError::Invalid(
            codingmage_plan::PlanError::DuplicateId
        ))
    ));
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
        request_id: None,
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
        request_id: None,
        result: Ok(doctor_json.clone()),
    };
    assert!(!harness.state_mut().handle_response(cross_project));
    let stale_project = Response {
        generation: old_generation,
        binding: Binding {
            config_path: Some(first.config.clone()),
            repository_id: None,
            campaign_id: None,
        },
        label: "project-open",
        request_id: None,
        result: Ok(Vec::new()),
    };
    assert!(!harness.state_mut().handle_response(stale_project));
    assert_eq!(harness.state().discarded_stale(), discarded_before + 3);
    let current = Response {
        generation: harness.state().generation(),
        binding: harness.state().binding(),
        label: "doctor",
        request_id: None,
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
        &format!(
            "#!/bin/sh\ncase \"$1\" in\n  project-open) exec {} \"$@\";;\n  doctor) echo '{{\"schema_version\":1,\"command\":\"doctor\",\"surprise\":true}}';;\n  *) exit 1;;\nesac\n",
            codingmage_ui::command::format_command(&coordinator_binary(), &[]).unwrap()
        ),
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
    harness.get_by_label("Malformed backend output");
    harness.get_by_label_contains("does not match the contract");
    harness.get_by_label_contains("no current repository diagnosis is available");
    drop(harness);
    let unsupported = fixture.executable(
        "codingmage",
        &format!(
            "#!/bin/sh\ncase \"$1\" in\n  project-open) exec {} \"$@\";;\n  *) echo '{{\"schema_version\":7}}';;\nesac\n",
            codingmage_ui::command::format_command(&coordinator_binary(), &[]).unwrap()
        ),
    );
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
fn failed_diagnosis_refresh_keeps_earlier_observation_stale() {
    let fixture = Fixture::new("diagnosis-retained", 3);
    let mut harness = harness(
        CoordinatorBinary::at(&coordinator_binary()),
        [1100.0, 720.0],
    );
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let earlier = harness.state().diagnosis().value.clone();
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "doctor",
        request_id: None,
        result: Err(BackendError::Spawn),
    }));
    harness.run_steps(2);
    assert_eq!(harness.state().diagnosis().value, earlier);
    assert_eq!(
        harness
            .state()
            .diagnosis()
            .freshness(std::time::Instant::now()),
        Freshness::Stale
    );
    harness.get_by_label("Coordinator unavailable");
    harness.get_by_label_contains("earlier diagnosis remains visible but is stale");
    harness.get_by_label_contains("Refresh diagnosis before relying on it");

    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "doctor",
        request_id: None,
        result: Ok(b"not json".to_vec()),
    }));
    harness.run_steps(2);
    assert_eq!(harness.state().diagnosis().value, earlier);
    harness.get_by_label("Malformed backend output");
    harness.get_by_label_contains("earlier diagnosis remains visible but is stale");
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
