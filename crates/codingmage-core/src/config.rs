//! Versioned, deny-by-default project configuration.

use std::{fmt, fs, path::Path};

use codingmage_contracts::AgentId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CONFIG_VERSION: u16 = 1;
const MAX_CONFIG_BYTES: u64 = 1_048_576;

/// Complete versioned configuration for one explicitly selected target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Configuration schema version.
    pub version: u16,
    /// Absolute target repository path.
    pub target_path: std::path::PathBuf,
    /// Relative task source within the target repository.
    pub task_source: std::path::PathBuf,
    /// Expected default branch.
    pub default_branch: String,
    /// CodingMage-owned integration branch prefix.
    pub integration_branch: String,
    /// Absolute root for owned worktrees.
    pub scratch_root: std::path::PathBuf,
    /// Absolute root for private runtime state.
    pub state_root: std::path::PathBuf,
    /// Configured provider profiles.
    pub agent_profiles: Vec<AgentProfile>,
    /// Maximum implementation and review corrections.
    pub correction_limit: u16,
    /// Literal deterministic gate commands.
    pub gate_commands: Vec<CommandSpec>,
    /// Explicitly granted external capabilities.
    #[serde(default)]
    pub capabilities: CapabilityPolicy,
    /// Publication behavior.
    pub publication: PublicationPolicy,
    /// Whether a future caller may search parent directories for task metadata.
    #[serde(default)]
    pub allow_parent_discovery: bool,
}

/// Content-minimized configuration suitable for diagnostics and evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveConfigView {
    /// Configuration schema version.
    pub version: u16,
    /// Fingerprint of the target path, without its text.
    pub target_path_sha256: String,
    /// Fingerprint of the scratch root, without its text.
    pub scratch_root_sha256: String,
    /// Fingerprint of the state root, without its text.
    pub state_root_sha256: String,
    /// Number of configured agent profiles.
    pub agent_profile_count: usize,
    /// Number of configured deterministic gates.
    pub gate_command_count: usize,
    /// Maximum correction count.
    pub correction_limit: u16,
    /// Explicit external capability grants.
    pub capabilities: CapabilityPolicy,
    /// Publication behavior.
    pub publication: PublicationPolicy,
    /// Explicit parent-discovery setting.
    pub allow_parent_discovery: bool,
}

impl Config {
    /// Produces a diagnostic view without paths, branch names, models, executables, or arguments.
    #[must_use]
    pub fn redacted_view(&self) -> EffectiveConfigView {
        EffectiveConfigView {
            version: self.version,
            target_path_sha256: path_fingerprint(&self.target_path),
            scratch_root_sha256: path_fingerprint(&self.scratch_root),
            state_root_sha256: path_fingerprint(&self.state_root),
            agent_profile_count: self.agent_profiles.len(),
            gate_command_count: self.gate_commands.len(),
            correction_limit: self.correction_limit,
            capabilities: self.capabilities,
            publication: self.publication,
            allow_parent_discovery: self.allow_parent_discovery,
        }
    }
}

/// One provider profile selected by typed identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProfile {
    /// Stable profile identity.
    pub id: AgentId,
    /// Provider adapter name, such as `claude` or `codex`.
    pub provider: String,
    /// Provider model profile requested by policy.
    pub model: String,
}

/// One executable and literal argument vector.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// Absolute executable path.
    pub executable: std::path::PathBuf,
    /// Literal arguments passed without shell interpretation.
    #[serde(default)]
    pub args: Vec<String>,
}

/// External capabilities denied unless set explicitly.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityPolicy {
    /// Permit network access for separately authorized operations.
    #[serde(default)]
    pub network: CapabilityGrant,
    /// Permit pushing an authorized feature branch.
    #[serde(default)]
    pub push: CapabilityGrant,
    /// Permit reading and updating GitHub issues.
    #[serde(default)]
    pub issues: CapabilityGrant,
    /// Permit creating or updating draft pull requests.
    #[serde(default)]
    pub pull_requests: CapabilityGrant,
}

/// One explicit capability grant.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityGrant {
    /// Capability is unavailable.
    #[default]
    Denied,
    /// Capability may be considered by its separately constrained adapter.
    Allowed,
}

impl CapabilityGrant {
    const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Publication policy for completed checkpoints.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPolicy {
    /// Highest publication action the coordinator may request.
    pub mode: PublicationMode,
}

/// Supported bootstrap publication modes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationMode {
    /// Keep all commits local.
    LocalOnly,
    /// Push only the configured feature branch.
    PushFeatureBranch,
    /// Push a feature branch and update a draft pull request.
    DraftPullRequest,
}

/// Content-free configuration load failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigLoadError {
    /// The selected file could not be opened safely.
    Unavailable,
    /// The selected file exceeded the byte bound.
    TooLarge,
    /// TOML or its typed schema was invalid.
    InvalidSchema,
    /// The schema version is unsupported.
    UnsupportedVersion,
    /// An authority root was relative, missing, or not a directory.
    InvalidAuthorityRoot,
    /// A task source, branch, profile, or command was invalid.
    InvalidValue,
    /// Two policies contradicted each other.
    ConflictingPolicy,
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "configuration is unavailable",
            Self::TooLarge => "configuration exceeds size limit",
            Self::InvalidSchema => "configuration schema is invalid",
            Self::UnsupportedVersion => "configuration version is unsupported",
            Self::InvalidAuthorityRoot => "configuration authority root is invalid",
            Self::InvalidValue => "configuration value is invalid",
            Self::ConflictingPolicy => "configuration policies conflict",
        })
    }
}

impl std::error::Error for ConfigLoadError {}

/// Loads only `selected_path` and validates every authority-bearing field.
///
/// # Errors
///
/// Returns [`ConfigLoadError`] without including file content, parser details, or selected paths.
pub fn load_config(selected_path: &Path) -> Result<Config, ConfigLoadError> {
    let metadata = fs::symlink_metadata(selected_path).map_err(|_| ConfigLoadError::Unavailable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ConfigLoadError::Unavailable);
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(ConfigLoadError::TooLarge);
    }
    let content = fs::read_to_string(selected_path).map_err(|_| ConfigLoadError::Unavailable)?;
    let config: Config = toml::from_str(&content).map_err(|_| ConfigLoadError::InvalidSchema)?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<(), ConfigLoadError> {
    if config.version != CONFIG_VERSION {
        return Err(ConfigLoadError::UnsupportedVersion);
    }
    for root in [
        &config.target_path,
        &config.scratch_root,
        &config.state_root,
    ] {
        if !root.is_absolute() || !root.is_dir() {
            return Err(ConfigLoadError::InvalidAuthorityRoot);
        }
    }
    if config.task_source.is_absolute()
        || config
            .task_source
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || config.task_source.as_os_str().is_empty()
        || !valid_branch(&config.default_branch)
        || !valid_branch(&config.integration_branch)
        || config.correction_limit == 0
        || config.correction_limit > 100
        || config.agent_profiles.is_empty()
        || config.gate_commands.is_empty()
    {
        return Err(ConfigLoadError::InvalidValue);
    }

    let mut profile_ids = config
        .agent_profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<Vec<_>>();
    profile_ids.sort_unstable();
    if profile_ids.windows(2).any(|pair| pair[0] == pair[1])
        || config
            .agent_profiles
            .iter()
            .any(|profile| profile.provider.is_empty() || profile.model.is_empty())
        || config.gate_commands.iter().any(|command| {
            !command.executable.is_absolute()
                || command.args.iter().any(|argument| argument.contains('\0'))
        })
    {
        return Err(ConfigLoadError::InvalidValue);
    }

    let capabilities = config.capabilities;
    if (capabilities.push.is_allowed()
        || capabilities.issues.is_allowed()
        || capabilities.pull_requests.is_allowed())
        && !capabilities.network.is_allowed()
    {
        return Err(ConfigLoadError::ConflictingPolicy);
    }
    match config.publication.mode {
        PublicationMode::LocalOnly => {
            if capabilities.push.is_allowed() || capabilities.pull_requests.is_allowed() {
                return Err(ConfigLoadError::ConflictingPolicy);
            }
        }
        PublicationMode::PushFeatureBranch => {
            if !capabilities.push.is_allowed() || capabilities.pull_requests.is_allowed() {
                return Err(ConfigLoadError::ConflictingPolicy);
            }
        }
        PublicationMode::DraftPullRequest => {
            if !capabilities.push.is_allowed() || !capabilities.pull_requests.is_allowed() {
                return Err(ConfigLoadError::ConflictingPolicy);
            }
        }
    }
    Ok(())
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with(['-', '.', '/'])
        && !value.ends_with(['.', '/'])
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
}

fn path_fingerprint(path: &Path) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "codingmage-config-{}-{}",
                std::process::id(),
                std::thread::current().name().unwrap_or("test")
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("target")).unwrap();
            fs::create_dir_all(root.join("scratch")).unwrap();
            fs::create_dir_all(root.join("state")).unwrap();
            Self { root }
        }

        fn valid_text(&self) -> String {
            format!(
                r#"version = 1
target_path = "{}"
task_source = "TASKS.md"
default_branch = "main"
integration_branch = "codingmage/integration"
scratch_root = "{}"
state_root = "{}"
correction_limit = 3

[[agent_profiles]]
id = "claude-implementer"
provider = "claude"
model = "sonnet"

[[gate_commands]]
executable = "/usr/bin/git"
args = ["diff", "--check"]

[capabilities]
network = "denied"
push = "denied"
issues = "denied"
pull_requests = "denied"

[publication]
mode = "local_only"
"#,
                self.root.join("target").display(),
                self.root.join("scratch").display(),
                self.root.join("state").display()
            )
        }

        fn write(&self, text: &str) -> PathBuf {
            let path = self.root.join("codingmage.toml");
            fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn valid_config_loads_deterministically() {
        let fixture = Fixture::new();
        let path = fixture.write(&fixture.valid_text());
        let first = load_config(&path).unwrap();
        let second = load_config(&path).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            toml::to_string(&first).unwrap(),
            toml::to_string(&second).unwrap()
        );
    }

    #[test]
    fn effective_view_omits_sensitive_capability_inputs() {
        let fixture = Fixture::new();
        let path = fixture.write(&fixture.valid_text());
        let mut config = load_config(&path).unwrap();
        let synthetic = ["sensitive", "-argument"].concat();
        config.gate_commands[0].args.push(synthetic.clone());
        config.agent_profiles[0].model = synthetic.clone();
        let encoded = toml::to_string(&config.redacted_view()).unwrap();
        assert!(!encoded.contains(&synthetic));
        assert!(!encoded.contains(&fixture.root.to_string_lossy().to_string()));
        assert_eq!(config.redacted_view().target_path_sha256.len(), 64);
    }

    #[test]
    fn unknown_duplicate_secret_and_unsupported_fields_fail_without_content() {
        let fixture = Fixture::new();
        let synthetic = ["sensitive", "-value"].concat();
        let mutations = [
            format!("{}\nunknown = true\n", fixture.valid_text()),
            format!("{}\nversion = 1\n", fixture.valid_text()),
            format!("{}\ntoken = \"{synthetic}\"\n", fixture.valid_text()),
            fixture
                .valid_text()
                .replacen("version = 1", "version = 999", 1),
        ];
        for mutation in mutations {
            let error = load_config(&fixture.write(&mutation)).unwrap_err();
            assert!(!error.to_string().contains(&synthetic));
        }
    }

    #[test]
    fn relative_traversing_and_conflicting_authority_fails() {
        let fixture = Fixture::new();
        let relative = fixture.valid_text().replace(
            &format!(
                "target_path = \"{}\"",
                fixture.root.join("target").display()
            ),
            "target_path = \"relative\"",
        );
        assert_eq!(
            load_config(&fixture.write(&relative)).unwrap_err(),
            ConfigLoadError::InvalidAuthorityRoot
        );

        let traversing = fixture.valid_text().replace(
            "task_source = \"TASKS.md\"",
            "task_source = \"../TASKS.md\"",
        );
        assert_eq!(
            load_config(&fixture.write(&traversing)).unwrap_err(),
            ConfigLoadError::InvalidValue
        );

        let conflicting = fixture
            .valid_text()
            .replace("push = \"denied\"", "push = \"allowed\"");
        assert_eq!(
            load_config(&fixture.write(&conflicting)).unwrap_err(),
            ConfigLoadError::ConflictingPolicy
        );
    }

    #[cfg(unix)]
    #[test]
    fn selected_config_symlink_is_not_followed() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let real = fixture.write(&fixture.valid_text());
        let alias = fixture.root.join("alias.toml");
        symlink(real, &alias).unwrap();
        assert_eq!(
            load_config(&alias).unwrap_err(),
            ConfigLoadError::Unavailable
        );
    }
}
