//! Explicit one-story unattended qualification over a disposable repository.

use codingmage_soak::materialize_unattended_pilot_fixture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("unattended pilot failed: {error}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 5 {
        return Err(
            "usage: unattended_live_pilot OUTPUT_ROOT CODINGMAGE CLAUDE CLAUDE_MODEL CLAUDE_EFFORT"
                .into(),
        );
    }
    let output_root = PathBuf::from(&arguments[0]);
    if output_root.exists() {
        return Err("output root must not already exist".into());
    }
    fs::create_dir(&output_root)?;
    let output_root = fs::canonicalize(output_root)?;
    let codingmage = canonical_file(Path::new(&arguments[1]))?;
    let claude = canonical_file(Path::new(&arguments[2]))?;
    let claude_model = text_argument(&arguments[3])?;
    let claude_effort = text_argument(&arguments[4])?;
    let target = output_root.join("target");
    let config_root = output_root.join("config");
    fs::create_dir(&target)?;
    fs::create_dir(&config_root)?;
    let fixture = materialize_unattended_pilot_fixture(&target)?;
    let fake_codex = write_fake_codex(&output_root)?;
    let config = config_root.join("codingmage.toml");
    let scratch = output_root.join("scratch");
    let state = output_root.join("state");

    let initialization = Command::new(&codingmage)
        .args(["init", "--repo"])
        .arg(&fixture.root)
        .arg("--config")
        .arg(&config)
        .arg("--scratch")
        .arg(&scratch)
        .arg("--state")
        .arg(&state)
        .output()?;
    require_success(&initialization, "configuration initialization")?;
    let mut config_file = fs::OpenOptions::new().append(true).open(&config)?;
    config_file.write_all(
        b"\n[[gate_commands]]\nexecutable = \"/usr/bin/grep\"\nargs = [\"-Fxq\", \"phase: complete\", \"artifact.txt\"]\n",
    )?;
    config_file.sync_all()?;
    let diagnosis = Command::new(&codingmage)
        .args(["doctor", "--config"])
        .arg(&config)
        .output()?;
    require_success(&diagnosis, "preflight diagnosis")?;
    let diagnosis: Value = serde_json::from_slice(&diagnosis.stdout)?;
    let campaign = output_root.join("campaign.toml");
    fs::write(
        &campaign,
        format!(
            "version = 3\n\
             campaign_id = \"unattended-pilot\"\n\
             repository_id = \"{}\"\n\
             repository_path = \"{}\"\n\
             initial_commit = \"{}\"\n\
             task_source_sha256 = \"{}\"\n\
             operator_authorization_sha256 = \"{}\"\n\
             max_parallel_pods = 1\n\
             max_units = 1\n\
             implementer_authentication = \"existing_login\"\n\
             campaign_branch = \"codingmage/unattended-pilot\"\n\
             allowed_paths = [\"artifact.txt\"]\n\
             denied_paths = []\n\
             protected_branches = [\"main\"]\n\
             publication = \"local_only\"\n\n\
             [limits]\n\
             provider_attempts = 20\n\
             malformed_report_repairs = 3\n\
             correction_rounds = 3\n\
             process_invocations = 100\n\
             output_bytes = 16777216\n\
             retained_state_bytes = 16777216\n\
             execution_elapsed_ms = 600000\n\n\
             [team_lead]\n\
             executable = \"{}\"\n\
             model = \"fixture-lead\"\n\
             effort = \"high\"\n\n\
             [implementer]\n\
             executable = \"{}\"\n\
             model = \"{claude_model}\"\n\
             effort = \"{claude_effort}\"\n\n\
             [reviewer]\n\
             executable = \"{}\"\n\
             model = \"fixture-reviewer\"\n\
             effort = \"high\"\n\n\
             [[gate_tiers]]\n\
             name = \"focused\"\n\
             profiles = [\"configured-gates\"]\n",
            required_text(&diagnosis, "repository_id")?,
            fixture.root.display(),
            required_text(&diagnosis, "head")?,
            required_text(&diagnosis, "task_source_sha256")?,
            hex_digest(b"owner-approved-sprint-17-unattended-pilot"),
            fake_codex.display(),
            claude.display(),
            fake_codex.display()
        ),
    )?;

    let head_before = git_output(&fixture.root, &["rev-parse", "HEAD"])?;
    let status_before = git_output(&fixture.root, &["status", "--porcelain=v2", "--branch"])?;
    let worktrees_before = git_output(&fixture.root, &["worktree", "list", "--porcelain"])?;
    let task_before = fs::read(fixture.root.join("TASKS.md"))?;
    let artifact_before = fs::read(fixture.root.join("artifact.txt"))?;

    eprintln!("[unattended-pilot] launching one bounded story without operator controls");
    let output = Command::new(&codingmage)
        .args(["campaign", "--config"])
        .arg(&config)
        .arg("--campaign")
        .arg(&campaign)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?
        .wait_with_output()?;
    require_success(&output, "unattended campaign")?;
    let outcome: Value = serde_json::from_slice(&output.stdout)?;
    if outcome["state"] != "complete"
        || outcome["stop_reason"] != "completion"
        || outcome["completed_units"] != 1
        || outcome["last_task_id"] != "0.1.1.1"
    {
        return Err("unexpected unattended terminal outcome".into());
    }
    let branch = required_text(&outcome, "branch")?;
    let head = required_text(&outcome, "head")?;
    if !valid_object_id(head) {
        return Err("invalid unattended campaign head".into());
    }

    require_equal(
        &head_before,
        &git_output(&fixture.root, &["rev-parse", "HEAD"])?,
        "active HEAD",
    )?;
    require_equal(
        &status_before,
        &git_output(&fixture.root, &["status", "--porcelain=v2", "--branch"])?,
        "active status",
    )?;
    let worktrees_after = git_output(&fixture.root, &["worktree", "list", "--porcelain"])?;
    if worktrees_before
        .split(|byte| *byte == b'\n')
        .filter(|line| line.starts_with(b"worktree "))
        .count()
        != 1
        || worktrees_after
            .split(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(b"worktree "))
            .count()
            != 2
    {
        return Err("unexpected campaign worktree inventory".into());
    }
    require_equal(
        &task_before,
        &fs::read(fixture.root.join("TASKS.md"))?,
        "active task source",
    )?;
    require_equal(
        &artifact_before,
        &fs::read(fixture.root.join("artifact.txt"))?,
        "active artifact",
    )?;
    let retained = fs::read_dir(&scratch)?.collect::<Result<Vec<_>, _>>()?;
    if retained.len() != 1 || !retained[0].file_type()?.is_dir() {
        return Err("campaign root worktree retention was not exact".into());
    }
    let retained_path = fs::canonicalize(retained[0].path())?;
    let retained_marker = format!("worktree {}\n", retained_path.display());
    if !String::from_utf8(worktrees_after)?.contains(&retained_marker) {
        return Err("retained campaign worktree was not Git-bound".into());
    }
    let pod_scratch = state.join("campaigns/unattended-pilot/scratch");
    if fs::read_dir(&pod_scratch)?.next().is_some() {
        return Err("pod worktree residue remained after unattended campaign".into());
    }
    require_equal(
        b"phase: complete\nreviewed: yes\n",
        &git_output(&fixture.root, &["show", &format!("{branch}:artifact.txt")])?,
        "review-corrected artifact",
    )?;
    let completed_tasks = git_output(&fixture.root, &["show", &format!("{branch}:TASKS.md")])?;
    if !String::from_utf8(completed_tasks)?.contains("- [x] **Sub-task 0.1.1.1:**") {
        return Err("campaign completion marker missing".into());
    }

    let status = Command::new(&codingmage)
        .args(["campaign-status", "--config"])
        .arg(&config)
        .arg("--campaign")
        .arg(&campaign)
        .output()?;
    require_success(&status, "campaign status")?;
    let status: Value = serde_json::from_slice(&status.stdout)?;
    if status["state"] != "complete"
        || status["completed_units"] != 1
        || status["outcomes"]["completed"] != 1
        || status["utilization"]["correction_rounds"] != 1
        || status["blocker_count"] != 0
    {
        return Err("durable unattended status did not reconcile".into());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "qualification": "sprint-17-unattended-story",
            "publication": "local_only",
            "operator_interventions": 0,
            "state": outcome["state"],
            "stop_reason": outcome["stop_reason"],
            "completed_units": outcome["completed_units"],
            "task_id": outcome["last_task_id"],
            "campaign_head": head,
            "correction_rounds": status["utilization"]["correction_rounds"],
            "provider_attempts": status["utilization"]["provider_attempts"],
            "process_invocations": status["utilization"]["process_invocations"],
            "active_checkout_preserved": true,
            "pod_worktrees_released": true,
            "campaign_worktree_retained_for_resume": true,
            "checkpoint_reconciled": true,
            "fixtures_retained": true
        }))?
    );
    Ok(())
}

fn write_fake_codex(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = root.join("deterministic-codex");
    fs::write(
        &path,
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
    campaign = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    report = {
        "campaign_id": campaign, "campaign_head": head,
        "task_source_sha256": digest, "disposition": "propose",
        "proposals": [{
            "task_id": task, "dependencies": [], "owned_paths": ["artifact.txt"],
            "gate_tiers": ["focused"], "test_resources": ["text-check"],
            "expected_artifacts": ["artifact.txt"], "risk": "routine",
            "rationale_summary": "The only supplied task is dependency-ready and path-bounded."
        }],
        "blocked": None, "deferred": None, "human_decision": None
    }
else:
    base = re.search(r"Base commit: ([0-9a-f]{40,64})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{40,64})", packet).group(1)
    corrected = "reviewed: yes" in Path("artifact.txt").read_text(encoding="utf-8")
    report = {
        "verdict": "pass" if corrected else "changes_required",
        "base_commit": base, "target_commit": target,
        "findings": [] if corrected else [{
            "id": "PILOT-REVIEW-1", "kind": "defect", "severity": "medium",
            "file": "artifact.txt", "line": 1,
            "claim": "The bounded artifact lacks the required review attestation.",
            "evidence": "The exact reviewed marker is absent.",
            "requested_correction": "Add a second line containing exactly 'reviewed: yes'.",
            "acceptance_test": "artifact.txt contains phase: complete followed by reviewed: yes."
        }],
        "blocker_code": None
    }
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#,
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    canonical_file(&path)
}

fn canonical_file(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("executable paths must name regular files, not links".into());
    }
    Ok(fs::canonicalize(path)?)
}

fn text_argument(value: &std::ffi::OsStr) -> Result<&str, Box<dyn std::error::Error>> {
    value
        .to_str()
        .ok_or_else(|| "profile arguments must be UTF-8".into())
}

fn require_success(
    output: &std::process::Output,
    operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{operation} failed").into())
    }
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["--no-pager", "-c", "core.hooksPath=/dev/null"])
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err("Git evidence observation failed".into());
    }
    Ok(output.stdout)
}

fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("missing field {field}").into())
}

fn valid_object_id(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require_equal(
    expected: &[u8],
    actual: &[u8],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!("{label} was not preserved exactly").into())
    }
}

fn hex_digest(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
