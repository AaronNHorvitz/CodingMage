//! Binary-level campaign preflight qualification without model inference.

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
            "codingmage-campaign-preflight-{}-{}",
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

fn preflight(config: &Path, campaign: &Path, authorization: &Path) -> Output {
    Fixture::command(&[
        "campaign-preflight",
        "--config",
        config.to_str().unwrap(),
        "--campaign",
        campaign.to_str().unwrap(),
        "--authorization",
        authorization.to_str().unwrap(),
    ])
}

#[test]
#[allow(clippy::too_many_lines)]
fn controlled_campaign_preflight_is_source_free_and_fails_closed() {
    let fixture = Fixture::new();
    let target = fixture.root.join("target");
    let mut task_items = String::new();
    for number in 1..=10 {
        writeln!(
            task_items,
            "  - [ ] **Sub-task 0.1.1.{number}:** PRIVATE_TASK_PROSE_{number}."
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
    fs::write(target.join("src/baseline.txt"), "preserved\n").unwrap();
    git(&target, &["add", "TASKS.md", "src"]);
    git(&target, &["commit", "-m", "add controlled fixture"]);
    git(&target, &["switch", "-c", "controlled-target"]);
    let initial_head = git_output(&target, &["rev-parse", "HEAD"]);
    let initial_worktrees = git_output(&target, &["worktree", "list", "--porcelain"]);

    let authorization = fixture.root.join("authorization.txt");
    fs::write(&authorization, "PRIVATE_OPERATOR_AUTHORIZATION_RECORD\n").unwrap();
    let authorization_sha256 = sha256(&authorization);
    let inference_marker = fixture.root.join("MODEL_INFERENCE_CALLED");
    let claude = fixture.executable(
        "preflight-claude",
        r#"#!/usr/bin/python3
import sys
from pathlib import Path
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
Path(__file__).with_name("MODEL_INFERENCE_CALLED").write_text("claude\n", encoding="utf-8")
raise SystemExit(9)
"#,
    );
    let codex = fixture.executable(
        "preflight-codex",
        r#"#!/usr/bin/python3
import sys
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
Path(__file__).with_name("MODEL_INFERENCE_CALLED").write_text("codex\n", encoding="utf-8")
raise SystemExit(9)
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
    let doctor = Fixture::command(&["doctor", "--config", config.to_str().unwrap()]);
    assert!(doctor.status.success());
    let diagnosis: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let campaign = fixture.root.join("campaign.toml");
    fs::write(
        &campaign,
        format!(
            r#"version = 3
campaign_id = "controlled-preflight"
repository_id = "{}"
repository_path = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/controlled-preflight"
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
retained_state_bytes = 1048576
execution_elapsed_ms = 86400000

[team_lead]
executable = "{}"
model = "PRIVATE_LEAD_MODEL"
effort = "high"

[implementer]
executable = "{}"
model = "PRIVATE_IMPLEMENTER_MODEL"
effort = "high"

[reviewer]
executable = "{}"
model = "PRIVATE_REVIEWER_MODEL"
effort = "high"

[[gate_tiers]]
name = "focused"
profiles = ["configured-gates"]
"#,
            diagnosis["repository_id"].as_str().unwrap(),
            target.display(),
            diagnosis["head"].as_str().unwrap(),
            diagnosis["task_source_sha256"].as_str().unwrap(),
            authorization_sha256,
            codex.display(),
            claude.display(),
            codex.display(),
        ),
    )
    .unwrap();

    let output = preflight(&config, &campaign, &authorization);
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["state"], "ready");
    assert_eq!(report["source_free"], true);
    assert_eq!(report["repository"]["initial_commit"], initial_head);
    assert_eq!(report["repository"]["open_subtask_count"], 10);
    assert_eq!(report["repository"]["clean"], true);
    assert_eq!(report["repository"]["dedicated_branch"], true);
    assert_eq!(report["repository"]["checkout_safe"], true);
    assert_eq!(report["policy"]["max_parallel_pods"], 1);
    assert_eq!(report["policy"]["max_accepted_outcomes"], 10);
    assert_eq!(report["policy"]["publication"], "local_only");
    assert_eq!(report["policy"]["allowed_path_count"], 1);
    assert_eq!(report["policy"]["external_capabilities_denied"], true);
    assert_eq!(report["providers"].as_array().unwrap().len(), 3);
    assert_eq!(report["providers"][0]["probe_process_count"], 2);
    assert_eq!(report["providers"][1]["probe_process_count"], 3);
    assert_eq!(report["providers"][2]["probe_process_count"], 3);
    assert_eq!(report["gates"]["command_count"], 1);
    assert_eq!(report["controls"]["operator_control_count"], 4);
    assert_eq!(report["controls"]["process_guard_verified"], true);
    assert_eq!(report["storage"]["sufficient"], true);
    assert_eq!(report["storage"]["scratch_sufficient"], true);
    assert_eq!(report["storage"]["state_sufficient"], true);
    assert!(report["storage"].get("scratch_available_bytes").is_none());
    assert!(report["storage"].get("state_available_bytes").is_none());
    assert!(!inference_marker.exists());
    assert!(!state.join("campaigns").exists());
    assert_eq!(
        git_output(&target, &["worktree", "list", "--porcelain"]),
        initial_worktrees
    );
    assert_eq!(git_output(&target, &["status", "--porcelain=v1"]), "");

    let repeated = preflight(&config, &campaign, &authorization);
    assert!(repeated.status.success());
    assert_eq!(repeated.stdout, output.stdout);

    let encoded = String::from_utf8(output.stdout).unwrap();
    for prohibited in [
        target.to_str().unwrap(),
        claude.to_str().unwrap(),
        codex.to_str().unwrap(),
        "PRIVATE_TASK_PROSE",
        "PRIVATE_OPERATOR_AUTHORIZATION_RECORD",
        "PRIVATE_LEAD_MODEL",
        "PRIVATE_IMPLEMENTER_MODEL",
        "PRIVATE_REVIEWER_MODEL",
    ] {
        assert!(!encoded.contains(prohibited), "report leaked {prohibited}");
    }

    fs::write(&authorization, "changed authorization\n").unwrap();
    assert!(
        !preflight(&config, &campaign, &authorization)
            .status
            .success()
    );
    fs::write(&authorization, "PRIVATE_OPERATOR_AUTHORIZATION_RECORD\n").unwrap();

    fs::write(target.join("src/dirty.txt"), "dirty\n").unwrap();
    assert!(
        !preflight(&config, &campaign, &authorization)
            .status
            .success()
    );
    fs::remove_file(target.join("src/dirty.txt")).unwrap();

    git(&target, &["switch", "main"]);
    assert!(
        !preflight(&config, &campaign, &authorization)
            .status
            .success()
    );
    git(&target, &["switch", "controlled-target"]);

    let wrong_ceiling = fixture.root.join("wrong-ceiling.toml");
    fs::write(
        &wrong_ceiling,
        fs::read_to_string(&campaign)
            .unwrap()
            .replace("max_units = 10", "max_units = 11"),
    )
    .unwrap();
    assert!(
        !preflight(&config, &wrong_ceiling, &authorization)
            .status
            .success()
    );
    assert!(!inference_marker.exists());
    assert_eq!(git_output(&target, &["rev-parse", "HEAD"]), initial_head);
    assert_eq!(git_output(&target, &["status", "--porcelain=v1"]), "");
}
