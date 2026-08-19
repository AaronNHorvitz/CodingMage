//! Stable terminal status, reconnectable events, and exact-run operator controls.

mod control;
mod status;

pub use control::{
    ControlAction, ControlEngine, ControlError, ControlOutcome, ControlRequest, ControlState,
    ReadCommand, ReadRequest, ReadResponse,
};
pub use status::{
    Known, MonitorError, MonitorEvent, MonitorSnapshot, StatusInput, StatusLabel, StatusStream,
    StatusView,
};

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_contracts::{AgentId, AttemptId, RepositoryId, RunId, TaskId};
    use codingmage_state::Journal;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn status(elapsed_ms: u64) -> StatusView {
        StatusInput {
            run_id: RunId::new("run-1").unwrap(),
            target: RepositoryId::new("repo-1").unwrap(),
            task: TaskId::new("task-13.1").unwrap(),
            state: StatusLabel::new("local_verification").unwrap(),
            agent: Some(AgentId::new("codex-review").unwrap()),
            model: Some(StatusLabel::new("gpt-5.6-sol-high").unwrap()),
            branch: Some(StatusLabel::new("codingmage/task-13.1").unwrap()),
            commit: Some(StatusLabel::new("0123456789abcdef").unwrap()),
            command: Some(StatusLabel::new("cargo.test.workspace").unwrap()),
            gate: Some(StatusLabel::new("gate-13.1").unwrap()),
            findings: 2,
            correction_count: 1,
            elapsed_ms,
            pause: None,
            blocker: None,
            usage: Known::Unknown,
            reset_at_ms: Known::Unknown,
        }
        .into()
    }

    fn root(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-monitor-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    fn engine() -> ControlEngine {
        ControlEngine::new(
            1000,
            RepositoryId::new("repo-1").unwrap(),
            RunId::new("run-1").unwrap(),
            TaskId::new("task-13.1").unwrap(),
        )
    }

    #[test]
    fn json_and_terminal_render_unknown_metrics_without_sensitive_content() {
        let status = status(42);
        let json = status.to_json().unwrap();
        assert!(json.contains(r#""usage":{"availability":"unknown"}"#));
        assert!(json.contains(r#""reset_at_ms":{"availability":"unknown"}"#));
        let terminal = status.render_terminal();
        assert!(terminal.contains("usage=unknown reset_at_ms=unknown"));
        assert!(!terminal.contains("prompt"));
        assert!(StatusLabel::new("token=secret value").is_err());
        let unknown = json.trim_end_matches('}').to_owned() + ",\"credential\":\"x\"}";
        assert!(serde_json::from_str::<StatusView>(&unknown).is_err());
    }

    #[test]
    fn attach_disconnect_and_reconnect_are_observational() {
        let mut stream = StatusStream::new(status(0));
        let correlation = AttemptId::new("attempt-1").unwrap();
        let first = stream
            .publish(correlation.clone(), 1_000, status(1_000))
            .unwrap();
        assert!(
            stream
                .publish(correlation.clone(), 1_100, status(1_100))
                .is_none()
        );
        let second = stream.publish(correlation, 1_300, status(1_300)).unwrap();
        assert_eq!(first.sequence, 0);
        assert_eq!(second.sequence, 1);
        let before = stream.attach(None);
        let reconnected = stream.attach(Some(0));
        let after = stream.attach(None);
        assert_eq!(before, after);
        assert_eq!(reconnected.current, status(1_300));
        assert_eq!(reconnected.events, vec![second]);
        assert!(!reconnected.history_gap);
    }

    #[test]
    fn read_commands_have_no_mutation_effect() {
        let engine = engine();
        let initial = engine.state();
        for command in [
            ReadCommand::Status,
            ReadCommand::ExplainBlocker,
            ReadCommand::OpenDiff,
            ReadCommand::OpenLog,
            ReadCommand::Doctor,
        ] {
            engine
                .read(
                    &ReadRequest {
                        requester_uid: 1000,
                        run_id: RunId::new("run-1").unwrap(),
                        command,
                    },
                    &status(0),
                )
                .unwrap();
            assert_eq!(engine.state(), initial);
        }
    }

    #[test]
    fn controls_authenticate_deduplicate_and_survive_restart() {
        let root = root("controls");
        let mut journal = Journal::open(&root, "owner").unwrap();
        let mut engine = engine();
        let pause = ControlRequest {
            request_id: StatusLabel::new("request-pause").unwrap(),
            requester_uid: 1000,
            run_id: RunId::new("run-1").unwrap(),
            action: ControlAction::Pause,
        };
        assert!(engine.apply(&pause, &mut journal, 1).unwrap().changed);
        assert!(!engine.apply(&pause, &mut journal, 2).unwrap().changed);
        let stale = ControlRequest {
            request_id: StatusLabel::new("request-stale").unwrap(),
            requester_uid: 1000,
            run_id: RunId::new("run-other").unwrap(),
            action: ControlAction::Resume,
        };
        assert_eq!(
            engine.apply(&stale, &mut journal, 3).unwrap_err(),
            ControlError::Unauthorized
        );
        let foreign = ControlRequest {
            requester_uid: 1001,
            run_id: RunId::new("run-1").unwrap(),
            ..stale
        };
        assert_eq!(
            engine.apply(&foreign, &mut journal, 4).unwrap_err(),
            ControlError::Unauthorized
        );
        let cancel = ControlRequest {
            request_id: StatusLabel::new("request-cancel").unwrap(),
            requester_uid: 1000,
            run_id: RunId::new("run-1").unwrap(),
            action: ControlAction::Cancel,
        };
        engine.apply(&cancel, &mut journal, 5).unwrap();
        assert!(engine.state().cancelled);
        let recovered = ControlEngine::recover(
            1000,
            RepositoryId::new("repo-1").unwrap(),
            RunId::new("run-1").unwrap(),
            TaskId::new("task-13.1").unwrap(),
            journal.records(),
        )
        .unwrap();
        assert_eq!(recovered.state(), engine.state());
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn all_lifecycle_controls_are_idempotent() {
        let root = root("all-controls");
        let mut journal = Journal::open(&root, "owner").unwrap();
        let mut engine = engine();
        for (index, action) in [
            ControlAction::Pause,
            ControlAction::Resume,
            ControlAction::StopAfterUnit,
            ControlAction::Cancel,
        ]
        .into_iter()
        .enumerate()
        {
            let request = ControlRequest {
                request_id: StatusLabel::new(format!("request-{index}")).unwrap(),
                requester_uid: 1000,
                run_id: RunId::new("run-1").unwrap(),
                action,
            };
            let first = engine.apply(&request, &mut journal, index as u64).unwrap();
            let repeated = engine
                .apply(&request, &mut journal, index as u64 + 10)
                .unwrap();
            assert!(!repeated.changed);
            assert_eq!(first.state, repeated.state);
        }
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
}
