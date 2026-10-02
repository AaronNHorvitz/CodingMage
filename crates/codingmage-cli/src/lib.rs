//! Content-minimized operator commands for configuration and local diagnosis.

use std::{
    collections::BTreeSet,
    fmt, fs,
    io::Write as _,
    path::PathBuf,
    time::{Duration, Instant},
};

use codingmage_campaign::CampaignSpec;
use codingmage_contracts::{AgentId, RunId};
use codingmage_core::{
    AgentProfile, CapabilityPolicy, CommandSpec, Config, PublicationMode, PublicationPolicy,
    RepositoryAuthorization, load_config,
};
use codingmage_git::{
    BlobReadError, inventory_repository, read_authorized_blob, read_authorized_changes,
};
use codingmage_plan::{PlanItem, PlanItemKind, TaskPlan};
use codingmage_runtime::{
    RecipeSpec, RunProgress, RunSpec, RuntimeError, admit_campaign_mission,
    answer_campaign_decision, approve_campaign_destination_promotion,
    approve_campaign_task_integration, campaign_blocker_explanation, campaign_mission_preflight,
    campaign_mission_status, campaign_preflight_with_mission, campaign_run_records,
    campaign_status, clear_campaign_blocker, export_support_bundle,
    observe_campaign_deferral_trigger, request_campaign_control, revoke_campaign_mission,
    run_one_with_progress, run_one_with_progress_for_id, run_team_campaign_with_progress,
    team_campaign_report,
};

mod campaign_read;
mod directory_read;
mod outcome_report;
mod project_read;
mod report_writer;
mod setup_writer;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = r"CodingMage local multi-agent engineering coordinator

Usage:
  codingmage <COMMAND> [OPTIONS]
  codingmage --version
  codingmage --help

Commands:
  init                          Create a deny-first local configuration
  doctor                        Validate configuration, repository, and task source
  status                        Show content-minimized local readiness
  plan                          Select the first dependency-ready task
  project-open                  Read one validated configuration and task source
  directory-list               List one selected directory for native browsing
  campaign-select              Read one validated campaign specification for native selection
  run                           Execute one explicitly scoped supervised unit
  campaign-preflight            Validate a campaign before provider inference
  campaign                      Execute a bounded serial or parallel campaign
  campaign-status               Read durable campaign status
  campaign-head-plan            Read task states at the exact reconciled campaign head
  campaign-task-detail          Read one bounded task excerpt at that head
  campaign-changes              Read bounded changes at the exact reconciled campaign head
  campaign-run-records          Read bound, content-minimized run evidence
  campaign-report               Read the final campaign report
  campaign-outcome-report       Assemble a source-bound outcome and blocker report
  report-export                 Write a source-bound report outside the repository
  setup-write-config           Write guided configuration bytes from stdin
  setup-write-authorization     Write exact owner authorization bytes from stdin
  setup-inspect-authorization   Inspect a bound external authorization record
  setup-write-campaign          Write exact campaign specification bytes from stdin
  setup-export-copy             Copy a validated Setup document outside the repository
  campaign-explain-blocker      Read typed blocker and deferral details
  campaign-clear-blocker        Record one exact external-prerequisite change
  campaign-observe-trigger      Record one exact deferral trigger
  campaign-control              Request pause, resume, stop, or cancellation
  campaign-approve-task         Approve one exact task-integration effect
  campaign-approve-destination  Approve one exact destination-promotion effect
  campaign-mission-admit        Admit or re-issue one mission charter for a campaign
  campaign-mission-status       Read durable mission authority and decision counts
  campaign-mission-revoke       Revoke a mission irreversibly; no new effect may start
  campaign-mission-answer       Answer one pending owner decision (supervised modes)
  recipe-instantiate            Render a versioned recipe into a supervised run spec
  support-bundle                Write a redacted support bundle into a new directory

Run `codingmage <COMMAND> --help` for exact command usage.";

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "init" => Some(
            "Usage: codingmage init --repo <ABSOLUTE_DIR> --config <ABSOLUTE_FILE> \\\n  --scratch <ABSOLUTE_NEW_DIR> --state <ABSOLUTE_NEW_DIR>",
        ),
        "doctor" | "status" | "plan" => {
            Some("Usage: codingmage <doctor|status|plan> --config <ABSOLUTE_FILE>")
        }
        "project-open" => Some("Usage: codingmage project-open --config <ABSOLUTE_FILE>"),
        "directory-list" => Some("Usage: codingmage directory-list --directory <ABSOLUTE_DIR>"),
        "campaign-select" => Some("Usage: codingmage campaign-select --campaign <ABSOLUTE_FILE>"),
        "run" => Some(
            "Usage: codingmage run --config <ABSOLUTE_FILE> --spec <ABSOLUTE_FILE> \\\n  [--run-id <EXACT_RUN_ID>]",
        ),
        "campaign-preflight" => Some(
            "Usage: codingmage campaign-preflight --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --authorization <ABSOLUTE_FILE> [--mission <ABSOLUTE_FILE>]",
        ),
        "campaign" => Some(
            "Usage: codingmage campaign --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> \\\n  [--mission <ABSOLUTE_FILE>]",
        ),
        "campaign-outcome-report" => Some(
            "Usage: codingmage campaign-outcome-report --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> [--include-paths true|false]",
        ),
        "report-export" => Some(
            "Usage: codingmage report-export --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> --output <ABSOLUTE_FILE> [--include-paths true|false] [--overwrite true|false]",
        ),
        "setup-write-config" => Some(
            "Usage: codingmage setup-write-config --repo <ABSOLUTE_DIR> --output <ABSOLUTE_FILE> [--overwrite true|false] < CONFIG_TOML",
        ),
        "setup-write-authorization" => Some(
            "Usage: codingmage setup-write-authorization --config <ABSOLUTE_FILE> --repository-id <OBSERVED_ID> --output <ABSOLUTE_FILE> [--overwrite true|false] < AUTHORIZATION_TEXT",
        ),
        "setup-inspect-authorization" => Some(
            "Usage: codingmage setup-inspect-authorization --config <ABSOLUTE_FILE> --repository-id <OBSERVED_ID> --head <FULL_COMMIT_ID> --task-source-sha256 <SHA256> --authorization <ABSOLUTE_FILE>",
        ),
        "setup-write-campaign" => Some(
            "Usage: codingmage setup-write-campaign --config <ABSOLUTE_FILE> --repository-id <OBSERVED_ID> --head <FULL_COMMIT_ID> --task-source-sha256 <SHA256> --authorization <ABSOLUTE_FILE> --output <ABSOLUTE_FILE> [--overwrite true|false] < CAMPAIGN_TOML",
        ),
        "setup-export-copy" => Some(
            "Usage: codingmage setup-export-copy --config <ABSOLUTE_FILE> --repository-id <OBSERVED_ID> --source <ABSOLUTE_FILE> --output <ABSOLUTE_FILE> [--campaign-authority-sha256 <SELECTED_SHA256>] [--overwrite true|false]",
        ),
        "campaign-status"
        | "campaign-report"
        | "campaign-explain-blocker"
        | "campaign-mission-status" => Some(
            "Usage: codingmage <campaign-status|campaign-report|campaign-explain-blocker|campaign-mission-status> \\\n  --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE>",
        ),
        "campaign-head-plan" => Some(
            "Usage: codingmage campaign-head-plan --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> --head <FULL_COMMIT_ID>",
        ),
        "campaign-task-detail" => Some(
            "Usage: codingmage campaign-task-detail --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> --head <FULL_COMMIT_ID> --item <TASK_ID>",
        ),
        "campaign-changes" => Some(
            "Usage: codingmage campaign-changes --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE> --head <FULL_COMMIT_ID>",
        ),
        "campaign-run-records" => Some(
            "Usage: codingmage campaign-run-records --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE>",
        ),
        "campaign-mission-admit" => Some(
            "Usage: codingmage campaign-mission-admit --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --mission <ABSOLUTE_FILE>",
        ),
        "campaign-mission-revoke" => Some(
            "Usage: codingmage campaign-mission-revoke --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --request <REQUEST_ID>",
        ),
        "support-bundle" => Some(
            "Usage: codingmage support-bundle --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --output <ABSOLUTE_NEW_DIR>",
        ),
        "recipe-instantiate" => Some(
            "Usage: codingmage recipe-instantiate --config <ABSOLUTE_FILE> \\\n  --recipe <ABSOLUTE_FILE> --template <ABSOLUTE_RUN_SPEC> --task <TASK_ID> \\\n  --output <ABSOLUTE_NEW_FILE>",
        ),
        "campaign-mission-answer" => Some(
            "Usage: codingmage campaign-mission-answer --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --decision <DECISION_ID> --request <REQUEST_ID> \\\n  --answer <ALTERNATIVE|block>",
        ),
        "campaign-clear-blocker" => Some(
            "Usage: codingmage campaign-clear-blocker --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --task <TASK_ID> --request <REQUEST_ID> \\\n  --prerequisite-sha256 <SHA256>",
        ),
        "campaign-observe-trigger" => Some(
            "Usage: codingmage campaign-observe-trigger --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --task <TASK_ID> --trigger <TRIGGER> \\\n  --request <REQUEST_ID> --evidence-sha256 <SHA256>",
        ),
        "campaign-control" => Some(
            "Usage: codingmage campaign-control --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --action <ACTION> --request <REQUEST_ID>",
        ),
        "campaign-approve-task" => Some(
            "Usage: codingmage campaign-approve-task --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --task <TASK_ID> --reviewed-commit <COMMIT> \\\n  --evidence-sha256 <SHA256> --request <REQUEST_ID>",
        ),
        "campaign-approve-destination" => Some(
            "Usage: codingmage campaign-approve-destination --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --reviewed-commit <COMMIT> --destination <BRANCH> \\\n  --evidence-sha256 <SHA256> --request <REQUEST_ID>",
        ),
        _ => None,
    }
}

/// Runs one CLI argument vector and returns bounded output.
///
/// # Errors
///
/// Returns [`CliError`] for malformed arguments, unavailable inputs, denied repository authority,
/// invalid plans, or deliberately unavailable execution.
pub fn run(arguments: &[String]) -> Result<String, CliError> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(CliError::Usage);
    };
    if matches!(command, "--help" | "help") && arguments.len() == 1 {
        return Ok(HELP.to_owned());
    }
    if arguments.len() == 2 && arguments[1] == "--help" {
        return command_help(command)
            .map(str::to_owned)
            .ok_or(CliError::Usage);
    }
    match command {
        "--version" | "version" if arguments.len() == 1 => Ok(format!("codingmage {VERSION}")),
        "init" => initialize(&arguments[1..]),
        "doctor" => diagnose(&arguments[1..], "doctor"),
        "status" => diagnose(&arguments[1..], "status"),
        "plan" => select_plan(&arguments[1..]),
        "project-open" => project_read::open(&arguments[1..]),
        "directory-list" => directory_read::list(&arguments[1..]),
        "campaign-select" => campaign_read::select(&arguments[1..]),
        "run" => execute(&arguments[1..]),
        "campaign-preflight" => preflight_campaign(&arguments[1..]),
        "campaign" => execute_campaign(&arguments[1..]),
        "campaign-status" => inspect_campaign(&arguments[1..]),
        "campaign-head-plan" => inspect_campaign_head_plan(&arguments[1..]),
        "campaign-task-detail" => inspect_campaign_task_detail(&arguments[1..]),
        "campaign-changes" => inspect_campaign_changes(&arguments[1..]),
        "campaign-run-records" => inspect_campaign_run_records(&arguments[1..]),
        "campaign-report" => inspect_campaign_report(&arguments[1..]),
        "campaign-outcome-report" => outcome_report::inspect(&arguments[1..]),
        "report-export" => outcome_report::export(&arguments[1..]),
        "setup-write-config" => {
            setup_writer::configuration(&arguments[1..], std::io::stdin().lock())
        }
        "setup-write-authorization" => {
            setup_writer::authorization(&arguments[1..], std::io::stdin().lock())
        }
        "setup-inspect-authorization" => setup_writer::inspect_authorization(&arguments[1..]),
        "setup-write-campaign" => setup_writer::campaign(&arguments[1..], std::io::stdin().lock()),
        "setup-export-copy" => setup_writer::export_copy(&arguments[1..]),
        "campaign-explain-blocker" => explain_campaign_blocker(&arguments[1..]),
        "campaign-clear-blocker" => clear_blocker(&arguments[1..]),
        "campaign-observe-trigger" => observe_trigger(&arguments[1..]),
        "campaign-control" => control_campaign(&arguments[1..]),
        "campaign-approve-task" => approve_campaign_task(&arguments[1..]),
        "campaign-approve-destination" => approve_campaign_destination(&arguments[1..]),
        "campaign-mission-admit" => admit_mission(&arguments[1..]),
        "campaign-mission-status" => inspect_mission(&arguments[1..]),
        "campaign-mission-revoke" => revoke_mission(&arguments[1..]),
        "campaign-mission-answer" => answer_mission_decision(&arguments[1..]),
        "recipe-instantiate" => instantiate_recipe(&arguments[1..]),
        "support-bundle" => write_support_bundle(&arguments[1..]),
        _ => Err(CliError::Usage),
    }
}

fn initialize(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["repo", "config", "scratch", "state"])?;
    let target = parsed.absolute_directory("repo")?;
    let scratch = parsed.new_directory("scratch")?;
    let state = parsed.new_directory("state")?;
    let config_path = parsed.absolute_path("config")?;
    if !config_path.parent().is_some_and(std::path::Path::is_dir) || config_path.exists() {
        return Err(CliError::Refused);
    }
    let config = Config {
        version: 1,
        target_path: target,
        task_source: PathBuf::from("TASKS.md"),
        default_branch: "main".to_owned(),
        integration_branch: "codingmage/integration".to_owned(),
        scratch_root: scratch,
        state_root: state,
        agent_profiles: vec![
            AgentProfile {
                id: AgentId::new("claude-implementer").map_err(|_| CliError::Internal)?,
                provider: "claude".to_owned(),
                model: "configured-by-operator".to_owned(),
            },
            AgentProfile {
                id: AgentId::new("codex-reviewer").map_err(|_| CliError::Internal)?,
                provider: "codex".to_owned(),
                model: "configured-by-operator".to_owned(),
            },
        ],
        correction_limit: 3,
        gate_commands: vec![CommandSpec {
            executable: PathBuf::from("/usr/bin/git"),
            args: vec!["diff".to_owned(), "--check".to_owned()],
        }],
        capabilities: CapabilityPolicy::default(),
        publication: PublicationPolicy {
            mode: PublicationMode::LocalOnly,
        },
        allow_parent_discovery: false,
    };
    let encoded = toml::to_string_pretty(&config).map_err(|_| CliError::Internal)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config_path)
        .map_err(|_| CliError::Refused)?;
    file.write_all(encoded.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| CliError::Internal)?;
    load_config(&config_path).map_err(|_| CliError::Internal)?;
    Ok("initialized deny-first local configuration".to_owned())
}

fn diagnose(arguments: &[String], command: &str) -> Result<String, CliError> {
    let config = configured(arguments)?;
    let authorization = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    let inventory = inventory_repository(&authorization).map_err(|_| CliError::Repository)?;
    let source =
        fs::read(config.target_path.join(&config.task_source)).map_err(|_| CliError::Plan)?;
    let plan = TaskPlan::parse(&source).map_err(|_| CliError::Plan)?;
    let value = serde_json::json!({
        "schema_version": 1,
        "command": command,
        "state": if inventory.condition.is_clean() { "ready" } else { "blocked-dirty" },
        "repository_id": authorization.identity().repository_id.as_str(),
        "head": inventory.head,
        "branch": inventory.branch,
        "clean": inventory.condition.is_clean(),
        "unsafe_checkout_features": inventory.unsafe_checkout_features,
        "task_source_sha256": plan.source_sha256,
        "sprints": plan.sprints.len(),
        "stories": plan.stories.len(),
        "items": plan.items.len(),
        "configuration": config.redacted_view(),
        "execution_available": true,
        "requires_run_spec": true,
    });
    serde_json::to_string_pretty(&value).map_err(|_| CliError::Internal)
}

fn execute(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(arguments, &["config", "spec"], &["run-id"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = RunSpec::load(&parsed.absolute_file("spec")?).map_err(CliError::Runtime)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let started = Instant::now();
    let outcome = if let Some(run_id) = parsed.optional_value("run-id") {
        let run_id = RunId::new(run_id).map_err(|_| CliError::InvalidArgument)?;
        run_one_with_progress_for_id(&config, spec, &executable, run_id, |progress| {
            write_progress(started.elapsed(), progress);
        })
    } else {
        run_one_with_progress(&config, spec, &executable, |progress| {
            write_progress(started.elapsed(), progress);
        })
    }
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn execute_campaign(arguments: &[String]) -> Result<String, CliError> {
    let parsed =
        ParsedArguments::new_with_optional(arguments, &["config", "campaign"], &["mission"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    if parsed.optional_value("mission").is_some() {
        let charter = parsed.absolute_file("mission")?;
        campaign_mission_preflight(&config, &spec, &executable, &charter)
            .map_err(CliError::Runtime)?;
        admit_campaign_mission(&config, &spec, &executable, &charter).map_err(CliError::Runtime)?;
    }
    let started = Instant::now();
    let outcome = run_team_campaign_with_progress(&config, spec, &executable, |progress| {
        write_progress(started.elapsed(), progress);
    })
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn preflight_campaign(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(
        arguments,
        &["config", "campaign", "authorization"],
        &["mission"],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let authorization = parsed.absolute_file("authorization")?;
    let mission = if parsed.optional_value("mission").is_some() {
        Some(parsed.absolute_file("mission")?)
    } else {
        None
    };
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let report = campaign_preflight_with_mission(
        &config,
        &spec,
        &executable,
        &authorization,
        mission.as_deref(),
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&report).map_err(|_| CliError::Internal)
}

fn write_support_bundle(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "output"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let output = parsed.absolute_path("output")?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let manifest =
        export_support_bundle(&config, &spec, &executable, &output).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&manifest).map_err(|_| CliError::Internal)
}

fn instantiate_recipe(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &["config", "recipe", "template", "task", "output"],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let recipe = RecipeSpec::load(&parsed.absolute_file("recipe")?).map_err(CliError::Runtime)?;
    let template = RunSpec::load(&parsed.absolute_file("template")?).map_err(CliError::Runtime)?;
    let output = parsed.absolute_path("output")?;
    if output.exists() || !output.parent().is_some_and(std::path::Path::is_dir) {
        return Err(CliError::InvalidArgument);
    }
    let configured_gates = (1..=config.gate_commands.len())
        .map(|index| format!("configured-gate-{index}"))
        .collect::<Vec<_>>();
    let spec = recipe
        .instantiate(&template, parsed.value("task")?, &configured_gates)
        .map_err(CliError::Runtime)?;
    let encoded = toml::to_string(&spec).map_err(|_| CliError::Internal)?;
    fs::write(&output, encoded).map_err(|_| CliError::InvalidArgument)?;
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "recipe_id": recipe.recipe_id,
        "kind": recipe.kind.code(),
        "task_id": spec.task_id,
        "owned_path_count": spec.owned_paths.len(),
        "regression_gate": spec.repair.as_ref().map(|repair| repair.regression_gate.clone()),
        "written": true,
    }))
    .map_err(|_| CliError::Internal)
}

fn admit_mission(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "mission"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let charter = parsed.absolute_file("mission")?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome =
        admit_campaign_mission(&config, &spec, &executable, &charter).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn inspect_mission(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let report = campaign_mission_status(&config, &spec, &executable).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&report).map_err(|_| CliError::Internal)
}

fn revoke_mission(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "request"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = revoke_campaign_mission(&config, &spec, &executable, parsed.value("request")?)
        .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn answer_mission_decision(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &["config", "campaign", "decision", "request", "answer"],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let answer = parsed.value("answer")?;
    let alternative = if answer == "block" {
        None
    } else {
        Some(answer)
    };
    let outcome = answer_campaign_decision(
        &config,
        &spec,
        &executable,
        parsed.value("decision")?,
        parsed.value("request")?,
        alternative,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn inspect_campaign(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let status = campaign_status(&config, &spec, &executable).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&status).map_err(|_| CliError::Internal)
}

fn inspect_campaign_head_plan(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "head"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let status = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .ok_or(CliError::Refused)?;
    let expected_head = parsed.value("head")?;
    if expected_head != status.head {
        return Err(CliError::StaleObservation);
    }
    let authorization = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id {
        return Err(CliError::Repository);
    }
    let task_source = config.task_source.components().collect::<PathBuf>();
    let source =
        read_authorized_blob(&authorization, expected_head, &task_source).map_err(|error| {
            match error {
                BlobReadError::InvalidBinding => CliError::InvalidArgument,
                BlobReadError::Identity => CliError::Repository,
                BlobReadError::Command => CliError::Plan,
            }
        })?;
    let plan = TaskPlan::parse(&source).map_err(|_| CliError::Plan)?;
    let still_current = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .is_some_and(|current| current.head == expected_head);
    if !still_current {
        return Err(CliError::StaleObservation);
    }
    let items = plan
        .items
        .iter()
        .filter(|item| item.kind == PlanItemKind::SubTask)
        .map(|item| serde_json::json!({ "id": item.id, "state": item.state }))
        .collect::<Vec<_>>();
    let projection = serde_json::json!({
        "schema_version": 1,
        "campaign_id": spec.campaign_id,
        "repository_id": spec.repository_id,
        "head": expected_head,
        "task_source_sha256": plan.source_sha256,
        "items": items,
    });
    let encoded = serde_json::to_string(&projection).map_err(|_| CliError::Internal)?;
    if encoded.len() >= 8 * 1024 * 1024 {
        return Err(CliError::Plan);
    }
    Ok(encoded)
}

// Task prose is read only from the authorized Git blob at the reconciled head. It is
// bounded before serialization because the UI's command worker has a separate output cap.
const MAX_TASK_DETAIL_BYTES: usize = 16 * 1024;
const MAX_TASK_CRITERIA: usize = 100;
const MAX_CRITERION_TITLE_BYTES: usize = 4096;

fn inspect_campaign_task_detail(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "head", "item"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let expected_head = parsed.value("head")?;
    let status = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .ok_or(CliError::Refused)?;
    if status.head != expected_head {
        return Err(CliError::StaleObservation);
    }
    let authorization = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id {
        return Err(CliError::Repository);
    }
    let task_source = config.task_source.components().collect::<PathBuf>();
    let source =
        read_authorized_blob(&authorization, expected_head, &task_source).map_err(|error| {
            match error {
                BlobReadError::InvalidBinding => CliError::InvalidArgument,
                BlobReadError::Identity => CliError::Repository,
                BlobReadError::Command => CliError::Plan,
            }
        })?;
    let plan = TaskPlan::parse(&source).map_err(|_| CliError::Plan)?;
    let requested_item = parsed.value("item")?;
    let mut matches = plan.items.iter().filter(|item| item.id == requested_item);
    let item = matches.next().ok_or(CliError::InvalidArgument)?;
    if matches.next().is_some() {
        return Err(CliError::InvalidArgument);
    }
    let (excerpt, truncated) = task_detail_excerpt(&source, &plan, item.anchor.line)?;
    let criteria_items = story_criteria_for(&plan, item);
    let criteria_truncated = criteria_items.len() > MAX_TASK_CRITERIA;
    let criteria = criteria_items
        .into_iter()
        .take(MAX_TASK_CRITERIA)
        .map(|candidate| {
            let (title, title_truncated) =
                bounded_utf8_prefix(&candidate.title, MAX_CRITERION_TITLE_BYTES);
            serde_json::json!({
                "id": candidate.id,
                "title": title,
                "title_truncated": title_truncated,
                "source_state": candidate.state,
            })
        })
        .collect::<Vec<_>>();
    let still_current = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .is_some_and(|current| current.head == expected_head);
    if !still_current {
        return Err(CliError::StaleObservation);
    }
    serde_json::to_string(&serde_json::json!({
        "schema_version": 1,
        "campaign_id": spec.campaign_id,
        "repository_id": spec.repository_id,
        "head": expected_head,
        "task_source_sha256": plan.source_sha256,
        "item_id": item.id,
        "kind": item.kind,
        "source_state": item.state,
        "source_line": item.anchor.line,
        "source_line_sha256": item.anchor.line_sha256,
        "excerpt": excerpt,
        "truncated": truncated,
        "story_criteria": criteria,
        "story_criteria_truncated": criteria_truncated,
    }))
    .map_err(|_| CliError::Internal)
}

fn task_detail_excerpt(
    source: &[u8],
    plan: &TaskPlan,
    line: usize,
) -> Result<(String, bool), CliError> {
    let text = std::str::from_utf8(source).map_err(|_| CliError::Plan)?;
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let start = line.checked_sub(1).ok_or(CliError::Plan)?;
    if start >= lines.len() {
        return Err(CliError::Plan);
    }
    let next_anchor = plan
        .items
        .iter()
        .map(|item| item.anchor.line)
        .chain(plan.stories.iter().map(|story| story.anchor.line))
        .chain(plan.sprints.iter().map(|sprint| sprint.anchor.line))
        .filter(|candidate| *candidate > line)
        .min()
        .unwrap_or(lines.len() + 1);
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(index, value)| {
            *index + 1 >= next_anchor || value.starts_with("## ") || value.starts_with("### ")
        })
        .map_or(lines.len(), |(index, _)| index);
    let mut excerpt = String::new();
    let mut truncated = false;
    for part in &lines[start..end] {
        let remaining = MAX_TASK_DETAIL_BYTES - excerpt.len();
        if part.len() > remaining {
            excerpt.push_str(bounded_utf8_prefix(part, remaining).0);
            truncated = true;
            break;
        }
        excerpt.push_str(part);
    }
    Ok((excerpt, truncated))
}

fn bounded_utf8_prefix(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let boundary = (0..=limit)
        .rev()
        .find(|offset| text.is_char_boundary(*offset))
        .unwrap_or(0);
    (&text[..boundary], true)
}

fn story_criteria_for<'a>(plan: &'a TaskPlan, item: &PlanItem) -> Vec<&'a PlanItem> {
    let story_id = match item.kind {
        PlanItemKind::Task | PlanItemKind::AcceptanceCriterion => Some(item.parent_id.as_str()),
        PlanItemKind::SubTask => item.parent_id.rsplit_once('.').map(|(story, _)| story),
        PlanItemKind::Gate => None,
    };
    let Some(story_id) =
        story_id.filter(|id| plan.stories.iter().any(|story| story.id.as_str() == *id))
    else {
        return Vec::new();
    };
    plan.items
        .iter()
        .filter(|candidate| {
            candidate.kind == PlanItemKind::AcceptanceCriterion && candidate.parent_id == story_id
        })
        .take(MAX_TASK_CRITERIA + 1)
        .collect()
}

fn inspect_campaign_changes(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "head"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let status = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .ok_or(CliError::Refused)?;
    let expected_head = parsed.value("head")?;
    if status.head != expected_head {
        return Err(CliError::StaleObservation);
    }
    let authorization = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    if authorization.identity().repository_id.as_str() != spec.repository_id {
        return Err(CliError::Repository);
    }
    let changes = read_authorized_changes(&authorization, &spec.initial_commit, expected_head)
        .map_err(|_| CliError::Repository)?;
    let still_current = campaign_status(&config, &spec, &executable)
        .map_err(CliError::Runtime)?
        .is_some_and(|current| current.head == expected_head);
    if !still_current {
        return Err(CliError::StaleObservation);
    }
    let encoded = serde_json::to_string(&serde_json::json!({
        "schema_version": 1,
        "campaign_id": spec.campaign_id,
        "repository_id": spec.repository_id,
        "base": spec.initial_commit,
        "head": expected_head,
        "commits": changes.commits,
        "files": changes.files,
        "commits_truncated": changes.commits_truncated,
        "files_truncated": changes.files_truncated,
    }))
    .map_err(|_| CliError::Internal)?;
    if encoded.len() >= 4 * 1024 * 1024 {
        return Err(CliError::Repository);
    }
    Ok(encoded)
}

fn inspect_campaign_run_records(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    campaign_run_records(&config, &spec, &executable).map_err(CliError::Runtime)
}

fn inspect_campaign_report(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let report = team_campaign_report(&config, &spec, &executable).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&report).map_err(|_| CliError::Internal)
}

fn explain_campaign_blocker(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let explanation =
        campaign_blocker_explanation(&config, &spec, &executable).map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&explanation).map_err(|_| CliError::Internal)
}

fn clear_blocker(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "config",
            "campaign",
            "task",
            "request",
            "prerequisite-sha256",
        ],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = clear_campaign_blocker(
        &config,
        &spec,
        &executable,
        parsed.value("task")?,
        parsed.value("request")?,
        parsed.value("prerequisite-sha256")?,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn observe_trigger(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "config",
            "campaign",
            "task",
            "trigger",
            "request",
            "evidence-sha256",
        ],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = observe_campaign_deferral_trigger(
        &config,
        &spec,
        &executable,
        parsed.value("task")?,
        parsed.value("trigger")?,
        parsed.value("request")?,
        parsed.value("evidence-sha256")?,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn control_campaign(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "action", "request"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = request_campaign_control(
        &config,
        &spec,
        &executable,
        parsed.value("action")?,
        parsed.value("request")?,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn approve_campaign_task(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "config",
            "campaign",
            "task",
            "campaign-head",
            "reviewed-commit",
            "request",
        ],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = approve_campaign_task_integration(
        &config,
        &spec,
        &executable,
        parsed.value("task")?,
        parsed.value("campaign-head")?,
        parsed.value("reviewed-commit")?,
        parsed.value("request")?,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn approve_campaign_destination(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "config",
            "campaign",
            "pull-request",
            "destination",
            "destination-head",
            "final-commit",
            "report-sha256",
            "request",
        ],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let pull_request_number = parsed
        .value("pull-request")?
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgument)?;
    let outcome = approve_campaign_destination_promotion(
        &config,
        &spec,
        &executable,
        pull_request_number,
        parsed.value("destination")?,
        parsed.value("destination-head")?,
        parsed.value("final-commit")?,
        parsed.value("report-sha256")?,
        parsed.value("request")?,
    )
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn write_progress(elapsed: Duration, progress: RunProgress) {
    let total_seconds = elapsed.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(
        stderr,
        "[codingmage +{minutes:02}:{seconds:02}] {:<11} {}",
        progress.actor.label(),
        progress.stage.summary()
    );
    let _ = stderr.flush();
}

fn select_plan(arguments: &[String]) -> Result<String, CliError> {
    let config = configured(arguments)?;
    let source =
        fs::read(config.target_path.join(&config.task_source)).map_err(|_| CliError::Plan)?;
    let plan = TaskPlan::parse(&source).map_err(|_| CliError::Plan)?;
    let selected = plan
        .select_next(&BTreeSet::new())
        .map_err(|_| CliError::NoReadyWork)?;
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "task_id": selected.item.id,
        "kind": selected.item.kind,
        "title": selected.item.title,
        "source_line": selected.item.anchor.line,
        "source_line_sha256": selected.item.anchor.line_sha256,
        "task_source_sha256": selected.source_sha256,
    }))
    .map_err(|_| CliError::Internal)
}

fn configured(arguments: &[String]) -> Result<Config, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config"])?;
    load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)
}

fn executable_parent() -> Result<PathBuf, CliError> {
    std::env::current_exe()
        .map_err(|_| CliError::Internal)?
        .parent()
        .ok_or(CliError::Internal)
        .and_then(|path| fs::canonicalize(path).map_err(|_| CliError::Internal))
}

struct ParsedArguments {
    values: std::collections::BTreeMap<String, String>,
}

impl ParsedArguments {
    fn new(arguments: &[String], allowed: &[&str]) -> Result<Self, CliError> {
        if arguments.len() != allowed.len() * 2 {
            return Err(CliError::Usage);
        }
        let mut values = std::collections::BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].strip_prefix("--").ok_or(CliError::Usage)?;
            if !allowed.contains(&name) || pair[1].is_empty() || values.contains_key(name) {
                return Err(CliError::Usage);
            }
            values.insert(name.to_owned(), pair[1].clone());
        }
        Ok(Self { values })
    }

    fn new_with_optional(
        arguments: &[String],
        required: &[&str],
        optional: &[&str],
    ) -> Result<Self, CliError> {
        if !arguments.len().is_multiple_of(2) {
            return Err(CliError::Usage);
        }
        let allowed = required.iter().chain(optional).copied().collect::<Vec<_>>();
        let mut values = std::collections::BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].strip_prefix("--").ok_or(CliError::Usage)?;
            if !allowed.contains(&name) || pair[1].is_empty() || values.contains_key(name) {
                return Err(CliError::Usage);
            }
            values.insert(name.to_owned(), pair[1].clone());
        }
        if required.iter().any(|name| !values.contains_key(*name)) {
            return Err(CliError::Usage);
        }
        Ok(Self { values })
    }

    fn absolute_path(&self, name: &str) -> Result<PathBuf, CliError> {
        let path = PathBuf::from(self.values.get(name).ok_or(CliError::Usage)?);
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(CliError::InvalidArgument);
        }
        Ok(path)
    }

    fn value(&self, name: &str) -> Result<&str, CliError> {
        self.values
            .get(name)
            .map(String::as_str)
            .ok_or(CliError::Usage)
    }

    fn optional_value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn absolute_file(&self, name: &str) -> Result<PathBuf, CliError> {
        let path = self.absolute_path(name)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| CliError::InvalidArgument)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CliError::InvalidArgument);
        }
        Ok(path)
    }

    fn absolute_directory(&self, name: &str) -> Result<PathBuf, CliError> {
        let path = self.absolute_path(name)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| CliError::InvalidArgument)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CliError::InvalidArgument);
        }
        fs::canonicalize(path).map_err(|_| CliError::InvalidArgument)
    }

    fn new_directory(&self, name: &str) -> Result<PathBuf, CliError> {
        let path = self.absolute_path(name)?;
        if path.exists() {
            return self.absolute_directory(name);
        }
        fs::create_dir_all(&path).map_err(|_| CliError::Refused)?;
        fs::canonicalize(path).map_err(|_| CliError::InvalidArgument)
    }
}

/// Stable content-free CLI failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliError {
    /// Argument grammar is invalid.
    Usage,
    /// One explicit path is invalid.
    InvalidArgument,
    /// Configuration could not be loaded.
    Config,
    /// Repository authorization or inventory failed.
    Repository,
    /// Task plan could not be parsed.
    Plan,
    /// No dependency-ready work exists.
    NoReadyWork,
    /// A requested write would overwrite or broaden authority.
    Refused,
    /// A concurrent change prevents proving the final published state.
    UncertainWrite,
    /// The observed repository or campaign head changed before an authorized effect.
    StaleObservation,
    /// Live orchestration is deliberately not enabled.
    ExecutionUnavailable,
    /// Supervised one-unit execution failed closed.
    Runtime(RuntimeError),
    /// Content-free internal failure.
    Internal,
}

impl CliError {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Usage => "codingmage.cli.usage",
            Self::InvalidArgument => "codingmage.cli.invalid_argument",
            Self::Config => "codingmage.cli.config",
            Self::Repository => "codingmage.cli.repository",
            Self::Plan => "codingmage.cli.plan",
            Self::NoReadyWork => "codingmage.cli.no_ready_work",
            Self::Refused => "codingmage.cli.refused",
            Self::UncertainWrite => "codingmage.cli.uncertain_write",
            Self::StaleObservation => "codingmage.cli.stale_observation",
            Self::ExecutionUnavailable => "codingmage.cli.execution_unavailable",
            Self::Runtime(error) => error.code(),
            Self::Internal => "codingmage.cli.internal",
        }
    }

    /// Process exit status grouped by operator actionability.
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Usage | Self::InvalidArgument => 2,
            Self::NoReadyWork => 3,
            Self::ExecutionUnavailable => 4,
            Self::Config
            | Self::Repository
            | Self::Plan
            | Self::Refused
            | Self::UncertainWrite
            | Self::StaleObservation
            | Self::Runtime(_)
            | Self::Internal => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_usage_and_run_contract_are_stable() {
        assert_eq!(run(&["--version".to_owned()]).unwrap(), "codingmage 0.1.0");
        assert!(run(&["--help".to_owned()]).unwrap().contains("Commands:"));
        assert!(run(&["help".to_owned()]).unwrap().contains("  run "));
        assert_eq!(run(&[]), Err(CliError::Usage));
        assert_eq!(run(&["run".to_owned()]), Err(CliError::Usage));
    }

    #[test]
    fn every_public_command_has_side_effect_free_exact_help() {
        for command in [
            "init",
            "doctor",
            "status",
            "plan",
            "run",
            "campaign-preflight",
            "campaign",
            "setup-write-authorization",
            "setup-inspect-authorization",
            "setup-write-campaign",
            "campaign-status",
            "campaign-head-plan",
            "campaign-task-detail",
            "campaign-report",
            "campaign-explain-blocker",
            "campaign-clear-blocker",
            "campaign-observe-trigger",
            "campaign-control",
            "campaign-approve-task",
            "campaign-approve-destination",
        ] {
            let output = run(&[command.to_owned(), "--help".to_owned()]).unwrap();
            assert!(output.starts_with("Usage: codingmage"), "{command}");
            assert!(!output.contains("/home/"), "{command}");
        }
        assert_eq!(
            run(&["unknown".to_owned(), "--help".to_owned()]),
            Err(CliError::Usage)
        );
    }

    #[test]
    fn duplicate_unknown_relative_and_missing_arguments_fail() {
        for arguments in [
            vec!["doctor", "--config", "relative.toml"],
            vec!["doctor", "--other", "/tmp/value"],
            vec!["doctor", "--config"],
            vec!["doctor", "--config", "/tmp/a", "--config", "/tmp/b"],
        ] {
            let arguments = arguments.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(run(&arguments).is_err());
        }
    }

    #[test]
    fn task_detail_excerpt_stops_at_next_item_and_bounds_unicode() {
        let source = "### [ ] Sprint 1 - Demo\n\n**Sprint goal:** Prove a task.\n\n#### [ ] Story 1.1 - Test\n\n- [ ] **Task 1.1.1 - Do work**\n  - [ ] **Sub-task 1.1.1.1:** First item.\n    detail line\n  - [ ] **Sub-task 1.1.1.2:** Second item.\n";
        let plan = TaskPlan::parse(source.as_bytes()).unwrap();
        let line = plan
            .items
            .iter()
            .find(|item| item.id == "1.1.1.1")
            .unwrap()
            .anchor
            .line;
        let (excerpt, truncated) = task_detail_excerpt(source.as_bytes(), &plan, line).unwrap();
        assert!(excerpt.contains("detail line"));
        assert!(!excerpt.contains("Second item"));
        assert!(!truncated);
        assert_eq!(
            task_detail_excerpt(source.as_bytes(), &plan, 0),
            Err(CliError::Plan)
        );

        let large = format!(
            "### [ ] Sprint 1 - Demo\n\n**Sprint goal:** Prove a task.\n\n#### [ ] Story 1.1 - Test\n\n- [ ] **Task 1.1.1 - Do work**\n  - [ ] **Sub-task 1.1.1.1:** First item.\n    {}\n",
            "界".repeat(6000)
        );
        let plan = TaskPlan::parse(large.as_bytes()).unwrap();
        let line = plan
            .items
            .iter()
            .find(|item| item.id == "1.1.1.1")
            .unwrap()
            .anchor
            .line;
        let (excerpt, truncated) = task_detail_excerpt(large.as_bytes(), &plan, line).unwrap();
        assert!(truncated);
        assert!(excerpt.len() <= MAX_TASK_DETAIL_BYTES);
        assert!(excerpt.is_char_boundary(excerpt.len()));
    }

    #[test]
    fn task_detail_criteria_follow_story_parent_not_gate_identifier() {
        let source = "### [ ] Sprint 1 - Demo\n\n**Sprint goal:** Prove a task.\n\n#### [ ] Story 1.1 - Test\n\n- [ ] **Task 1.1.1 - Do work**\n  - [ ] **Sub-task 1.1.1.1:** First item.\n- [ ] **Story AC 1.1.AC1:** Story outcome.\n- [ ] **Sprint AC 1.AC1:** Sprint outcome.\n- [ ] **Gate 1.1:** Verify sprint.\n";
        let plan = TaskPlan::parse(source.as_bytes()).unwrap();
        let subtask = plan.items.iter().find(|item| item.id == "1.1.1.1").unwrap();
        let criterion = plan.items.iter().find(|item| item.id == "1.1.AC1").unwrap();
        let gate = plan
            .items
            .iter()
            .find(|item| item.kind == PlanItemKind::Gate)
            .unwrap();
        assert_eq!(story_criteria_for(&plan, subtask), vec![criterion]);
        assert!(story_criteria_for(&plan, gate).is_empty());
    }

    #[test]
    fn run_arguments_accept_one_valid_optional_identity_and_reject_malformed_shapes() {
        let parsed = ParsedArguments::new_with_optional(
            &[
                "--run-id".to_owned(),
                "run-resume-1".to_owned(),
                "--spec".to_owned(),
                "/tmp/run.toml".to_owned(),
                "--config".to_owned(),
                "/tmp/config.toml".to_owned(),
            ],
            &["config", "spec"],
            &["run-id"],
        )
        .unwrap();
        assert_eq!(parsed.optional_value("run-id"), Some("run-resume-1"));
        assert!(RunId::new(parsed.optional_value("run-id").unwrap()).is_ok());

        for arguments in [
            vec!["--config", "/tmp/config.toml", "--run-id", "run-1"],
            vec![
                "--config",
                "/tmp/config.toml",
                "--spec",
                "/tmp/run.toml",
                "--run-id",
                "../escape",
            ],
            vec![
                "--config",
                "/tmp/config.toml",
                "--spec",
                "/tmp/run.toml",
                "--run-id",
                "run-1",
                "--run-id",
                "run-2",
            ],
        ] {
            let arguments = arguments.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let parsed =
                ParsedArguments::new_with_optional(&arguments, &["config", "spec"], &["run-id"]);
            if arguments.iter().any(|argument| argument == "../escape") {
                assert!(
                    parsed
                        .ok()
                        .and_then(|parsed| parsed.optional_value("run-id").map(str::to_owned))
                        .is_some_and(|value| RunId::new(value).is_err())
                );
            } else {
                assert!(parsed.is_err());
            }
        }
    }
}
