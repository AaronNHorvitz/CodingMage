//! Binary-level mission charter admission, preflight, revocation and answer controls.
//!
//! No model inference, campaign worktree or target mutation happens here: every command either
//! validates and records durable mission authority or is refused before any effect.

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
            "codingmage-campaign-mission-{}-{}",
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

struct Campaign {
    fixture: Fixture,
    config: PathBuf,
    spec: PathBuf,
    authorization: PathBuf,
    state: PathBuf,
    authority_sha256: String,
    id: String,
    repository_id: String,
    initial_commit: String,
    task_source_sha256: String,
    authorization_sha256: String,
}

const PROBE_ONLY_LEAD: &str = r#"#!/usr/bin/python3
import sys
if "--version" in sys.argv:
    print("codex-cli 0.144.5")
    raise SystemExit(0)
if "--help" in sys.argv and "resume" in sys.argv:
    print("SESSION_ID --json --output-schema --model --ignore-user-config")
    raise SystemExit(0)
if "--help" in sys.argv:
    print("Run Codex non-interactively --json --output-schema resume --model read-only --ignore-user-config")
    raise SystemExit(0)
raise SystemExit(9)
"#;

/// A lead that asks one typed mission decision for the first task, then blocks every task.
///
/// The decision domain comes from `decision-domain.txt` beside the script so one script serves
/// delegated and undelegated scenarios. Once the packet lists an accepted decision the lead
/// stops asking and records that it observed the acceptance.
const DECIDING_LEAD: &str = r#"#!/usr/bin/python3
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
if not packet.startswith("CODINGMAGE READ-ONLY CAMPAIGN LEAD PACKET"):
    raise SystemExit(9)
campaign_id = re.search(r"Campaign: ([A-Za-z0-9._-]+)", packet).group(1)
head = re.search(r"Head: ([0-9a-f]{40,64})", packet).group(1)
digest = re.search(r"Task source SHA-256: ([0-9a-f]{64})", packet).group(1)
task = re.search(r"- id=([0-9.]+)", packet).group(1)
accepted = "decision_id=layout-1" in packet
domain = (root / "decision-domain.txt").read_text(encoding="utf-8").strip()
with (root / "lead.log").open("a", encoding="utf-8") as stream:
    stream.write(task + ("|accepted" if accepted else "") + "\n")
binding = {
    "campaign_id": campaign_id, "campaign_head": head,
    "task_source_sha256": digest, "task_id": task, "dependencies": []
}
asked = root / "decision-asked"
if task == "0.1.1.1" and not accepted and not asked.exists():
    asked.write_text("asked\n", encoding="utf-8")
    report = {
        "campaign_id": campaign_id, "campaign_head": head,
        "task_source_sha256": digest, "disposition": "human_decision_required",
        "proposals": [], "blocked": None, "deferred": None,
        "human_decision": {
            "binding": binding, "reason": "material_architecture_choice",
            "summary": "PRIVATE_SUMMARY choose the storage layout",
            "decision": {
                "decision_id": "layout-1", "domain_id": domain, "class": "architecture",
                "selected_alternative": "flat", "affected_paths": ["src/baseline.txt"],
                "risk": "routine", "gate_tiers": ["focused"],
                "rationale_summary": "PRIVATE_RATIONALE flat is simpler"
            }
        }
    }
else:
    report = {
        "campaign_id": campaign_id, "campaign_head": head,
        "task_source_sha256": digest, "disposition": "blocked", "proposals": [],
        "blocked": {"binding": binding, "reason": "unavailable_external_dependency"},
        "deferred": None, "human_decision": None
    }
print(json.dumps({"type": "thread.started", "thread_id": "123e4567-e89b-12d3-a456-426614174000"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": json.dumps(report)}}))
print(json.dumps({"type": "turn.completed"}))
"#;

impl Campaign {
    fn new() -> Self {
        Self::with_lead_script(PROBE_ONLY_LEAD)
    }

    #[allow(clippy::too_many_lines)]
    fn with_lead_script(lead_script: &str) -> Self {
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
        git(&target, &["commit", "-m", "add mission fixture"]);
        git(&target, &["switch", "-c", "controlled-target"]);

        let authorization = fixture.root.join("authorization.txt");
        fs::write(&authorization, "PRIVATE_OPERATOR_AUTHORIZATION_RECORD\n").unwrap();
        let authorization_sha256 = sha256(&authorization);
        let claude = fixture.executable(
            "mission-claude",
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
        let codex = fixture.executable("mission-codex", lead_script);

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
        let initial_commit = diagnosis["head"].as_str().unwrap().to_owned();
        let task_source_sha256 = diagnosis["task_source_sha256"].as_str().unwrap().to_owned();
        let campaign = fixture.root.join("campaign.toml");
        fs::write(
            &campaign,
            format!(
                r#"version = 3
campaign_id = "mission-fixture"
repository_id = "{repository_id}"
repository_path = "{}"
initial_commit = "{initial_commit}"
task_source_sha256 = "{task_source_sha256}"
operator_authorization_sha256 = "{authorization_sha256}"
max_parallel_pods = 1
max_units = 10
implementer_authentication = "existing_login"
campaign_branch = "codingmage/mission-fixture"
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
                target.display(),
                codex.display(),
                claude.display(),
                codex.display(),
            ),
        )
        .unwrap();
        let preflight = Fixture::command(&[
            "campaign-preflight",
            "--config",
            config.to_str().unwrap(),
            "--campaign",
            campaign.to_str().unwrap(),
            "--authorization",
            authorization.to_str().unwrap(),
        ]);
        assert!(
            preflight.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&preflight.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&preflight.stdout).unwrap();
        assert!(
            report.get("mission").is_none(),
            "no charter, no mission section"
        );
        Self {
            fixture,
            config,
            spec: campaign,
            authorization,
            state,
            authority_sha256: report["authority_sha256"].as_str().unwrap().to_owned(),
            id: "mission-fixture".to_owned(),
            repository_id,
            initial_commit,
            task_source_sha256,
            authorization_sha256,
        }
    }

    fn charter(
        &self,
        name: &str,
        generation: u64,
        involvement: &str,
        escalation: &str,
        objective: &str,
    ) -> PathBuf {
        let now_ms = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        self.charter_with_expiry(
            name,
            generation,
            involvement,
            escalation,
            objective,
            now_ms - 60_000,
            now_ms + 30 * 24 * 60 * 60 * 1000,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn charter_with_expiry(
        &self,
        name: &str,
        generation: u64,
        involvement: &str,
        escalation: &str,
        objective: &str,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> PathBuf {
        let path = self.fixture.root.join(name);
        fs::write(
            &path,
            format!(
                r#"version = 1
mission_id = "mission-one"
generation = {generation}
campaign_id = "{}"
repository_id = "{}"
initial_commit = "{}"
task_source_sha256 = "{}"
operator_authorization_sha256 = "{}"
campaign_authority_sha256 = "{}"
objective = "{objective}"
success_criteria = ["PRIVATE_CRITERION every unit accepted"]
exclusions = ["PRIVATE_EXCLUSION no network"]
architecture_invariants = ["one writer per checkout"]
command_registry = ["configured-gates"]
involvement = "{involvement}"
issued_at_ms = {issued_at_ms}
expires_at_ms = {expires_at_ms}
revocation_epoch = 0

[budgets]
max_decisions = 10
max_decision_retries = 2
max_no_progress_cycles = 3

[[decision_domains]]
domain_id = "layout"
class = "architecture"
alternatives = ["flat", "nested"]
scope_paths = ["src"]
max_risk = "routine"
required_gate_tiers = ["focused"]
escalation = "{escalation}"
"#,
                self.id,
                self.repository_id,
                self.initial_commit,
                self.task_source_sha256,
                self.authorization_sha256,
                self.authority_sha256,
            ),
        )
        .unwrap();
        path
    }

    fn run(&self, command: &str, extra: &[&str]) -> Output {
        let mut arguments = vec![
            command,
            "--config",
            self.config.to_str().unwrap(),
            "--campaign",
            self.spec.to_str().unwrap(),
        ];
        arguments.extend_from_slice(extra);
        Fixture::command(&arguments)
    }

    fn json(&self, command: &str, extra: &[&str]) -> serde_json::Value {
        let output = self.run(command, extra);
        assert!(
            output.status.success(),
            "{command} {extra:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn refused(&self, command: &str, extra: &[&str]) -> String {
        let output = self.run(command, extra);
        assert!(
            !output.status.success(),
            "{command} {extra:?} unexpectedly succeeded"
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    fn state_files_are_content_free(&self) {
        let mut pending = vec![self.state.clone()];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else if entry.file_name() != "mission.json"
                    && let Ok(text) = fs::read_to_string(entry.path())
                {
                    assert!(
                        !text.contains("PRIVATE_"),
                        "{} retains private content",
                        entry.path().display()
                    );
                }
            }
        }
    }
}

#[test]
fn mission_admission_binds_the_exact_campaign_and_is_idempotent() {
    let campaign = Campaign::new();
    let charter = campaign.charter("mission.toml", 1, "hands_off", "block", "PRIVATE_OBJECTIVE");
    let mission = charter.to_str().unwrap();

    let admitted = campaign.json("campaign-mission-admit", &["--mission", mission]);
    assert_eq!(admitted["action"], "admit");
    assert_eq!(admitted["created"], true);
    assert_eq!(admitted["generation"], 1);
    assert_eq!(admitted["revocation_epoch"], 0);

    let repeated = campaign.json("campaign-mission-admit", &["--mission", mission]);
    assert_eq!(repeated["created"], false);
    assert_eq!(repeated["generation"], 1);

    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["involvement"], "hands_off");
    assert_eq!(status["generation"], 1);
    assert_eq!(status["revoked"], false);
    assert_eq!(status["expired"], false);
    assert_eq!(status["pending_owner_decisions"], 0);
    assert_eq!(status["authority_sha256"], campaign.authority_sha256);
    assert!(!serde_json::to_string(&status).unwrap().contains("PRIVATE_"));
    campaign.state_files_are_content_free();

    let preflight = campaign.json(
        "campaign-preflight",
        &[
            "--authorization",
            campaign.authorization.to_str().unwrap(),
            "--mission",
            mission,
        ],
    );
    let section = &preflight["mission"];
    assert_eq!(section["involvement"], "hands_off");
    assert_eq!(section["durable_state"], "bound");
    assert_eq!(section["no_intervention_valid"], true);
    assert_eq!(section["interactive_prerequisites"], serde_json::json!([]));
    assert_eq!(section["decision_domains"], 1);
    assert_eq!(section["roles"].as_array().unwrap().len(), 3);
    for role in section["roles"].as_array().unwrap() {
        assert_eq!(role["executable_available"], true);
    }
    assert!(
        section["retained_holds"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("task_integration_human_required"))
    );
    let text = serde_json::to_string(&preflight).unwrap();
    assert!(!text.contains("PRIVATE_"), "preflight leaked charter text");
}

#[test]
fn reissue_requires_a_higher_generation_of_the_same_mission() {
    let campaign = Campaign::new();
    let first = campaign.charter("mission-1.toml", 1, "supervised", "ask_owner", "first");
    campaign.json(
        "campaign-mission-admit",
        &["--mission", first.to_str().unwrap()],
    );

    let same_generation = campaign.charter("mission-1b.toml", 1, "supervised", "block", "changed");
    let stderr = campaign.refused(
        "campaign-mission-admit",
        &["--mission", same_generation.to_str().unwrap()],
    );
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");

    let next = campaign.charter("mission-2.toml", 2, "hands_off", "block", "second");
    let reissued = campaign.json(
        "campaign-mission-admit",
        &["--mission", next.to_str().unwrap()],
    );
    assert_eq!(reissued["action"], "reissue");
    assert_eq!(reissued["created"], true);
    assert_eq!(reissued["generation"], 2);
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["involvement"], "hands_off");

    let stale = campaign.refused(
        "campaign-mission-admit",
        &["--mission", first.to_str().unwrap()],
    );
    assert!(stale.contains("codingmage.runtime.authority"), "{stale}");
    let preflight_stale = campaign.json(
        "campaign-preflight",
        &[
            "--authorization",
            campaign.authorization.to_str().unwrap(),
            "--mission",
            first.to_str().unwrap(),
        ],
    );
    assert_eq!(
        preflight_stale["mission"]["durable_state"], "mismatch",
        "preflight reports a superseded charter; admission refuses it"
    );
    campaign.state_files_are_content_free();
}

#[test]
fn unbound_or_prompting_charters_are_refused_before_any_effect() {
    let campaign = Campaign::new();
    let prompting = campaign.charter("prompting.toml", 1, "hands_off", "ask_owner", "prompt");
    let stderr = campaign.refused(
        "campaign-mission-admit",
        &["--mission", prompting.to_str().unwrap()],
    );
    assert!(
        stderr.contains("codingmage.runtime.campaign.authority"),
        "{stderr}"
    );

    let unbound = campaign.charter("unbound.toml", 1, "supervised", "block", "unbound");
    let text = fs::read_to_string(&unbound).unwrap();
    fs::write(
        &unbound,
        text.replace(&campaign.authorization_sha256, &"0".repeat(64)),
    )
    .unwrap();
    let stderr = campaign.refused(
        "campaign-mission-admit",
        &["--mission", unbound.to_str().unwrap()],
    );
    assert!(
        stderr.contains("codingmage.runtime.campaign.authority"),
        "{stderr}"
    );

    let unknown = campaign.charter("unknown.toml", 1, "supervised", "block", "unknown");
    let mut text = fs::read_to_string(&unknown).unwrap();
    text.push_str("\nunexpected_grant = \"everything\"\n");
    fs::write(&unknown, text).unwrap();
    let stderr = campaign.refused(
        "campaign-mission-admit",
        &["--mission", unknown.to_str().unwrap()],
    );
    assert!(
        stderr.contains("codingmage.runtime.campaign.spec"),
        "{stderr}"
    );

    assert!(!campaign.state.join("missions").exists());
    let status = campaign.refused("campaign-mission-status", &[]);
    assert!(status.contains("codingmage.runtime.state"), "{status}");
}

#[test]
fn revocation_is_authenticated_idempotent_and_final() {
    let campaign = Campaign::new();
    let charter = campaign.charter("mission.toml", 1, "exception_only", "ask_owner", "x");
    let mission = charter.to_str().unwrap();
    campaign.json("campaign-mission-admit", &["--mission", mission]);
    let preflight = campaign.json(
        "campaign-preflight",
        &[
            "--authorization",
            campaign.authorization.to_str().unwrap(),
            "--mission",
            mission,
        ],
    );
    assert_eq!(
        preflight["mission"]["interactive_prerequisites"],
        serde_json::json!(["owner_exception_channel"])
    );
    assert_eq!(preflight["mission"]["no_intervention_valid"], false);
    assert_eq!(
        preflight["mission"]["no_intervention_defects"],
        serde_json::json!(["owner_prompt_configured"])
    );

    let stderr = campaign.refused("campaign-mission-revoke", &["--request", "bad id"]);
    assert!(stderr.contains("codingmage.runtime.spec"), "{stderr}");

    let revoked = campaign.json("campaign-mission-revoke", &["--request", "revoke-1"]);
    assert_eq!(revoked["created"], true);
    assert_eq!(revoked["revocation_epoch"], 1);
    let repeated = campaign.json("campaign-mission-revoke", &["--request", "revoke-1"]);
    assert_eq!(repeated["created"], false);
    let second = campaign.json("campaign-mission-revoke", &["--request", "revoke-2"]);
    assert_eq!(second["created"], true);
    assert_eq!(
        second["revocation_epoch"], 1,
        "epoch advances once per revocation"
    );

    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["revoked"], true);
    assert_eq!(status["revocation_epoch"], 1);

    let next = campaign.charter("mission-2.toml", 2, "hands_off", "block", "y");
    let stderr = campaign.refused(
        "campaign-mission-admit",
        &["--mission", next.to_str().unwrap()],
    );
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");

    let stderr = campaign.refused(
        "campaign-mission-answer",
        &[
            "--decision",
            "decision-1",
            "--request",
            "answer-1",
            "--answer",
            "flat",
        ],
    );
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");
    let preflight = campaign.json(
        "campaign-preflight",
        &[
            "--authorization",
            campaign.authorization.to_str().unwrap(),
            "--mission",
            mission,
        ],
    );
    assert_eq!(preflight["mission"]["durable_state"], "mismatch");
    campaign.state_files_are_content_free();
}

#[test]
fn answers_are_refused_without_a_pending_decision() {
    let campaign = Campaign::new();
    let charter = campaign.charter("mission.toml", 1, "supervised", "ask_owner", "x");
    let mission = charter.to_str().unwrap();
    campaign.json("campaign-mission-admit", &["--mission", mission]);
    let stderr = campaign.refused(
        "campaign-mission-answer",
        &[
            "--decision",
            "decision-1",
            "--request",
            "answer-1",
            "--answer",
            "flat",
        ],
    );
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["owner_answers"], 0);
    assert_eq!(status["pending_owner_decisions"], 0);
}

fn inference_marker(campaign: &Campaign) -> PathBuf {
    campaign.fixture.root.join("MODEL_INFERENCE_CALLED")
}

#[test]
fn revoked_mission_stops_a_real_campaign_process_before_any_provider_runs() {
    let campaign = Campaign::new();
    let charter = campaign.charter("mission.toml", 1, "hands_off", "block", "x");
    let mission = charter.to_str().unwrap();
    campaign.json("campaign-mission-admit", &["--mission", mission]);
    campaign.json("campaign-mission-revoke", &["--request", "revoke-1"]);

    let outcome = campaign.json("campaign", &[]);
    assert_eq!(outcome["state"], "cancelled");
    assert_eq!(outcome["stop_reason"], "mission_revoked");
    assert_eq!(outcome["blocker_code"], "codingmage.mission.revoked");
    assert_eq!(outcome["completed_units"], 0);
    assert!(
        !inference_marker(&campaign).exists(),
        "no provider may run after revocation"
    );

    let again = campaign.json("campaign", &[]);
    assert_eq!(again["state"], "cancelled");
    assert_eq!(again["stop_reason"], "mission_revoked");
    campaign.state_files_are_content_free();
}

#[test]
fn expired_mission_holds_a_real_campaign_and_hands_off_admission_is_refused() {
    let campaign = Campaign::new();
    let supervised =
        campaign.charter_with_expiry("expired.toml", 1, "supervised", "block", "x", 1, 2);
    let mission = supervised.to_str().unwrap();
    campaign.json("campaign-mission-admit", &["--mission", mission]);
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["expired"], true);

    let outcome = campaign.json("campaign", &[]);
    assert_eq!(outcome["state"], "blocked");
    assert_eq!(outcome["stop_reason"], "mission_expired");
    assert_eq!(outcome["blocker_code"], "codingmage.mission.expired");
    assert!(!inference_marker(&campaign).exists());

    let hands_off =
        campaign.charter_with_expiry("expired-hands-off.toml", 2, "hands_off", "block", "y", 1, 2);
    let stderr = campaign.refused("campaign", &["--mission", hands_off.to_str().unwrap()]);
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(
        status["generation"], 1,
        "a refused hands-off charter is not admitted"
    );
}

fn control(campaign: &Campaign, action: &str, request: &str) -> serde_json::Value {
    campaign.json(
        "campaign-control",
        &["--action", action, "--request", request],
    )
}

fn lead_log(campaign: &Campaign) -> String {
    fs::read_to_string(campaign.fixture.root.join("lead.log")).unwrap_or_default()
}

#[test]
fn hands_off_mission_resolves_a_delegated_lead_decision_in_a_real_campaign_process() {
    let campaign = Campaign::with_lead_script(DECIDING_LEAD);
    fs::write(
        campaign.fixture.root.join("decision-domain.txt"),
        "layout\n",
    )
    .unwrap();
    let charter = campaign.charter("mission.toml", 1, "hands_off", "block", "x");
    let mission = charter.to_str().unwrap();

    let outcome = campaign.json("campaign", &["--mission", mission]);
    assert_eq!(outcome["state"], "paused", "{outcome}");
    assert_eq!(outcome["stop_reason"], "unit_limit", "{outcome}");
    assert_eq!(outcome["completed_units"], 0);
    assert!(
        !inference_marker(&campaign).exists(),
        "no implementation provider ran"
    );
    let log = lead_log(&campaign);
    assert!(log.starts_with("0.1.1.1\n0.1.1.1|accepted\n"), "{log}");

    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["decisions_recorded"], 1);
    assert_eq!(status["permitted_choices"], 1);
    assert_eq!(status["held_decisions"], 0);
    assert_eq!(
        status["pending_owner_decisions"], 0,
        "hands-off never asks the owner"
    );
    campaign.state_files_are_content_free();
    let campaign_status = campaign.json("campaign-status", &[]);
    assert!(
        !serde_json::to_string(&campaign_status)
            .unwrap()
            .contains("PRIVATE_")
    );

    assert_eq!(control(&campaign, "pause", "pause-1")["created"], true);
    assert_eq!(control(&campaign, "resume", "resume-1")["created"], true);
    assert_eq!(
        control(&campaign, "stop_after_unit", "stop-1")["created"],
        true
    );
    assert_eq!(control(&campaign, "resume", "resume-2")["created"], true);
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(
        status["revocation_epoch"], 0,
        "controls never touch mission authority"
    );
    assert_eq!(status["generation"], 1);

    let lead_calls_before = lead_log(&campaign).lines().count();
    campaign.json("campaign-mission-revoke", &["--request", "revoke-1"]);
    let revoked = campaign.json("campaign", &[]);
    match revoked["state"].as_str() {
        Some("cancelled") => assert_eq!(revoked["stop_reason"], "mission_revoked", "{revoked}"),
        Some("paused") => assert_eq!(
            revoked["stop_reason"], "unit_limit",
            "the ceiling reached earlier is reported truthfully: {revoked}"
        ),
        other => panic!("unexpected state {other:?}: {revoked}"),
    }
    assert_eq!(
        lead_log(&campaign).lines().count(),
        lead_calls_before,
        "no planning or provider effect after revocation"
    );
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["revoked"], true);
    assert_eq!(control(&campaign, "cancel", "cancel-1")["created"], true);
    let cancelled = campaign.json("campaign", &[]);
    assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
    assert!(
        matches!(
            cancelled["stop_reason"].as_str(),
            Some("operator_cancellation" | "mission_revoked")
        ),
        "{cancelled}"
    );
    let campaign_status = campaign.json("campaign-status", &[]);
    assert_eq!(campaign_status["state"], "cancelled");
}

#[test]
fn supervised_mission_holds_an_undelegated_decision_for_the_owner_and_continues() {
    let campaign = Campaign::with_lead_script(DECIDING_LEAD);
    fs::write(
        campaign.fixture.root.join("decision-domain.txt"),
        "unknown\n",
    )
    .unwrap();
    let charter = campaign.charter("mission.toml", 1, "supervised", "ask_owner", "x");
    let mission = charter.to_str().unwrap();

    let outcome = campaign.json("campaign", &["--mission", mission]);
    assert_eq!(outcome["state"], "paused", "{outcome}");
    assert_eq!(outcome["stop_reason"], "unit_limit", "{outcome}");
    assert!(!inference_marker(&campaign).exists());
    let log = lead_log(&campaign);
    assert!(
        log.starts_with("0.1.1.1\n0.1.1.2\n"),
        "independent work continued: {log}"
    );
    assert!(!log.contains("accepted"));

    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["decisions_recorded"], 1);
    assert_eq!(status["permitted_choices"], 0);
    assert_eq!(status["held_decisions"], 1);
    assert_eq!(status["pending_owner_decisions"], 1);
    let explanation = campaign.json("campaign-explain-blocker", &[]);
    assert!(
        !serde_json::to_string(&explanation)
            .unwrap()
            .contains("PRIVATE_")
    );

    let stderr = campaign.refused(
        "campaign-mission-answer",
        &[
            "--decision",
            "layout-1",
            "--request",
            "answer-1",
            "--answer",
            "unapproved",
        ],
    );
    assert!(stderr.contains("codingmage.runtime.authority"), "{stderr}");
    let answered = campaign.json(
        "campaign-mission-answer",
        &[
            "--decision",
            "layout-1",
            "--request",
            "answer-2",
            "--answer",
            "block",
        ],
    );
    assert_eq!(answered["created"], true);
    let repeated = campaign.json(
        "campaign-mission-answer",
        &[
            "--decision",
            "layout-1",
            "--request",
            "answer-2",
            "--answer",
            "block",
        ],
    );
    assert_eq!(repeated["created"], false);
    let status = campaign.json("campaign-mission-status", &[]);
    assert_eq!(status["pending_owner_decisions"], 0);
    assert_eq!(status["owner_answers"], 1);
    campaign.state_files_are_content_free();
}

#[test]
fn exception_only_mission_deduplicates_the_exception_request_across_runs() {
    let campaign = Campaign::with_lead_script(DECIDING_LEAD);
    fs::write(
        campaign.fixture.root.join("decision-domain.txt"),
        "unknown\n",
    )
    .unwrap();
    let charter = campaign.charter("mission.toml", 1, "exception_only", "ask_owner", "x");
    let mission = charter.to_str().unwrap();
    let outcome = campaign.json("campaign", &["--mission", mission]);
    assert_eq!(outcome["state"], "paused", "{outcome}");
    assert_eq!(outcome["stop_reason"], "unit_limit", "{outcome}");
    let first = campaign.json("campaign-mission-status", &[]);
    assert_eq!(first["pending_owner_decisions"], 1);
    let again = campaign.json("campaign", &[]);
    assert_eq!(again["state"], "paused", "{again}");
    assert_eq!(again["stop_reason"], "unit_limit", "{again}");
    let second = campaign.json("campaign-mission-status", &[]);
    assert_eq!(
        second["pending_owner_decisions"], 1,
        "one exception request per decision"
    );
    assert_eq!(second["decisions_recorded"], first["decisions_recorded"]);
}
