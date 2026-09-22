//! Explicit admission, detached coordinator ownership, controls, detach, reconnect and recovery.

mod common;

use std::{fs, time::Duration};

use codingmage_ui::{
    admission::Staleness,
    backend::CoordinatorBinary,
    controls::{ControlAction, ControlRefusal},
    launch::LaunchState,
};
use common::{
    FAKE_CLAUDE_SLOW, Fixture, coordinator_binary, git, harness_with_state, pid_alive, settle,
    write_controlled_campaign,
};

struct Ready {
    fixture: Fixture,
    spec: std::path::PathBuf,
    record: std::path::PathBuf,
    state_home: std::path::PathBuf,
}

fn ready(label: &str) -> Ready {
    let fixture = Fixture::new(label, 10);
    git(
        &fixture.target,
        &["switch", "-q", "-c", "controlled-target"],
    );
    let (spec, record) = write_controlled_campaign(&fixture, &format!("{label}-campaign"));
    let state_home = fixture.root.join("ui-state");
    Ready {
        fixture,
        spec,
        record,
        state_home,
    }
}

fn open(ready: &Ready) -> egui_kittest::Harness<'static, codingmage_ui::App> {
    let binary = CoordinatorBinary::at(&coordinator_binary());
    let mut harness = harness_with_state(binary, [1100.0, 900.0], ready.state_home.clone());
    let config = ready.fixture.config.clone();
    harness.state_mut().open_project(&config);
    assert!(settle(&mut harness, Duration::from_secs(30), |app| {
        app.diagnosis().value.is_some()
    }));
    harness.state_mut().select_campaign(&ready.spec);
    harness.state_mut().set_authorization_record(&ready.record);
    harness.run_steps(2);
    harness
}

fn admit(harness: &mut egui_kittest::Harness<'static, codingmage_ui::App>) {
    harness.state_mut().run_preflight();
    assert!(settle(harness, Duration::from_mins(3), |app| {
        app.preflight().value.is_some() || app.preflight().last_error.is_some()
    }));
    let digest = harness
        .state()
        .preflight()
        .value
        .as_ref()
        .unwrap_or_else(|| {
            panic!(
                "preflight failed: {:?}",
                harness.state().preflight().last_error
            )
        })
        .report_sha256
        .clone();
    harness.state_mut().set_confirmation(&digest[..12]);
    harness.state_mut().admit_campaign();
    assert!(
        harness.state().execution().admission.is_some(),
        "{:?}",
        harness.state().execution().error
    );
}

#[test]
fn admission_requires_the_reviewed_report_and_goes_stale_when_the_repository_moves() {
    let ready = ready("admit");
    let mut harness = open(&ready);
    harness.state_mut().set_confirmation("000000000000");
    harness.state_mut().admit_campaign();
    assert_eq!(
        harness.state().execution().error.as_deref(),
        Some("run preflight and review its report first")
    );
    assert!(!harness.state().start_refusals().is_empty());
    harness.state_mut().run_preflight();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        app.preflight().value.is_some()
    }));
    harness.state_mut().set_confirmation("000000000000");
    harness.state_mut().admit_campaign();
    assert!(
        harness
            .state()
            .execution()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("exactly as shown"))
    );
    assert!(harness.state().execution().admission.is_none());
    admit(&mut harness);
    assert!(harness.state().start_refusals().is_empty());
    // The admission survives reopening and goes stale when the checkout moves.
    drop(harness);
    let mut reopened = open(&ready);
    assert!(reopened.state().execution().admission.is_some());
    assert_eq!(reopened.state().admission_staleness(), None);
    fs::write(ready.fixture.target.join("note.txt"), b"moved\n").unwrap();
    git(&ready.fixture.target, &["add", "note.txt"]);
    git(&ready.fixture.target, &["commit", "-q", "-m", "move head"]);
    let moved_head = ready.fixture.head();
    reopened.state_mut().refresh_diagnosis();
    assert!(settle(&mut reopened, Duration::from_secs(30), |app| {
        app.diagnosis()
            .value
            .as_ref()
            .is_some_and(|diagnosis| diagnosis.head == moved_head)
    }));
    assert_eq!(
        reopened.state().admission_staleness(),
        Some(Staleness::Head)
    );
    assert!(
        reopened
            .state()
            .start_refusals()
            .iter()
            .any(|r| r.contains("stale"))
    );
    reopened.state_mut().start_campaign();
    assert!(reopened.state().execution().record.is_none());
    assert!(!ready.fixture.state.join("campaigns").exists());
}

#[test]
#[allow(clippy::too_many_lines)]
fn start_detach_reconnect_stop_after_unit_and_replayed_controls() {
    let ready = ready("run");
    let mut harness = open(&ready);
    admit(&mut harness);
    harness.state_mut().start_campaign();
    let pid = harness
        .state()
        .execution()
        .record
        .as_ref()
        .expect("launch record")
        .pid;
    assert!(harness.state().launch_is_live());
    assert!(
        harness
            .state()
            .start_refusals()
            .iter()
            .any(|r| r.contains("still running"))
    );
    // Detach: closing the interface leaves the coordinator running.
    drop(harness);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        pid_alive(pid),
        "coordinator must survive the window closing"
    );
    let mut reconnected = open(&ready);
    assert!(
        reconnected.state().launch_is_live(),
        "reconnect must observe the live launch"
    );
    assert_eq!(
        reconnected
            .state()
            .execution()
            .record
            .as_ref()
            .map(|record| record.pid),
        Some(pid)
    );
    // Wait until the coordinator has admitted a unit, so stop-after-unit finishes that unit.
    let waited = std::time::Instant::now();
    loop {
        reconnected.state_mut().refresh_campaign();
        let admitted = settle(&mut reconnected, Duration::from_secs(5), |app| {
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
    let request_id = reconnected
        .state_mut()
        .request_control(ControlAction::StopAfterUnit)
        .unwrap();
    assert!(matches!(
        reconnected
            .state_mut()
            .request_control(ControlAction::Pause),
        Err(ControlRefusal::Pending(_))
    ));
    assert!(settle(&mut reconnected, Duration::from_mins(2), |app| {
        app.execution().ledger.pending().is_none()
    }));
    let entry = reconnected
        .state()
        .execution()
        .ledger
        .entries
        .iter()
        .find(|entry| entry.request_id == request_id)
        .unwrap()
        .clone();
    assert_eq!(
        entry.result.as_ref().and_then(|r| r.code.clone()),
        None,
        "{entry:?}"
    );
    assert!(entry.result.as_ref().is_some_and(|r| r.created));
    assert!(settle(&mut reconnected, Duration::from_mins(4), |app| {
        !app.launch_is_live()
    }));
    let state = reconnected
        .state()
        .execution()
        .observed
        .clone()
        .map(|(_, s)| s);
    match state {
        Some(LaunchState::Exited(outcome)) => {
            assert_eq!(outcome.state, "paused");
            assert_eq!(outcome.stop_reason, "stop_after_unit");
            assert!(outcome.completed_units >= 1, "{outcome:?}");
        }
        other => panic!("unexpected launch state {other:?}"),
    }
    assert!(!pid_alive(pid));
    assert!(settle(&mut reconnected, Duration::from_mins(1), |app| {
        app.status()
            .value
            .as_ref()
            .is_some_and(|s| s.as_ref().is_some_and(|s| s.completed_units >= 1))
    }));
    // A lost outcome is replayed with the same identity and the coordinator reports no new intent.
    let pause = reconnected
        .state_mut()
        .request_control(ControlAction::Pause)
        .unwrap();
    assert!(settle(&mut reconnected, Duration::from_mins(2), |app| {
        app.execution().ledger.pending().is_none()
    }));
    reconnected.state_mut().ledger_mut().finish(
        &pause,
        false,
        Some("codingmage.ui.outcome_unknown".to_owned()),
    );
    let replayed = reconnected
        .state_mut()
        .request_control(ControlAction::Pause)
        .unwrap();
    assert_eq!(replayed, pause);
    assert!(settle(&mut reconnected, Duration::from_mins(2), |app| {
        app.execution().ledger.pending().is_none()
    }));
    let entry = reconnected
        .state()
        .execution()
        .ledger
        .entries
        .iter()
        .find(|entry| entry.request_id == pause)
        .unwrap()
        .clone();
    assert_eq!(entry.attempts, 2);
    assert!(
        entry
            .result
            .as_ref()
            .is_some_and(|r| !r.created && r.code.is_none())
    );
    // Resume writes the intent only; starting again is explicit and allowed for a paused campaign.
    let resume = reconnected
        .state_mut()
        .request_control(ControlAction::Resume)
        .unwrap();
    assert!(settle(&mut reconnected, Duration::from_mins(2), |app| {
        app.execution().ledger.pending().is_none()
    }));
    assert!(resume.starts_with("ui-resume-"));
    assert!(
        reconnected.state().start_refusals().is_empty(),
        "{:?}",
        reconnected.state().start_refusals()
    );
    assert_eq!(
        fs::read_to_string(ready.fixture.target.join("TASKS.md")).unwrap(),
        common::task_source(10),
        "the active checkout's task source must be untouched"
    );
}

#[test]
fn cancel_terminates_only_the_owned_provider_and_retains_state() {
    let ready = ready("cancel");
    ready.fixture.executable("fake-claude", FAKE_CLAUDE_SLOW);
    let mut harness = open(&ready);
    admit(&mut harness);
    harness.state_mut().start_campaign();
    let coordinator_pid = harness.state().execution().record.as_ref().unwrap().pid;
    let pid_file = ready.fixture.root.join("slow-claude.pid");
    let started = std::time::Instant::now();
    while !pid_file.exists() && started.elapsed() < Duration::from_mins(2) {
        harness.step();
        std::thread::sleep(Duration::from_millis(200));
    }
    let provider_pid: u32 = fs::read_to_string(&pid_file)
        .expect("provider started")
        .trim()
        .parse()
        .unwrap();
    assert!(pid_alive(provider_pid));
    // Cancel needs a second confirmation.
    assert_eq!(
        harness.state_mut().request_control(ControlAction::Cancel),
        Err(ControlRefusal::ConfirmCancel)
    );
    let cancel = harness
        .state_mut()
        .request_control(ControlAction::Cancel)
        .unwrap();
    assert!(cancel.starts_with("ui-cancel-"));
    assert!(settle(&mut harness, Duration::from_mins(3), |app| {
        !app.launch_is_live()
    }));
    let state = harness.state().execution().observed.clone().map(|(_, s)| s);
    match state {
        Some(LaunchState::Exited(outcome)) => {
            assert_eq!(outcome.state, "cancelled");
            assert_eq!(outcome.stop_reason, "operator_cancellation");
        }
        other => panic!("unexpected launch state {other:?}"),
    }
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !pid_alive(provider_pid),
        "owned provider must be terminated"
    );
    assert!(!pid_alive(coordinator_pid));
    assert!(
        std::process::id() > 0 && pid_alive(std::process::id()),
        "the interface itself is unaffected"
    );
    assert!(
        ready.fixture.state.join("campaigns").exists(),
        "durable state is retained"
    );
    assert!(settle(&mut harness, Duration::from_mins(1), |app| {
        app.status()
            .value
            .as_ref()
            .is_some_and(|s| s.as_ref().is_some_and(|s| s.state == "cancelled"))
    }));
    assert!(
        harness
            .state()
            .start_refusals()
            .iter()
            .any(|r| r.contains("cancelled"))
    );
    assert_eq!(ready.fixture.head(), ready.fixture.head());
}

#[test]
fn a_second_launch_is_refused_while_the_coordinator_is_live() {
    let ready = ready("double");
    ready.fixture.executable("fake-claude", FAKE_CLAUDE_SLOW);
    let mut harness = open(&ready);
    admit(&mut harness);
    harness.state_mut().start_campaign();
    let first = harness.state().execution().record.as_ref().unwrap().pid;
    harness.state_mut().start_campaign();
    assert_eq!(
        harness.state().execution().record.as_ref().unwrap().pid,
        first
    );
    assert!(
        harness
            .state()
            .execution()
            .error
            .as_deref()
            .is_some_and(|e| e.contains("still running"))
    );
    // A control before the first checkpoint exists is refused by the coordinator with a code,
    // so wait for the checkpoint before cancelling.
    let waited = std::time::Instant::now();
    loop {
        harness.state_mut().refresh_campaign();
        let checkpointed = settle(&mut harness, Duration::from_secs(5), |app| {
            app.status().value.as_ref().is_some_and(Option::is_some)
        });
        if checkpointed || waited.elapsed() > Duration::from_mins(2) {
            break;
        }
    }
    // Clean up: cancel the campaign so no coordinator outlives the test.
    assert_eq!(
        harness.state_mut().request_control(ControlAction::Cancel),
        Err(ControlRefusal::ConfirmCancel)
    );
    harness
        .state_mut()
        .request_control(ControlAction::Cancel)
        .unwrap();
    assert!(settle(&mut harness, Duration::from_mins(3), |app| !app
        .launch_is_live()));
}
