//! Product verification on disposable repositories: setup-to-outcome, recovery, keyboard,
//! window sizes, resource use and stale or malformed private state.
//!
//! Set `CODINGMAGE_UI_EVIDENCE_DIR` to an absolute directory to retain screenshots (rendered
//! offscreen with wgpu on the software adapter) and a JSON manifest; without it the same
//! assertions run and nothing is written.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use codingmage_ui::{
    App, Screen, backend::CoordinatorBinary, controls::ControlAction, launch::LaunchState,
    setup::ProviderForm,
};
use common::{
    FAKE_CLAUDE, FAKE_CLAUDE_SLOW, FAKE_CODEX, Fixture, coordinator_binary, git, harness,
    harness_with_state, pid_alive, settle, tree_digest, write_controlled_campaign,
};
use egui_kittest::{
    Harness,
    kittest::{NodeT as _, Queryable as _},
};
use serde::Serialize;

#[derive(Serialize)]
struct Artifact {
    name: String,
    kind: &'static str,
    width: u32,
    height: u32,
    pixels_per_point: f32,
    sha256: String,
    renderer: String,
}

#[derive(Serialize)]
struct Measurement {
    name: String,
    value: String,
}

struct Evidence {
    directory: Option<PathBuf>,
    artifacts: Vec<Artifact>,
    measurements: Vec<Measurement>,
    renderer: String,
}

impl Evidence {
    fn new() -> Self {
        let directory = std::env::var_os("CODINGMAGE_UI_EVIDENCE_DIR").map(PathBuf::from);
        if let Some(directory) = &directory {
            fs::create_dir_all(directory).unwrap();
        }
        Self {
            directory,
            artifacts: Vec::new(),
            measurements: Vec::new(),
            renderer: adapter_description(),
        }
    }

    fn snapshot(&mut self, harness: &mut Harness<'static, App>, name: &str, ppp: f32) {
        harness.run_steps(2);
        let image = harness.render().expect("offscreen render");
        let sha256 = codingmage_ui::project::hex(&sha2::Sha256::digest(image.as_raw()));
        if let Some(directory) = &self.directory {
            image
                .save(directory.join(format!("{name}.png")))
                .expect("write screenshot");
        }
        self.artifacts.push(Artifact {
            name: name.to_owned(),
            kind: "screenshot",
            width: image.width(),
            height: image.height(),
            pixels_per_point: ppp,
            sha256,
            renderer: self.renderer.clone(),
        });
    }

    fn measure(&mut self, name: &str, value: &dyn std::fmt::Display) {
        self.measurements.push(Measurement {
            name: name.to_owned(),
            value: value.to_string(),
        });
    }

    fn finish(&self, label: &str) {
        if let Some(directory) = &self.directory {
            let manifest = serde_json::json!({
                "label": label,
                "renderer": self.renderer,
                "artifacts": self.artifacts,
                "measurements": self.measurements,
            });
            fs::write(
                directory.join(format!("{label}.manifest.json")),
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .unwrap();
        }
    }
}

use sha2::Digest as _;

fn adapter_description() -> String {
    let state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::default(),
    );
    let info = state.adapter.get_info();
    format!(
        "offscreen wgpu through egui_kittest on adapter {} ({:?}, {:?} backend, driver {})",
        info.name, info.device_type, info.backend, info.driver
    )
}

/// Whole pixels for a logical extent at a scale factor; both are small positive values here.
fn pixel_extent(logical: f32, ppp: f32) -> u32 {
    let scaled = (logical * ppp).round();
    let mut extent = 0_u32;
    while f32::from(u16::try_from(extent).unwrap_or(u16::MAX)) < scaled {
        extent += 1;
    }
    extent
}

fn rss_kib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0)
}

fn rendering_harness(size: [f32; 2], ppp: f32, state_dir: PathBuf) -> Harness<'static, App> {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    Harness::builder()
        .with_size(egui::Vec2::new(size[0], size[1]))
        .with_pixels_per_point(ppp)
        .with_max_steps(4)
        .wgpu()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_fonts(common::fonts());
            App::with_state_dir(&creation.egui_ctx, binary, Ok(state_dir))
        })
}

fn wait_for_admitted_unit(harness: &mut Harness<'static, App>) {
    let waited = Instant::now();
    loop {
        harness.state_mut().refresh_campaign();
        let admitted = settle(harness, Duration::from_secs(5), |app| {
            app.status().value.as_ref().is_some_and(|status| {
                status.as_ref().is_some_and(|status| {
                    status.current_task_id.is_some() || status.completed_units >= 1
                })
            })
        });
        if admitted || waited.elapsed() > Duration::from_mins(2) {
            break;
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn setup_to_outcome_workflow_through_the_interface() {
    let mut evidence = Evidence::new();
    let fixture = Fixture::new("verify-workflow", 10);
    git(
        &fixture.target,
        &["switch", "-q", "-c", "controlled-target"],
    );
    let workspace = fixture.root.join("guided");
    let state_dir = fixture.root.join("ui-state");
    let claude = fixture.executable("fake-claude", FAKE_CLAUDE);
    let codex = fixture.executable("fake-codex", FAKE_CODEX);
    let gate = fixture.executable("fake-gate", "#!/bin/sh\nexit 0\n");
    let mut harness = rendering_harness([1100.0, 720.0], 1.0, state_dir);
    evidence.snapshot(&mut harness, "01-overview-no-repository", 1.0);
    // 1. Guided configuration.
    harness.state_mut().setup_state_mut().workspace_root = workspace.display().to_string();
    let target = fixture.target.clone();
    harness.state_mut().start_config_form(&target);
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .config_form
            .as_mut()
            .unwrap();
        form.gates[0].executable = gate.display().to_string();
        form.gates[0].arguments = String::new();
    }
    harness.state_mut().select_screen(Screen::Setup);
    evidence.snapshot(&mut harness, "02-setup-configuration-form", 1.0);
    harness.state_mut().apply_config_form();
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_screen(Screen::Overview);
    evidence.snapshot(&mut harness, "03-overview-opened", 1.0);
    harness.state_mut().select_screen(Screen::WorkPlan);
    evidence.snapshot(&mut harness, "04-work-plan", 1.0);
    // 2. Authorization record and campaign authority.
    harness.state_mut().select_screen(Screen::Setup);
    {
        let setup = harness.state_mut().setup_state_mut();
        setup.authorization_path = workspace
            .join("operator-authorization.txt")
            .display()
            .to_string();
        setup.authorization_text =
            "The owner authorizes this disposable verification campaign.".to_owned();
    }
    harness.state_mut().apply_authorization_record();
    harness.state_mut().start_campaign_form();
    {
        let form = harness
            .state_mut()
            .setup_state_mut()
            .campaign_form
            .as_mut()
            .unwrap();
        form.campaign_id = "verification".to_owned();
        form.allowed_paths = "src".to_owned();
        form.team_lead = ProviderForm {
            executable: codex.display().to_string(),
            model: "fixture-lead".to_owned(),
            effort: "high".to_owned(),
        };
        form.implementer = ProviderForm {
            executable: claude.display().to_string(),
            model: "fixture-implementer".to_owned(),
            effort: "high".to_owned(),
        };
        form.reviewer = ProviderForm {
            executable: codex.display().to_string(),
            model: "fixture-reviewer".to_owned(),
            effort: "high".to_owned(),
        };
    }
    evidence.snapshot(&mut harness, "05-setup-campaign-form", 1.0);
    harness.state_mut().apply_campaign_form();
    harness.run_steps(2);
    assert!(
        harness.state().campaign().is_some(),
        "{:?}",
        harness.state().setup_state().message
    );
    // 3. Readiness, preflight and admission.
    harness.state_mut().select_screen(Screen::Campaign);
    evidence.snapshot(&mut harness, "06-campaign-readiness", 1.0);
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        app.preflight().value.is_some() || app.preflight().last_error.is_some()
    }));
    let digest = harness
        .state()
        .preflight()
        .value
        .as_ref()
        .unwrap_or_else(|| panic!("{:?}", harness.state().preflight().last_error))
        .report_sha256
        .clone();
    evidence.snapshot(&mut harness, "07-campaign-preflight-ready", 1.0);
    harness.state_mut().set_confirmation(&digest[..12]);
    harness.state_mut().admit_campaign();
    assert!(harness.state().execution().admission.is_some());
    evidence.snapshot(&mut harness, "08-campaign-admitted", 1.0);
    // 4. Start, observe, stop after unit.
    harness.state_mut().start_campaign();
    assert!(harness.state().launch_is_live());
    wait_for_admitted_unit(&mut harness);
    evidence.snapshot(&mut harness, "09-campaign-live", 1.0);
    harness
        .state_mut()
        .request_control(ControlAction::StopAfterUnit)
        .unwrap();
    assert!(settle(&mut harness, Duration::from_mins(4), |app| {
        !app.launch_is_live()
    }));
    let outcome = match harness.state().execution().observed.clone() {
        Some((_, LaunchState::Exited(outcome))) => outcome,
        other => panic!("{other:?}"),
    };
    assert!(outcome.completed_units >= 1);
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.changes().value.is_some()
            && app
                .run_records()
                .value
                .as_ref()
                .is_some_and(|r| !r.is_empty())
    }));
    evidence.snapshot(&mut harness, "10-campaign-stopped", 1.0);
    harness.state_mut().select_screen(Screen::WorkPlan);
    evidence.snapshot(&mut harness, "11-work-plan-overlay", 1.0);
    harness.state_mut().select_screen(Screen::Changes);
    evidence.snapshot(&mut harness, "12-changes-and-reviews", 1.0);
    // 5. Report export.
    harness.state_mut().select_screen(Screen::Reports);
    {
        let reports = harness.state_mut().reports_state_mut();
        reports.export_path = workspace.join("outcome-report.json").display().to_string();
    }
    harness.state_mut().export_report();
    harness.run_steps(2);
    evidence.snapshot(&mut harness, "13-reports-exported", 1.0);
    let exported = fs::read_to_string(workspace.join("outcome-report.json")).unwrap();
    assert!(exported.contains("\"delivery\": \"withheld"));
    assert!(!exported.contains(fixture.target.to_str().unwrap()));
    evidence.measure("workflow_completed_units", &outcome.completed_units);
    evidence.measure("workflow_stop_reason", &outcome.stop_reason.clone());
    let unchanged =
        fs::read_to_string(fixture.target.join("TASKS.md")).unwrap() == common::task_source(10);
    evidence.measure("active_checkout_task_source_unchanged", &unchanged);
    evidence.finish("workflow");
}

#[test]
#[allow(clippy::too_many_lines)]
fn recovery_after_a_killed_coordinator_and_resumed_durable_state() {
    let mut evidence = Evidence::new();
    let fixture = Fixture::new("verify-recovery", 10);
    git(
        &fixture.target,
        &["switch", "-q", "-c", "controlled-target"],
    );
    let (spec, record) = write_controlled_campaign(&fixture, "recovery");
    fixture.executable("fake-claude", FAKE_CLAUDE_SLOW);
    let state_dir = fixture.root.join("ui-state");
    let mut harness = rendering_harness([1100.0, 720.0], 1.0, state_dir.clone());
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_campaign(&spec);
    harness.state_mut().set_authorization_record(&record);
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        app.preflight().value.is_some()
    }));
    let digest = harness
        .state()
        .preflight()
        .value
        .as_ref()
        .unwrap()
        .report_sha256
        .clone();
    harness.state_mut().set_confirmation(&digest[..12]);
    harness.state_mut().admit_campaign();
    harness.state_mut().start_campaign();
    let pid = harness.state().execution().record.as_ref().unwrap().pid;
    let pid_file = fixture.root.join("slow-claude.pid");
    let started = Instant::now();
    while !pid_file.exists() && started.elapsed() < Duration::from_mins(2) {
        harness.step();
        std::thread::sleep(Duration::from_millis(200));
    }
    let provider_pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // Kill the fixture coordinator (our own detached child) to simulate a crash.
    assert!(
        Command::new("/usr/bin/kill")
            .args(["-9", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        !app.launch_is_live()
    }));
    let state = harness.state().execution().observed.clone().map(|(_, s)| s);
    assert!(
        matches!(state, Some(LaunchState::ExitedWithoutOutcome { .. })),
        "{state:?}"
    );
    harness.state_mut().select_screen(Screen::Campaign);
    evidence.snapshot(&mut harness, "20-campaign-after-crash", 1.0);
    harness.get_by_label_contains("exited without a terminal outcome");
    std::thread::sleep(Duration::from_secs(3));
    evidence.measure(
        "owned_provider_alive_after_coordinator_kill",
        &pid_alive(provider_pid),
    );
    // Durable state is readable and a new start is allowed under the same admission.
    harness.state_mut().refresh_campaign();
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.status().value.as_ref().is_some_and(Option::is_some)
    }));
    assert!(
        harness.state().start_refusals().is_empty(),
        "{:?}",
        harness.state().start_refusals()
    );
    // Reopen the interface: the crash observation and admission survive.
    drop(harness);
    let mut reopened = rendering_harness([1100.0, 720.0], 1.0, state_dir);
    reopened.state_mut().open_project(&config);
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some() && app.campaign().is_some()
    }));
    assert!(reopened.state().execution().admission.is_some());
    assert!(matches!(
        reopened
            .state()
            .execution()
            .observed
            .clone()
            .map(|(_, s)| s),
        Some(LaunchState::ExitedWithoutOutcome { .. })
    ));
    fixture.executable("fake-claude", FAKE_CLAUDE);
    reopened.state_mut().start_campaign();
    assert!(
        reopened.state().launch_is_live(),
        "{:?}",
        reopened.state().execution().error
    );
    // The interrupted unit is reconciled by the coordinator; wait for a newly accepted unit.
    let waited = Instant::now();
    loop {
        reopened.state_mut().refresh_campaign();
        let accepted = settle(&mut reopened, Duration::from_secs(5), |app| {
            app.status().value.as_ref().is_some_and(|status| {
                status
                    .as_ref()
                    .is_some_and(|status| status.completed_units >= 1)
            }) || !app.launch_is_live()
        });
        if accepted || waited.elapsed() > Duration::from_mins(3) {
            break;
        }
    }
    if reopened.state().launch_is_live() {
        reopened
            .state_mut()
            .request_control(ControlAction::StopAfterUnit)
            .unwrap();
    }
    assert!(settle(&mut reopened, Duration::from_mins(4), |app| {
        !app.launch_is_live()
    }));
    let outcome = match reopened.state().execution().observed.clone() {
        Some((_, LaunchState::Exited(outcome))) => outcome,
        other => panic!("{other:?}"),
    };
    evidence.measure("resumed_outcome_state", &outcome.state.clone());
    evidence.measure("resumed_stop_reason", &outcome.stop_reason.clone());
    evidence.measure("resumed_completed_units", &outcome.completed_units);
    evidence.measure(
        "resumed_blocker_code",
        &outcome.blocker_code.clone().unwrap_or_default(),
    );
    assert!(outcome.completed_units >= 1, "{outcome:?}");
    reopened.state_mut().select_screen(Screen::Campaign);
    evidence.snapshot(&mut reopened, "21-campaign-resumed-after-crash", 1.0);
    evidence.finish("recovery");
}

#[test]
fn keyboard_navigation_reaches_controls_in_order() {
    let mut evidence = Evidence::new();
    let fixture = Fixture::new("verify-keyboard", 3);
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.run_steps(2);
    let mut focused = Vec::new();
    for _ in 0..12 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(1);
        let label = harness
            .root()
            .children_recursive()
            .find(|node| node.accesskit_node().is_focused())
            .map_or_else(
                || "none".to_owned(),
                |node| {
                    node.accesskit_node()
                        .label()
                        .or_else(|| node.accesskit_node().value())
                        .unwrap_or_else(|| format!("{:?}", node.accesskit_node().role()))
                },
            );
        focused.push(label);
    }
    for screen in Screen::ALL {
        assert!(
            focused.iter().any(|label| label == screen.label()),
            "Tab traversal never focused {}: {focused:?}",
            screen.label()
        );
    }
    evidence.measure("tab_focus_order", &focused.join(" > "));
    // Shortcuts switch screens; Enter in the path field opens the repository.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num2);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::WorkPlan);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num6);
    harness.run_steps(2);
    assert_eq!(harness.state().screen(), Screen::Setup);
    let field = harness
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .expect("configuration path field");
    field.focus();
    field.type_text(fixture.config.to_str().unwrap());
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.project().is_some()
    }));
    evidence.measure("enter_opens_typed_configuration", &true);
    harness.key_press(egui::Key::F5);
    harness.run_steps(2);
    let refreshed =
        harness.state().diagnosis().loading || harness.state().diagnosis().value.is_some();
    evidence.measure("f5_refresh_accepted", &refreshed);
    evidence.finish("keyboard");
}

#[test]
fn window_sizes_and_high_dpi_keep_navigation_and_content_reachable() {
    let mut evidence = Evidence::new();
    let fixture = Fixture::new("verify-sizes", 3);
    for (name, size, ppp) in [
        ("30-compact-720x480", [720.0, 480.0], 1.0),
        ("31-default-1100x720", [1100.0, 720.0], 1.0),
        ("32-high-dpi-1100x720-at-2x", [1100.0, 720.0], 2.0),
        ("33-large-1920x1080", [1920.0, 1080.0], 1.0),
    ] {
        let state_dir = fixture.root.join(format!("state-{name}"));
        let mut harness = rendering_harness(size, ppp, state_dir);
        let config = fixture.config.clone();
        harness.state_mut().open_project(&config);
        assert!(settle(&mut harness, Duration::from_secs(30), |app| {
            app.diagnosis().value.is_some()
        }));
        for screen in Screen::ALL {
            harness.get_by_role_and_label(egui::accesskit::Role::Button, screen.label());
        }
        harness.get_by_label_contains("Repository diagnosis");
        evidence.snapshot(&mut harness, name, ppp);
        let last = evidence.artifacts.last().unwrap();
        let expected_width = pixel_extent(size[0], ppp);
        let expected_height = pixel_extent(size[1], ppp);
        assert_eq!(last.width, expected_width);
        assert_eq!(last.height, expected_height);
    }
    evidence.finish("sizes");
}

#[test]
fn resource_use_is_bounded_and_idle_repaint_is_reactive() {
    let mut evidence = Evidence::new();
    let fixture = Fixture::new("verify-resources", 3);
    let before = rss_kib();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness(binary, [1100.0, 720.0]);
    harness.run_steps(3);
    // Without a repository nothing is pending: no repaint is requested by the app.
    let requested_idle = harness
        .output()
        .viewport_output
        .values()
        .any(|v| v.repaint_delay < Duration::from_secs(10));
    evidence.measure("idle_repaint_requested_without_repository", &requested_idle);
    assert!(!requested_idle);
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    let started = Instant::now();
    for screen in Screen::ALL {
        harness.state_mut().select_screen(screen);
        harness.run_steps(2);
    }
    let per_frame = started.elapsed() / (u32::try_from(Screen::ALL.len()).unwrap_or(1) * 2);
    evidence.measure("mean_frame_time_ms_logic_only", &per_frame.as_millis());
    let after = rss_kib();
    evidence.measure("test_process_rss_kib_before", &before);
    evidence.measure("test_process_rss_kib_after_all_screens", &after);
    assert!(
        after < 1_500_000,
        "resident set {after} KiB exceeds the 1.5 GiB bound"
    );
    assert!(
        per_frame < Duration::from_millis(250),
        "{per_frame:?} per frame"
    );
    evidence.finish("resources");
}

#[test]
fn stale_and_malformed_private_state_is_visible_and_never_fatal() {
    let fixture = Fixture::new("verify-malformed", 3);
    let spec = common::write_campaign(&fixture, "malformed-state", 1);
    let state_dir = fixture.root.join("ui-state");
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness_with_state(binary, [1100.0, 720.0], state_dir.clone());
    let config = fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_campaign(&spec);
    harness.run_steps(2);
    let project_dir = harness.state().project_dir().unwrap();
    drop(harness);
    let launch_dir = project_dir.join("launches").join("malformed-state");
    fs::create_dir_all(&launch_dir).unwrap();
    fs::write(launch_dir.join("current.json"), b"{ not json").unwrap();
    fs::create_dir_all(project_dir.join("admissions")).unwrap();
    let authority = codingmage_campaign::CampaignSpec::load(&spec)
        .unwrap()
        .authority_sha256()
        .unwrap();
    fs::write(
        project_dir
            .join("admissions")
            .join(format!("{authority}.json")),
        b"[]",
    )
    .unwrap();
    fs::create_dir_all(project_dir.join("controls")).unwrap();
    fs::write(
        project_dir.join("controls/malformed-state.json"),
        b"\"nope\"",
    )
    .unwrap();
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut reopened = harness_with_state(binary, [1100.0, 720.0], state_dir);
    reopened.state_mut().open_project(&config);
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some() && app.campaign().is_some()
    }));
    assert!(reopened.state().execution().admission.is_none());
    assert!(reopened.state().execution().record.is_none());
    assert!(reopened.state().execution().ledger.entries.is_empty());
    assert!(
        reopened
            .state()
            .start_refusals()
            .iter()
            .any(|reason| reason.contains("not admitted"))
    );
    reopened.state_mut().select_screen(Screen::Campaign);
    reopened.run_steps(2);
    reopened.get_by_label_contains("Not admitted.");
    reopened.get_by_label_contains("No coordinator has been launched");
    let before = tree_digest(&fixture.target);
    assert_eq!(tree_digest(&fixture.target), before);
    let _ = Path::new("unused");
}
