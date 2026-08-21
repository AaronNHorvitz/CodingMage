//! Exact unattended blocker and provider-quota qualification over disposable repositories.

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

const BLOCKER_REASONS: &[&str] = &[
    "unavailable_external_dependency",
    "unavailable_supported_hardware",
    "missing_operator_managed_authentication",
    "unavailable_external_service",
    "unsupported_platform",
    "implementation_condition_outside_authority",
];

#[derive(Clone, Copy)]
enum CaseKind {
    Blocker(&'static str),
    LeadQuota,
    ImplementerQuota,
    ReviewerQuota,
}

impl CaseKind {
    fn id(self) -> String {
        match self {
            Self::Blocker(reason) => format!("blocker-{reason}"),
            Self::LeadQuota => "quota-lead".to_owned(),
            Self::ImplementerQuota => "quota-implementer".to_owned(),
            Self::ReviewerQuota => "quota-reviewer".to_owned(),
        }
    }

    const fn expected_invocations(self) -> &'static [&'static str] {
        match self {
            Self::Blocker(_) | Self::LeadQuota => &["lead"],
            Self::ImplementerQuota => &["lead", "implementer"],
            Self::ReviewerQuota => &["lead", "implementer", "reviewer"],
        }
    }

    const fn expected_terminal(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Blocker(_) => (
                "blocked",
                "no_independent_ready_work",
                "codingmage.campaign.no_unblocked_ready_work",
            ),
            Self::LeadQuota => (
                "paused",
                "capacity_pause",
                "codingmage.provider.codex.quota",
            ),
            Self::ImplementerQuota | Self::ReviewerQuota => (
                "paused",
                "capacity_pause",
                "codingmage.campaign.provider_quota",
            ),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("blocker/quota pilot failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err("usage: blocker_quota_pilot OUTPUT_ROOT CODINGMAGE".into());
    }
    let output_root = PathBuf::from(&arguments[0]);
    if output_root.exists() {
        return Err("output root must not already exist".into());
    }
    fs::create_dir(&output_root)?;
    let output_root = fs::canonicalize(output_root)?;
    let codingmage = canonical_file(Path::new(&arguments[1]))?;
    let mut cases = BLOCKER_REASONS
        .iter()
        .copied()
        .map(CaseKind::Blocker)
        .collect::<Vec<_>>();
    cases.extend([
        CaseKind::LeadQuota,
        CaseKind::ImplementerQuota,
        CaseKind::ReviewerQuota,
    ]);

    let mut reports = Vec::with_capacity(cases.len());
    for case in cases {
        eprintln!("[blocker-quota-pilot] running {}", case.id());
        reports.push(run_case(&output_root, &codingmage, case)?);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "qualification": "sprint-17-unattended-blocker-quota",
            "publication": "local_only",
            "operator_interventions": 0,
            "case_count": reports.len(),
            "external_blocker_cases": BLOCKER_REASONS.len(),
            "quota_boundary_cases": 3,
            "all_active_checkouts_preserved": true,
            "all_tasks_unchecked": true,
            "all_attempts_bounded": true,
            "all_checkpoints_reconciled": true,
            "fixtures_retained": true,
            "cases": reports
        }))?
    );
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_case(
    output_root: &Path,
    codingmage: &Path,
    case: CaseKind,
) -> Result<Value, Box<dyn std::error::Error>> {
    let case_id = case.id();
    let root = output_root.join(&case_id);
    let target = root.join("target");
    let config_root = root.join("config");
    fs::create_dir_all(&target)?;
    fs::create_dir_all(&config_root)?;
    let fixture = materialize_unattended_pilot_fixture(&target)?;
    let invocation_log = root.join("provider-invocations.log");
    let codex = write_fake_codex(&root, &invocation_log, case)?;
    let claude = write_fake_claude(&root, &invocation_log, case)?;
    let config = config_root.join("codingmage.toml");
    let scratch = root.join("scratch");
    let state = root.join("state");

    let initialization = Command::new(codingmage)
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
    let diagnosis = Command::new(codingmage)
        .args(["doctor", "--config"])
        .arg(&config)
        .output()?;
    require_success(&diagnosis, "preflight diagnosis")?;
    let diagnosis: Value = serde_json::from_slice(&diagnosis.stdout)?;
    let campaign = root.join("campaign.toml");
    fs::write(
        &campaign,
        campaign_spec(&diagnosis, &fixture.root, &case_id, &codex, &claude)?,
    )?;

    let head_before = git_output(&fixture.root, &["rev-parse", "HEAD"])?;
    let status_before = git_output(&fixture.root, &["status", "--porcelain=v2", "--branch"])?;
    let tasks_before = fs::read(fixture.root.join("TASKS.md"))?;
    let artifact_before = fs::read(fixture.root.join("artifact.txt"))?;

    let output = Command::new(codingmage)
        .args(["campaign", "--config"])
        .arg(&config)
        .arg("--campaign")
        .arg(&campaign)
        .stdin(Stdio::null())
        .output()?;
    require_success(&output, "bounded campaign")?;
    let outcome: Value = serde_json::from_slice(&output.stdout)?;
    let (expected_state, expected_stop_reason, expected_blocker_code) = case.expected_terminal();
    if outcome["state"] != expected_state
        || outcome["stop_reason"] != expected_stop_reason
        || outcome["blocker_code"] != expected_blocker_code
        || outcome["completed_units"] != 0
        || !outcome["last_task_id"].is_null()
    {
        return Err(format!("unexpected terminal outcome for {case_id}: {outcome}").into());
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
    require_equal(
        &tasks_before,
        &fs::read(fixture.root.join("TASKS.md"))?,
        "active task source",
    )?;
    require_equal(
        &artifact_before,
        &fs::read(fixture.root.join("artifact.txt"))?,
        "active artifact",
    )?;

    let branch = required_text(&outcome, "branch")?;
    let campaign_tasks = git_output(&fixture.root, &["show", &format!("{branch}:TASKS.md")])?;
    if !String::from_utf8(campaign_tasks)?.contains("- [ ] **Sub-task 0.1.1.1:**") {
        return Err(format!("{case_id} falsely completed the task").into());
    }
    let invocations = fs::read_to_string(&invocation_log)?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if invocations
        != case
            .expected_invocations()
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    {
        return Err(format!("{case_id} retried or crossed an unexpected provider boundary").into());
    }

    let status = Command::new(codingmage)
        .args(["campaign-status", "--config"])
        .arg(&config)
        .arg("--campaign")
        .arg(&campaign)
        .output()?;
    require_success(&status, "campaign status")?;
    let status: Value = serde_json::from_slice(&status.stdout)?;
    if status["state"] != expected_state
        || status["blocker_code"] != expected_blocker_code
        || status["completed_units"] != 0
        || !status["current_task_id"].is_null()
    {
        return Err(format!("durable status did not reconcile for {case_id}: {status}").into());
    }
    if let CaseKind::Blocker(reason) = case
        && (status["blocker_count"] != 1
            || status["blockers"][0]["task_id"] != "0.1.1.1"
            || status["blockers"][0]["reason_code"] != reason)
    {
        return Err(format!("durable blocker projection was inexact for {case_id}").into());
    }

    let worktrees = git_output(&fixture.root, &["worktree", "list", "--porcelain"])?;
    if worktrees
        .split(|byte| *byte == b'\n')
        .filter(|line| line.starts_with(b"worktree "))
        .count()
        != 2
    {
        return Err(format!("{case_id} did not retain exactly one campaign worktree").into());
    }
    let pod_scratch = state.join("campaigns").join(&case_id).join("scratch");
    if fs::read_dir(&pod_scratch)?.next().is_some() {
        return Err(format!("{case_id} retained a pod worktree").into());
    }

    Ok(json!({
        "case_id": case_id,
        "state": expected_state,
        "stop_reason": expected_stop_reason,
        "blocker_code": expected_blocker_code,
        "provider_invocations": invocations,
        "provider_attempts": status["utilization"]["provider_attempts"],
        "process_invocations": status["utilization"]["process_invocations"],
        "active_checkout_preserved": true,
        "task_unchecked": true,
        "campaign_worktree_retained_for_resume": true,
        "pod_worktrees_released": true,
        "checkpoint_reconciled": true
    }))
}

fn campaign_spec(
    diagnosis: &Value,
    repository: &Path,
    case_id: &str,
    codex: &Path,
    claude: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        "version = 3\n\
         campaign_id = \"{case_id}\"\n\
         repository_id = \"{}\"\n\
         repository_path = \"{}\"\n\
         initial_commit = \"{}\"\n\
         task_source_sha256 = \"{}\"\n\
         operator_authorization_sha256 = \"{}\"\n\
         max_parallel_pods = 1\n\
         max_units = 1\n\
         implementer_authentication = \"existing_login\"\n\
         campaign_branch = \"codingmage/{case_id}\"\n\
         allowed_paths = [\"artifact.txt\"]\n\
         denied_paths = []\n\
         protected_branches = [\"main\"]\n\
         publication = \"local_only\"\n\n\
         [limits]\n\
         provider_attempts = 8\n\
         malformed_report_repairs = 1\n\
         correction_rounds = 1\n\
         process_invocations = 48\n\
         output_bytes = 1048576\n\
         retained_state_bytes = 4194304\n\
         execution_elapsed_ms = 120000\n\n\
         [team_lead]\n\
         executable = \"{}\"\n\
         model = \"fixture-lead\"\n\
         effort = \"high\"\n\n\
         [implementer]\n\
         executable = \"{}\"\n\
         model = \"fixture-implementer\"\n\
         effort = \"high\"\n\n\
         [reviewer]\n\
         executable = \"{}\"\n\
         model = \"fixture-reviewer\"\n\
         effort = \"high\"\n\n\
         [[gate_tiers]]\n\
         name = \"focused\"\n\
         profiles = [\"configured-gates\"]\n",
        required_text(diagnosis, "repository_id")?,
        repository.display(),
        required_text(diagnosis, "head")?,
        required_text(diagnosis, "task_source_sha256")?,
        hex_digest(format!("owner-approved-{case_id}").as_bytes()),
        codex.display(),
        claude.display(),
        codex.display()
    ))
}

fn write_fake_codex(
    root: &Path,
    invocation_log: &Path,
    case: CaseKind,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = root.join("deterministic-codex");
    let mode = match case {
        CaseKind::Blocker(reason) => format!("blocker:{reason}"),
        CaseKind::LeadQuota => "lead-quota".to_owned(),
        CaseKind::ImplementerQuota => "implementer-quota".to_owned(),
        CaseKind::ReviewerQuota => "reviewer-quota".to_owned(),
    };
    let script = format!(
        r#"#!/usr/bin/python3
import json, re, sys
from pathlib import Path
MODE = {mode}
LOG = Path({log})
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
is_lead = packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET")
with LOG.open("a", encoding="utf-8") as stream:
    stream.write(("lead" if is_lead else "reviewer") + "\n")
if is_lead and MODE == "lead-quota":
    print(json.dumps({{"type": "error", "message": "quota exhausted"}}))
    raise SystemExit(0)
if not is_lead and MODE == "reviewer-quota":
    print(json.dumps({{"type": "error", "message": "quota exhausted"}}))
    raise SystemExit(0)
if is_lead:
    campaign = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
    head = re.search(r"Head: ([0-9a-f]{{40,64}})", packet).group(1)
    digest = re.search(r"Task source SHA-256: ([0-9a-f]{{64}})", packet).group(1)
    task = re.search(r"- id=([0-9.]+)", packet).group(1)
    binding = {{
        "campaign_id": campaign, "campaign_head": head,
        "task_source_sha256": digest, "task_id": task, "dependencies": []
    }}
    if MODE.startswith("blocker:"):
        report = {{
            "campaign_id": campaign, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "blocked",
            "proposals": [],
            "blocked": {{"binding": binding, "reason": MODE.split(":", 1)[1]}},
            "deferred": None, "human_decision": None
        }}
    else:
        report = {{
            "campaign_id": campaign, "campaign_head": head,
            "task_source_sha256": digest, "disposition": "propose",
            "proposals": [{{
                "task_id": task, "dependencies": [], "owned_paths": ["artifact.txt"],
                "gate_tiers": ["focused"], "test_resources": ["text-check"],
                "expected_artifacts": ["artifact.txt"], "risk": "routine",
                "rationale_summary": "The only supplied task is dependency-ready and path-bounded."
            }}],
            "blocked": None, "deferred": None, "human_decision": None
        }}
else:
    base = re.search(r"Base commit: ([0-9a-f]{{40,64}})", packet).group(1)
    target = re.search(r"Target commit: ([0-9a-f]{{40,64}})", packet).group(1)
    report = {{
        "verdict": "pass", "base_commit": base, "target_commit": target,
        "findings": [], "blocker_code": None
    }}
print(json.dumps({{"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}}))
print(json.dumps({{"type": "item.completed", "item": {{"type": "agent_message", "text": json.dumps(report)}}}}))
print(json.dumps({{"type": "turn.completed"}}))
"#,
        mode = serde_json::to_string(&mode)?,
        log = serde_json::to_string(invocation_log.to_str().ok_or("non-UTF-8 log path")?)?
    );
    fs::write(&path, script)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    canonical_file(&path)
}

fn write_fake_claude(
    root: &Path,
    invocation_log: &Path,
    case: CaseKind,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = root.join("deterministic-claude");
    let quota = matches!(case, CaseKind::ImplementerQuota);
    let script = format!(
        r#"#!/usr/bin/python3
import json, sys
from pathlib import Path
QUOTA = {quota}
LOG = Path({log})
if "--version" in sys.argv:
    print("2.1.136 (Claude Code)")
    raise SystemExit(0)
if "--help" in sys.argv:
    print('--print "json" "stream-json" --json-schema --session-id --resume --model --effort --permission-mode --bare')
    raise SystemExit(0)
with LOG.open("a", encoding="utf-8") as stream:
    stream.write("implementer\n")
if QUOTA:
    print(json.dumps({{"type": "result", "is_error": True, "subtype": "rate_limit"}}))
    raise SystemExit(0)
Path("artifact.txt").write_text("phase: complete\n", encoding="utf-8")
print(json.dumps({{
    "type": "result", "is_error": False,
    "structured_output": {{
        "changed_paths": ["artifact.txt"], "tests": [], "commit": None,
        "ready_for_commit": True, "limitations": [], "blocker_code": None
    }}
}}))
"#,
        quota = if quota { "True" } else { "False" },
        log = serde_json::to_string(invocation_log.to_str().ok_or("non-UTF-8 log path")?)?
    );
    fs::write(&path, script)?;
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

fn require_success(
    output: &std::process::Output,
    operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{operation} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
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
