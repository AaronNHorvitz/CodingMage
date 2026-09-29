//! Manual diagnostics stay bound to the selected campaign and never modify its repository.

mod common;

use std::{fs, os::unix::fs::PermissionsExt as _, time::Duration};

use codingmage_ui::{Screen, backend::CoordinatorBinary};
use common::{Fixture, coordinator_binary, harness, settle, tree_digest, write_campaign};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

#[test]
fn manual_support_bundle_uses_exact_command_and_fresh_external_directory() {
    let fixture = Fixture::new("support-ui", 2);
    let spec = write_campaign(&fixture, "support-ui-campaign", 2);
    let before = tree_digest(&fixture.target);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 900.0]);
    harness.state_mut().open_project(&fixture.config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_campaign(&spec);
    let output = fixture.root.join("support-ui-bundle");
    harness.state_mut().support_state_mut().output_path = output.display().to_string();
    harness.state_mut().select_screen(Screen::Help);
    reveal_support_command(&mut harness);
    assert!(!output.exists());
    let mut reached = false;
    for _ in 0..64 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(1);
        let control = harness.get_by_role_and_label(
            egui::accesskit::Role::Button,
            "Create redacted support bundle",
        );
        if control.accesskit_node().is_focused() {
            assert!(control.accesskit_node().has_bounds());
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "keyboard focus never reached the manual diagnostic action"
    );
    harness.state_mut().request_support_bundle();
    assert!(harness.state().support_state().pending());
    harness.state_mut().request_support_bundle();
    assert!(harness.state().support_state().pending());
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.support_state().pending() && app.support_state().message.is_some()
    }));
    let success = harness.state().support_state().message.as_ref().unwrap();
    assert!(success.is_ok(), "{success:?}");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["campaign_id"], "support-ui-campaign");
    assert_eq!(manifest["redacted"], true);
    assert_eq!(manifest["uploaded"], false);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for entry in fs::read_dir(&output).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains(fixture.target.to_str().unwrap()));
        assert!(!text.contains(fixture.config.to_str().unwrap()));
        assert!(!text.contains("fake-claude"));
    }
    assert_eq!(tree_digest(&fixture.target), before);
    harness.state_mut().request_support_bundle();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.support_state().pending() && app.support_state().message.is_some()
    }));
    assert!(
        harness
            .state()
            .support_state()
            .message
            .as_ref()
            .unwrap()
            .is_err()
    );
    assert_eq!(tree_digest(&fixture.target), before);
    let inside = fixture.target.join("support-ui-bundle");
    harness.state_mut().support_state_mut().output_path = inside.display().to_string();
    harness.state_mut().request_support_bundle();
    assert!(
        harness
            .state()
            .support_state()
            .message
            .as_ref()
            .unwrap()
            .is_err()
    );
    assert!(!inside.exists());
    assert_eq!(tree_digest(&fixture.target), before);
}

fn reveal_support_command(harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>) {
    harness.run_steps(2);
    let mut preview_reached = false;
    for _ in 0..128 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(1);
        if harness
            .get_by_label("Show command: Create support bundle")
            .accesskit_node()
            .is_focused()
        {
            preview_reached = true;
            break;
        }
    }
    assert!(
        preview_reached,
        "keyboard focus never reached the command preview"
    );
    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.get_by_label_contains("support-bundle --config");
}
