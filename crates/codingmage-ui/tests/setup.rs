//! Guided setup over the existing configuration and campaign contracts.

mod common;

use std::{
    fs, thread,
    time::{Duration, Instant},
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::load_config;
use codingmage_ui::{
    Screen,
    backend::{CoordinatorBinary, Response},
    campaign::SelectError,
    command::format_command,
    setup::ProviderForm,
};
use common::{Fixture, coordinator_binary, harness, settle, tree_digest};
use egui_kittest::kittest::Queryable as _;

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
                .is_err_and(|message| message.contains("resulting campaign could not be selected"))
        })
    }));
    assert!(harness.state().campaign().is_none());
    assert!(matches!(
        harness.state().campaign_error(),
        Some(SelectError::ReceiptMismatch)
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
                .is_err_and(|message| message.contains("campaign write not confirmed"))
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
    let mut harness = harness(binary, [1100.0, 800.0]);
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
fn guided_configuration_is_validated_by_the_existing_loader_and_opened() {
    let fixture = Fixture::new("setup-config", 3);
    let workspace = fixture.root.join("guided");
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 800.0]);
    harness.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    let target = fixture.target.clone();
    harness.state_mut().start_config_form(&target);
    harness.state_mut().select_screen(Screen::Setup);
    harness.run_steps(2);
    harness.get_by_label_contains("Repository configuration (version 1, deny-first)");
    harness.get_by_label("local only");
    let before = tree_digest(&fixture.target);
    harness.state_mut().apply_config_form();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
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
    harness.get_by_label_contains("configuration written and validated");
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
    harness.run_steps(2);
    harness.get_by_label_contains("policies conflict");
    assert_eq!(load_config(&written).unwrap(), config);
    // Overwrite is refused unless requested.
    harness.state_mut().start_config_form(&target);
    harness.state_mut().apply_config_form();
    harness.run_steps(2);
    harness.get_by_label_contains("already exists");
}

#[test]
#[allow(clippy::too_many_lines)]
fn guided_campaign_binds_the_live_diagnosis_and_refuses_records_inside_the_repository() {
    let fixture = Fixture::new("setup-campaign", 10);
    let workspace = fixture.root.join("guided");
    fs::create_dir_all(&workspace).unwrap();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 800.0]);
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
    harness.run_steps(2);
    harness.get_by_label_contains("is inside the target repository");
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.export_path = workspace.join("copy.toml").display().to_string();
    }
    harness.state_mut().export_document(&source);
    harness.state_mut().export_document(&source);
    harness.run_steps(2);
    harness.get_by_label_contains("already exists");
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
        "#!/bin/sh\necho codingmage.cli.repository >&2\nexit 1\n",
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
