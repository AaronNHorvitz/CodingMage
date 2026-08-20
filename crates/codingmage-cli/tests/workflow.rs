//! Binary-level local operator workflow.

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    process::Command,
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::load_config;
use codingmage_runtime::{ProgressStage, run_serial_campaign_with_progress};

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
path = Path("src/lib.rs")
prior = path.read_text(encoding="utf-8")
value = 2 if "{ 1 }" in prior else 3 if "{ 2 }" in prior else 4
path.write_text(f"pub fn value() -> u8 {{ {value} }}\n", encoding="utf-8")
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
from pathlib import Path
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
needs_correction = "{ 3 }" in Path("src/lib.rs").read_text(encoding="utf-8")
report = json.dumps({
    "verdict": "changes_required" if needs_correction else "pass",
    "base_commit": base,
    "target_commit": target,
    "findings": [{
        "id": "FIX-1",
        "kind": "defect",
        "severity": "medium",
        "file": "src/lib.rs",
        "line": 1,
        "claim": "The fixture still needs the reviewed correction.",
        "evidence": "The current value is three.",
        "requested_correction": "Change the value to four.",
        "acceptance_test": "The source contains value four."
    }] if needs_correction else [],
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
    let gate = fixture.executable(
        "fake-gate",
        r#"#!/usr/bin/python3
from pathlib import Path
import sys
source = Path("src/lib.rs").read_text(encoding="utf-8")
if "{ 3 }" not in source and "{ 4 }" not in source:
    print("fixture requires one bounded correction", file=sys.stderr)
    raise SystemExit(1)
"#,
    );
    let configured = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        configured.replace("/usr/bin/git", gate.to_str().unwrap()),
    )
    .unwrap();
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
    assert_eq!(outcome["correction_rounds"], 2);
    assert!(outcome["candidate_commit"].as_str().is_some());
    assert!(outcome["completion_commit"].as_str().is_some());
    let progress = String::from_utf8(run.stderr).unwrap();
    let expected_progress = [
        "coordinator validating configuration, repository, and task authority",
        "coordinator acquiring the exact repository and task claim",
        "coordinator creating the worktree and probing provider capabilities",
        "claude      implementing the bounded task in the isolated worktree",
        "local-gates running deterministic gates on the candidate",
        "local-gates candidate gates blocked; bounded correction will run",
        "claude      correcting the bounded candidate from verified diagnostics",
        "local-gates running deterministic gates on the candidate",
        "codex       reviewing the immutable candidate commit read-only",
        "claude      correcting the bounded candidate from verified diagnostics",
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
    let corrected = Command::new("/usr/bin/git")
        .current_dir(&target)
        .args(["show", &format!("{branch}:src/lib.rs")])
        .output()
        .unwrap();
    assert!(corrected.status.success());
    assert_eq!(
        String::from_utf8(corrected.stdout).unwrap(),
        "pub fn value() -> u8 { 4 }\n"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    let task_source = "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the first fixture operation.\n  - [ ] **Sub-task 0.1.1.2:** Complete the dependent fixture operation.\n<!-- depends-on: 0.1.1.1 -->\n";
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add campaign fixture"]);

    let claude = fixture.executable(
        "campaign-claude",
        r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
path = Path("src/lib.rs")
prior = path.read_text(encoding="utf-8")
value = 2 if "{ 1 }" in prior else 3
path.write_text(f"pub fn value() -> u8 {{ {value} }}\n", encoding="utf-8")
print(json.dumps({
    "type": "result", "is_error": False,
    "structured_output": {
        "changed_paths": ["src/lib.rs"], "tests": [], "commit": None,
        "ready_for_commit": True, "limitations": [], "blocker_code": None
    }
}))
"#,
    );
    let codex = fixture.executable(
        "campaign-codex",
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
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    dependencies = [] if task.endswith(".1") else ["0.1.1.1"]
    report = {
        "campaign_head": head, "task_source_sha256": digest,
        "proposals": [{
            "task_id": task, "dependencies": dependencies, "owned_paths": ["src"],
            "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
            "expected_artifacts": ["src/lib.rs"], "risk": "routine",
            "rationale_summary": "The supplied task is dependency-ready and path-bounded."
        }],
        "human_decision": None
    }
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    report = {
        "verdict": "pass", "base_commit": base, "target_commit": target,
        "findings": [], "blocker_code": None
    }
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let config = fixture.root.join("config/campaign.toml");
    let scratch = fixture.root.join("campaign-scratch");
    let state = fixture.root.join("campaign-state");
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
    let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
    let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let campaign = fixture.root.join("campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 1
campaign_id = "fixture-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 2
maximum_budget_usd = "10.00"
implementer_authentication = "existing_login"
maximum_invocation_budget_usd = "1.00"
campaign_branch = "codingmage/fixture-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[team_lead]
executable = "{}"
model = "fixture-lead"
effort = "high"

[implementer]
executable = "{}"
model = "fixture-implementer"
effort = "high"

[reviewer]
executable = "{}"
model = "fixture-reviewer"
effort = "high"

[[gate_tiers]]
name = "focused"
profiles = ["configured-gates"]
"#,
            diagnosis["repository_id"].as_str().unwrap(),
            target.display(),
            diagnosis["head"].as_str().unwrap(),
            diagnosis["task_source_sha256"].as_str().unwrap(),
            "a".repeat(64),
            codex.display(),
            claude.display(),
            codex.display(),
        ),
    )
    .unwrap();

    let config_value = load_config(&config).unwrap();
    let campaign_value = CampaignSpec::load(&campaign).unwrap();
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        run_serial_campaign_with_progress(
            &config_value,
            campaign_value,
            Path::new(env!("CARGO_BIN_EXE_codingmage")),
            |progress| {
                assert_ne!(progress.stage, ProgressStage::Failed);
                assert_ne!(
                    progress.stage,
                    ProgressStage::Integrating,
                    "fixture interruption after durable integration intent"
                );
            },
        )
        .unwrap();
    }));
    assert!(interrupted.is_err());
    let interrupted_status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(interrupted_status.status.success());
    let interrupted_status: serde_json::Value =
        serde_json::from_slice(&interrupted_status.stdout).unwrap();
    assert_eq!(interrupted_status["state"], "integrating");
    assert_eq!(interrupted_status["current_task_id"], "0.1.1.1");
    assert_eq!(interrupted_status["completed_units"], 0);

    let run = Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(outcome["state"], "complete");
    assert_eq!(outcome["completed_units"], 2);
    assert_eq!(outcome["last_task_id"], "0.1.1.2");
    assert_eq!(
        fs::read_to_string(target.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u8 { 1 }\n"
    );
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );
    let branch = outcome["branch"].as_str().unwrap();
    let completed = Command::new("/usr/bin/git")
        .current_dir(&target)
        .args(["show", &format!("{branch}:TASKS.md")])
        .output()
        .unwrap();
    let completed = String::from_utf8(completed.stdout).unwrap();
    assert!(completed.contains("- [x] **Sub-task 0.1.1.1:**"));
    assert!(completed.contains("- [x] **Sub-task 0.1.1.2:**"));
    let source = Command::new("/usr/bin/git")
        .current_dir(&target)
        .args(["show", &format!("{branch}:src/lib.rs")])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(source.stdout).unwrap(),
        "pub fn value() -> u8 { 3 }\n"
    );
    let progress = String::from_utf8(run.stderr).unwrap();
    assert_eq!(progress.matches("codex-lead  proposing").count(), 1);
    assert_eq!(progress.matches("integration advancing").count(), 2);

    let resumed = Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(
        resumed.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&resumed.stderr),
        String::from_utf8_lossy(&resumed.stdout)
    );
    let resumed_outcome: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(resumed_outcome, outcome);
    let resumed_progress = String::from_utf8(resumed.stderr).unwrap();
    assert!(!resumed_progress.contains("codex-lead"));
    assert!(!resumed_progress.contains("claude"));
    assert!(!resumed_progress.contains("codex       reviewing"));
    assert!(!resumed_progress.contains("integration"));

    let status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "complete");
    assert_eq!(status["actor"], "coordinator");
    assert_eq!(status["completed_units"], 2);
    assert_eq!(status["blocker_count"], 0);
    assert_eq!(status["current_task_id"], serde_json::Value::Null);
    assert_eq!(status["last_task_id"], "0.1.1.2");
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
