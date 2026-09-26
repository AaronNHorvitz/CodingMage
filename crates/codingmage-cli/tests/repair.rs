//! Reproduce-before-repair units through the real coordinator binary.
//!
//! A run spec may name one configured gate as the regression gate. The coordinator must observe
//! that gate failing at the base commit before the implementer runs and passing on the reviewed
//! candidate, and it retains an integrity-protected repair receipt. Provider prose never
//! satisfies the requirement, and a gate that already passes refuses the unit before any
//! provider process starts.

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
            "codingmage-repair-{}-{}",
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

fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

const IMPLEMENTER: &str = r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
packet = sys.stdin.read()
Path(__file__).with_name("IMPLEMENTER_CALLED").write_text("called\n", encoding="utf-8")
Path(__file__).with_name("PACKET.txt").write_text(packet, encoding="utf-8")
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
"#;

const REVIEWER: &str = r#"#!/usr/bin/python3
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
report = json.dumps({"verdict": "pass", "base_commit": base, "target_commit": target, "findings": [], "blocker_code": None})
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": report}}))
print(json.dumps({"type": "turn.completed"}))
"#;

const PASSING_GATE: &str = "#!/usr/bin/python3\nraise SystemExit(0)\n";

/// Fails until the implementer has changed the value: the regression that must be reproduced.
const REGRESSION_GATE: &str = r#"#!/usr/bin/python3
import sys
from pathlib import Path
if "{ 2 }" not in Path("src/lib.rs").read_text(encoding="utf-8"):
    print("regression: value is not two", file=sys.stderr)
    raise SystemExit(1)
"#;

struct Repair {
    fixture: Fixture,
    config: PathBuf,
    state: PathBuf,
    initial_head: String,
    repository_id: String,
}

impl Repair {
    fn new(initial_value: u8) -> Self {
        let fixture = Fixture::new();
        let target = fixture.root.join("target");
        fs::write(
            target.join("src/lib.rs"),
            format!("pub fn value() -> u8 {{ {initial_value} }}\n"),
        )
        .unwrap();
        fs::write(
            target.join("TASKS.md"),
            "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Repair the value.\n\n- [ ] **AC 0.1:** Given the fixture, when it runs, then the value changes.\n",
        )
        .unwrap();
        git(&target, &["add", "TASKS.md", "src/lib.rs"]);
        git(&target, &["commit", "-m", "add repair fixture"]);
        let initial_head = git_output(&target, &["rev-parse", "HEAD"]);
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
        let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
        assert!(doctor.status.success());
        let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
        let repository_id = diagnosis["repository_id"].as_str().unwrap().to_owned();
        let passing = fixture.executable("passing-gate", PASSING_GATE);
        let regression = fixture.executable("regression-gate", REGRESSION_GATE);
        let configured = fs::read_to_string(&config).unwrap();
        let mut rewritten = configured.replace("/usr/bin/git", passing.to_str().unwrap());
        let _ = writeln!(
            rewritten,
            "\n[[gate_commands]]\nexecutable = \"{}\"\nargs = []",
            regression.display()
        );
        fs::write(&config, rewritten).unwrap();
        Self {
            fixture,
            config,
            state,
            initial_head,
            repository_id,
        }
    }

    fn spec(&self, regression_gate: Option<&str>) -> PathBuf {
        let claude = self.fixture.executable("repair-claude", IMPLEMENTER);
        let codex = self.fixture.executable("repair-codex", REVIEWER);
        let repair = regression_gate.map_or(String::new(), |gate| {
            format!("\n[repair]\nregression_gate = \"{gate}\"\n")
        });
        let spec = self.fixture.root.join("run.toml");
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
{repair}"#,
                claude.display(),
                codex.display()
            ),
        )
        .unwrap();
        spec
    }

    fn run(&self, spec: &Path) -> Output {
        Fixture::command(&[
            "run",
            "--config",
            self.config.to_str().unwrap(),
            "--spec",
            spec.to_str().unwrap(),
        ])
    }

    fn implementer_called(&self) -> bool {
        self.fixture.root.join("IMPLEMENTER_CALLED").exists()
    }

    fn receipts(&self) -> Vec<PathBuf> {
        let root = self.state.join("gate-baselines").join(&self.repository_id);
        let Ok(entries) = fs::read_dir(root) else {
            return Vec::new();
        };
        let mut paths = entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("repair-"))
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }
}

#[test]
fn reproduced_regression_is_repaired_and_receipted() {
    let repair = Repair::new(1);
    let spec = repair.spec(Some("configured-gate-2"));
    let output = repair.run(&spec);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["state"], "complete");
    assert_eq!(outcome["review_verdict"], "pass");
    assert!(repair.implementer_called());
    let candidate = outcome["candidate_commit"].as_str().unwrap().to_owned();

    let receipts = repair.receipts();
    assert_eq!(receipts.len(), 1, "{receipts:?}");
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipts[0]).unwrap()).unwrap();
    let receipt = &document["payload"];
    assert_eq!(receipt["regression_gate"], "configured-gate-2");
    assert_eq!(receipt["base_commit"], repair.initial_head);
    assert_eq!(receipt["candidate_commit"], candidate);
    assert_ne!(
        receipt["base_evidence_sha256"],
        receipt["candidate_evidence_sha256"]
    );
    assert_eq!(receipt["repository_id"], repair.repository_id);
    assert!(
        !fs::read_to_string(&receipts[0])
            .unwrap()
            .contains("regression: value"),
        "receipts retain digests, never gate output"
    );
}

#[test]
fn a_gate_that_already_passes_refuses_the_unit_before_any_provider_runs() {
    let repair = Repair::new(2);
    let spec = repair.spec(Some("configured-gate-2"));
    let output = repair.run(&spec);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("codingmage.runtime.repair_not_reproduced"),
        "{stderr}"
    );
    assert!(!repair.implementer_called(), "no provider ran");
    assert!(repair.receipts().is_empty());
    let target = repair.fixture.root.join("target");
    assert_eq!(
        git_output(&target, &["rev-parse", "HEAD"]),
        repair.initial_head,
        "the active checkout is untouched"
    );
}

#[test]
fn unknown_regression_gate_is_a_spec_refusal_and_ordinary_units_are_unchanged() {
    let repair = Repair::new(1);
    let unknown = repair.spec(Some("configured-gate-9"));
    let output = repair.run(&unknown);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("codingmage.runtime.spec"), "{stderr}");
    assert!(!repair.implementer_called());

    let malformed = repair.spec(Some("../escape"));
    let output = repair.run(&malformed);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("codingmage.runtime.spec"));

    let ordinary = repair.spec(None);
    let output = repair.run(&ordinary);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["state"], "complete");
    assert!(
        repair.receipts().is_empty(),
        "no repair receipt without a requirement"
    );
}

#[test]
fn a_recipe_instantiates_into_a_repair_unit_under_template_authority() {
    let repair = Repair::new(1);
    let template = repair.spec(None);
    let recipe = repair.fixture.root.join("recipe.toml");
    fs::write(
        &recipe,
        r#"version = 1
recipe_id = "value-repair"
kind = "security_repair"
summary = "Raise the value to two so the regression gate passes"
scope_paths = ["src"]
prerequisite_gates = ["configured-gate-1"]
verification_gate = "configured-gate-2"

[parameters]
target_value = "2"

[rollback]
git_recoverable = true
"#,
    )
    .unwrap();
    let output = repair.fixture.root.join("recipe-run.toml");
    let instantiate = |args: &[&str]| {
        let mut arguments = vec![
            "recipe-instantiate",
            "--config",
            repair.config.to_str().unwrap(),
            "--recipe",
            recipe.to_str().unwrap(),
            "--template",
            template.to_str().unwrap(),
        ];
        arguments.extend_from_slice(args);
        Fixture::command(&arguments)
    };
    let rendered = instantiate(&["--task", "0.1.1.1", "--output", output.to_str().unwrap()]);
    assert!(
        rendered.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&rendered.stdout).unwrap();
    assert_eq!(summary["recipe_id"], "value-repair");
    assert_eq!(summary["regression_gate"], "configured-gate-2");
    let written = fs::read_to_string(&output).unwrap();
    assert!(written.contains("regression_gate = \"configured-gate-2\""));
    assert!(written.contains("RECIPE value-repair version 1 kind security_repair"));
    assert!(!written.contains("PRIVATE"), "{written}");

    let duplicate = instantiate(&["--task", "0.1.1.1", "--output", output.to_str().unwrap()]);
    assert!(
        !duplicate.status.success(),
        "an existing output is never overwritten"
    );

    let run = repair.run(&output);
    assert!(
        run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(outcome["state"], "complete");
    let packet = fs::read_to_string(repair.fixture.root.join("PACKET.txt")).unwrap();
    assert!(
        packet.contains("RECIPE value-repair version 1 kind security_repair"),
        "{packet}"
    );
    assert!(packet.contains("parameter target_value=2"), "{packet}");
    assert!(packet.contains("grants no path, command, dependency or credential authority"));
    assert_eq!(
        repair.receipts().len(),
        1,
        "the recipe's verification gate was receipted"
    );

    let mut unconfigured = fs::read_to_string(&recipe).unwrap();
    unconfigured = unconfigured.replace("configured-gate-2", "configured-gate-7");
    fs::write(&recipe, unconfigured).unwrap();
    let other = repair.fixture.root.join("recipe-run-2.toml");
    let refused = instantiate(&["--task", "0.1.1.1", "--output", other.to_str().unwrap()]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("codingmage.runtime.spec"));
    assert!(!other.exists());
}
