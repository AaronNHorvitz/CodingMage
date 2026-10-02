//! Guided setup over the existing configuration and campaign contracts.

mod common;

use std::{
    fs,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::load_config;
use codingmage_ui::{
    Screen,
    backend::{CoordinatorBinary, Response},
    campaign::SelectError,
    command::format_command,
    project::{OpenError, Project},
    setup::ProviderForm,
    state_dir::project_private_dir,
};
use common::{Fixture, coordinator_binary, harness, harness_with_state, settle, tree_digest};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use sha2::{Digest as _, Sha256};

fn export_helper_unlocked(private: &Path) -> bool {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(private.join("setup-export-intent.lock"))
        .is_ok_and(|file| file.try_lock().is_ok())
}

fn config_helper_unlocked(private: &Path) -> bool {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(private.join("setup-config-intent.lock"))
        .is_ok_and(|file| file.try_lock().is_ok())
}

#[test]
fn configuration_recovery_snapshot_requires_the_exact_published_digest() {
    let fixture = Fixture::new("setup-config-digest", 2);
    let output = Command::new(coordinator_binary())
        .args(["project-open", "--config"])
        .arg(&fixture.config)
        .output()
        .unwrap();
    assert!(output.status.success());
    let digest = Sha256::digest(fs::read(&fixture.config).unwrap());
    let digest = codingmage_ui::project::hex(&digest);
    assert!(Project::from_snapshot_for_write(&fixture.config, &digest, &output.stdout).is_ok());
    assert_eq!(
        Project::from_snapshot_for_write(&fixture.config, &"0".repeat(64), &output.stdout)
            .unwrap_err(),
        OpenError::Contract
    );
    assert_eq!(
        Project::from_snapshot_for_write(&fixture.config, "invalid", &output.stdout).unwrap_err(),
        OpenError::Contract
    );
}

#[test]
fn configuration_helper_refuses_malformed_and_unknown_request_fields() {
    let helper = env!("CARGO_BIN_EXE_codingmage-ui");
    let requests: [&[u8]; 2] = [
        b"{",
        br#"{"schema_version":1,"intent_path":"/missing","intent_sha256":"0000000000000000000000000000000000000000000000000000000000000000","binary_path":"/missing","deadline_ms":1,"unexpected":true}"#,
    ];
    for request in requests {
        let mut child = Command::new(helper)
            .arg("--setup-config-helper")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(request).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(2));
    }
}

fn export_result_count(private: &Path) -> usize {
    fs::read_dir(private)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("setup-export-result-")
        })
        .count()
}

#[test]
fn native_setup_export_uses_public_command_and_verifies_the_destination() {
    let fixture = Fixture::new("setup-export-public", 1);
    let destination = fixture.root.join("export.toml");
    let binary = coordinator_binary();
    let state_dir = fixture.root.join("ui-private");
    let private = project_private_dir(&state_dir, &fixture.config);
    let mut harness =
        harness_with_state(CoordinatorBinary::at(&binary), [1100.0, 2200.0], state_dir);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    harness.state_mut().setup_state_mut().export_path = destination.display().to_string();
    harness
        .state_mut()
        .export_document(&fixture.root.join("unselected.toml"));
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| result
                .as_ref()
                .is_err_and(|message| message.contains("select the configuration or campaign")))
    );
    assert!(!destination.exists());
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: export configuration")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("setup-export-copy");
    harness.state_mut().export_document(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|message| message.contains("exported and verified"))
        })
    }));
    assert_eq!(
        fs::read(&destination).unwrap(),
        fs::read(&fixture.config).unwrap()
    );
    assert_eq!(export_result_count(&private), 0);
    assert!(!private.join("setup-export-intent.json").exists());

    let second_destination = fixture.root.join("second-export.toml");
    harness.state_mut().setup_state_mut().export_path = second_destination.display().to_string();
    harness.state_mut().export_document(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|message| message.contains("exported and verified"))
        })
    }));
    assert_eq!(
        fs::read(&second_destination).unwrap(),
        fs::read(&fixture.config).unwrap()
    );
    assert_eq!(export_result_count(&private), 0);
    assert!(!private.join("setup-export-intent.json").exists());
}

#[test]
fn malformed_setup_export_receipt_never_reports_success() {
    let fixture = Fixture::new("setup-export-malformed", 1);
    let destination = fixture.root.join("export.toml");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "malformed-export-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then printf '{{\"schema_version\":1,\"written\":true,\"unexpected\":1}}'; exit 0; fi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let state_dir = fixture.root.join("ui-private");
    let private = project_private_dir(&state_dir, &fixture.config);
    let mut harness =
        harness_with_state(CoordinatorBinary::at(&wrapper), [1100.0, 2200.0], state_dir);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    harness.state_mut().setup_state_mut().export_path = destination.display().to_string();
    harness.state_mut().export_document(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("not confirmed"))
        })
    }));
    assert!(!destination.exists());
    assert_eq!(export_result_count(&private), 1);
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("I inspected the destination; clear export notice")
        .click();
    assert!(settle(&mut harness, Duration::from_secs(30), |_| {
        !private.join("setup-export-intent.json").exists()
    }));
    assert_eq!(export_result_count(&private), 0);
    assert!(!private.join("setup-export-intent.json").exists());
}

#[test]
fn export_notice_for_replaced_repository_remains_bound_to_original_observation() {
    let original = Fixture::new("setup-export-original-repository", 1);
    let replacement = Fixture::new("setup-export-replacement-repository", 1);
    let real_binary = coordinator_binary();
    let wrapper = original.executable(
        "failed-export-coordinator",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then printf '%s\\n' '{{\"schema_version\":1,\"written\":true,\"unexpected\":1}}'; exit 0; fi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let state_dir = original.root.join("ui-private");
    let private = project_private_dir(&state_dir, &original.config);
    let mut first = harness_with_state(
        CoordinatorBinary::at(&wrapper),
        [1100.0, 800.0],
        state_dir.clone(),
    );
    first.state_mut().open_project(&original.config);
    assert!(settle(&mut first, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    let original_repository_id = first
        .state()
        .diagnosis()
        .value
        .as_ref()
        .unwrap()
        .repository_id
        .clone();
    first.state_mut().setup_state_mut().export_path =
        original.root.join("export.toml").display().to_string();
    first.state_mut().export_document(&original.config);
    assert!(settle(&mut first, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("not confirmed"))
        })
    }));
    assert_eq!(export_result_count(&private), 1);
    let intent_path = private.join("setup-export-intent.json");
    assert!(intent_path.exists());
    drop(first);

    fs::copy(&replacement.config, &original.config).unwrap();
    let mut reopened = harness_with_state(
        CoordinatorBinary::at(&real_binary),
        [1100.0, 800.0],
        state_dir,
    );
    reopened.state_mut().open_project(&original.config);
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.diagnosis().value.as_ref().is_some_and(|diagnosis| {
            diagnosis.repository_id != original_repository_id
                && app.setup_state().message.as_ref().is_some_and(|result| {
                    result
                        .as_ref()
                        .is_err_and(|message| message.contains("another repository observation"))
                })
        })
    }));
    reopened.state_mut().select_screen(Screen::Setup);
    reopened.run_steps(2);
    let clear = reopened.get_by_label("I inspected the destination; clear export notice");
    assert!(clear.accesskit_node().is_disabled());
    clear.click();
    reopened.run_steps(2);
    assert!(intent_path.exists());
    assert_eq!(export_result_count(&private), 1);
}

#[test]
fn replaced_setup_export_after_public_receipt_is_not_reported_as_success() {
    let fixture = Fixture::new("setup-export-replaced", 1);
    let destination = fixture.root.join("export.toml");
    let receipt_file = fixture.root.join("export-receipt.json");
    let written_marker = fixture.root.join("export-writer-finished");
    let release_marker = fixture.root.join("release-export-response");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "held-export-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then\n  {} \"$@\" > {} || exit $?\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&written_marker, &[]).unwrap(),
            format_command(&release_marker, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut harness = harness(CoordinatorBinary::at(&wrapper), [1100.0, 2200.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    harness.state_mut().setup_state_mut().export_path = destination.display().to_string();
    harness.state_mut().export_document(&fixture.config);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !written_marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(written_marker.exists(), "public exporter did not finish");
    let generation = harness.state().generation();
    let binding = harness.state().binding();
    assert!(!harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "setup-export-copy",
        request_id: Some("unrelated-export".to_owned()),
        result: Ok(fs::read(&receipt_file).unwrap()),
    }));
    harness.state_mut().setup_state_mut().export_path =
        fixture.root.join("changed-path.toml").display().to_string();
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: export configuration")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains(&format!("{} --overwrite false", destination.display()));
    let replacement_path = fixture.root.join("replacement-export.toml");
    fs::write(&replacement_path, b"changed after the public receipt").unwrap();
    fs::rename(&replacement_path, &destination).unwrap();
    fs::write(&release_marker, b"").unwrap();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("setup_export_invalid_receipt"))
        })
    }));
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"changed after the public receipt"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn started_export_survives_window_close_and_campaign_reselection() {
    for (phase, close_window) in [
        ("before", true),
        ("after", true),
        ("before", false),
        ("after", false),
    ] {
        let fixture = Fixture::new(&format!("setup-export-survival-{phase}-{close_window}"), 1);
        let state_dir = fixture.root.join("private-ui-state");
        let destination = fixture.root.join("surviving-export.toml");
        let started = fixture.root.join("export-started");
        let release = fixture.root.join("release-export");
        let receipt = fixture.root.join("held-receipt.json");
        let real_binary = coordinator_binary();
        let body = if phase == "before" {
            format!(
                "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\nfi\nexec {} \"$@\"\n",
                format_command(&started, &[]).unwrap(),
                format_command(&release, &[]).unwrap(),
                format_command(&real_binary, &[]).unwrap(),
            )
        } else {
            format!(
                "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then\n  {} \"$@\" > {} || exit $?\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
                format_command(&real_binary, &[]).unwrap(),
                format_command(&receipt, &[]).unwrap(),
                format_command(&started, &[]).unwrap(),
                format_command(&release, &[]).unwrap(),
                format_command(&receipt, &[]).unwrap(),
                format_command(&real_binary, &[]).unwrap(),
            )
        };
        let wrapper = fixture.executable("held-setup-export", &body);
        let mut harness = Some(harness_with_state(
            CoordinatorBinary::at(&wrapper),
            [1100.0, 2200.0],
            state_dir.clone(),
        ));
        let active = harness.as_mut().unwrap();
        active.state_mut().open_project(&fixture.config);
        assert!(settle(active, Duration::from_secs(30), |app| app
            .diagnosis()
            .value
            .is_some()));
        active.state_mut().setup_state_mut().export_path = destination.display().to_string();
        active.state_mut().export_document(&fixture.config);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !started.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if !started.exists() {
            fs::write(&release, b"").unwrap();
            panic!("Setup export helper did not start for {phase}/{close_window}");
        }
        assert_eq!(destination.exists(), phase == "after");
        if close_window {
            active.state_mut().close_project();
            drop(harness.take());
        } else {
            let before = active.state().generation();
            active
                .state_mut()
                .select_campaign(&fixture.root.join("replacement-campaign.toml"));
            assert!(active.state().generation() > before);
            active.state_mut().select_screen(Screen::Setup);
            active.run_steps(2);
            // The helper holds the private intent while its coordinator command is in flight.
            // A premature manual clear cannot admit a second export.
            active
                .get_by_label("I inspected the destination; clear export notice")
                .click();
            assert!(settle(active, Duration::from_secs(30), |app| {
                app.setup_state().message.as_ref().is_some_and(|result| {
                    result
                        .as_ref()
                        .is_err_and(|message| message.contains("still running"))
                })
            }));
            assert!(
                active
                    .state()
                    .setup_state()
                    .message
                    .as_ref()
                    .is_some_and(|result| {
                        result
                            .as_ref()
                            .is_err_and(|message| message.contains("still running"))
                    })
            );
        }
        fs::write(&release, b"").unwrap();
        let private = codingmage_ui::state_dir::project_private_dir(&state_dir, &fixture.config);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut recorded = false;
        let mut finished = false;
        while Instant::now() < deadline {
            recorded = fs::read_dir(&private).is_ok_and(|entries| {
                entries.filter_map(Result::ok).any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("setup-export-result-")
                })
            });
            finished = recorded && export_helper_unlocked(&private);
            if finished && destination.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(recorded, "terminal export result was not retained");
        assert!(finished, "helper did not release the private intent lock");
        assert_eq!(
            fs::read(&destination).unwrap(),
            fs::read(&fixture.config).unwrap()
        );
        if let Some(active) = harness.as_mut() {
            active.run_steps(4);
            assert!(
                !active
                    .state()
                    .setup_state()
                    .message
                    .as_ref()
                    .is_some_and(|result| {
                        result
                            .as_ref()
                            .is_ok_and(|message| message.contains("exported and verified"))
                    })
            );
        }
        drop(harness);
        let mut reopened =
            harness_with_state(CoordinatorBinary::at(&wrapper), [1100.0, 800.0], state_dir);
        reopened.state_mut().open_project(&fixture.config);
        assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
            app.setup_state().message.as_ref().is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|message| message.contains("previous export completed and verified"))
            })
        }));
    }
}

#[test]
fn setup_export_helper_records_result_after_its_launcher_process_exits() {
    let fixture = Fixture::new("setup-export-orphan", 1);
    let destination = fixture.root.join("orphan-export.toml");
    let started = fixture.root.join("orphan-started");
    let release = fixture.root.join("orphan-release");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "orphan-export-coordinator",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-export-copy' ]; then\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\nfi\nexec {} \"$@\"\n",
            format_command(&started, &[]).unwrap(),
            format_command(&release, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut harness = harness(CoordinatorBinary::at(&wrapper), [1100.0, 800.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| app
        .diagnosis()
        .value
        .is_some()));
    let repository_id = harness
        .state()
        .diagnosis()
        .value
        .as_ref()
        .unwrap()
        .repository_id
        .clone();
    drop(harness);
    let private = codingmage_ui::state_dir::project_private_dir(
        &fixture.root.join("ui-state"),
        &fixture.config,
    );
    let intent_path = private.join("setup-export-intent.json");
    let request_id = "0123456789abcdef0123456789abcdef";
    let intent = serde_json::json!({
        "schema_version": 1,
        "request_id": request_id,
        "config_path": fixture.config,
        "repository_id": repository_id,
        "source": fixture.config,
        "destination": destination,
        "arguments": [
            "setup-export-copy", "--config", fixture.config,
            "--repository-id", repository_id, "--source", fixture.config,
            "--output", destination, "--overwrite", "false"
        ]
    });
    let intent_bytes = serde_json::to_vec(&intent).unwrap();
    codingmage_ui::state_dir::write_private(&intent_path, &intent_bytes).unwrap();
    let helper_request = serde_json::json!({
        "schema_version": 1,
        "intent_path": intent_path,
        "intent_sha256": codingmage_ui::project::hex(&Sha256::digest(&intent_bytes)),
        "binary_path": wrapper,
        "deadline_ms": 60000
    });
    let request_path = fixture.root.join("helper-request.json");
    fs::write(&request_path, serde_json::to_vec(&helper_request).unwrap()).unwrap();
    let ui_binary = std::path::Path::new(env!("CARGO_BIN_EXE_codingmage-ui"));
    let launch = format!(
        "{} < {} >/dev/null 2>&1 &",
        format_command(ui_binary, &["--setup-export-helper".to_owned()]).unwrap(),
        format_command(&request_path, &[]).unwrap(),
    );
    assert!(
        std::process::Command::new("sh")
            .args(["-c", &launch])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if !started.exists() {
        fs::write(&release, b"").unwrap();
        panic!("orphaned helper did not reach the coordinator");
    }
    assert!(!destination.exists());
    fs::write(&release, b"").unwrap();
    let result = private.join(format!("setup-export-result-{request_id}.json"));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut finished = false;
    while Instant::now() < deadline {
        finished = result.exists() && export_helper_unlocked(&private);
        if finished {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        finished,
        "orphaned helper did not finish and record its result"
    );
    assert_eq!(
        fs::read(destination).unwrap(),
        fs::read(&fixture.config).unwrap()
    );
}

#[test]
fn replaced_campaign_after_writer_receipt_is_not_selected() {
    let fixture = Fixture::new("setup-campaign-replaced", 1);
    let record = fixture.root.join("operator-authorization.txt");
    fs::write(&record, "Synthetic owner authorization").unwrap();
    let destination = fixture.root.join("campaign.toml");
    let receipt_file = fixture.root.join("campaign-receipt.json");
    let written_marker = fixture.root.join("writer-finished");
    let release_marker = fixture.root.join("release-response");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "held-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-write-campaign' ]; then\n  {} \"$@\" > {} || exit $?\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&written_marker, &[]).unwrap(),
            format_command(&release_marker, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut harness = harness(CoordinatorBinary::at(&wrapper), [1100.0, 800.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let provider = fixture.executable("provider", "#!/bin/sh\nexit 0\n");
    harness.state_mut().start_campaign_form();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .campaign_form
            .as_mut()
            .unwrap();
        form.spec_path = destination.display().to_string();
        form.authorization_path = record.display().to_string();
        form.allowed_paths = "src".to_owned();
        for candidate in [
            &mut form.team_lead,
            &mut form.implementer,
            &mut form.reviewer,
        ] {
            *candidate = ProviderForm {
                executable: provider.display().to_string(),
                model: "fixture".to_owned(),
                effort: "high".to_owned(),
            };
        }
    }
    harness.state_mut().apply_campaign_form();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !written_marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(written_marker.exists(), "public writer did not finish");
    let original = CampaignSpec::load(&destination).unwrap();
    let mut replacement = original.clone();
    replacement.max_units += 1;
    replacement.verify().unwrap();
    let replacement_path = fixture.root.join("replacement.toml");
    fs::write(
        &replacement_path,
        toml::to_string_pretty(&replacement).unwrap(),
    )
    .unwrap();
    fs::rename(&replacement_path, &destination).unwrap();
    fs::write(&release_marker, b"").unwrap();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("campaign write or selection not confirmed"))
        })
    }));
    assert!(harness.state().campaign().is_none());
    assert!(matches!(
        harness.state().campaign_error(),
        Some(SelectError::Backend(_))
    ));
    assert_eq!(CampaignSpec::load(&destination).unwrap(), replacement);
    assert_eq!(original.campaign_id, replacement.campaign_id);
}

#[test]
fn pending_campaign_preview_is_frozen_and_bad_inspection_never_writes() {
    let fixture = Fixture::new("setup-campaign-preview", 1);
    let record = fixture.root.join("operator-authorization.txt");
    fs::write(&record, "Synthetic owner authorization").unwrap();
    let first = fixture.root.join("campaign-a.toml");
    let second = fixture.root.join("campaign-b.toml");
    let write_marker = fixture.root.join("write-invoked");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "delayed-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-inspect-authorization' ]; then sleep 3; printf '%s\\n' '{{\"schema_version\":1,\"repository_id\":\"wrong-repository\",\"head\":\"wrong-head\",\"task_source_sha256\":\"wrong-digest\",\"observed\":true,\"bytes\":29,\"sha256\":\"{}\"}}'; exit 0; fi\nif [ \"$1\" = 'setup-write-campaign' ]; then printf invoked > {}; exit 0; fi\nexec {} \"$@\"\n",
            "a".repeat(64),
            format_command(&write_marker, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut harness = harness(CoordinatorBinary::at(&wrapper), [1100.0, 2200.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let diagnosis = harness.state().diagnosis().value.as_ref().unwrap().clone();
    let provider = fixture.executable("provider", "#!/bin/sh\nexit 0\n");
    harness.state_mut().start_campaign_form();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .campaign_form
            .as_mut()
            .unwrap();
        form.spec_path = first.display().to_string();
        form.authorization_path = record.display().to_string();
        form.allowed_paths = "src".to_owned();
        for candidate in [
            &mut form.team_lead,
            &mut form.implementer,
            &mut form.reviewer,
        ] {
            *candidate = ProviderForm {
                executable: provider.display().to_string(),
                model: "fixture".to_owned(),
                effort: "high".to_owned(),
            };
        }
    }
    let inspect = vec![
        "setup-inspect-authorization".to_owned(),
        "--config".to_owned(),
        fixture.config.display().to_string(),
        "--repository-id".to_owned(),
        diagnosis.repository_id,
        "--head".to_owned(),
        diagnosis.head,
        "--task-source-sha256".to_owned(),
        diagnosis.task_source_sha256,
        "--authorization".to_owned(),
        record.display().to_string(),
    ];
    let submitted = format_command(&wrapper, &inspect).unwrap();
    harness.state_mut().apply_campaign_form();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .campaign_form
            .as_mut()
            .unwrap();
        form.spec_path = second.display().to_string();
        form.overwrite = true;
    }
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("Show command: inspect authorization record")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains(&submitted);
    harness
        .get_by_label("Show command: write campaign specification")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains(&first.display().to_string());
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("campaign write or selection not confirmed"))
        })
    }));
    assert!(!first.exists() && !second.exists() && !write_marker.exists());
    assert!(harness.state().campaign().is_none());
}

#[test]
fn pending_authorization_preview_keeps_the_submitted_arguments() {
    let fixture = Fixture::new("setup-pending-preview", 1);
    let real_binary = coordinator_binary();
    let real_command = format_command(&real_binary, &[]).unwrap();
    let wrapper = fixture.executable(
        "delayed-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-write-authorization' ]; then sleep 3; fi\nexec {real_command} \"$@\"\n"
        ),
    );
    let binary = CoordinatorBinary::at(&wrapper);
    let mut harness = harness(binary, [1100.0, 2200.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let first = fixture.root.join("authorization-a.txt");
    let second = fixture.root.join("authorization-b.txt");
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = first.display().to_string();
        setup.authorization_text = "Synthetic owner authorization".to_owned();
        setup.authorization_overwrite = false;
    }
    let repository_id = harness
        .state()
        .diagnosis()
        .value
        .as_ref()
        .unwrap()
        .repository_id
        .clone();
    let arguments = vec![
        "setup-write-authorization".to_owned(),
        "--config".to_owned(),
        fixture.config.display().to_string(),
        "--repository-id".to_owned(),
        repository_id,
        "--output".to_owned(),
        first.display().to_string(),
        "--overwrite".to_owned(),
        "false".to_owned(),
    ];
    let submitted = format_command(&wrapper, &arguments).unwrap();
    harness.state_mut().apply_authorization_record();
    {
        // Simulate a late form edit even though the widgets are disabled while pending.
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = second.display().to_string();
        setup.authorization_text = "Changed after submission".to_owned();
        setup.authorization_overwrite = true;
    }
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness.get_by_label_contains("The fields are locked until its result is known");
    harness
        .get_by_label("Show command: write authorization record")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains(&submitted);
    let mut changed_arguments = arguments;
    changed_arguments[6] = second.display().to_string();
    changed_arguments[8] = "true".to_owned();
    let changed = format_command(&wrapper, &changed_arguments).unwrap();
    assert!(harness.query_by_label_contains(&changed).is_none());
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|text| text.contains("authorization record written"))
        })
    }));
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        "Synthetic owner authorization"
    );
    assert!(!second.exists());
}

#[test]
fn authorization_response_for_another_repository_clears_pending_state() {
    let fixture = Fixture::new("setup-stale-response", 1);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 800.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let record = fixture.root.join("authorization.txt");
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = record.display().to_string();
        setup.authorization_text = "Synthetic owner authorization".to_owned();
    }
    harness.state_mut().apply_authorization_record();
    let generation = harness.state().generation();
    let mut binding = harness.state().binding();
    binding.repository_id = Some("another-repository".to_owned());
    assert!(!harness.state_mut().handle_response(Response {
        generation,
        binding,
        label: "setup-write-authorization",
        request_id: Some("setup-authorization-1".to_owned()),
        result: Ok(Vec::new()),
    }));
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| result
                .as_ref()
                .is_err_and(|text| text.contains("selected authority changed")))
    );
    assert!(harness.state().authorization_record().is_none());
}

#[test]
#[allow(clippy::too_many_lines)]
fn guided_configuration_is_validated_by_the_existing_loader_and_opened() {
    let fixture = Fixture::new("setup-config", 3);
    let workspace = fixture.root.join("guided");
    let receipt_file = fixture.root.join("config-receipt.json");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "observed-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-write-config' ]; then\n  {} \"$@\" > {} || exit $?\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&receipt_file, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let binary = CoordinatorBinary::at(&wrapper);
    let mut harness = harness_with_state(binary, [1100.0, 2200.0], fixture.root.join("ui-private"));
    harness.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    let target = fixture.target.clone();
    harness.state_mut().start_config_form(&target);
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness.get_by_label_contains("Repository configuration (version 1, deny-first)");
    harness.get_by_label("local only");
    harness
        .get_by_label("Show command: write configuration")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("setup-write-config");
    let before = tree_digest(&fixture.target);
    harness.state_mut().apply_config_form();
    let settled = settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
            && app.setup_state().message.as_ref().is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|message| message.contains("written and verified"))
            })
    });
    assert!(
        settled,
        "config result: {:?}; published: {:?}; receipt: {:?}",
        harness.state().setup_state().message,
        fs::metadata(workspace.join("codingmage.toml")).map(|value| {
            use std::os::unix::fs::MetadataExt as _;
            (value.len(), value.mode() & 0o777, value.nlink())
        }),
        fs::read_to_string(&receipt_file)
    );
    let written = workspace.join("codingmage.toml");
    let config = load_config(&written).expect("existing loader accepts the guided file");
    assert_eq!(config.correction_limit, 3);
    assert!(config.capabilities == codingmage_core::CapabilityPolicy::default());
    assert_eq!(
        harness
            .state()
            .project()
            .map(|project| project.config_path.clone()),
        Some(written.clone())
    );
    assert_eq!(tree_digest(&fixture.target), before);
    harness.run_steps(2);
    harness.get_by_label_contains("configuration written and verified");
    // A conflicting policy is refused by the existing loader and never replaces the file.
    harness.state_mut().edit_opened_config();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .config_form
            .as_mut()
            .unwrap();
        form.capabilities.push = codingmage_core::CapabilityGrant::Allowed;
    }
    harness.state_mut().apply_config_form();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("configuration write was not confirmed"))
        })
    }));
    assert_eq!(load_config(&written).unwrap(), config);
    // Inspect and clear the failed request before trying another write.
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("I inspected the destination; clear configuration notice")
        .click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|message| message.contains("notice cleared"))
        })
    }));
    // Overwrite is refused unless requested.
    harness.state_mut().start_config_form(&target);
    harness.state_mut().apply_config_form();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("configuration write was not confirmed"))
        })
    }));
    assert_eq!(load_config(&written).unwrap(), config);
}

#[test]
#[allow(clippy::too_many_lines)]
fn configuration_helper_survives_window_close_and_requires_explicit_recovery() {
    let fixture = Fixture::new("setup-config-recovery", 2);
    let workspace = fixture.root.join("guided");
    let state_dir = fixture.root.join("ui-private");
    let destination = workspace.join("codingmage.toml");
    let started = fixture.root.join("config-written");
    let release = fixture.root.join("release-config-receipt");
    let receipt = fixture.root.join("held-config-receipt.json");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "held-config-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-write-config' ]; then\n  {} \"$@\" > {} || exit $?\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
            format_command(&receipt, &[]).unwrap(),
            format_command(&started, &[]).unwrap(),
            format_command(&release, &[]).unwrap(),
            format_command(&receipt, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut first = harness_with_state(
        CoordinatorBinary::at(&wrapper),
        [1100.0, 2200.0],
        state_dir.clone(),
    );
    first.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    first.state_mut().start_config_form(&fixture.target);
    first.state_mut().apply_config_form();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if !started.exists() {
        fs::write(&release, b"").unwrap();
        panic!("public configuration write did not finish");
    }
    assert!(destination.exists());
    assert!(state_dir.join("setup-config-intent.json").exists());
    drop(first);
    fs::write(&release, b"").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut finished = false;
    while Instant::now() < deadline {
        finished = fs::read_dir(&state_dir).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("setup-config-result-")
            })
        }) && config_helper_unlocked(&state_dir);
        if finished {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        finished,
        "detached configuration helper lost its terminal record"
    );
    let scratch = workspace.join("scratch");
    fs::remove_dir(&scratch).unwrap();
    let mut reopened = harness_with_state(
        CoordinatorBinary::at(&wrapper),
        [1100.0, 2200.0],
        state_dir.clone(),
    );
    reopened.state_mut().start_config_form(&fixture.target);
    assert!(reopened.state().setup_state().config_form.is_none());
    reopened.state_mut().select_screen(Screen::Setup);
    reopened.run_steps(2);
    reopened
        .get_by_label("Show command: write configuration")
        .click();
    reopened.run_steps(2);
    reopened.get_by_label_contains("setup-write-config");
    reopened
        .get_by_label("Check previous configuration outcome")
        .click();
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("not confirmed"))
        })
    }));
    assert!(reopened.state().project().is_none());
    assert!(
        reopened
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| {
                result
                    .as_ref()
                    .is_err_and(|message| message.contains("not confirmed"))
            })
    );
    assert!(state_dir.join("setup-config-intent.json").exists());
    fs::create_dir(&scratch).unwrap();
    reopened
        .get_by_label("Check previous configuration outcome")
        .click();
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|message| message.contains("written and verified"))
        })
    }));
    assert_eq!(
        reopened
            .state()
            .project()
            .map(|project| project.config_path.clone()),
        Some(destination.clone())
    );
    assert!(
        reopened
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|message| message.contains("written and verified"))
            })
    );
    assert!(!state_dir.join("setup-config-intent.json").exists());
    assert_eq!(
        load_config(&destination).unwrap().target_path,
        fixture.target
    );
}

#[test]
fn changed_selection_and_replaced_configuration_cannot_claim_success() {
    let fixture = Fixture::new("setup-config-replaced", 2);
    let workspace = fixture.root.join("guided");
    let state_dir = fixture.root.join("ui-private");
    let destination = workspace.join("codingmage.toml");
    let started = fixture.root.join("config-written");
    let release = fixture.root.join("release-config-receipt");
    let receipt = fixture.root.join("held-config-receipt.json");
    let real_binary = coordinator_binary();
    let wrapper = fixture.executable(
        "held-config-codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'setup-write-config' ]; then\n  {} \"$@\" > {} || exit $?\n  : > {}\n  until [ -f {} ]; do sleep 0.05; done\n  cat {}\n  exit 0\nfi\nexec {} \"$@\"\n",
            format_command(&real_binary, &[]).unwrap(),
            format_command(&receipt, &[]).unwrap(),
            format_command(&started, &[]).unwrap(),
            format_command(&release, &[]).unwrap(),
            format_command(&receipt, &[]).unwrap(),
            format_command(&real_binary, &[]).unwrap(),
        ),
    );
    let mut harness = harness_with_state(
        CoordinatorBinary::at(&wrapper),
        [1100.0, 2200.0],
        state_dir.clone(),
    );
    harness.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    harness.state_mut().start_config_form(&fixture.target);
    harness.state_mut().apply_config_form();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if !started.exists() {
        fs::write(&release, b"").unwrap();
        panic!("public configuration write did not finish");
    }
    let replacement = fixture.root.join("replacement-config.toml");
    fs::write(&replacement, b"changed after public receipt").unwrap();
    fs::rename(&replacement, &destination).unwrap();
    harness.state_mut().open_project(&fixture.config);
    fs::write(&release, b"").unwrap();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some() && config_helper_unlocked(&state_dir)
    }));
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("Check previous configuration outcome")
        .click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|message| message.contains("not confirmed"))
        })
    }));
    assert_eq!(
        harness
            .state()
            .project()
            .map(|project| project.config_path.clone()),
        Some(fixture.config.clone())
    );
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| {
                result
                    .as_ref()
                    .is_err_and(|message| message.contains("not confirmed"))
            })
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"changed after public receipt"
    );
    assert!(state_dir.join("setup-config-intent.json").exists());
}

#[test]
#[allow(clippy::too_many_lines)]
fn guided_campaign_binds_the_live_diagnosis_and_refuses_records_inside_the_repository() {
    let fixture = Fixture::new("setup-campaign", 10);
    let workspace = fixture.root.join("guided");
    fs::create_dir_all(&workspace).unwrap();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 2200.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness.get_by_label_contains("Bound from the live diagnosis");
    // Authorization record inside the repository is refused.
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = fixture
            .target
            .join("operator-authorization.txt")
            .display()
            .to_string();
        setup.authorization_text = "I authorize this campaign.".to_owned();
    }
    harness.state_mut().apply_authorization_record();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|text| text.contains("authorization write not confirmed"))
        })
    }));
    harness.get_by_label_contains("Inspect the destination before retrying");
    assert!(!fixture.target.join("operator-authorization.txt").exists());
    // Outside the repository it is written with the exact bytes.
    let record = workspace.join("operator-authorization.txt");
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = record.display().to_string();
        setup.authorization_text = "I authorize this campaign.".to_owned();
    }
    harness.state_mut().apply_authorization_record();
    let generation = harness.state().generation();
    let mut stale_binding = harness.state().binding();
    stale_binding.repository_id = Some("another-repository".to_owned());
    assert!(!harness.state_mut().handle_response(Response {
        generation,
        binding: stale_binding,
        label: "setup-write-authorization",
        request_id: Some("obsolete-request".to_owned()),
        result: Ok(Vec::new()),
    }));
    harness.state_mut().apply_authorization_record();
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| result
                .as_ref()
                .is_err_and(|text| text.contains("already pending")))
    );
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|text| text.contains("authorization record written"))
        })
    }));
    harness.run_steps(2);
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        "I authorize this campaign."
    );
    harness.get_by_label_contains("authorization record written");
    let fake = fixture.executable("provider", "#!/bin/sh\nexit 0\n");
    harness.state_mut().start_campaign_form();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .campaign_form
            .as_mut()
            .unwrap();
        form.authorization_path = record.display().to_string();
        form.allowed_paths = "src".to_owned();
        form.campaign_id = "guided-campaign".to_owned();
        for provider in [
            &mut form.team_lead,
            &mut form.implementer,
            &mut form.reviewer,
        ] {
            *provider = ProviderForm {
                executable: fake.display().to_string(),
                model: "fixture".to_owned(),
                effort: "high".to_owned(),
            };
        }
    }
    harness.state_mut().apply_campaign_form();
    harness.run_steps(2);
    harness
        .get_by_label("Show command: select written campaign")
        .click();
    harness.run_steps(2);
    harness.get_by_label_contains("codingmage campaign-select --campaign");
    assert!(
        settle(&mut harness, Duration::from_secs(30), |app| {
            app.setup_state().message.as_ref().is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|message| message.contains("campaign specification written"))
                    || result.as_ref().is_err()
            })
        }),
        "campaign response did not settle"
    );
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(Result::is_ok),
        "campaign setup failed: {:?}",
        harness.state().setup_state().message
    );
    harness.run_steps(2);
    harness.get_by_label_contains("campaign specification written and verified");
    let spec = CampaignSpec::load(&workspace.join("campaign.toml")).unwrap();
    let diagnosis = harness.state().diagnosis().value.clone().unwrap();
    assert_eq!(spec.repository_id, diagnosis.repository_id);
    assert_eq!(spec.initial_commit, diagnosis.head);
    assert_eq!(spec.task_source_sha256, diagnosis.task_source_sha256);
    assert_eq!(spec.max_parallel_pods, 1);
    assert!(spec.multi_agent.is_none());
    assert_eq!(
        harness
            .state()
            .campaign()
            .map(|campaign| campaign.spec.campaign_id.clone()),
        Some("guided-campaign".to_owned())
    );
    // Export refuses the repository and overwrite without consent.
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.export_path = fixture.target.join("copy.toml").display().to_string();
    }
    let source = workspace.join("campaign.toml");
    harness.state_mut().export_document(&source);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|text| text.contains("not confirmed"))
        })
    }));
    assert!(!fixture.target.join("copy.toml").exists());
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness
        .get_by_label("I inspected the destination; clear export notice")
        .click();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|text| text.contains("notice cleared"))
        })
    }));
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.export_path = workspace.join("copy.toml").display().to_string();
    }
    harness.state_mut().export_document(&source);
    harness.state_mut().export_document(&source);
    assert!(
        harness
            .state()
            .setup_state()
            .message
            .as_ref()
            .is_some_and(|result| result
                .as_ref()
                .is_err_and(|text| text.contains("already pending")))
    );
    assert!(
        settle(&mut harness, Duration::from_secs(30), |app| {
            app.setup_state().message.as_ref().is_some_and(|result| {
                result
                    .as_ref()
                    .is_ok_and(|text| text.contains("exported and verified"))
            })
        }),
        "export response: {:?}",
        harness.state().setup_state().message
    );
    harness.state_mut().export_document(&source);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.setup_state().message.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|text| text.contains("export not confirmed"))
        })
    }));
    assert_eq!(
        fs::read(workspace.join("copy.toml")).unwrap(),
        fs::read(&source).unwrap()
    );
    let exported = fs::read_to_string(workspace.join("copy.toml")).unwrap();
    assert!(!exported.contains("token") && !exported.contains("password"));
}

#[test]
fn campaign_form_without_a_live_diagnosis_is_refused() {
    let fixture = Fixture::new("setup-nodiag", 3);
    let fake = fixture.executable(
        "codingmage",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = 'project-open' ]; then exec {} \"$@\"; fi\necho codingmage.cli.repository >&2\nexit 1\n",
            format_command(&coordinator_binary(), &[]).unwrap()
        ),
    );
    let binary = CoordinatorBinary::at(&fake);
    let mut harness = harness(binary, [1100.0, 800.0]);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().last_error.is_some()
    }));
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness.get_by_label_contains("A live repository diagnosis is required");
    harness.state_mut().start_campaign_form();
    harness.run_steps(2);
    harness
        .get_by_label("Verify and write campaign specification")
        .click();
    harness.run_steps(2);
    assert!(harness.state().setup_state().message.is_none());
    harness.state_mut().apply_campaign_form();
    harness.run_steps(2);
    harness.get_by_label_contains("a live repository diagnosis is required");
}
