//! Binary-level local operator workflow.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use codingmage_campaign::{
    CampaignAuthentication, CampaignConcurrency, CampaignExecutionMode, CampaignGateTier,
    CampaignLimits, CampaignProvider, CampaignPublication, CampaignSpec,
    DestinationPromotionPolicy, MultiAgentPolicy, TaskIntegrationPolicy, TaskMergeStrategy,
    TaskPublicationMode, TeamResourcePolicy,
};
use codingmage_core::load_config;
use codingmage_runtime::{
    CampaignState, ProgressStage, campaign_status, request_campaign_control,
    run_serial_campaign_with_progress, run_team_campaign_with_progress, team_campaign_report,
};

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
        let executable = std::env::var_os("CODINGMAGE_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_codingmage").into());
        Command::new(executable).args(arguments).output().unwrap()
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

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, current: &Path, snapshot: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let mut entries = fs::read_dir(current)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                snapshot.insert(relative, None);
                visit(root, &path, snapshot);
            } else {
                snapshot.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
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
counter = Path(__file__).with_suffix(".count")
attempt = int(counter.read_text(encoding="utf-8")) if counter.exists() else 0
counter.write_text(str(attempt + 1), encoding="utf-8")
if attempt == 0:
    print("x" * 65536)
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
            r#"version = 2
task_id = "0.1.1.1"
owned_paths = ["src"]
completion_policy = "close_task"

[implementer]
executable = "{}"
model = "fixture-implementer"
effort = "high"
authentication = "existing_login"

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
    assert_eq!(outcome["utilization"]["provider_attempts"], 6);
    assert_eq!(outcome["utilization"]["malformed_report_repairs"], 1);
    assert_eq!(outcome["utilization"]["process_invocations"], 15);
    assert!(
        outcome["utilization"]["output_bytes"].as_u64().unwrap() > 65536,
        "the rejected provider process receipt must contribute its distinctive output"
    );
    assert!(
        outcome["utilization"]["retained_state_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        outcome["utilization"]["execution_elapsed_ms"]
            .as_u64()
            .is_some()
    );
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
    let task_source = "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the first fixture operation.\n  - [ ] **Sub-task 0.1.1.2:** Complete the second independent fixture operation.\n";
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add campaign fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);

    let claude = fixture.executable(
        "campaign-claude",
        r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    if Path(__file__).with_name("break-resume-capability").exists():
        print('--print "json"')
        raise SystemExit(0)
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
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    root = Path(__file__).parent
    capacity = root / "campaign-capacity-count"
    capacity_count = int(capacity.read_text(encoding="utf-8")) if capacity.exists() else 0
    if capacity_count < 2:
        capacity.write_text(str(capacity_count + 1), encoding="utf-8")
        message = "quota exhausted" if capacity_count == 0 else "authentication expired"
        print(json.dumps({"type": "error", "message": message}))
        raise SystemExit(0)
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    marker = root / "campaign-deferral-observed"
    if not marker.exists():
        marker.write_text("deferred\n", encoding="utf-8")
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "deferred",
            "proposals": [], "blocked": None,
            "deferred": {
                "binding": {
                    "campaign_id": campaign_id, "campaign_head": head,
                    "task_source_sha256": digest, "task_id": task,
                    "dependencies": []
                },
                "reason": "operator_pause",
                "reconsideration_trigger": "operator_resume"
            },
            "human_decision": None
        }
    else:
        report = {
            "campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
            "disposition": "propose",
            "proposals": [{
                "task_id": task, "dependencies": [], "owned_paths": ["src"],
                "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
                "expected_artifacts": ["src/lib.rs"], "risk": "routine",
                "rationale_summary": "The supplied task is dependency-ready and path-bounded."
            }],
            "blocked": None, "deferred": None, "human_decision": None
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
            r#"version = 3
campaign_id = "fixture-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 3
implementer_authentication = "existing_login"
campaign_branch = "codingmage/fixture-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 100
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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

    for (expected_code, expected_reason) in [
        ("codingmage.provider.codex.quota", "capacity_pause"),
        ("codingmage.provider.codex.authentication", "capacity_pause"),
    ] {
        let paused = Fixture::command(&[
            "campaign",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
        ]);
        assert!(
            paused.status.success(),
            "stderr={} stdout={}",
            String::from_utf8_lossy(&paused.stderr),
            String::from_utf8_lossy(&paused.stdout)
        );
        let paused: serde_json::Value = serde_json::from_slice(&paused.stdout).unwrap();
        assert_eq!(paused["state"], "paused");
        assert_eq!(paused["stop_reason"], expected_reason);
        assert_eq!(paused["completed_units"], 0);
        assert_eq!(paused["last_task_id"], serde_json::Value::Null);
        assert_eq!(paused["blocker_code"], expected_code);
        assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
        assert_eq!(
            fs::read_to_string(target.join("TASKS.md")).unwrap(),
            task_source
        );
        if expected_code == "codingmage.provider.codex.quota" {
            let pause_arguments = [
                "campaign-control",
                "--config",
                config.to_str().unwrap(),
                "--campaign",
                campaign.to_str().unwrap(),
                "--action",
                "pause",
                "--request",
                "pause-1",
            ];
            let pause = Fixture::command(&pause_arguments);
            assert!(pause.status.success());
            let pause: serde_json::Value = serde_json::from_slice(&pause.stdout).unwrap();
            assert_eq!(pause["created"], true);
            let repeated = Fixture::command(&pause_arguments);
            assert!(repeated.status.success());
            let repeated: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
            assert_eq!(repeated["created"], false);

            let operator_paused = Fixture::command(&[
                "campaign",
                "--config",
                config.to_str().unwrap(),
                "--campaign",
                campaign.to_str().unwrap(),
            ]);
            assert!(operator_paused.status.success());
            let operator_paused: serde_json::Value =
                serde_json::from_slice(&operator_paused.stdout).unwrap();
            assert_eq!(operator_paused["state"], "paused");
            assert_eq!(operator_paused["stop_reason"], "operator_pause");
            assert_eq!(
                operator_paused["blocker_code"],
                "codingmage.campaign.control.paused"
            );

            let resumed = Fixture::command(&[
                "campaign-control",
                "--config",
                config.to_str().unwrap(),
                "--campaign",
                campaign.to_str().unwrap(),
                "--action",
                "resume",
                "--request",
                "resume-1",
            ]);
            assert!(resumed.status.success());

            fs::write(fixture.root.join("break-resume-capability"), "break\n").unwrap();
            let refused_resume = Fixture::command(&[
                "campaign",
                "--config",
                config.to_str().unwrap(),
                "--campaign",
                campaign.to_str().unwrap(),
            ]);
            assert!(!refused_resume.status.success());
            let checkpoint: serde_json::Value = serde_json::from_slice(
                &fs::read(
                    state
                        .join("campaigns")
                        .join("fixture-campaign")
                        .join("checkpoint.json"),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(checkpoint["checkpoint"]["resume_validation"], "pending");
            fs::remove_file(fixture.root.join("break-resume-capability")).unwrap();
        }
    }

    let config_value = load_config(&config).unwrap();
    let campaign_value = CampaignSpec::load(&campaign).unwrap();
    let mut stop_requested = false;
    let mut planning_status_observed = false;
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        run_serial_campaign_with_progress(
            &config_value,
            campaign_value,
            Path::new(env!("CARGO_BIN_EXE_codingmage")),
            |progress| {
                assert_ne!(progress.stage, ProgressStage::Failed);
                if progress.stage == ProgressStage::PlanningCampaign {
                    let status = Fixture::command(&[
                        "campaign-status",
                        "--config",
                        config.to_str().unwrap(),
                        "--campaign",
                        campaign.to_str().unwrap(),
                    ]);
                    assert!(status.status.success());
                    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
                    assert_eq!(status["state"], "planning");
                    assert_eq!(status["actor"], "codex-lead");
                    assert_eq!(status["model"], "fixture-lead");
                    assert!(status["attempt_count"].as_u64().unwrap() > 0);
                    planning_status_observed = true;
                }
                if progress.stage == ProgressStage::Implementing && !stop_requested {
                    let stop = Fixture::command(&[
                        "campaign-control",
                        "--config",
                        config.to_str().unwrap(),
                        "--campaign",
                        campaign.to_str().unwrap(),
                        "--action",
                        "stop_after_unit",
                        "--request",
                        "stop-after-first-unit-1",
                    ]);
                    assert!(stop.status.success());
                    stop_requested = true;
                }
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
    assert!(planning_status_observed);
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
    assert_eq!(interrupted_status["current_task_id"], "0.1.1.2");
    assert_eq!(interrupted_status["current_round"], 0);
    assert_eq!(interrupted_status["completed_units"], 0);

    let premature_observation = Fixture::command(&[
        "campaign-observe-trigger",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--task",
        "0.1.1.1",
        "--trigger",
        "operator_resume",
        "--request",
        "resume-deferral-1",
        "--evidence-sha256",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    ]);
    assert!(!premature_observation.status.success());

    let paused = Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(
        paused.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&paused.stderr),
        String::from_utf8_lossy(&paused.stdout)
    );
    let paused: serde_json::Value = serde_json::from_slice(&paused.stdout).unwrap();
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["stop_reason"], "stop_after_unit");
    assert_eq!(paused["completed_units"], 1);
    assert_eq!(
        paused["blocker_code"],
        "codingmage.campaign.control.stop_after_unit"
    );

    let resumed_after_stop = Fixture::command(&[
        "campaign-control",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--action",
        "resume",
        "--request",
        "resume-after-stop-1",
    ]);
    assert!(resumed_after_stop.status.success());

    let paused = Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(
        paused.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&paused.stderr),
        String::from_utf8_lossy(&paused.stdout)
    );
    let paused: serde_json::Value = serde_json::from_slice(&paused.stdout).unwrap();
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["stop_reason"], "no_independent_ready_work");
    assert_eq!(paused["completed_units"], 1);
    assert_eq!(
        paused["blocker_code"],
        "codingmage.campaign.no_deferred_trigger_observed"
    );
    let deferred_status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(deferred_status.status.success());
    let deferred_status: serde_json::Value =
        serde_json::from_slice(&deferred_status.stdout).unwrap();
    assert_eq!(deferred_status["deferrals"].as_array().unwrap().len(), 1);
    assert_eq!(deferred_status["deferrals"][0]["task_id"], "0.1.1.1");
    assert_eq!(
        deferred_status["deferrals"][0]["reason_code"],
        "operator_pause"
    );
    assert_eq!(
        deferred_status["deferrals"][0]["trigger_code"],
        "operator_resume"
    );
    assert_eq!(deferred_status["deferrals"][0]["trigger_state"], "pending");

    let observation_arguments = [
        "campaign-observe-trigger",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--task",
        "0.1.1.1",
        "--trigger",
        "operator_resume",
        "--request",
        "resume-deferral-1",
        "--evidence-sha256",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    ];
    let observed = Fixture::command(&observation_arguments);
    assert!(
        observed.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&observed.stderr),
        String::from_utf8_lossy(&observed.stdout)
    );
    let observed: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    assert_eq!(observed["changed"], true);
    assert_eq!(observed["trigger"], "operator_resume");
    let repeated = Fixture::command(&observation_arguments);
    assert!(repeated.status.success());
    let repeated: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(repeated["changed"], false);
    let satisfied_status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(satisfied_status.status.success());
    let satisfied_status: serde_json::Value =
        serde_json::from_slice(&satisfied_status.stdout).unwrap();
    assert_eq!(satisfied_status["deferrals"].as_array().unwrap().len(), 1);
    assert_eq!(
        satisfied_status["deferrals"][0]["trigger_state"],
        "satisfied"
    );
    let conflicting_observation = Fixture::command(&[
        "campaign-observe-trigger",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--task",
        "0.1.1.1",
        "--trigger",
        "operator_resume",
        "--request",
        "resume-deferral-1",
        "--evidence-sha256",
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    ]);
    assert!(!conflicting_observation.status.success());

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
    assert_eq!(outcome["stop_reason"], "completion");
    assert_eq!(outcome["completed_units"], 2);
    assert_eq!(outcome["last_task_id"], "0.1.1.1");
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
    assert_eq!(progress.matches("integration advancing").count(), 1);

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
    assert_eq!(status["schema_version"], 3);
    assert_eq!(status["state"], "complete");
    assert_eq!(status["actor"], "coordinator");
    assert_eq!(status["model"], serde_json::Value::Null);
    assert_eq!(status["completed_units"], 2);
    assert_eq!(
        status["attempt_count"],
        status["utilization"]["provider_attempts"]
    );
    assert!(status["attempt_count"].as_u64().unwrap() > 0);
    assert_eq!(status["outcomes"]["completed"], 2);
    assert_eq!(status["outcomes"]["blocked"], 0);
    assert_eq!(status["outcomes"]["deferred"], 0);
    assert_eq!(status["outcomes"]["pending_human_decision"], 0);
    assert_eq!(status["outcomes"]["accepted"], 3);
    assert_eq!(status["outcomes"]["max_accepted"], 3);
    assert!(status["limits"]["provider_attempts"].as_u64().unwrap() > 0);
    assert!(status["limits"]["process_invocations"].as_u64().unwrap() > 0);
    assert!(status["limits"]["output_bytes"].as_u64().unwrap() > 0);
    assert!(
        status["utilization"]["process_invocations"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(status["blocker_count"], 0);
    assert_eq!(status["current_task_id"], serde_json::Value::Null);
    assert_eq!(status["current_round"], serde_json::Value::Null);
    assert_eq!(status["last_task_id"], "0.1.1.1");
}

#[test]
#[allow(clippy::too_many_lines)]
fn parallel_campaign_runs_five_pods_and_serializes_reviewed_integrations() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    let source_names = ["a", "b", "c", "d", "e"];
    for name in source_names {
        fs::write(
            target.join(format!("src/{name}.rs")),
            format!("pub fn {name}() -> u8 {{ 1 }}\n"),
        )
        .unwrap();
    }
    let mut task_items = String::new();
    for (index, name) in source_names.iter().enumerate() {
        writeln!(
            task_items,
            "  - [ ] **Sub-task 0.1.1.{}:** Complete source {} independently.",
            index.saturating_add(1),
            name.to_ascii_uppercase()
        )
        .unwrap();
    }
    let task_source = format!(
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - Parallel work\n\n- [ ] **Task 0.1.1 - Work**\n{task_items}"
    );
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src"]);
    git(&target, &["commit", "-m", "add parallel campaign fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);
    let original_tasks = fs::read(target.join("TASKS.md")).unwrap();
    let original_sources = source_names
        .iter()
        .map(|name| fs::read(target.join(format!("src/{name}.rs"))).unwrap())
        .collect::<Vec<_>>();

    let claude = fixture.executable(
        "parallel-claude",
        r#"#!/usr/bin/python3
import json, re, sys, time
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
packet = sys.stdin.read()
task = re.search(r"Task: ([0-9.]+)", packet).group(1)
name = chr(ord("a") + int(task.rsplit(".", 1)[1]) - 1)
path = Path(f"src/{name}.rs")
log = Path(__file__).with_name("parallel-pods.log")
with log.open("a", encoding="utf-8") as stream:
    stream.write(f"{task} start {time.monotonic_ns()}\n")
deadline = time.monotonic() + 30
while sum(line.split()[1] == "start" for line in log.read_text(encoding="utf-8").splitlines()) < 5:
    if time.monotonic() >= deadline:
        raise RuntimeError("five implementation processes did not overlap")
    time.sleep(0.01)
path.write_text(path.read_text(encoding="utf-8").replace("{ 1 }", "{ 2 }"), encoding="utf-8")
with log.open("a", encoding="utf-8") as stream:
    stream.write(f"{task} end {time.monotonic_ns()}\n")
print(json.dumps({
    "type": "result", "is_error": False,
    "structured_output": {
        "changed_paths": [str(path)], "tests": [], "commit": None,
        "ready_for_commit": True, "limitations": [], "blocker_code": None
    }
}))
"#,
    );
    let codex = fixture.executable(
        "parallel-codex",
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
with Path(__file__).with_name("parallel-codex.log").open("a", encoding="utf-8") as stream:
    stream.write("invoked\n")
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    tasks = re.findall(r"- id=([0-9.]+)", packet)
    proposals = []
    for task in tasks:
        name = chr(ord("a") + int(task.rsplit(".", 1)[1]) - 1)
        path = f"src/{name}.rs"
        proposals.append({
            "task_id": task, "dependencies": [], "owned_paths": [path],
            "gate_tiers": ["focused"], "test_resources": [f"fixture-{task}"],
            "expected_artifacts": [path], "risk": "routine",
            "rationale_summary": "The task is dependency-ready and owns one exact file."
        })
    report = {
        "campaign_id": campaign_id, "campaign_head": head,
        "task_source_sha256": digest, "disposition": "propose",
        "proposals": proposals, "blocked": None, "deferred": None,
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
    let gate = fixture.executable("parallel-gate", "#!/bin/sh\nexit 0\n");
    let config = fixture.root.join("config/parallel-campaign.toml");
    let scratch = fixture.root.join("parallel-scratch");
    let state = fixture.root.join("parallel-state");
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
    let configured = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        configured.replace("/usr/bin/git", gate.to_str().unwrap()),
    )
    .unwrap();
    let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
    let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let provider = |executable: &Path, model: &str| CampaignProvider {
        executable: executable.to_path_buf(),
        model: model.to_owned(),
        effort: "high".to_owned(),
    };
    let spec = CampaignSpec {
        version: 3,
        campaign_id: "parallel-fixture".to_owned(),
        repository_id: diagnosis["repository_id"].as_str().unwrap().to_owned(),
        repository_path: target.clone(),
        initial_commit: diagnosis["head"].as_str().unwrap().to_owned(),
        task_source_sha256: diagnosis["task_source_sha256"].as_str().unwrap().to_owned(),
        operator_authorization_sha256: "a".repeat(64),
        max_parallel_pods: 5,
        max_units: 5,
        limits: CampaignLimits {
            provider_attempts: 1_000,
            malformed_report_repairs: 100,
            correction_rounds: 100,
            process_invocations: 10_000,
            output_bytes: 1_073_741_824,
            retained_state_bytes: 1_073_741_824,
            execution_elapsed_ms: 86_400_000,
        },
        team_lead: provider(&codex, "fixture-lead"),
        implementer: provider(&claude, "fixture-implementer"),
        implementer_authentication: CampaignAuthentication::ExistingLogin,
        reviewer: provider(&codex, "fixture-reviewer"),
        gate_tiers: vec![CampaignGateTier {
            name: "focused".to_owned(),
            profiles: vec!["configured-gates".to_owned()],
        }],
        campaign_branch: "codingmage/parallel-fixture".to_owned(),
        allowed_paths: vec![PathBuf::from("src")],
        denied_paths: vec![],
        protected_branches: vec!["main".to_owned()],
        publication: CampaignPublication::LocalOnly,
        multi_agent: Some(MultiAgentPolicy {
            version: 1,
            execution_mode: CampaignExecutionMode::Parallel,
            publication_mode: TaskPublicationMode::LocalOnly,
            task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
            destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
            task_merge_strategy: TaskMergeStrategy::Squash,
            github: None,
            concurrency: CampaignConcurrency {
                claude_implementers: 5,
                codex_team_leads: 1,
                codex_reviewers: 5,
                test_workers: 5,
                github_writers: 1,
                integration_workers: 1,
            },
            resources: TeamResourcePolicy::default(),
            max_campaign_tokens: 1_000_000,
            max_task_tokens: 500_000,
            max_task_correction_cycles: 3,
            max_follow_up_tasks: 0,
            integration_validation_interval: 1,
        }),
    };
    spec.verify().unwrap();
    let config_value = load_config(&config).unwrap();
    let mut progress = Vec::new();
    let mut pause_requested = false;
    let paused = run_team_campaign_with_progress(
        &config_value,
        spec.clone(),
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
        |event| {
            if event.stage == ProgressStage::Implementing && !pause_requested {
                let control = request_campaign_control(
                    &config_value,
                    &spec,
                    Path::new(env!("CARGO_BIN_EXE_codingmage")),
                    "pause",
                    "parallel-pause-1",
                )
                .unwrap();
                assert!(control.created);
                pause_requested = true;
            }
            progress.push(event);
        },
    )
    .unwrap();
    assert_eq!(paused.state, CampaignState::Paused);
    assert_eq!(paused.completed_units, 0);
    assert!(pause_requested);
    let status = campaign_status(
        &config_value,
        &spec,
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(status.state, "paused");
    assert_eq!(status.outcomes.accepted, 5);
    assert_eq!(status.outcomes.completed, 0);
    let repeated = request_campaign_control(
        &config_value,
        &spec,
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
        "pause",
        "parallel-pause-1",
    )
    .unwrap();
    assert!(!repeated.created);
    assert!(
        request_campaign_control(
            &config_value,
            &spec,
            Path::new(env!("CARGO_BIN_EXE_codingmage")),
            "resume",
            "parallel-resume-1",
        )
        .unwrap()
        .created
    );
    let outcome = run_team_campaign_with_progress(
        &config_value,
        spec.clone(),
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
        |event| progress.push(event),
    )
    .unwrap();
    assert_eq!(outcome.state, CampaignState::Complete);
    assert_eq!(outcome.completed_units, 5);
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(git_output(&target, &["status", "--porcelain=v1"]), "");
    assert_eq!(fs::read(target.join("TASKS.md")).unwrap(), original_tasks);
    for (name, original) in source_names.iter().zip(original_sources) {
        assert_eq!(
            fs::read(target.join(format!("src/{name}.rs"))).unwrap(),
            original
        );
    }
    let completed = git_output(&target, &["show", &format!("{}:TASKS.md", outcome.branch)]);
    for (index, name) in source_names.iter().enumerate() {
        assert!(completed.contains(&format!(
            "- [x] **Sub-task 0.1.1.{}:**",
            index.saturating_add(1)
        )));
        assert_eq!(
            git_output(
                &target,
                &["show", &format!("{}:src/{name}.rs", outcome.branch)]
            ),
            format!("pub fn {name}() -> u8 {{ 2 }}")
        );
    }
    assert_eq!(
        progress
            .iter()
            .filter(|event| event.stage == ProgressStage::Implementing)
            .count(),
        5
    );
    assert_eq!(
        progress
            .iter()
            .filter(|event| event.stage == ProgressStage::Integrating)
            .count(),
        5
    );
    let timings = fs::read_to_string(fixture.root.join("parallel-pods.log")).unwrap();
    let mut starts = BTreeMap::new();
    let mut ends = BTreeMap::new();
    for line in timings.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let timestamp = fields[2].parse::<u128>().unwrap();
        if fields[1] == "start" {
            starts.insert(fields[0], timestamp);
        } else {
            ends.insert(fields[0], timestamp);
        }
    }
    assert_eq!(starts.len(), 5);
    assert_eq!(ends.len(), 5);
    assert!(starts.values().max().unwrap() < ends.values().min().unwrap());
    let complete_status = campaign_status(
        &config_value,
        &spec,
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(complete_status.state, "complete");
    assert_eq!(complete_status.outcomes.completed, 5);
    let report = team_campaign_report(
        &config_value,
        &spec,
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(report.final_commit, outcome.head);
    assert_eq!(report.initial_commit, original_head);
    assert_eq!(report.tasks.len(), 5);
    assert_eq!(report.final_gate_evidence_sha256.len(), 64);
    assert_eq!(report.final_review_evidence_sha256.len(), 64);
    let campaign_file = fixture.root.join("parallel-campaign.toml");
    fs::write(&campaign_file, toml::to_string(&spec).unwrap()).unwrap();
    let report_output = Fixture::command(&[
        "campaign-report",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign_file.to_str().unwrap(),
    ]);
    assert!(report_output.status.success());
    let reported: serde_json::Value = serde_json::from_slice(&report_output.stdout).unwrap();
    assert_eq!(reported["final_commit"], outcome.head);
    assert_eq!(reported["tasks"].as_object().unwrap().len(), 5);

    let codex_log = fixture.root.join("parallel-codex.log");
    let codex_before = fs::read(&codex_log).unwrap();
    let claude_before = fs::read(fixture.root.join("parallel-pods.log")).unwrap();
    let repeated = run_team_campaign_with_progress(
        &config_value,
        spec,
        Path::new(env!("CARGO_BIN_EXE_codingmage")),
        |_| {},
    )
    .unwrap();
    assert_eq!(repeated.state, CampaignState::Complete);
    assert_eq!(repeated.completed_units, 0);
    assert_eq!(fs::read(codex_log).unwrap(), codex_before);
    assert_eq!(
        fs::read(fixture.root.join("parallel-pods.log")).unwrap(),
        claude_before
    );
}

#[test]
#[ignore = "explicit local sustained qualification; requires CODINGMAGE_SUSTAINED_SOAK=approved"]
fn sustained_one_pod_campaign_qualification() {
    assert_eq!(
        std::env::var("CODINGMAGE_SUSTAINED_SOAK").as_deref(),
        Ok("approved"),
        "set the explicit sustained-soak guard"
    );
    let cycles = std::env::var("CODINGMAGE_SUSTAINED_SOAK_CYCLES")
        .expect("set a bounded sustained-soak cycle count")
        .parse::<u16>()
        .expect("sustained-soak cycles must be an integer");
    assert!((2..=100).contains(&cycles));
    for _ in 0..cycles {
        serial_campaign_advances_two_reviewed_tasks_without_touching_active_checkout();
    }
}

#[test]
#[ignore = "explicit local sustained qualification; requires CODINGMAGE_SUSTAINED_SOAK=approved"]
fn sustained_five_pod_campaign_qualification() {
    assert_eq!(
        std::env::var("CODINGMAGE_SUSTAINED_SOAK").as_deref(),
        Ok("approved"),
        "set the explicit sustained-soak guard"
    );
    let cycles = std::env::var("CODINGMAGE_SUSTAINED_SOAK_CYCLES")
        .expect("set a bounded sustained-soak cycle count")
        .parse::<u16>()
        .expect("sustained-soak cycles must be an integer");
    assert!((2..=100).contains(&cycles));
    for _ in 0..cycles {
        parallel_campaign_runs_five_pods_and_serializes_reviewed_integrations();
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn campaign_cancel_terminates_owned_provider_without_signalling_unrelated_process() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    fs::write(
        target.join("TASKS.md"),
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the exact fixture operation.\n",
    )
    .unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add cancellation fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);

    let claude = fixture.executable(
        "cancel-claude",
        r#"#!/usr/bin/python3
import sys
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
raise SystemExit(1)
"#,
    );
    let codex = fixture.executable(
        "cancel-codex",
        r#"#!/usr/bin/python3
import sys, time
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
Path(__file__).with_name("lead-started").write_text("started\n", encoding="utf-8")
time.sleep(30)
raise SystemExit(1)
"#,
    );
    let config = fixture.root.join("config/cancel.toml");
    let scratch = fixture.root.join("cancel-scratch");
    let state = fixture.root.join("cancel-state");
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
    let campaign = fixture.root.join("cancel-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "cancel-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 1
implementer_authentication = "existing_login"
campaign_branch = "codingmage/cancel-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 100
malformed_report_repairs = 10
correction_rounds = 10
process_invocations = 100
output_bytes = 10485760
retained_state_bytes = 10485760
execution_elapsed_ms = 60000

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

    let mut unrelated = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
    let mut running = Command::new(env!("CARGO_BIN_EXE_codingmage"))
        .args([
            "campaign",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let marker = fixture.root.join("lead-started");
    let wait_started = Instant::now();
    while !marker.exists() && wait_started.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "campaign lead did not start");

    let cancel_started = Instant::now();
    let cancel = Fixture::command(&[
        "campaign-control",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--action",
        "cancel",
        "--request",
        "cancel-live-1",
    ]);
    assert!(cancel.status.success());
    while running.try_wait().unwrap().is_none() && cancel_started.elapsed() < Duration::from_secs(5)
    {
        thread::sleep(Duration::from_millis(10));
    }
    if running.try_wait().unwrap().is_none() {
        let _ = running.kill();
        panic!("campaign did not terminate promptly after cancel");
    }
    let output = running.wait_with_output().unwrap();
    assert!(output.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["state"], "cancelled");
    assert_eq!(outcome["stop_reason"], "operator_cancellation");
    assert_eq!(
        outcome["blocker_code"],
        "codingmage.campaign.control.cancelled"
    );
    assert!(cancel_started.elapsed() < Duration::from_secs(5));
    assert!(unrelated.try_wait().unwrap().is_none());
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    let _ = unrelated.kill();
    let _ = unrelated.wait();
}

#[test]
#[allow(clippy::too_many_lines)]
fn serial_campaign_resumes_interrupted_correction_without_replaying_implementation() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    git(&target, &["add", "src/lib.rs"]);
    git(
        &target,
        &["commit", "-m", "add interrupted correction fixture"],
    );
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);
    let original_tasks = fs::read(target.join("TASKS.md")).unwrap();
    let original_source = fs::read(target.join("src/lib.rs")).unwrap();

    let claude = fixture.executable(
        "recovery-claude",
        r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
root = Path(__file__).parent
log = root / "recovery-claude.log"
path = Path("src/lib.rs")
source = path.read_text(encoding="utf-8")
if "{ 2 }" in source:
    path.write_text("pub fn value() -> u8 { 3 }\n", encoding="utf-8")
    with log.open("a", encoding="utf-8") as stream:
        stream.write("correction-interrupted\n")
    print(json.dumps({"type": "result", "is_error": True, "subtype": "provider_unavailable"}))
    raise SystemExit(0)
if "--resume" in sys.argv and "{ 3 }" in source:
    with log.open("a", encoding="utf-8") as stream:
        stream.write("correction-resumed\n")
    print(json.dumps({
        "type": "result", "is_error": False,
        "structured_output": {
            "changed_paths": ["src/lib.rs"], "tests": [], "commit": None,
            "ready_for_commit": True, "limitations": [], "blocker_code": None
        }
    }))
    raise SystemExit(0)
value = 2
with log.open("a", encoding="utf-8") as stream:
    stream.write("implementation-start\n")
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
        "recovery-codex",
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
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    report = {
        "campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
        "disposition": "propose",
        "proposals": [{
            "task_id": task, "dependencies": [], "owned_paths": ["src"],
            "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
            "expected_artifacts": ["src/lib.rs"], "risk": "routine",
            "rationale_summary": "The supplied task is dependency-ready and path-bounded."
        }],
        "blocked": None, "deferred": None, "human_decision": None
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
    let gate = fixture.executable(
        "recovery-gate",
        r#"#!/usr/bin/python3
from pathlib import Path
import sys
if "{ 2 }" in Path("src/lib.rs").read_text(encoding="utf-8"):
    print("fixture requires correction", file=sys.stderr)
    raise SystemExit(1)
"#,
    );
    let config = fixture.root.join("config/recovery-campaign.toml");
    let scratch = fixture.root.join("recovery-scratch");
    let state = fixture.root.join("recovery-state");
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
    let configured = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        configured.replace("/usr/bin/git", gate.to_str().unwrap()),
    )
    .unwrap();
    let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
    let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let campaign = fixture.root.join("recovery-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "recovery-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 1
implementer_authentication = "existing_login"
campaign_branch = "codingmage/recovery-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 100
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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
    assert_eq!(outcome["completed_units"], 1);
    let calls = fs::read_to_string(fixture.root.join("recovery-claude.log")).unwrap();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        [
            "implementation-start",
            "correction-interrupted",
            "correction-resumed"
        ]
    );
    let progress = String::from_utf8(run.stderr).unwrap();
    assert_eq!(progress.matches("codex-lead  proposing").count(), 1);
    assert_eq!(progress.matches("implementing the bounded task").count(), 1);
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(fs::read(target.join("TASKS.md")).unwrap(), original_tasks);
    assert_eq!(
        fs::read(target.join("src/lib.rs")).unwrap(),
        original_source
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn serial_campaign_persists_blocker_and_continues_independent_work() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    let task_source = "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Wait for an external prerequisite.\n  - [ ] **Sub-task 0.1.1.2:** Complete independent local work.\n  - [ ] **Sub-task 0.1.1.3:** Complete work that depends on the blocker.\n<!-- depends-on: 0.1.1.1 -->\n";
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(
        &target,
        &["commit", "-m", "add blocker continuation fixture"],
    );
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);

    let claude = fixture.executable(
        "blocker-claude",
        r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
root = Path(__file__).parent
with (root / "blocker-claude.log").open("a", encoding="utf-8") as stream:
    stream.write("implementation\n")
path = Path("src/lib.rs")
value = int(re.search(r"\{ (\d+) \}", path.read_text(encoding="utf-8")).group(1)) + 1
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
        "blocker-codex",
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
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    root = Path(__file__).parent
    with (root / "blocker-lead.log").open("a", encoding="utf-8") as stream:
        stream.write("lead\n")
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    tasks = re.findall(r"- id=([0-9.]+)", packet)
    if "0.1.1.1" in tasks and not (root / "prerequisite-ready").exists():
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "blocked",
            "proposals": [],
            "blocked": {
                "binding": {
                    "campaign_id": campaign_id, "campaign_head": head,
                    "task_source_sha256": digest, "task_id": "0.1.1.1",
                    "dependencies": []
                },
                "reason": "unavailable_external_dependency"
            },
            "deferred": None, "human_decision": None
        }
    else:
        task = tasks[0]
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "propose",
            "proposals": [{
                "task_id": task,
                "dependencies": [] if task != "0.1.1.3" else ["0.1.1.1"],
                "owned_paths": ["src"],
                "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
                "expected_artifacts": ["src/lib.rs"], "risk": "routine",
                "rationale_summary": "The independent task is ready and bounded."
            }],
            "blocked": None, "deferred": None, "human_decision": None
        }
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    report = {"verdict": "pass", "base_commit": base, "target_commit": target, "findings": [], "blocker_code": None}
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let config = fixture.root.join("config/blocker-campaign.toml");
    let scratch = fixture.root.join("blocker-scratch");
    let state = fixture.root.join("blocker-state");
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
    let campaign = fixture.root.join("blocker-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "blocker-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/blocker-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 100
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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
    assert_eq!(outcome["state"], "blocked", "outcome={outcome}");
    assert_eq!(outcome["stop_reason"], "no_independent_ready_work");
    assert_eq!(outcome["completed_units"], 1);
    assert_eq!(
        outcome["blocker_code"],
        "codingmage.campaign.no_unblocked_ready_work"
    );

    let state_before_reads = snapshot_tree(&state);
    let scratch_before_reads = snapshot_tree(&scratch);
    let head_before_reads = git_output(&target, &["rev-parse", "HEAD"]);
    let status_before_reads = git_output(&target, &["status", "--porcelain=v2"]);
    let refs_before_reads = git_output(
        &target,
        &["for-each-ref", "--format=%(refname)%00%(objectname)"],
    );
    let lead_log_before_reads = fs::read(fixture.root.join("blocker-lead.log")).unwrap();
    let implementer_log_before_reads = fs::read(fixture.root.join("blocker-claude.log")).unwrap();

    let status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "blocked");
    assert_eq!(status["blocker_count"], 1);
    assert_eq!(
        status["blocker_code"],
        "codingmage.campaign.no_unblocked_ready_work"
    );
    assert_eq!(status["blockers"].as_array().unwrap().len(), 1);
    assert_eq!(status["blockers"][0]["task_id"], "0.1.1.1");
    assert_eq!(
        status["blockers"][0]["reason_code"],
        "unavailable_external_dependency"
    );
    assert!(status["deferrals"].as_array().unwrap().is_empty());
    assert!(status["human_decisions"].as_array().unwrap().is_empty());
    let attempt_count_before_reads = status["attempt_count"].as_u64().unwrap();

    let explanation_arguments = [
        "campaign-explain-blocker",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ];
    let explanation = Fixture::command(&explanation_arguments);
    assert!(explanation.status.success());
    let explanation: serde_json::Value = serde_json::from_slice(&explanation.stdout).unwrap();
    assert_eq!(explanation["schema_version"], 1);
    assert_eq!(explanation["campaign_id"], "blocker-campaign");
    assert_eq!(explanation["state"], "blocked");
    assert_eq!(
        explanation["blocker_code"],
        "codingmage.campaign.no_unblocked_ready_work"
    );
    assert_eq!(explanation["blockers"], status["blockers"]);
    assert_eq!(explanation["deferrals"], status["deferrals"]);
    assert_eq!(explanation["human_decisions"], status["human_decisions"]);

    for _ in 0..16 {
        let polled = Fixture::command(&[
            "campaign-status",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
        ]);
        assert!(polled.status.success());
        let polled: serde_json::Value = serde_json::from_slice(&polled.stdout).unwrap();
        assert_eq!(polled["attempt_count"], attempt_count_before_reads);
        assert_eq!(polled["blockers"], status["blockers"]);

        let reattached = Fixture::command(&explanation_arguments);
        assert!(reattached.status.success());
        let reattached: serde_json::Value = serde_json::from_slice(&reattached.stdout).unwrap();
        assert_eq!(reattached, explanation);
    }

    assert_eq!(snapshot_tree(&state), state_before_reads);
    assert_eq!(snapshot_tree(&scratch), scratch_before_reads);
    assert_eq!(
        git_output(&target, &["rev-parse", "HEAD"]),
        head_before_reads
    );
    assert_eq!(
        git_output(&target, &["status", "--porcelain=v2"]),
        status_before_reads
    );
    assert_eq!(
        git_output(
            &target,
            &["for-each-ref", "--format=%(refname)%00%(objectname)"],
        ),
        refs_before_reads
    );
    assert_eq!(
        fs::read(fixture.root.join("blocker-lead.log")).unwrap(),
        lead_log_before_reads
    );
    assert_eq!(
        fs::read(fixture.root.join("blocker-claude.log")).unwrap(),
        implementer_log_before_reads
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-lead.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-claude.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let branch = outcome["branch"].as_str().unwrap();
    let tasks = git_output(&target, &["show", &format!("{branch}:TASKS.md")]);
    assert!(tasks.contains("- [ ] **Sub-task 0.1.1.1:**"));
    assert!(tasks.contains("- [x] **Sub-task 0.1.1.2:**"));
    assert!(tasks.contains("- [ ] **Sub-task 0.1.1.3:**"));
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );

    fs::write(fixture.root.join("prerequisite-ready"), "ready\n").unwrap();
    let clearance_arguments = [
        "campaign-clear-blocker",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--task",
        "0.1.1.1",
        "--request",
        "clear-blocker-1",
        "--prerequisite-sha256",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    ];
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;

        let campaign_state = state.join("campaigns/blocker-campaign");
        fs::set_permissions(&campaign_state, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!Fixture::command(&clearance_arguments).status.success());
        fs::set_permissions(&campaign_state, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let cleared = Fixture::command(&clearance_arguments);
    assert!(
        cleared.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&cleared.stderr),
        String::from_utf8_lossy(&cleared.stdout)
    );
    let cleared: serde_json::Value = serde_json::from_slice(&cleared.stdout).unwrap();
    assert_eq!(cleared["changed"], true);
    assert_eq!(cleared["task_id"], "0.1.1.1");
    assert_eq!(cleared["request_id"], "clear-blocker-1");
    assert_eq!(cleared["campaign_revalidation_required"], true);
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-lead.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-claude.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    let repeated = Fixture::command(&clearance_arguments);
    assert!(repeated.status.success());
    let repeated: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(repeated["changed"], false);

    let cleared_status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(cleared_status.status.success());
    let cleared_status: serde_json::Value = serde_json::from_slice(&cleared_status.stdout).unwrap();
    assert_eq!(cleared_status["state"], "ready");
    assert_eq!(cleared_status["blocker_count"], 0);
    assert_eq!(cleared_status["blocker_code"], serde_json::Value::Null);
    assert!(cleared_status["blockers"].as_array().unwrap().is_empty());

    let conflicting = Fixture::command(&[
        "campaign-clear-blocker",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--task",
        "0.1.1.1",
        "--request",
        "clear-blocker-1",
        "--prerequisite-sha256",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    ]);
    assert!(!conflicting.status.success());

    let resumed = Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(resumed.status.success());
    let resumed: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(resumed["state"], "complete");
    assert_eq!(resumed["completed_units"], 3);
    assert_eq!(resumed["blocker_code"], serde_json::Value::Null);
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-lead.log"))
            .unwrap()
            .lines()
            .count(),
        4
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("blocker-claude.log"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    let completed_tasks = git_output(&target, &["show", &format!("{branch}:TASKS.md")]);
    assert!(completed_tasks.contains("- [x] **Sub-task 0.1.1.1:**"));
    assert!(completed_tasks.contains("- [x] **Sub-task 0.1.1.2:**"));
    assert!(completed_tasks.contains("- [x] **Sub-task 0.1.1.3:**"));
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn serial_campaign_pauses_cleanly_when_aggregate_correction_limit_is_reached() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    git(&target, &["add", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add correction fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);
    let original_tasks = fs::read(target.join("TASKS.md")).unwrap();
    let original_source = fs::read(target.join("src/lib.rs")).unwrap();

    let claude = fixture.executable(
        "paused-campaign-claude",
        r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
path = Path("src/lib.rs")
source = path.read_text(encoding="utf-8")
value = int(re.search(r"\{ (\d+) \}", source).group(1)) + 1
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
        "paused-campaign-codex",
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
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    report = {
        "campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
        "disposition": "propose",
        "proposals": [{
            "task_id": task, "dependencies": [], "owned_paths": ["src"],
            "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
            "expected_artifacts": ["src/lib.rs"], "risk": "routine",
            "rationale_summary": "The supplied task is dependency-ready and path-bounded."
        }],
        "blocked": None, "deferred": None, "human_decision": None
    }
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    report = {
        "verdict": "changes_required", "base_commit": base, "target_commit": target,
        "findings": [{
            "id": "FIX-1", "kind": "defect", "severity": "medium",
            "file": "src/lib.rs", "line": 1,
            "claim": "The bounded fixture deliberately remains incomplete.",
            "evidence": "The reviewer requires another bounded correction.",
            "requested_correction": "Increment the fixture value once more.",
            "acceptance_test": "The independent reviewer accepts the candidate."
        }],
        "blocker_code": None
    }
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let config = fixture.root.join("config/paused-campaign.toml");
    let scratch = fixture.root.join("paused-campaign-scratch");
    let state = fixture.root.join("paused-campaign-state");
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
    let campaign = fixture.root.join("paused-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "paused-fixture-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 1
implementer_authentication = "existing_login"
campaign_branch = "codingmage/paused-fixture-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 1
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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
    assert_eq!(outcome["state"], "paused");
    assert_eq!(outcome["stop_reason"], "attempt_limit");
    assert_eq!(outcome["completed_units"], 0);
    assert_eq!(outcome["last_task_id"], "0.1.1.1");
    assert_eq!(
        outcome["blocker_code"],
        "codingmage.campaign.limit.correction_rounds"
    );
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(fs::read(target.join("TASKS.md")).unwrap(), original_tasks);
    assert_eq!(
        fs::read(target.join("src/lib.rs")).unwrap(),
        original_source
    );

    let status = Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "paused");
    assert_eq!(status["current_task_id"], serde_json::Value::Null);
    assert_eq!(status["current_round"], serde_json::Value::Null);
    assert_eq!(status["last_task_id"], "0.1.1.1");
    assert_eq!(status["blocker_count"], 1);
    assert_eq!(
        status["blocker_code"],
        "codingmage.campaign.limit.correction_rounds"
    );
    let candidates = Command::new("/usr/bin/git")
        .current_dir(&target)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "branch",
            "--list",
            "codingmage/paused-fixture-campaign/pod/*",
            "--format=%(refname:short)",
        ])
        .output()
        .unwrap();
    assert!(candidates.status.success());
    assert!(!candidates.stdout.is_empty());
}

#[test]
#[allow(clippy::too_many_lines)]
fn human_decision_survives_restart_while_independent_work_continues() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    let task_source = "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Resolve one bounded architecture decision.\n  - [ ] **Sub-task 0.1.1.2:** Complete independent local implementation work.\n";
    fs::write(target.join("TASKS.md"), task_source).unwrap();
    git(&target, &["add", "TASKS.md", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add human decision fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);

    let claude = fixture.executable(
        "decision-claude",
        r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
root = Path(__file__).parent
with (root / "decision-claude.log").open("a", encoding="utf-8") as stream:
    stream.write("implementation\n")
path = Path("src/lib.rs")
value = int(re.search(r"\{ (\d+) \}", path.read_text(encoding="utf-8")).group(1)) + 1
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
        "decision-codex",
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
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    root = Path(__file__).parent
    with (root / "decision-lead.log").open("a", encoding="utf-8") as stream:
        stream.write("lead\n")
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    tasks = re.findall(r"- id=([0-9.]+)", packet)
    if "0.1.1.1" in tasks:
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "human_decision_required",
            "proposals": [], "blocked": None, "deferred": None,
            "human_decision": {
                "binding": {
                    "campaign_id": campaign_id, "campaign_head": head,
                    "task_source_sha256": digest, "task_id": "0.1.1.1",
                    "dependencies": []
                },
                "reason": "material_architecture_choice",
                "summary": "PRIVATE_PROVIDER_QUESTION_MUST_NOT_PERSIST"
            }
        }
    else:
        task = tasks[0]
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "propose",
            "proposals": [{
                "task_id": task, "dependencies": [], "owned_paths": ["src"],
                "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
                "expected_artifacts": ["src/lib.rs"], "risk": "routine",
                "rationale_summary": "The independent task is ready and bounded."
            }],
            "blocked": None, "deferred": None, "human_decision": None
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
    let config = fixture.root.join("config/decision-campaign.toml");
    let scratch = fixture.root.join("decision-scratch");
    let state = fixture.root.join("decision-state");
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
    let campaign = fixture.root.join("decision-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "decision-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 3
implementer_authentication = "existing_login"
campaign_branch = "codingmage/decision-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 100
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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

    let run_arguments = [
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ];
    let run = Fixture::command(&run_arguments);
    assert!(
        run.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(outcome["state"], "blocked");
    assert_eq!(outcome["stop_reason"], "no_independent_ready_work");
    assert_eq!(outcome["completed_units"], 1);
    assert_eq!(
        outcome["blocker_code"],
        "codingmage.campaign.no_independent_ready_work_pending_human_decision"
    );
    let branch = outcome["branch"].as_str().unwrap();
    let tasks = git_output(&target, &["show", &format!("{branch}:TASKS.md")]);
    assert!(tasks.contains("- [ ] **Sub-task 0.1.1.1:**"));
    assert!(tasks.contains("- [x] **Sub-task 0.1.1.2:**"));
    assert_eq!(
        git_output(&target, &["show", &format!("{branch}:src/lib.rs")]),
        "pub fn value() -> u8 { 2 }"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("decision-lead.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("decision-claude.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let checkpoint =
        fs::read_to_string(state.join("campaigns/decision-campaign/checkpoint.json")).unwrap();
    assert!(!checkpoint.contains("PRIVATE_PROVIDER_QUESTION_MUST_NOT_PERSIST"));

    let restarted = Fixture::command(&run_arguments);
    assert!(restarted.status.success());
    let restarted: serde_json::Value = serde_json::from_slice(&restarted.stdout).unwrap();
    assert_eq!(restarted["state"], "blocked");
    assert_eq!(restarted["completed_units"], 1);
    assert_eq!(
        fs::read_to_string(fixture.root.join("decision-lead.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("decision-claude.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );
    assert_eq!(
        fs::read_to_string(target.join("src/lib.rs")).unwrap(),
        "pub fn value() -> u8 { 1 }\n"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn rejected_lead_output_has_no_downstream_effect_and_consumes_no_unit() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    fs::create_dir(target.join("src")).unwrap();
    fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    git(&target, &["add", "src/lib.rs"]);
    git(&target, &["commit", "-m", "add rejection fixture"]);
    let original_head = git_output(&target, &["rev-parse", "HEAD"]);
    let original_tasks = fs::read(target.join("TASKS.md")).unwrap();
    let original_source = fs::read(target.join("src/lib.rs")).unwrap();

    let claude = fixture.executable(
        "rejected-claude",
        r#"#!/usr/bin/python3
from pathlib import Path
Path(__file__).with_name("rejected-claude-called").write_text("called\n", encoding="utf-8")
raise SystemExit(9)
"#,
    );
    let codex = fixture.executable(
        "rejected-codex",
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
root = Path(__file__).parent
log = root / "rejected-lead.log"
prior_calls = len(log.read_text(encoding="utf-8").splitlines()) if log.exists() else 0
with log.open("a", encoding="utf-8") as stream:
    stream.write("lead\n")
campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
task = re.search(r"- id=([0-9.]+)", packet).group(1)
report = {
    "campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
    "disposition": "propose",
    "proposals": [{
        "task_id": task, "dependencies": [], "owned_paths": ["../escape"],
        "gate_tiers": ["focused"], "test_resources": ["rust-tests"],
        "expected_artifacts": ["../escape/result"], "risk": "routine",
        "rationale_summary": "HOSTILE_PROVIDER_PROSE_MUST_NOT_PERSIST"
    }],
    "blocked": None, "deferred": None, "human_decision": None
}
if prior_calls == 1:
    report["undeclared_authority"] = "HOSTILE_UNKNOWN_FIELD_MUST_NOT_PERSIST"
elif prior_calls == 2:
    report["campaign_head"] = "f" * 40
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let config = fixture.root.join("config/rejected-campaign.toml");
    let scratch = fixture.root.join("rejected-scratch");
    let state = fixture.root.join("rejected-state");
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
    let campaign = fixture.root.join("rejected-campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "rejected-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 1
implementer_authentication = "existing_login"
campaign_branch = "codingmage/rejected-campaign"
allowed_paths = ["src"]
denied_paths = []
protected_branches = ["main"]
publication = "local_only"

[limits]
provider_attempts = 1000
malformed_report_repairs = 100
correction_rounds = 100
process_invocations = 10000
output_bytes = 1073741824
retained_state_bytes = 1073741824
execution_elapsed_ms = 86400000

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

    for (expected_rejections, expected_code) in [
        (1, "codingmage.campaign.lead_rejected.invalid_proposal"),
        (2, "codingmage.campaign.lead_rejected.malformed_output"),
        (3, "codingmage.campaign.lead_rejected.malformed_output"),
    ] {
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
        assert_eq!(outcome["state"], "paused");
        assert_eq!(outcome["stop_reason"], "attempt_limit");
        assert_eq!(outcome["completed_units"], 0);
        assert_eq!(outcome["blocker_code"], expected_code);

        let checkpoint =
            fs::read_to_string(state.join("campaigns/rejected-campaign/checkpoint.json")).unwrap();
        assert!(!checkpoint.contains("HOSTILE_PROVIDER_PROSE_MUST_NOT_PERSIST"));
        assert!(!checkpoint.contains("HOSTILE_UNKNOWN_FIELD_MUST_NOT_PERSIST"));
        let checkpoint: serde_json::Value = serde_json::from_str(&checkpoint).unwrap();
        assert_eq!(
            checkpoint["checkpoint"]["rejected_proposals"]
                .as_array()
                .unwrap()
                .len(),
            expected_rejections
        );
    }

    assert!(!fixture.root.join("rejected-claude-called").exists());
    assert_eq!(
        fs::read_to_string(fixture.root.join("rejected-lead.log"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(fs::read(target.join("TASKS.md")).unwrap(), original_tasks);
    assert_eq!(
        fs::read(target.join("src/lib.rs")).unwrap(),
        original_source
    );
    assert_eq!(
        git_output(&target, &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count(),
        2
    );
    assert!(
        !git_output(&target, &["branch", "--format=%(refname:short)"])
            .lines()
            .any(|branch| branch.contains("/pod/"))
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

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
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
