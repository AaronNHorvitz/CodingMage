//! Binary-level local operator workflow.

use std::{fs, path::Path, process::Command};

struct Fixture {
    root: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-cli-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir(root.join("config")).unwrap();
        git(&root.join("target"), &["init", "--initial-branch=main"]);
        git(
            &root.join("target"),
            &["config", "user.name", "CodingMage Fixture"],
        );
        git(
            &root.join("target"),
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(
            root.join("target/TASKS.md"),
            "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the exact fixture operation.\n",
        )
        .unwrap();
        git(&root.join("target"), &["add", "TASKS.md"]);
        git(&root.join("target"), &["commit", "-m", "fixture"]);
        Self { root }
    }

    fn command(arguments: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args(arguments)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn init_doctor_plan_status_and_run_refusal_are_truthful() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    let config = fixture.root.join("config/codingmage.toml");
    let scratch = fixture.root.join("scratch");
    let state = fixture.root.join("state");
    let init = Fixture::command(&[
        "init",
        "--repo",
        target.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--scratch",
        scratch.to_str().unwrap(),
        "--state",
        state.to_str().unwrap(),
    ]);
    assert!(init.status.success());

    let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
    assert!(doctor.status.success());
    let output: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(output["state"], "ready");
    assert_eq!(output["execution_available"], false);
    let rendered = String::from_utf8(doctor.stdout).unwrap();
    assert!(!rendered.contains(fixture.root.to_str().unwrap()));

    let plan = Fixture::command(&["plan", "--config", config.to_str().unwrap()]);
    assert!(plan.status.success());
    let output: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(output["task_id"], "0.1.1.1");

    fs::write(
        target.join("private-untracked-name.txt"),
        "private content\n",
    )
    .unwrap();
    let status = Fixture::command(&["status", "--config", config.to_str().unwrap()]);
    assert!(status.status.success());
    let rendered = String::from_utf8(status.stdout).unwrap();
    assert!(rendered.contains("blocked-dirty"));
    assert!(!rendered.contains("private-untracked-name"));
    assert!(!rendered.contains("private content"));

    let unavailable = Fixture::command(&["run"]);
    assert_eq!(unavailable.status.code(), Some(4));
    assert_eq!(
        String::from_utf8(unavailable.stderr).unwrap().trim(),
        "codingmage.cli.execution_unavailable"
    );
}
