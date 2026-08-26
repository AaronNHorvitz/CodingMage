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
use codingmage_git::inventory_repository;
use codingmage_plan::TaskPlan;
use codingmage_runtime::{
    RunProgress, RunSpec, RuntimeError, approve_campaign_destination_promotion,
    approve_campaign_task_integration, campaign_blocker_explanation, campaign_preflight,
    campaign_status, clear_campaign_blocker, observe_campaign_deferral_trigger,
    request_campaign_control, run_one_with_progress, run_one_with_progress_for_id,
    run_team_campaign_with_progress, team_campaign_report,
};

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
  run                           Execute one explicitly scoped supervised unit
  campaign-preflight            Validate a campaign before provider inference
  campaign                      Execute a bounded serial or parallel campaign
  campaign-status               Read durable campaign status
  campaign-report               Read the final campaign report
  campaign-explain-blocker      Read typed blocker and deferral details
  campaign-clear-blocker        Record one exact external-prerequisite change
  campaign-observe-trigger      Record one exact deferral trigger
  campaign-control              Request pause, resume, stop, or cancellation
  campaign-approve-task         Approve one exact task-integration effect
  campaign-approve-destination  Approve one exact destination-promotion effect

Run `codingmage <COMMAND> --help` for exact command usage.";

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "init" => Some(
            "Usage: codingmage init --repo <ABSOLUTE_DIR> --config <ABSOLUTE_FILE> \\\n  --scratch <ABSOLUTE_NEW_DIR> --state <ABSOLUTE_NEW_DIR>",
        ),
        "doctor" | "status" | "plan" => {
            Some("Usage: codingmage <doctor|status|plan> --config <ABSOLUTE_FILE>")
        }
        "run" => Some(
            "Usage: codingmage run --config <ABSOLUTE_FILE> --spec <ABSOLUTE_FILE> \\\n  [--run-id <EXACT_RUN_ID>]",
        ),
        "campaign-preflight" => Some(
            "Usage: codingmage campaign-preflight --config <ABSOLUTE_FILE> \\\n  --campaign <ABSOLUTE_FILE> --authorization <ABSOLUTE_FILE>",
        ),
        "campaign" | "campaign-status" | "campaign-report" | "campaign-explain-blocker" => Some(
            "Usage: codingmage <campaign|campaign-status|campaign-report|campaign-explain-blocker> \\\n  --config <ABSOLUTE_FILE> --campaign <ABSOLUTE_FILE>",
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
        "run" => execute(&arguments[1..]),
        "campaign-preflight" => preflight_campaign(&arguments[1..]),
        "campaign" => execute_campaign(&arguments[1..]),
        "campaign-status" => inspect_campaign(&arguments[1..]),
        "campaign-report" => inspect_campaign_report(&arguments[1..]),
        "campaign-explain-blocker" => explain_campaign_blocker(&arguments[1..]),
        "campaign-clear-blocker" => clear_blocker(&arguments[1..]),
        "campaign-observe-trigger" => observe_trigger(&arguments[1..]),
        "campaign-control" => control_campaign(&arguments[1..]),
        "campaign-approve-task" => approve_campaign_task(&arguments[1..]),
        "campaign-approve-destination" => approve_campaign_destination(&arguments[1..]),
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
    let parsed = ParsedArguments::new(arguments, &["config", "campaign"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let started = Instant::now();
    let outcome = run_team_campaign_with_progress(&config, spec, &executable, |progress| {
        write_progress(started.elapsed(), progress);
    })
    .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
}

fn preflight_campaign(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["config", "campaign", "authorization"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let authorization = parsed.absolute_file("authorization")?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let report = campaign_preflight(&config, &spec, &executable, &authorization)
        .map_err(CliError::Runtime)?;
    serde_json::to_string_pretty(&report).map_err(|_| CliError::Internal)
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
            "campaign-status",
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
