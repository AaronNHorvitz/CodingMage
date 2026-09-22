//! Guided configuration and campaign authoring through the existing validated contracts.
//!
//! Forms are plain data; `build` produces the existing `Config` or `CampaignSpec` types and the
//! writers persist them only after the existing loaders accept the exact bytes. Nothing here
//! stores a credential, and every write refuses to replace a file unless the caller opted in.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_campaign::{
    CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
    CampaignPublication, CampaignSpec,
};
use codingmage_contracts::AgentId;
use codingmage_core::{
    AgentProfile, CapabilityPolicy, CommandSpec, Config, PublicationMode, PublicationPolicy,
    load_config,
};

use crate::project::{file_sha256, hex};

/// Largest operator authorization record the interface reads or writes.
pub const MAX_AUTHORIZATION_BYTES: u64 = 1024 * 1024;
/// Closed provider effort selectors accepted by the campaign contract.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// One field-level problem found before the existing loader runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldError {
    /// Field label.
    pub field: &'static str,
    /// What is wrong.
    pub message: String,
}

/// Failure to persist a document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriteError {
    /// Field problems found before validation.
    Fields(Vec<FieldError>),
    /// The destination exists and overwrite was not requested, or it is not a regular file.
    Exists(PathBuf),
    /// The destination lies inside the target repository.
    InsideRepository(PathBuf),
    /// Serialization failed.
    Encode,
    /// The existing loader rejected the exact bytes.
    Rejected(String),
    /// Filesystem failure.
    Io,
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fields(errors) => {
                formatter.write_str("fields need attention:")?;
                for error in errors {
                    write!(formatter, " {}: {};", error.field, error.message)?;
                }
                Ok(())
            }
            Self::Exists(path) => write!(
                formatter,
                "{} already exists; choose another path or confirm overwrite",
                path.display()
            ),
            Self::InsideRepository(path) => write!(
                formatter,
                "{} is inside the target repository; keep authority files outside it",
                path.display()
            ),
            Self::Encode => formatter.write_str("the document could not be encoded"),
            Self::Rejected(reason) => {
                write!(
                    formatter,
                    "the existing loader rejected the document: {reason}"
                )
            }
            Self::Io => formatter.write_str("the document could not be written"),
        }
    }
}

impl std::error::Error for WriteError {}

/// One agent profile row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProfileForm {
    /// Stable profile identifier.
    pub id: String,
    /// Provider adapter name (`claude` or `codex`).
    pub provider: String,
    /// Model profile passed to the provider unchanged.
    pub model: String,
}

/// One gate command row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GateForm {
    /// Absolute executable.
    pub executable: String,
    /// Whitespace-separated literal arguments.
    pub arguments: String,
}

/// Configuration form (version 1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigForm {
    /// Destination configuration file.
    pub config_path: String,
    /// Absolute target repository.
    pub target_path: String,
    /// Relative task source.
    pub task_source: String,
    /// Default branch.
    pub default_branch: String,
    /// Integration branch prefix.
    pub integration_branch: String,
    /// Absolute scratch root.
    pub scratch_root: String,
    /// Absolute state root.
    pub state_root: String,
    /// Agent profiles.
    pub profiles: Vec<ProfileForm>,
    /// Correction limit.
    pub correction_limit: String,
    /// Gate commands.
    pub gates: Vec<GateForm>,
    /// Publication mode.
    pub publication: PublicationMode,
    /// Explicit external capability grants.
    pub capabilities: CapabilityPolicy,
    /// Parent-discovery setting.
    pub allow_parent_discovery: bool,
    /// Replace an existing configuration file.
    pub overwrite: bool,
}

impl ConfigForm {
    /// Deny-first defaults for one target directory, matching `codingmage init`.
    #[must_use]
    pub fn defaults(target: &Path, workspace_root: &Path) -> Self {
        Self {
            config_path: workspace_root.join("codingmage.toml").display().to_string(),
            target_path: target.display().to_string(),
            task_source: "TASKS.md".to_owned(),
            default_branch: "main".to_owned(),
            integration_branch: "codingmage/integration".to_owned(),
            scratch_root: workspace_root.join("scratch").display().to_string(),
            state_root: workspace_root.join("state").display().to_string(),
            profiles: vec![
                ProfileForm {
                    id: "claude-implementer".to_owned(),
                    provider: "claude".to_owned(),
                    model: "configured-by-operator".to_owned(),
                },
                ProfileForm {
                    id: "codex-reviewer".to_owned(),
                    provider: "codex".to_owned(),
                    model: "configured-by-operator".to_owned(),
                },
            ],
            correction_limit: "3".to_owned(),
            gates: vec![GateForm {
                executable: "/usr/bin/git".to_owned(),
                arguments: "diff --check".to_owned(),
            }],
            publication: PublicationMode::LocalOnly,
            capabilities: CapabilityPolicy::default(),
            allow_parent_discovery: false,
            overwrite: false,
        }
    }

    /// Form populated from an existing validated configuration.
    #[must_use]
    pub fn from_config(config_path: &Path, config: &Config) -> Self {
        Self {
            config_path: config_path.display().to_string(),
            target_path: config.target_path.display().to_string(),
            task_source: config.task_source.display().to_string(),
            default_branch: config.default_branch.clone(),
            integration_branch: config.integration_branch.clone(),
            scratch_root: config.scratch_root.display().to_string(),
            state_root: config.state_root.display().to_string(),
            profiles: config
                .agent_profiles
                .iter()
                .map(|profile| ProfileForm {
                    id: profile.id.as_str().to_owned(),
                    provider: profile.provider.clone(),
                    model: profile.model.clone(),
                })
                .collect(),
            correction_limit: config.correction_limit.to_string(),
            gates: config
                .gate_commands
                .iter()
                .map(|command| GateForm {
                    executable: command.executable.display().to_string(),
                    arguments: command.args.join(" "),
                })
                .collect(),
            publication: config.publication.mode,
            capabilities: config.capabilities,
            allow_parent_discovery: config.allow_parent_discovery,
            overwrite: false,
        }
    }

    /// Builds the configuration value, reporting field-level problems first.
    ///
    /// # Errors
    ///
    /// Returns the field problems found before the existing loader would run.
    pub fn build(&self) -> Result<Config, Vec<FieldError>> {
        let mut errors = Vec::new();
        for (field, value) in [
            ("configuration path", &self.config_path),
            ("target path", &self.target_path),
            ("scratch root", &self.scratch_root),
            ("state root", &self.state_root),
        ] {
            if !Path::new(value.trim()).is_absolute() {
                errors.push(FieldError {
                    field,
                    message: "must be an absolute path".to_owned(),
                });
            }
        }
        if self.task_source.trim().is_empty() || Path::new(self.task_source.trim()).is_absolute() {
            errors.push(FieldError {
                field: "task source",
                message: "must be a relative path inside the repository".to_owned(),
            });
        }
        let correction_limit = match self.correction_limit.trim().parse::<u16>() {
            Ok(limit) if (1..=100).contains(&limit) => limit,
            _ => {
                errors.push(FieldError {
                    field: "correction limit",
                    message: "must be a whole number from 1 to 100".to_owned(),
                });
                0
            }
        };
        let profiles = self.build_profiles(&mut errors);
        let gates = self.build_gates(&mut errors);
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(Config {
            version: 1,
            target_path: PathBuf::from(self.target_path.trim()),
            task_source: PathBuf::from(self.task_source.trim()),
            default_branch: self.default_branch.trim().to_owned(),
            integration_branch: self.integration_branch.trim().to_owned(),
            scratch_root: PathBuf::from(self.scratch_root.trim()),
            state_root: PathBuf::from(self.state_root.trim()),
            agent_profiles: profiles,
            correction_limit,
            gate_commands: gates,
            capabilities: self.capabilities,
            publication: PublicationPolicy {
                mode: self.publication,
            },
            allow_parent_discovery: self.allow_parent_discovery,
        })
    }

    fn build_profiles(&self, errors: &mut Vec<FieldError>) -> Vec<AgentProfile> {
        let mut profiles = Vec::new();
        for (index, profile) in self.profiles.iter().enumerate() {
            match AgentId::new(profile.id.trim()) {
                Ok(id)
                    if !profile.provider.trim().is_empty() && !profile.model.trim().is_empty() =>
                {
                    profiles.push(AgentProfile {
                        id,
                        provider: profile.provider.trim().to_owned(),
                        model: profile.model.trim().to_owned(),
                    });
                }
                _ => errors.push(FieldError {
                    field: "agent profiles",
                    message: format!(
                        "row {} needs a canonical identifier, a provider and a model",
                        index + 1
                    ),
                }),
            }
        }
        if profiles.is_empty() {
            errors.push(FieldError {
                field: "agent profiles",
                message: "at least one profile is required".to_owned(),
            });
        }
        profiles
    }

    fn build_gates(&self, errors: &mut Vec<FieldError>) -> Vec<CommandSpec> {
        let mut gates = Vec::new();
        for (index, gate) in self.gates.iter().enumerate() {
            let executable = PathBuf::from(gate.executable.trim());
            if executable.is_absolute() {
                gates.push(CommandSpec {
                    executable,
                    args: gate
                        .arguments
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect(),
                });
            } else {
                errors.push(FieldError {
                    field: "gate commands",
                    message: format!("row {} needs an absolute executable", index + 1),
                });
            }
        }
        if gates.is_empty() {
            errors.push(FieldError {
                field: "gate commands",
                message: "at least one deterministic gate is required".to_owned(),
            });
        }
        gates
    }

    /// Creates missing scratch and state roots, then writes the configuration after the existing
    /// loader accepts the exact bytes.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError`] for field problems, an existing destination without overwrite,
    /// loader rejection or I/O failure.
    pub fn write(&self) -> Result<PathBuf, WriteError> {
        let config = self.build().map_err(WriteError::Fields)?;
        for root in [&config.scratch_root, &config.state_root] {
            if !root.exists() {
                fs::create_dir_all(root).map_err(|_| WriteError::Io)?;
            }
        }
        let destination = PathBuf::from(self.config_path.trim());
        let encoded = toml::to_string_pretty(&config).map_err(|_| WriteError::Encode)?;
        write_validated(&destination, encoded.as_bytes(), self.overwrite, |path| {
            load_config(path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}

/// One campaign provider row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderForm {
    /// Absolute provider executable.
    pub executable: String,
    /// Model selector passed unchanged to the provider.
    pub model: String,
    /// Closed effort selector.
    pub effort: String,
}

impl Default for ProviderForm {
    fn default() -> Self {
        Self {
            executable: String::new(),
            model: String::new(),
            effort: "high".to_owned(),
        }
    }
}

/// Immutable binding values taken from the coordinator's diagnosis, never typed by hand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignBinding {
    /// Repository identity from `doctor`.
    pub repository_id: String,
    /// Canonical repository path from the configuration.
    pub repository_path: PathBuf,
    /// Current head from `doctor`.
    pub initial_commit: String,
    /// Task-source digest from `doctor`.
    pub task_source_sha256: String,
    /// Default branch from the configuration.
    pub default_branch: String,
}

/// Campaign authority form (version 3, serial execution).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignForm {
    /// Destination specification file.
    pub spec_path: String,
    /// Campaign identity.
    pub campaign_id: String,
    /// Accepted-outcome ceiling.
    pub max_units: String,
    /// Existing operator authorization record (absolute file outside the repository).
    pub authorization_path: String,
    /// Team lead provider.
    pub team_lead: ProviderForm,
    /// Implementer provider.
    pub implementer: ProviderForm,
    /// Reviewer provider.
    pub reviewer: ProviderForm,
    /// Implementer login boundary.
    pub implementer_authentication: CampaignAuthentication,
    /// Whitespace-separated allowed repository-relative roots.
    pub allowed_paths: String,
    /// Whitespace-separated denied roots.
    pub denied_paths: String,
    /// Whitespace-separated protected branches.
    pub protected_branches: String,
    /// Gate tier name.
    pub gate_tier: String,
    /// Whitespace-separated gate profile names.
    pub gate_profiles: String,
    /// Aggregate limits as text.
    pub limits: LimitsForm,
    /// Replace an existing specification file.
    pub overwrite: bool,
}

/// Aggregate limit fields as text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitsForm {
    /// Provider attempts.
    pub provider_attempts: String,
    /// Malformed-report repairs.
    pub malformed_report_repairs: String,
    /// Correction rounds.
    pub correction_rounds: String,
    /// Process invocations.
    pub process_invocations: String,
    /// Output bytes.
    pub output_bytes: String,
    /// Retained state bytes.
    pub retained_state_bytes: String,
    /// Execution milliseconds.
    pub execution_elapsed_ms: String,
}

impl Default for LimitsForm {
    fn default() -> Self {
        Self {
            provider_attempts: "1000".to_owned(),
            malformed_report_repairs: "100".to_owned(),
            correction_rounds: "100".to_owned(),
            process_invocations: "10000".to_owned(),
            output_bytes: "1073741824".to_owned(),
            retained_state_bytes: "1073741824".to_owned(),
            execution_elapsed_ms: "86400000".to_owned(),
        }
    }
}

impl LimitsForm {
    fn build(&self, errors: &mut Vec<FieldError>) -> CampaignLimits {
        fn number<T>(value: &str, field: &'static str, errors: &mut Vec<FieldError>) -> T
        where
            T: std::str::FromStr + Default,
        {
            value.trim().parse::<T>().unwrap_or_else(|_| {
                errors.push(FieldError {
                    field,
                    message: "must be a whole number".to_owned(),
                });
                T::default()
            })
        }
        CampaignLimits {
            provider_attempts: number(&self.provider_attempts, "provider attempts", errors),
            malformed_report_repairs: number(
                &self.malformed_report_repairs,
                "malformed-report repairs",
                errors,
            ),
            correction_rounds: number(&self.correction_rounds, "correction rounds", errors),
            process_invocations: number(&self.process_invocations, "process invocations", errors),
            output_bytes: number(&self.output_bytes, "output bytes", errors),
            retained_state_bytes: number(
                &self.retained_state_bytes,
                "retained state bytes",
                errors,
            ),
            execution_elapsed_ms: number(
                &self.execution_elapsed_ms,
                "execution milliseconds",
                errors,
            ),
        }
    }
}

impl CampaignForm {
    /// Defaults for one repository: local-only, one pod, ten accepted outcomes.
    #[must_use]
    pub fn defaults(workspace_root: &Path, default_branch: &str) -> Self {
        Self {
            spec_path: workspace_root.join("campaign.toml").display().to_string(),
            campaign_id: "campaign-1".to_owned(),
            max_units: "10".to_owned(),
            authorization_path: workspace_root
                .join("operator-authorization.txt")
                .display()
                .to_string(),
            team_lead: ProviderForm::default(),
            implementer: ProviderForm::default(),
            reviewer: ProviderForm::default(),
            implementer_authentication: CampaignAuthentication::ExistingLogin,
            allowed_paths: String::new(),
            denied_paths: String::new(),
            protected_branches: default_branch.to_owned(),
            gate_tier: "focused".to_owned(),
            gate_profiles: "configured-gates".to_owned(),
            limits: LimitsForm::default(),
            overwrite: false,
        }
    }

    /// Form populated from an existing verified specification.
    #[must_use]
    pub fn from_spec(
        spec_path: &Path,
        spec: &CampaignSpec,
        authorization_path: Option<&Path>,
    ) -> Self {
        let provider = |provider: &CampaignProvider| ProviderForm {
            executable: provider.executable.display().to_string(),
            model: provider.model.clone(),
            effort: provider.effort.clone(),
        };
        let join = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        Self {
            spec_path: spec_path.display().to_string(),
            campaign_id: spec.campaign_id.clone(),
            max_units: spec.max_units.to_string(),
            authorization_path: authorization_path
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            team_lead: provider(&spec.team_lead),
            implementer: provider(&spec.implementer),
            reviewer: provider(&spec.reviewer),
            implementer_authentication: spec.implementer_authentication,
            allowed_paths: join(&spec.allowed_paths),
            denied_paths: join(&spec.denied_paths),
            protected_branches: spec.protected_branches.join(" "),
            gate_tier: spec
                .gate_tiers
                .first()
                .map(|tier| tier.name.clone())
                .unwrap_or_default(),
            gate_profiles: spec
                .gate_tiers
                .first()
                .map(|tier| tier.profiles.join(" "))
                .unwrap_or_default(),
            limits: LimitsForm {
                provider_attempts: spec.limits.provider_attempts.to_string(),
                malformed_report_repairs: spec.limits.malformed_report_repairs.to_string(),
                correction_rounds: spec.limits.correction_rounds.to_string(),
                process_invocations: spec.limits.process_invocations.to_string(),
                output_bytes: spec.limits.output_bytes.to_string(),
                retained_state_bytes: spec.limits.retained_state_bytes.to_string(),
                execution_elapsed_ms: spec.limits.execution_elapsed_ms.to_string(),
            },
            overwrite: false,
        }
    }

    /// Builds the specification from the form and the coordinator-observed binding.
    ///
    /// # Errors
    ///
    /// Returns field problems found before the existing verifier would run.
    pub fn build(&self, binding: &CampaignBinding) -> Result<CampaignSpec, Vec<FieldError>> {
        let mut errors = Vec::new();
        let authorization_sha256 = self.authorization_digest(binding, &mut errors);
        let max_units = self.max_units.trim().parse::<u32>().unwrap_or_else(|_| {
            errors.push(FieldError {
                field: "accepted-outcome ceiling",
                message: "must be a whole number".to_owned(),
            });
            0
        });
        let team_lead = provider_from(&self.team_lead, "team lead", &mut errors);
        let implementer = provider_from(&self.implementer, "implementer", &mut errors);
        let reviewer = provider_from(&self.reviewer, "reviewer", &mut errors);
        let paths = |value: &str| {
            value
                .split_whitespace()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        };
        let allowed_paths = paths(&self.allowed_paths);
        if allowed_paths.is_empty() {
            errors.push(FieldError {
                field: "allowed paths",
                message: "name at least one repository-relative root".to_owned(),
            });
        }
        let protected_branches = self
            .protected_branches
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if protected_branches.is_empty() {
            errors.push(FieldError {
                field: "protected branches",
                message: "name at least the default branch".to_owned(),
            });
        }
        let limits = self.limits.build(&mut errors);
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(CampaignSpec {
            version: 3,
            campaign_id: self.campaign_id.trim().to_owned(),
            repository_id: binding.repository_id.clone(),
            repository_path: binding.repository_path.clone(),
            initial_commit: binding.initial_commit.clone(),
            task_source_sha256: binding.task_source_sha256.clone(),
            operator_authorization_sha256: authorization_sha256,
            max_parallel_pods: 1,
            max_units,
            limits,
            team_lead,
            implementer,
            implementer_authentication: self.implementer_authentication,
            reviewer,
            gate_tiers: vec![CampaignGateTier {
                name: self.gate_tier.trim().to_owned(),
                profiles: self
                    .gate_profiles
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
            }],
            campaign_branch: format!("codingmage/{}", self.campaign_id.trim()),
            allowed_paths,
            task_path_authority: Vec::new(),
            denied_paths: paths(&self.denied_paths),
            protected_branches,
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        })
    }

    fn authorization_digest(
        &self,
        binding: &CampaignBinding,
        errors: &mut Vec<FieldError>,
    ) -> String {
        let authorization = PathBuf::from(self.authorization_path.trim());
        if !authorization.is_absolute() {
            errors.push(FieldError {
                field: "authorization record",
                message: "must be an absolute path".to_owned(),
            });
            return String::new();
        }
        if authorization.starts_with(&binding.repository_path) {
            errors.push(FieldError {
                field: "authorization record",
                message: "must be outside the target repository".to_owned(),
            });
        }
        file_sha256(&authorization, MAX_AUTHORIZATION_BYTES).unwrap_or_else(|| {
            errors.push(FieldError {
                field: "authorization record",
                message: "must be an existing regular file under 1 MiB".to_owned(),
            });
            String::new()
        })
    }

    /// Writes the specification after the existing verifier and loader accept it.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError`] for field problems, verifier rejection, an existing destination
    /// without overwrite, a destination inside the repository or I/O failure.
    pub fn write(&self, binding: &CampaignBinding) -> Result<PathBuf, WriteError> {
        let spec = self.build(binding).map_err(WriteError::Fields)?;
        spec.verify()
            .map_err(|error| WriteError::Rejected(error.to_string()))?;
        let destination = PathBuf::from(self.spec_path.trim());
        if destination.starts_with(&binding.repository_path) {
            return Err(WriteError::InsideRepository(destination));
        }
        let encoded = toml::to_string_pretty(&spec).map_err(|_| WriteError::Encode)?;
        write_validated(&destination, encoded.as_bytes(), self.overwrite, |path| {
            CampaignSpec::load(path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}

fn provider_from(
    form: &ProviderForm,
    field: &'static str,
    errors: &mut Vec<FieldError>,
) -> CampaignProvider {
    let executable = PathBuf::from(form.executable.trim());
    if !executable.is_absolute() {
        errors.push(FieldError {
            field,
            message: "executable must be an absolute path".to_owned(),
        });
    }
    if form.model.trim().is_empty() {
        errors.push(FieldError {
            field,
            message: "model selector is required".to_owned(),
        });
    }
    if !EFFORTS.contains(&form.effort.trim()) {
        errors.push(FieldError {
            field,
            message: "effort must be one of low, medium, high, xhigh, max".to_owned(),
        });
    }
    CampaignProvider {
        executable,
        model: form.model.trim().to_owned(),
        effort: form.effort.trim().to_owned(),
    }
}

/// Writes an operator authorization record with the exact typed text.
///
/// # Errors
///
/// Returns [`WriteError`] for empty text, a path inside the repository, an existing file without
/// overwrite or I/O failure.
pub fn write_authorization_record(
    path: &Path,
    text: &str,
    repository: &Path,
    overwrite: bool,
) -> Result<String, WriteError> {
    if text.trim().is_empty() {
        return Err(WriteError::Fields(vec![FieldError {
            field: "authorization text",
            message: "record the owner's authorization in your own words".to_owned(),
        }]));
    }
    if !path.is_absolute() {
        return Err(WriteError::Fields(vec![FieldError {
            field: "authorization record",
            message: "must be an absolute path".to_owned(),
        }]));
    }
    if path.starts_with(repository) {
        return Err(WriteError::InsideRepository(path.to_path_buf()));
    }
    write_validated(path, text.as_bytes(), overwrite, |_| Ok(()))?;
    Ok(hex(&sha2::Sha256::digest(text.as_bytes())))
}

use sha2::Digest as _;

fn write_validated(
    destination: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, WriteError> {
    if !destination.is_absolute() {
        return Err(WriteError::Fields(vec![FieldError {
            field: "destination",
            message: "must be an absolute path".to_owned(),
        }]));
    }
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if !overwrite || metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(WriteError::Exists(destination.to_path_buf()));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(WriteError::Io),
    }
    let parent = destination.parent().ok_or(WriteError::Io)?;
    fs::create_dir_all(parent).map_err(|_| WriteError::Io)?;
    let temporary = parent.join(format!(
        ".{}.candidate-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document"),
        std::process::id()
    ));
    let _ = fs::remove_file(&temporary);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        use std::io::Write as _;
        file.write_all(bytes).and_then(|()| file.sync_all())
    });
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(WriteError::Io);
    }
    if let Err(reason) = validate(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(WriteError::Rejected(reason));
    }
    if let Err(_error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        return Err(WriteError::Io);
    }
    Ok(destination.to_path_buf())
}

/// Exports a validated document to another path with the same overwrite safeguard.
///
/// # Errors
///
/// Returns [`WriteError`] when the source is unreadable or the destination is refused.
pub fn export_copy(
    source: &Path,
    destination: &Path,
    overwrite: bool,
) -> Result<PathBuf, WriteError> {
    let metadata = fs::symlink_metadata(source).map_err(|_| WriteError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(WriteError::Io);
    }
    let bytes = fs::read(source).map_err(|_| WriteError::Io)?;
    write_validated(destination, &bytes, overwrite, |_| Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-setup-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        root
    }

    #[test]
    fn config_form_writes_only_loader_accepted_documents_and_refuses_overwrite() {
        let root = root("config");
        let mut form = ConfigForm::defaults(&root.join("target"), &root.join("work"));
        let written = form.write().unwrap();
        assert!(load_config(&written).is_ok());
        assert_eq!(form.write(), Err(WriteError::Exists(written.clone())));
        form.overwrite = true;
        form.correction_limit = "0".to_owned();
        assert!(matches!(form.write(), Err(WriteError::Fields(_))));
        form.correction_limit = "5".to_owned();
        form.capabilities.push = codingmage_core::CapabilityGrant::Allowed;
        let rejected = form.write().unwrap_err();
        assert!(matches!(rejected, WriteError::Rejected(_)), "{rejected}");
        assert!(
            load_config(&written).unwrap().correction_limit == 3,
            "rejected write must not replace the file"
        );
        assert!(
            !root
                .join("work")
                .join(".codingmage.toml.candidate")
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn campaign_form_binds_coordinator_values_and_refuses_records_inside_the_repository() {
        let root = root("campaign");
        let binding = CampaignBinding {
            repository_id: "repo-1-2".to_owned(),
            repository_path: root.join("target"),
            initial_commit: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            default_branch: "main".to_owned(),
        };
        let mut form = CampaignForm::defaults(&root.join("work"), "main");
        let claude = root.join("claude");
        fs::write(&claude, b"#!/bin/sh\n").unwrap();
        for provider in [
            &mut form.team_lead,
            &mut form.implementer,
            &mut form.reviewer,
        ] {
            provider.executable = claude.display().to_string();
            provider.model = "fixture".to_owned();
        }
        form.allowed_paths = "src".to_owned();
        let inside = root.join("target/authorization.txt");
        assert_eq!(
            write_authorization_record(&inside, "authorized", &root.join("target"), false),
            Err(WriteError::InsideRepository(inside))
        );
        let record = root.join("work/operator-authorization.txt");
        let digest = write_authorization_record(
            &record,
            "I authorize this campaign.",
            &root.join("target"),
            false,
        )
        .unwrap();
        form.authorization_path = record.display().to_string();
        let spec = form.build(&binding).unwrap();
        assert_eq!(spec.operator_authorization_sha256, digest);
        assert_eq!(spec.repository_id, "repo-1-2");
        assert_eq!(spec.campaign_branch, "codingmage/campaign-1");
        assert_eq!(spec.publication, CampaignPublication::LocalOnly);
        assert!(spec.multi_agent.is_none());
        let written = form.write(&binding).unwrap();
        let loaded = CampaignSpec::load(&written).unwrap();
        assert_eq!(loaded, spec);
        form.spec_path = root.join("target/campaign.toml").display().to_string();
        assert!(matches!(
            form.write(&binding),
            Err(WriteError::InsideRepository(_))
        ));
        form.spec_path = written.display().to_string();
        form.campaign_id = "bad id".to_owned();
        form.overwrite = true;
        assert!(matches!(form.write(&binding), Err(WriteError::Rejected(_))));
        assert_eq!(CampaignSpec::load(&written).unwrap(), spec);
        fs::remove_dir_all(root).unwrap();
    }
}
