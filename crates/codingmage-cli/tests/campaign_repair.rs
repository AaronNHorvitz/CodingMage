//! Reproduce-before-repair through campaign task authority with a real coordinator process.
//!
//! The campaign specification binds one configured gate to one exact task. The coordinator must
//! observe that gate failing at the unit's base before the implementer runs and passing on the
//! candidate; a base that already passes retains a typed blocker for that task and the campaign
//! continues with independent work.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-campaign-repair-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("target/src")).unwrap();
        fs::create_dir_all(root.join("target/docs")).unwrap();
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

fn sha256(path: &Path) -> String {
    let output = Command::new("/usr/bin/sha256sum")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

/// Implementer: raises the value to two and records that it ran.
const IMPLEMENTER: &str = r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
sys.stdin.read()
Path(__file__).with_name("IMPLEMENTER_CALLED").write_text("called\n", encoding="utf-8")
Path("src/lib.rs").write_text("pub fn value() -> u8 { 2 }\n", encoding="utf-8")
print(json.dumps({
    "type": "result", "is_error": False,
    "structured_output": {
        "changed_paths": ["src/lib.rs"], "tests": [], "commit": None,
        "ready_for_commit": True, "limitations": [], "blocker_code": None
    }
}))
"#;

/// Lead and reviewer: proposes the first task once, blocks every other task, passes reviews.
const LEAD_AND_REVIEWER: &str = r#"#!/usr/bin/python3
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
    binding = {"campaign_id": campaign_id, "campaign_head": head,
               "task_source_sha256": digest, "task_id": task, "dependencies": []}
    proposed = root / "proposed"
    if task == "0.1.1.1" and not proposed.exists():
        proposed.write_text("yes\n", encoding="utf-8")
        report = {"campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
                  "disposition": "propose",
                  "proposals": [{"task_id": task, "dependencies": [], "owned_paths": ["src"],
                                 "gate_tiers": ["focused"], "test_resources": ["fixture-gate"],
                                 "expected_artifacts": [], "risk": "routine",
                                 "rationale_summary": "repair the value"}],
                  "blocked": None, "deferred": None, "human_decision": None}
    else:
        report = {"campaign_id": campaign_id, "campaign_head": head, "task_source_sha256": digest,
                  "disposition": "blocked", "proposals": [],
                  "blocked": {"binding": binding, "reason": "unavailable_external_dependency"},
                  "deferred": None, "human_decision": None}
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    report = {"verdict": "pass", "base_commit": base, "target_commit": target, "findings": [], "blocker_code": None}
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#;

const PASSING_GATE: &str = "#!/usr/bin/python3\nraise SystemExit(0)\n";

const REGRESSION_GATE: &str = r#"#!/usr/bin/python3
import sys
from pathlib import Path
if "{ 2 }" not in Path("src/lib.rs").read_text(encoding="utf-8"):
    print("regression: value is not two", file=sys.stderr)
    raise SystemExit(1)
"#;

struct Campaign {
    fixture: Fixture,
    config: PathBuf,
    spec: PathBuf,
    state: PathBuf,
}

impl Campaign {
    #[allow(clippy::too_many_lines)]
    fn new(initial_value: u8) -> Self {
        let fixture = Fixture::new();
        let target = fixture.root.join("target");
        let mut task_items = String::new();
        for number in 1..=10 {
            writeln!(
                task_items,
                "  - [ ] **Sub-task 0.1.1.{number}:** Repair fixture unit {number}."
            )
            .unwrap();
        }
        fs::write(
            target.join("TASKS.md"),
            format!(
                "# Tasks\n\n## Sprint 0 - Controlled\n\n**Sprint goal:** Qualify.\n\n### Story 0.1 - Work\n\n- [ ] **Task 0.1.1 - Units**\n{task_items}"
            ),
        )
        .unwrap();
        fs::write(
            target.join("src/lib.rs"),
            format!("pub fn value() -> u8 {{ {initial_value} }}\n"),
        )
        .unwrap();
        fs::write(target.join("docs/README.md"), "docs\n").unwrap();
        git(&target, &["add", "TASKS.md", "src", "docs"]);
        git(&target, &["commit", "-m", "add repair campaign fixture"]);
        git(&target, &["switch", "-c", "controlled-target"]);

        let authorization = fixture.root.join("authorization.txt");
        fs::write(&authorization, "OPERATOR_AUTHORIZATION_RECORD\n").unwrap();
        let authorization_sha256 = sha256(&authorization);
        let claude = fixture.executable("repair-claude", IMPLEMENTER);
        let codex = fixture.executable("repair-codex", LEAD_AND_REVIEWER);
        let passing = fixture.executable("passing-gate", PASSING_GATE);
        let regression = fixture.executable("regression-gate", REGRESSION_GATE);

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
        let configured = fs::read_to_string(&config).unwrap();
        let mut rewritten = configured.replace("/usr/bin/git", passing.to_str().unwrap());
        let _ = writeln!(
            rewritten,
            "\n[[gate_commands]]\nexecutable = \"{}\"\nargs = []",
            regression.display()
        );
        fs::write(&config, rewritten).unwrap();
        let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
        assert!(doctor.status.success());
        let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
        let spec = fixture.root.join("campaign.toml");
        fs::write(
            &spec,
            format!(
                r#"version = 3
campaign_id = "repair-campaign"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{authorization_sha256}"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/repair-campaign"
allowed_paths = ["src", "docs"]
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

[[task_path_authority]]
task_id = "0.1.1.1"
companion_paths = ["docs"]
regression_gate = "configured-gate-2"
"#,
                diagnosis["repository_id"].as_str().unwrap(),
                target.display(),
                diagnosis["head"].as_str().unwrap(),
                diagnosis["task_source_sha256"].as_str().unwrap(),
                codex.display(),
                claude.display(),
                codex.display(),
            ),
        )
        .unwrap();
        Self {
            fixture,
            config,
            spec,
            state,
        }
    }

    fn json(&self, command: &str) -> serde_json::Value {
        let output = Fixture::command(&[
            command,
            "--config",
            self.config.to_str().unwrap(),
            "--campaign",
            self.spec.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn implementer_called(&self) -> bool {
        self.fixture.root.join("IMPLEMENTER_CALLED").exists()
    }

    fn repair_receipts(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut pending = vec![self.state.clone()];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(path);
                } else if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("repair-"))
                {
                    found.push(path);
                }
            }
        }
        found.sort();
        found
    }
}

#[test]
fn campaign_task_authority_reproduces_the_regression_before_the_pod_repairs_it() {
    let campaign = Campaign::new(1);
    let outcome = campaign.json("campaign");
    assert_eq!(outcome["completed_units"], 1, "{outcome}");
    assert_eq!(outcome["state"], "paused", "{outcome}");
    assert_eq!(outcome["stop_reason"], "unit_limit", "{outcome}");
    assert!(campaign.implementer_called());
    let receipts = campaign.repair_receipts();
    assert_eq!(receipts.len(), 1, "{receipts:?}");
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipts[0]).unwrap()).unwrap();
    assert_eq!(document["payload"]["regression_gate"], "configured-gate-2");
    let status = campaign.json("campaign-status");
    let text = serde_json::to_string(&status).unwrap();
    assert!(
        !text.contains("regression: value"),
        "status stays content-minimized"
    );
}

#[test]
fn a_base_that_already_passes_stops_the_invocation_with_a_typed_blocker() {
    let campaign = Campaign::new(2);
    let outcome = campaign.json("campaign");
    assert_eq!(outcome["state"], "blocked", "{outcome}");
    assert_eq!(
        outcome["blocker_code"], "codingmage.campaign.unit_repair_not_reproduced",
        "{outcome}"
    );
    assert_eq!(outcome["completed_units"], 0, "{outcome}");
    assert!(
        !campaign.implementer_called(),
        "no provider ran for the task"
    );
    assert!(campaign.repair_receipts().is_empty());
    let lead_log = fs::read_to_string(campaign.fixture.root.join("lead.log")).unwrap();
    assert_eq!(lead_log, "0.1.1.1\n", "the lead was consulted exactly once");
    let status = campaign.json("campaign-status");
    assert_eq!(
        status["blocker_code"], "codingmage.campaign.unit_repair_not_reproduced",
        "{status}"
    );
}
