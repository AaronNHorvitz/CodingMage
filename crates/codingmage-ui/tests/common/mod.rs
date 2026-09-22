//! Disposable repository fixtures and a display-less harness for the native workspace.
#![allow(dead_code)]

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use codingmage_ui::{App, backend::CoordinatorBinary};
use egui_kittest::Harness;

/// Resolves the real `codingmage` executable built by this workspace.
pub fn coordinator_binary() -> PathBuf {
    if let Some(explicit) = std::env::var_os("CODINGMAGE_TEST_BINARY") {
        return PathBuf::from(explicit);
    }
    let current = std::env::current_exe().unwrap();
    let target_dir = current.parent().unwrap().parent().unwrap();
    let candidate = target_dir.join("codingmage");
    assert!(
        candidate.is_file(),
        "the coordinator executable {} is missing; build it with `cargo build -p codingmage-cli` before running the interface tests",
        candidate.display()
    );
    candidate
}

/// One disposable Git repository with a configuration written by `codingmage init`.
pub struct Fixture {
    pub root: PathBuf,
    pub target: PathBuf,
    pub config: PathBuf,
    pub scratch: PathBuf,
    pub state: PathBuf,
}

impl Fixture {
    /// Creates a repository with `open_subtasks` open sub-tasks and initializes a configuration.
    pub fn new(label: &str, open_subtasks: usize) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = root.join("target");
        fs::create_dir_all(target.join("src")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        git(&target, &["init", "--initial-branch=main"]);
        git(&target, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &target,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
        fs::write(target.join("TASKS.md"), task_source(open_subtasks)).unwrap();
        git(&target, &["add", "TASKS.md", "src/lib.rs"]);
        git(&target, &["commit", "-q", "-m", "fixture"]);
        let config = root.join("config/codingmage.toml");
        let scratch = root.join("scratch");
        let state = root.join("state");
        let output = Command::new(coordinator_binary())
            .args([
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
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self {
            root,
            target,
            config,
            scratch,
            state,
        }
    }

    /// Writes an executable fixture script.
    pub fn executable(&self, name: &str, content: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = self.root.join(name);
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    /// Current head of the target.
    pub fn head(&self) -> String {
        git_output(&self.target, &["rev-parse", "HEAD"])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Strict task source with the requested number of open sub-tasks.
pub fn task_source(open_subtasks: usize) -> String {
    let mut source = String::from(
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n",
    );
    for index in 1..=open_subtasks {
        let _ = writeln!(
            source,
            "  - [ ] **Sub-task 0.1.1.{index}:** Complete fixture operation number {index} safely."
        );
    }
    source
        .push_str("\n- [ ] **AC 0.1:** Given the fixture, when it runs, then the value changes.\n");
    source
}

pub fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// System fonts for the display-less harness.
pub fn fonts() -> egui::FontDefinitions {
    let selected = codingmage_ui::fonts::select_fonts().expect("system fonts");
    codingmage_ui::fonts::font_definitions(&selected).expect("font bytes")
}

/// Builds a harness around the application with the given coordinator resolution.
pub fn harness(
    binary: Result<CoordinatorBinary, codingmage_ui::backend::BackendError>,
    size: [f32; 2],
) -> Harness<'static, App> {
    let state_dir = std::env::temp_dir().join(format!(
        "codingmage-ui-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    harness_with_state(binary, size, state_dir)
}

/// Builds a harness with an explicit private state directory.
pub fn harness_with_state(
    binary: Result<CoordinatorBinary, codingmage_ui::backend::BackendError>,
    size: [f32; 2],
    state_dir: PathBuf,
) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::Vec2::new(size[0], size[1]))
        .with_max_steps(4)
        .build_eframe(move |creation| {
            creation.egui_ctx.set_fonts(fonts());
            App::with_state_dir(&creation.egui_ctx, binary, Ok(state_dir))
        })
}

/// Steps the harness until `done` holds or the timeout passes.
pub fn settle(
    harness: &mut Harness<'static, App>,
    timeout: Duration,
    done: impl Fn(&App) -> bool,
) -> bool {
    let started = Instant::now();
    loop {
        harness.step();
        if done(harness.state()) {
            return true;
        }
        if started.elapsed() > timeout {
            let app = harness.state();
            eprintln!(
                "settle timed out: diagnosis={:?} status={:?} head_plan={:?} report={:?}",
                app.diagnosis()
                    .last_error
                    .as_ref()
                    .map(|(_, error)| error.to_string()),
                app.status()
                    .last_error
                    .as_ref()
                    .map(|(_, error)| error.to_string()),
                app.head_plan()
                    .last_error
                    .as_ref()
                    .map(|(_, error)| error.to_string()),
                app.report()
                    .last_error
                    .as_ref()
                    .map(|(_, error)| error.to_string()),
            );
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Snapshot of every regular file under a directory for change detection.
pub fn tree_digest(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out
}

/// Fake Claude implementer: bumps the value in `src/lib.rs` and reports readiness.
pub const FAKE_CLAUDE: &str = r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
packet = sys.stdin.read()
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
"#;

/// Fake Codex lead and reviewer. When a `block-first-task` marker exists next to the script,
/// the lead reports the first offered task as blocked once, then proposes remaining tasks.
pub const FAKE_CODEX: &str = r#"#!/usr/bin/python3
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
    tasks = re.findall(r"- id=([0-9.]+)", packet)
    marker = root / "block-first-task"
    if marker.exists() and "0.1.1.1" in tasks and not (root / "blocked-once").exists():
        (root / "blocked-once").write_text("blocked\n", encoding="utf-8")
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
"#;

/// Reads the diagnosis of a fixture through the real coordinator.
pub fn doctor(fixture: &Fixture) -> serde_json::Value {
    let output = Command::new(coordinator_binary())
        .args(["doctor", "--config", fixture.config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Writes fake providers, a gate that always passes and a serial campaign specification.
pub fn write_campaign(fixture: &Fixture, campaign_id: &str, max_units: u32) -> PathBuf {
    let claude = fixture.executable("fake-claude", FAKE_CLAUDE);
    let codex = fixture.executable("fake-codex", FAKE_CODEX);
    let gate = fixture.executable("fake-gate", "#!/bin/sh\nexit 0\n");
    let configured = fs::read_to_string(&fixture.config).unwrap();
    fs::write(
        &fixture.config,
        configured.replace("/usr/bin/git", gate.to_str().unwrap()),
    )
    .unwrap();
    let diagnosis = doctor(fixture);
    let spec = fixture.root.join(format!("{campaign_id}.toml"));
    fs::write(
        &spec,
        format!(
            r#"version = 3
campaign_id = "{campaign_id}"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = {max_units}
implementer_authentication = "existing_login"
campaign_branch = "codingmage/{campaign_id}"
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
            fixture.target.display(),
            diagnosis["head"].as_str().unwrap(),
            diagnosis["task_source_sha256"].as_str().unwrap(),
            "a".repeat(64),
            codex.display(),
            claude.display(),
            codex.display(),
        ),
    )
    .unwrap();
    spec
}

/// Runs one `codingmage campaign` invocation to its terminal JSON.
pub fn run_campaign(fixture: &Fixture, spec: &Path) -> serde_json::Value {
    let output = Command::new(coordinator_binary())
        .args([
            "campaign",
            "--config",
            fixture.config.to_str().unwrap(),
            "--campaign",
            spec.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "campaign failed: {} {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Writes an authorization record and a controlled-target campaign (ten outcomes) bound to it.
pub fn write_controlled_campaign(fixture: &Fixture, campaign_id: &str) -> (PathBuf, PathBuf) {
    let record = fixture.root.join("operator-authorization.txt");
    fs::write(
        &record,
        b"The owner authorizes this disposable fixture campaign.\n",
    )
    .unwrap();
    let digest = codingmage_ui::project::file_sha256(&record, 1024 * 1024).unwrap();
    let spec = write_campaign(fixture, campaign_id, 10);
    let text = fs::read_to_string(&spec).unwrap();
    fs::write(&spec, text.replace(&"a".repeat(64), &digest)).unwrap();
    (spec, record)
}
