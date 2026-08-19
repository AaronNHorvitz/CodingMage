//! Unprivileged user-service specification, ownership, and retry scheduling.

mod capacity;
mod lifecycle;

pub use capacity::{
    CapacityClass, CapacityInput, CapacityMetrics, RetryDecision, RetryError, RetryPolicy,
    RetryState, StructuredFailure, classify_capacity,
};
pub use lifecycle::{
    CoordinatorLock, LifecycleError, ServiceAction, ServiceLimits, ServicePlan, ServiceSpec,
};

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_contracts::{RepositoryId, RunId, TaskId};
    use codingmage_state::Journal;
    use std::{
        fs,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn root(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-service-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    fn service_spec(root: &std::path::Path) -> ServiceSpec {
        fs::create_dir_all(root.join("state")).unwrap();
        fs::create_dir_all(root.join("scratch")).unwrap();
        fs::write(root.join("config.toml"), b"version = 1\n").unwrap();
        ServiceSpec::new(
            std::path::Path::new("/usr/bin/true"),
            &root.join("config.toml"),
            &root.join("state"),
            &root.join("scratch"),
            ServiceLimits {
                memory_bytes: 1024 * 1024 * 1024,
                tasks: 64,
                cpu_percent: 200,
            },
        )
        .unwrap()
    }

    #[test]
    fn user_unit_and_all_lifecycle_previews_are_unprivileged() {
        let root = root("unit");
        let spec = service_spec(&root);
        let unit = spec.render_unit();
        for expected in [
            "NoNewPrivileges=true",
            "ProtectSystem=strict",
            "ProtectHome=read-only",
            "MemoryMax=1073741824",
            "TasksMax=64",
            "CPUQuota=200%",
        ] {
            assert!(unit.contains(expected));
        }
        assert!(!unit.contains("User=root"));
        assert!(!unit.contains("loginctl enable-linger"));
        for action in [
            ServiceAction::Install,
            ServiceAction::Verify,
            ServiceAction::Start,
            ServiceAction::Stop,
            ServiceAction::Uninstall,
        ] {
            let plan = spec.plan(action, &root.join("user-units"));
            assert!(!plan.requires_root);
            assert!(!plan.enables_lingering);
            assert!(!plan.enables_at_boot);
            assert!(plan.steps.iter().all(|step| !step.contains(' ')));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_verify_and_uninstall_preserve_changed_units() {
        let root = root("install");
        let spec = service_spec(&root);
        let unit_root = root.join("user-units");
        let path = spec.install_unit(&unit_root).unwrap();
        spec.verify_installed(&unit_root).unwrap();
        spec.install_unit(&unit_root).unwrap();
        fs::write(&path, b"human change\n").unwrap();
        assert_eq!(
            spec.uninstall_unit(&unit_root).unwrap_err(),
            LifecycleError::Drift
        );
        fs::write(&path, spec.render_unit()).unwrap();
        spec.uninstall_unit(&unit_root).unwrap();
        spec.uninstall_unit(&unit_root).unwrap();
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_systemd_analyze_accepts_rendered_user_unit() {
        let available = Command::new("systemd-analyze")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !available {
            return;
        }
        let root = root("systemd-verify");
        let spec = service_spec(&root);
        let unit_root = root.join("user-units");
        let path = spec.install_unit(&unit_root).unwrap();
        let output = Command::new("systemd-analyze")
            .args(["--user", "verify"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "systemd-analyze failed with status {:?}",
            output.status.code()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_repository_lock_rejects_duplicate_and_releases() {
        let root = root("lock");
        let repository = RepositoryId::new("repo-1").unwrap();
        let first = CoordinatorLock::acquire(&root, &repository, "pid-1-start-1").unwrap();
        assert_eq!(
            CoordinatorLock::acquire(&root, &repository, "pid-2-start-2").unwrap_err(),
            LifecycleError::AlreadyOwned
        );
        drop(first);
        let second = CoordinatorLock::acquire(&root, &repository, "pid-2-start-2").unwrap();
        drop(second);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn coordinator_lock_holder() {
        let Ok(root) = std::env::var("CODINGMAGE_SERVICE_LOCK_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let _lock =
            CoordinatorLock::acquire(&root, &RepositoryId::new("repo-1").unwrap(), "child-owner")
                .unwrap();
        fs::write(root.join("child-ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn crashed_owner_is_not_adopted_and_kernel_releases_lock() {
        let root = root("crash");
        fs::create_dir_all(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::coordinator_lock_holder", "--nocapture"])
            .env("CODINGMAGE_SERVICE_LOCK_ROOT", &root)
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if root.join("child-ready").exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(root.join("child-ready").exists());
        let repository = RepositoryId::new("repo-1").unwrap();
        assert_eq!(
            CoordinatorLock::acquire(&root, &repository, "parent-owner").unwrap_err(),
            LifecycleError::AlreadyOwned
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let recovered = CoordinatorLock::acquire(&root, &repository, "new-owner").unwrap();
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capacity_classification_prefers_structured_signals() {
        let cases = [
            (
                StructuredFailure::Authentication,
                CapacityClass::Authentication,
            ),
            (StructuredFailure::Quota, CapacityClass::Quota),
            (StructuredFailure::RateLimit, CapacityClass::Quota),
            (StructuredFailure::Overload, CapacityClass::Overload),
            (StructuredFailure::Network, CapacityClass::Network),
            (StructuredFailure::Malformed, CapacityClass::Malformed),
            (StructuredFailure::Terminal, CapacityClass::Terminal),
        ];
        for (structured, expected) in cases {
            assert_eq!(
                classify_capacity(CapacityInput {
                    structured: Some(structured),
                    http_status: Some(200),
                    ..CapacityInput::default()
                }),
                expected
            );
        }
        assert_eq!(
            classify_capacity(CapacityInput {
                http_status: Some(429),
                ..CapacityInput::default()
            }),
            CapacityClass::Quota
        );
    }

    #[test]
    fn retry_is_bounded_nonzero_and_persists_across_restart() {
        let policy = RetryPolicy::new(1_000, 8_000, 4, 100).unwrap();
        let mut state = RetryState::default();
        let first = policy.decide(state, CapacityClass::Network, 10_000, None, 7);
        assert_eq!(
            first,
            RetryDecision::PauseUntil {
                attempt: 1,
                next_at_ms: 11_007
            }
        );
        let root = root("retry");
        let mut journal = Journal::open(&root, "owner").unwrap();
        let repository = RepositoryId::new("repo-1").unwrap();
        let run = RunId::new("run-1").unwrap();
        let task = TaskId::new("task-14.2").unwrap();
        RetryPolicy::persist(
            first,
            CapacityClass::Network,
            (&repository, &run, &task),
            10_000,
            &mut journal,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            RetryPolicy::recover(journal.records(), &repository, &run, &task),
            state
        );
        let reset = policy.decide(state, CapacityClass::Quota, 20_000, Some(50_000), 7);
        assert_eq!(
            reset,
            RetryDecision::PauseUntil {
                attempt: 2,
                next_at_ms: 50_007
            }
        );
        assert_eq!(
            policy.decide(state, CapacityClass::Authentication, 20_000, None, 7),
            RetryDecision::Stop {
                class: CapacityClass::Authentication
            }
        );
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_reset_backoff_caps_and_terminal_attempt_stops() {
        let policy = RetryPolicy::new(100, 400, 3, 0).unwrap();
        let mut state = RetryState::default();
        for (attempt, delay) in [(1, 100), (2, 200), (3, 400)] {
            let decision = policy.decide(state, CapacityClass::Overload, 1_000, None, 0);
            assert_eq!(
                decision,
                RetryDecision::PauseUntil {
                    attempt,
                    next_at_ms: 1_000 + delay
                }
            );
            state.attempt = attempt;
        }
        assert_eq!(
            policy.decide(state, CapacityClass::Overload, 1_000, None, 0),
            RetryDecision::Stop {
                class: CapacityClass::Overload
            }
        );
    }

    #[test]
    fn sustained_fake_provider_never_retries_before_deadline_or_after_stop() {
        let policy = RetryPolicy::new(100, 800, 5, 0).unwrap();
        let mut state = RetryState::default();
        let mut invocations = 0_u32;
        for now_ms in 0..2_000 {
            if !state.ready(now_ms) {
                continue;
            }
            invocations += 1;
            let class = if invocations == 4 {
                CapacityClass::Authentication
            } else {
                CapacityClass::Network
            };
            let decision = policy.decide(state, class, now_ms, None, 0);
            match decision {
                RetryDecision::PauseUntil {
                    attempt,
                    next_at_ms,
                } => {
                    assert!(next_at_ms > now_ms);
                    state = RetryState {
                        attempt,
                        next_at_ms: Some(next_at_ms),
                        last_class: Some(class),
                        terminal: false,
                    };
                }
                RetryDecision::Stop { .. } => state.terminal = true,
            }
        }
        assert_eq!(invocations, 4);
        assert!(state.terminal);
        assert!(!state.ready(u64::MAX));
    }
}
