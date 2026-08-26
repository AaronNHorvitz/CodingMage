//! Production-coordinator qualification against the prescribed disposable schedule.

use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::load_config;
use codingmage_runtime::{
    CampaignState, ProgressStage, campaign_status, run_serial_campaign_with_progress,
};
use codingmage_soak::{PRESCRIBED_OUTCOME_COUNT, prescribed_ten_outcome_schedule};

const INSTALLED_QUALIFICATION_APPROVAL: &str = "approved";
const INSTALLED_QUALIFICATION_APPROVAL_ENV: &str = "CODINGMAGE_INSTALLED_QUALIFICATION";
const INSTALLED_QUALIFICATION_BINARY_ENV: &str = "CODINGMAGE_INSTALLED_BINARY";

fn select_coordinator_binary(
    approval: Option<&OsStr>,
    installed_binary: Option<&OsStr>,
) -> Result<PathBuf, String> {
    match (approval, installed_binary) {
        (None, None) => Ok(PathBuf::from(env!("CARGO_BIN_EXE_codingmage"))),
        (Some(value), Some(path)) if value == INSTALLED_QUALIFICATION_APPROVAL => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(format!(
                    "{INSTALLED_QUALIFICATION_BINARY_ENV} must be absolute"
                ));
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                format!(
                    "cannot inspect {INSTALLED_QUALIFICATION_BINARY_ENV} {}: {error}",
                    path.display()
                )
            })?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(format!(
                    "{INSTALLED_QUALIFICATION_BINARY_ENV} must name an ordinary file"
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                if metadata.permissions().mode() & 0o111 == 0 {
                    return Err(format!(
                        "{INSTALLED_QUALIFICATION_BINARY_ENV} must be executable"
                    ));
                }
            }
            Ok(path)
        }
        (Some(_), Some(_)) => Err(format!(
            "{INSTALLED_QUALIFICATION_APPROVAL_ENV} must equal {INSTALLED_QUALIFICATION_APPROVAL:?}"
        )),
        _ => Err(format!(
            "{INSTALLED_QUALIFICATION_APPROVAL_ENV} and {INSTALLED_QUALIFICATION_BINARY_ENV} must be set together"
        )),
    }
}

fn coordinator_binary() -> PathBuf {
    select_coordinator_binary(
        std::env::var_os(INSTALLED_QUALIFICATION_APPROVAL_ENV).as_deref(),
        std::env::var_os(INSTALLED_QUALIFICATION_BINARY_ENV).as_deref(),
    )
    .unwrap_or_else(|error| panic!("installed-candidate qualification refused: {error}"))
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-prescribed-campaign-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("target/src")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        git(&root.join("target"), &["init", "--initial-branch=main"]);
        git(
            &root.join("target"),
            &["config", "user.name", "CodingMage Fixture"],
        );
        git(
            &root.join("target"),
            &["config", "user.email", "fixture@invalid.example"],
        );
        Self { root }
    }

    fn executable(&self, name: &str, content: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = self.root.join(name);
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn command(arguments: &[&str]) -> Output {
        Command::new(coordinator_binary())
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
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn run_command(config: &Path, campaign: &Path) -> Output {
    Fixture::command(&[
        "campaign",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ])
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn installed_candidate_selection_requires_explicit_exact_authority() {
    let fixture = Fixture::new();
    let executable = fixture.executable("installed-candidate", "#!/bin/sh\nexit 0\n");

    assert_eq!(
        select_coordinator_binary(None, None).unwrap(),
        PathBuf::from(env!("CARGO_BIN_EXE_codingmage"))
    );
    assert!(
        select_coordinator_binary(
            Some(OsStr::new(INSTALLED_QUALIFICATION_APPROVAL)),
            Some(executable.as_os_str())
        )
        .is_ok()
    );
    assert!(
        select_coordinator_binary(Some(OsStr::new("yes")), Some(executable.as_os_str()))
            .unwrap_err()
            .contains("must equal")
    );
    assert!(
        select_coordinator_binary(
            Some(OsStr::new(INSTALLED_QUALIFICATION_APPROVAL)),
            Some(OsStr::new("relative-candidate"))
        )
        .unwrap_err()
        .contains("must be absolute")
    );
    assert!(
        select_coordinator_binary(Some(OsStr::new(INSTALLED_QUALIFICATION_APPROVAL)), None)
            .unwrap_err()
            .contains("must be set together")
    );
    assert!(
        select_coordinator_binary(None, Some(executable.as_os_str()))
            .unwrap_err()
            .contains("must be set together")
    );

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&executable, fixture.root.join("candidate-link")).unwrap();
        assert!(
            select_coordinator_binary(
                Some(OsStr::new(INSTALLED_QUALIFICATION_APPROVAL)),
                Some(fixture.root.join("candidate-link").as_os_str())
            )
            .unwrap_err()
            .contains("ordinary file")
        );
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn production_coordinator_executes_prescribed_ten_outcome_schedule() {
    let schedule = prescribed_ten_outcome_schedule();
    schedule.validate().unwrap();
    assert_eq!(schedule.outcomes.len(), PRESCRIBED_OUTCOME_COUNT);

    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    let tasks = (1..=9).fold(String::new(), |mut tasks, number| {
        writeln!(
            tasks,
            "  - [ ] **Sub-task 0.1.1.{number}:** Complete prescribed outcome {number}."
        )
        .unwrap();
        tasks
    });
    let task_source = format!(
        "# Tasks\n\n## Sprint 0 - Qualification\n\n**Sprint goal:** Qualify the production coordinator.\n\n### Story 0.1 - Prescribed schedule\n\n- [ ] **Task 0.1.1 - Execute outcomes**\n{tasks}"
    );
    fs::write(target.join("TASKS.md"), &task_source).unwrap();
    fs::write(target.join("src/baseline.txt"), "preserved\n").unwrap();
    git(&target, &["add", "TASKS.md", "src"]);
    git(
        &target,
        &["commit", "-m", "add prescribed campaign fixture"],
    );
    let initial_head = git_output(&target, &["rev-parse", "HEAD"]);

    let implementer = fixture.executable(
        "prescribed-claude",
        r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
packet = sys.stdin.read()
task = re.search(r"Task: ([0-9.]+)", packet).group(1)
root = Path(__file__).parent
log = root / "implementer.log"
prior = [line for line in log.read_text(encoding="utf-8").splitlines() if line == task] if log.exists() else []
with log.open("a", encoding="utf-8") as stream:
    stream.write(task + "\n")
if task == "0.1.1.6" and not prior:
    print("x" * 65536)
    raise SystemExit(0)
path = Path("src") / ("outcome-" + task.rsplit(".", 1)[1] + ".txt")
if task == "0.1.1.2" and not prior:
    value = "needs-gate-correction\n"
elif task == "0.1.1.3" and not prior:
    value = "needs-review-correction\n"
else:
    value = "complete\n"
path.write_text(value, encoding="utf-8")
print(json.dumps({
    "type": "result", "is_error": False,
    "structured_output": {
        "changed_paths": [str(path)], "tests": [], "commit": None,
        "ready_for_commit": True, "limitations": [], "blocker_code": None
    }
}))
"#,
    );
    let lead_and_reviewer = fixture.executable(
        "prescribed-codex",
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
if packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    with (root / "lead.log").open("a", encoding="utf-8") as stream:
        stream.write(task + "\n")
    binding = {
        "campaign_id": campaign_id, "campaign_head": head,
        "task_source_sha256": digest, "task_id": task, "dependencies": []
    }
    if task == "0.1.1.4":
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "blocked", "proposals": [],
            "blocked": {"binding": binding, "reason": "unavailable_external_dependency"},
            "deferred": None, "human_decision": None
        }
    elif task == "0.1.1.5" and not (root / "deferral-recorded").exists():
        (root / "deferral-recorded").write_text("recorded\n", encoding="utf-8")
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "deferred", "proposals": [],
            "blocked": None,
            "deferred": {"binding": binding, "reason": "operator_pause", "reconsideration_trigger": "operator_resume"},
            "human_decision": None
        }
    elif task == "0.1.1.7" and not (root / "capacity-observed").exists():
        (root / "capacity-observed").write_text("observed\n", encoding="utf-8")
        print(json.dumps({"type": "error", "message": "quota exhausted"}))
        raise SystemExit(0)
    else:
        suffix = task.rsplit(".", 1)[1]
        report = {
            "campaign_id": campaign_id, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "propose",
            "proposals": [{
                "task_id": task, "dependencies": [], "owned_paths": ["src"],
                "gate_tiers": ["focused"], "test_resources": ["fixture-gate"],
                "expected_artifacts": ["src/outcome-" + suffix + ".txt"], "risk": "routine",
                "rationale_summary": "The prescribed task is dependency-ready and path-bounded."
            }],
            "blocked": None, "deferred": None, "human_decision": None
        }
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    review_path = Path("src/outcome-3.txt")
    needs_correction = review_path.exists() and "needs-review-correction" in review_path.read_text(encoding="utf-8")
    findings = [{
        "id": "REVIEW-1", "kind": "defect", "severity": "medium",
        "file": "src/outcome-3.txt", "line": 1,
        "claim": "The prescribed review correction remains outstanding.",
        "evidence": "The candidate contains the correction marker.",
        "requested_correction": "Replace the marker with the completed value.",
        "acceptance_test": "The focused gate and repeated review pass."
    }] if needs_correction else []
    report = {
        "verdict": "changes_required" if needs_correction else "pass",
        "base_commit": base, "target_commit": target,
        "findings": findings, "blocker_code": None
    }
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    );
    let gate = fixture.executable(
        "prescribed-gate",
        r#"#!/usr/bin/python3
from pathlib import Path
import sys
for path in Path("src").glob("outcome-*.txt"):
    if "needs-gate-correction" in path.read_text(encoding="utf-8"):
        print("prescribed gate correction required", file=sys.stderr)
        raise SystemExit(1)
"#,
    );

    let config = fixture.root.join("config/campaign.toml");
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
    let configured = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        configured.replace("/usr/bin/git", gate.to_str().unwrap()),
    )
    .unwrap();
    let diagnosis = json(&Fixture::command(&[
        "doctor",
        "--config",
        config.to_str().unwrap(),
    ]));
    let campaign = fixture.root.join("campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "prescribed-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/prescribed-campaign"
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
            lead_and_reviewer.display(),
            implementer.display(),
            lead_and_reviewer.display(),
        ),
    )
    .unwrap();

    let first = json(&run_command(&config, &campaign));
    assert_eq!(first["state"], "paused");
    assert_eq!(first["stop_reason"], "capacity_pause");
    assert_eq!(first["blocker_code"], "codingmage.provider.codex.quota");
    assert_eq!(first["completed_units"], 4);

    let config_value = load_config(&config).unwrap();
    let campaign_value = CampaignSpec::load(&campaign).unwrap();
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        run_serial_campaign_with_progress(
            &config_value,
            campaign_value,
            &coordinator_binary(),
            |progress| {
                if progress.stage == ProgressStage::Integrating {
                    let status = campaign_status(
                        &config_value,
                        &CampaignSpec::load(&campaign).unwrap(),
                        &coordinator_binary(),
                    )
                    .unwrap()
                    .unwrap();
                    assert_ne!(
                        status.current_task_id.as_deref(),
                        Some("0.1.1.8"),
                        "prescribed interruption after durable integration intent"
                    );
                }
            },
        )
        .unwrap();
    }));
    assert!(interrupted.is_err());

    let config_value = load_config(&config).unwrap();
    let campaign_value = CampaignSpec::load(&campaign).unwrap();
    let mut stop_requested = false;
    let stopped = run_serial_campaign_with_progress(
        &config_value,
        campaign_value,
        &coordinator_binary(),
        |progress| {
            if progress.stage == ProgressStage::Implementing && !stop_requested {
                let status = campaign_status(
                    &config_value,
                    &CampaignSpec::load(&campaign).unwrap(),
                    &coordinator_binary(),
                )
                .unwrap()
                .unwrap();
                if status.current_task_id.as_deref() == Some("0.1.1.9") {
                    let control = Fixture::command(&[
                        "campaign-control",
                        "--config",
                        config.to_str().unwrap(),
                        "--campaign",
                        campaign.to_str().unwrap(),
                        "--action",
                        "stop_after_unit",
                        "--request",
                        "prescribed-stop-1",
                    ]);
                    assert!(control.status.success());
                    stop_requested = true;
                }
            }
        },
    )
    .unwrap();
    assert!(stop_requested);
    assert_eq!(stopped.state, CampaignState::Paused);
    assert_eq!(stopped.completed_units, 7);

    assert!(
        Fixture::command(&[
            "campaign-control",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
            "--action",
            "resume",
            "--request",
            "prescribed-resume-1",
        ])
        .status
        .success()
    );
    assert!(
        Fixture::command(&[
            "campaign-observe-trigger",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
            "--task",
            "0.1.1.5",
            "--trigger",
            "operator_resume",
            "--request",
            "prescribed-trigger-1",
            "--evidence-sha256",
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        ])
        .status
        .success()
    );

    let terminal = json(&run_command(&config, &campaign));
    assert_eq!(terminal["state"], "paused");
    assert_eq!(terminal["stop_reason"], "unit_limit");
    assert_eq!(terminal["completed_units"], 8);
    assert_eq!(terminal["last_task_id"], "0.1.1.5");
    let status = json(&Fixture::command(&[
        "campaign-status",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
    ]));
    assert_eq!(status["outcomes"]["completed"], 8);
    assert_eq!(status["outcomes"]["blocked"], 1);
    assert_eq!(status["outcomes"]["deferred"], 0);
    assert_eq!(status["outcomes"]["accepted"], 10);
    assert_eq!(status["outcomes"]["max_accepted"], 10);
    assert_eq!(status["deferrals"][0]["task_id"], "0.1.1.5");
    assert_eq!(status["deferrals"][0]["trigger_state"], "satisfied");
    assert_eq!(status["utilization"]["malformed_report_repairs"], 1);
    assert_eq!(status["utilization"]["correction_rounds"], 2);
    assert!(status["utilization"]["provider_attempts"].as_u64().unwrap() > 0);
    assert!(
        status["utilization"]["process_invocations"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        status["utilization"]["retained_state_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );

    let branch = terminal["branch"].as_str().unwrap();
    let branch_tasks = git_output(&target, &["show", &format!("{branch}:TASKS.md")]);
    for task in schedule
        .outcomes
        .iter()
        .filter(|outcome| outcome.disposition == codingmage_soak::PrescribedDisposition::Completed)
    {
        assert!(branch_tasks.contains(&format!("- [x] **Sub-task {}:**", task.task_id)));
    }
    assert!(branch_tasks.contains("- [ ] **Sub-task 0.1.1.4:**"));
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), initial_head);
    assert_eq!(git_output(&target, &["status", "--porcelain=v1"]), "");
    assert_eq!(
        fs::read_to_string(target.join("TASKS.md")).unwrap(),
        task_source
    );
    assert_eq!(
        fs::read_to_string(target.join("src/baseline.txt")).unwrap(),
        "preserved\n"
    );
    let worktrees = git_output(&target, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 2);
    assert!(!worktrees.contains("/pod/"));
    let retained = fs::read_dir(&scratch)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    assert!(worktrees.contains(retained[0].to_str().unwrap()));

    let implementer_calls = fs::read_to_string(fixture.root.join("implementer.log")).unwrap();
    for (task, expected) in [
        ("0.1.1.1", 1),
        ("0.1.1.2", 2),
        ("0.1.1.3", 2),
        ("0.1.1.5", 1),
        ("0.1.1.6", 2),
        ("0.1.1.7", 1),
        ("0.1.1.8", 1),
        ("0.1.1.9", 1),
    ] {
        assert_eq!(
            implementer_calls
                .lines()
                .filter(|value| *value == task)
                .count(),
            expected,
            "unexpected implementer call count for {task}"
        );
    }
    assert_eq!(
        implementer_calls
            .lines()
            .filter(|task| *task == "0.1.1.8")
            .count(),
        1,
        "interrupted integration must not replay implementation"
    );
    assert_eq!(
        implementer_calls
            .lines()
            .filter(|task| *task == "0.1.1.6")
            .count(),
        2,
        "malformed report must consume one bounded repair"
    );
    let lead_calls = fs::read_to_string(fixture.root.join("lead.log")).unwrap();
    assert_eq!(
        lead_calls.lines().filter(|task| *task == "0.1.1.7").count(),
        2,
        "capacity pause must retry only the affected lead selection"
    );
    assert_eq!(
        lead_calls.lines().filter(|task| *task == "0.1.1.4").count(),
        1,
        "the external blocker must not be replayed"
    );
    assert_eq!(
        lead_calls.lines().filter(|task| *task == "0.1.1.5").count(),
        2,
        "the deferred task must be selected before and after its trigger"
    );
}
