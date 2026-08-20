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

    fn executable(&self, name: &str, content: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = self.root.join(name);
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn supervised_run_composes_fake_agents_git_gates_state_and_cleanup() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    let task_source = "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the exact fixture operation.\n\n- [ ] **AC 0.1:** Given the fixture, when it runs, then the value changes.\n";
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add runtime fixture"]);

    let claude = fixture.executable(
        "fake-claude",
        r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
Path("src/lib.rs").write_text("pub fn value() -> u8 { 2 }\n", encoding="utf-8")
print(json.dumps({
    "type": "result",
    "is_error": False,
    "structured_output": {
        "changed_paths": ["src/lib.rs"],
        "tests": [],
        "commit": None,
        "ready_for_commit": True,
        "limitations": [],
        "blocker_code": None
    }
}))
"#,
    );
    let codex = fixture.executable(
        "fake-codex",
        r#"#!/usr/bin/python3
import json, re, sys
if "--version" in sys.argv:
    print("codex-cli 0.144.5")
    raise SystemExit(0)
if "--help" in sys.argv and "resume" in sys.argv:
    print("SESSION_ID --json --output-schema --model --ignore-user-config")
    raise SystemExit(0)
if "--help" in sys.argv:
    print("Run Codex non-interactively --json --output-schema resume --model read-only --ignore-user-config")
    raise SystemExit(0)
packet = sys.stdin.read()
base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
report = json.dumps({
    "verdict": "pass",
    "base_commit": base,
    "target_commit": target,
    "findings": [],
    "blocker_code": None
})
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": report}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let config = fixture.root.join("config/codingmage.toml");
    let scratch = fixture.root.join("scratch");
    let state = fixture.root.join("state");
    assert!(
        Fixture::command(&[
            "init",
            "--repo",
            target.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "--scratch",
            scratch.to_str().unwrap(),
            "--state",
            state.to_str().unwrap(),
        ])
        .status
        .success()
    );
    let spec = fixture.root.join("run.toml");
    fs::write(
        &spec,
        format!(
            r#"version = 1
task_id = "0.1.1.1"
owned_paths = ["src"]
completion_policy = "close_task"

[implementer]
executable = "{}"
model = "fixture-implementer"
effort = "high"
authentication = "existing_login"
maximum_budget_usd = "1.00"

[reviewer]
executable = "{}"
model = "fixture-reviewer"
effort = "high"
"#,
            claude.display(),
            codex.display()
        ),
    )
    .unwrap();

    let run = Fixture::command(&[
        "run",
        "--config",
        config.to_str().unwrap(),
        "--spec",
        spec.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(outcome["state"], "complete");
    assert_eq!(outcome["review_verdict"], "pass");
    assert!(outcome["candidate_commit"].as_str().is_some());
    assert!(outcome["completion_commit"].as_str().is_some());
    let progress = String::from_utf8(run.stderr).unwrap();
    let expected_progress = [
        "coordinator validating configuration, repository, and task authority",
        "coordinator acquiring the exact repository and task claim",
        "coordinator creating the worktree and probing provider capabilities",
        "claude      implementing the bounded task in the isolated worktree",
        "local-gates running deterministic gates on the candidate",
        "codex       reviewing the immutable candidate commit read-only",
        "local-gates repeating deterministic gates after review",
        "coordinator writing the durable reviewed checkpoint",
        "coordinator reconciling the exact task completion marker",
        "coordinator releasing owned worktree, processes, and locks",
        "coordinator run finished; inspect the final JSON state",
    ];
    let mut prior = 0;
    for expected in expected_progress {
        let offset = progress[prior..]
            .find(expected)
            .unwrap_or_else(|| panic!("missing ordered progress {expected:?}: {progress}"));
        prior += offset + expected.len();
    }
    assert!(!progress.contains(fixture.root.to_str().unwrap()));
    assert!(!progress.contains("Complete the exact fixture operation"));
    assert_eq!(
        fs::read_to_string(target.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u8 { 1 }\n"
    );
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );
    assert!(fs::read_dir(&scratch).unwrap().next().is_none());
    let branch = outcome["branch"].as_str().unwrap();
    let completed = Command::new("/usr/bin/git")
        .current_dir(&target)
        .args(["show", &format!("{branch}:TASKS.md")])
        .output()
        .unwrap();
    assert!(completed.status.success());
    assert!(
        String::from_utf8(completed.stdout)
            .unwrap()
            .contains("- [x] **Sub-task 0.1.1.1:**")
    );
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
fn init_doctor_plan_status_and_run_contract_are_truthful() {
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
    assert_eq!(output["execution_available"], true);
    assert_eq!(output["requires_run_spec"], true);
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
    assert_eq!(unavailable.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(unavailable.stderr).unwrap().trim(),
        "codingmage.cli.usage"
    );
}
