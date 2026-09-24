//! Guided setup over the existing configuration and campaign contracts.

mod common;

use std::{fs, time::Duration};

use codingmage_campaign::CampaignSpec;
use codingmage_core::load_config;
use codingmage_ui::{Screen, backend::CoordinatorBinary, setup::ProviderForm};
use common::{Fixture, coordinator_binary, harness, settle, tree_digest};
use egui_kittest::kittest::Queryable as _;

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
    harness.run_steps(2);
    harness.get_by_label_contains("is inside the target repository");
    assert!(!fixture.target.join("operator-authorization.txt").exists());
    // Outside the repository it is written with the exact bytes.
    let record = workspace.join("operator-authorization.txt");
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = record.display().to_string();
        setup.authorization_text = "I authorize this campaign.".to_owned();
    }
    harness.state_mut().apply_authorization_record();
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
    harness.state_mut().apply_campaign_form();
    harness.run_steps(2);
    harness.get_by_label_contains("a live repository diagnosis is required");
}
