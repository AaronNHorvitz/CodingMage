//! Content-minimized operator commands for configuration and local diagnosis.

use std::{collections::BTreeSet, fmt, fs, io::Write as _, path::PathBuf};

use codingmage_contracts::AgentId;
use codingmage_core::{
    AgentProfile, CapabilityPolicy, CommandSpec, Config, PublicationMode, PublicationPolicy,
    RepositoryAuthorization, load_config,
};
use codingmage_git::inventory_repository;
use codingmage_plan::TaskPlan;
use codingmage_runtime::{RunSpec, run_one};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    match command {
        "--version" | "version" if arguments.len() == 1 => Ok(format!("codingmage {VERSION}")),
        "init" => initialize(&arguments[1..]),
        "doctor" => diagnose(&arguments[1..], "doctor"),
        "status" => diagnose(&arguments[1..], "status"),
        "plan" => select_plan(&arguments[1..]),
        "run" => execute(&arguments[1..]),
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
    let parsed = ParsedArguments::new(arguments, &["config", "spec"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = RunSpec::load(&parsed.absolute_file("spec")?).map_err(|_| CliError::Runtime)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let outcome = run_one(&config, spec, &executable).map_err(|_| CliError::Runtime)?;
    serde_json::to_string_pretty(&outcome).map_err(|_| CliError::Internal)
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
    Runtime,
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
            Self::Runtime => "codingmage.cli.runtime",
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
            | Self::Runtime
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
        assert_eq!(run(&[]), Err(CliError::Usage));
        assert_eq!(run(&["run".to_owned()]), Err(CliError::Usage));
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
}
