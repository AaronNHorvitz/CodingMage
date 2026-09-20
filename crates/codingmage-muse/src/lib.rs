//! Muse worker protocol requirements, CLI pins, and typed adapter.
//!
//! Local specification for Sub-tasks 31.1.1.1–31.1.1.3: the required
//! structured probe/start/continue/cancel/usage behavior, the exact
//! installed-CLI identity pins, deterministic fake transcripts, and the
//! typed [`MuseAdapter`] built on the provider-neutral
//! [`codingmage_agent`] contract. No CLI is executed by this crate, no
//! authentication material is read, and no live integration is
//! authorized: using a worker additionally requires confinement proofs
//! (31.1.1.4), fault fixtures (31.1.1.5), and explicitly authorized live
//! qualification (Story 31.2).

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use codingmage_agent::{
    AdapterError, AgentCapabilities, AgentCapability, AgentEvent, AgentEventKind, AgentFinal,
    AgentFinalStatus, AgentRole, NormalizedTranscript, ProviderClaims, normalize_events,
};

/// Adapter name a Muse worker must report during the capability probe.
pub const MUSE_PROVIDER: &str = "muse";

/// Required structured behavior for a Muse worker: noninteractive
/// structured output, ordered event streaming, exact session continuation,
/// explicit cancellation, and usage observation. A worker missing any of
/// these fails admission; the gap is a blocker, never a bypass.
#[must_use]
pub fn required_capabilities() -> BTreeSet<AgentCapability> {
    BTreeSet::from([
        AgentCapability::StructuredOutput,
        AgentCapability::EventStream,
        AgentCapability::SessionContinuation,
        AgentCapability::Cancellation,
        AgentCapability::UsageObservation,
    ])
}

/// Reason a reported capability set fails Muse worker admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MuseSpecError {
    /// The probe did not report the Muse adapter name.
    WrongProvider,
    /// The probe reported no version or fingerprint to pin later drift against.
    MissingVersion,
    /// The probe omitted one required structured behavior.
    MissingCapability(AgentCapability),
    /// The adapter profile, session, packet, or environment was invalid.
    InvalidProfile,
    /// The worker role must route to an independent reviewer, not this adapter.
    UnauthorizedRole,
    /// Observed CLI identity differs from the pinned version.
    VersionDrift,
}

impl fmt::Display for MuseSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongProvider => formatter.write_str("codingmage.muse.wrong_provider"),
            Self::MissingVersion => formatter.write_str("codingmage.muse.missing_version"),
            Self::MissingCapability(capability) => {
                write!(
                    formatter,
                    "codingmage.muse.missing_capability.{capability:?}"
                )
            }
            Self::InvalidProfile => formatter.write_str("codingmage.muse.invalid_profile"),
            Self::UnauthorizedRole => formatter.write_str("codingmage.muse.unauthorized_role"),
            Self::VersionDrift => formatter.write_str("codingmage.muse.version_drift"),
        }
    }
}

impl std::error::Error for MuseSpecError {}

/// Exact installed CLI surface observed for Sub-task 31.1.1.2.
///
/// Only explicitly declared flags and subcommands are pinned: identity
/// comes from `--version`, and structured-protocol markers come from the
/// root and `exec` help text. Anything not declared there is recorded as
/// a blocker by [`MuseCliCapabilities::blockers`], never assumed. No
/// executable path is stored: absolute paths are operational mapping and
/// stay outside the repository.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuseCliCapabilities {
    /// Exact observed `--version` text used as the drift pin.
    pub version: String,
    /// A headless `exec` subcommand is declared.
    pub headless_exec: bool,
    /// `exec --json` machine-readable JSONL events are declared.
    pub json_events: bool,
    /// `resume` and `exec --session-id` continuation hooks are declared.
    pub session_resume: bool,
    /// `exec --output-schema` final-answer shaping is declared.
    pub output_schema: bool,
    /// Provider-reported usage counters are advertised.
    pub usage_reporting: bool,
    /// A dedicated CLI-level cancel command is declared.
    pub cancel_command: bool,
}

impl MuseCliCapabilities {
    /// Parses observed `--version`, root `--help`, and `exec --help` text.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::WrongProvider`] when the version text does
    /// not identify Muse Code.
    pub fn parse(version: &str, help: &str, exec_help: &str) -> Result<Self, MuseSpecError> {
        if !version.trim_start().starts_with("Muse Code") {
            return Err(MuseSpecError::WrongProvider);
        }
        Ok(Self {
            version: version.trim().to_owned(),
            headless_exec: help.contains("exec"),
            json_events: exec_help.contains("--json"),
            session_resume: help.contains("resume") && exec_help.contains("--session-id"),
            output_schema: exec_help.contains("--output-schema"),
            usage_reporting: exec_help.contains("--usage")
                || exec_help.contains("usage-report")
                || help.contains("--usage"),
            cancel_command: help.contains("\n  cancel ") || exec_help.contains("\n  cancel "),
        })
    }

    /// Minimum surface for attempting the typed adapter in 31.1.1.3:
    /// headless execution with JSONL events and a session-resume hook.
    #[must_use]
    pub const fn adapter_surface_ready(&self) -> bool {
        self.headless_exec && self.json_events && self.session_resume
    }

    /// Truthful blockers for required worker behavior the CLI does not
    /// declare. Usage observation and CLI-level cancellation stay open
    /// until adapter tests (31.1.1.3/31.1.1.5) or confinement proofs
    /// (31.1.1.4) demonstrate them another way.
    #[must_use]
    pub fn blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();
        if !self.usage_reporting {
            blockers.push("usage counters are not advertised; observe_usage needs adapter proof");
        }
        if !self.cancel_command {
            blockers.push(
                "no CLI cancel command is advertised; cancellation needs process-level proof",
            );
        }
        blockers
    }
}

/// Maximum accepted provider stdout: 4 MiB, matching the process output bound.
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum retained untrusted progress text per event.
const MAX_SUMMARY_BYTES: usize = 4096;
/// Default worker invocation deadline: one hour.
const DEFAULT_DEADLINE_MILLIS: u64 = 60 * 60 * 1000;
/// Minimum worker invocation deadline: one minute.
const MINIMUM_DEADLINE_MILLIS: u64 = 60 * 1000;
/// Maximum worker invocation deadline: one hour.
const MAXIMUM_DEADLINE_MILLIS: u64 = 60 * 60 * 1000;
/// Observed reasoning-effort values.
const OBSERVED_EFFORTS: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];
/// Observed provider modes.
const OBSERVED_PROVIDERS: [&str; 2] = ["echo", "meta"];
/// Observed approval modes.
const OBSERVED_APPROVALS: [&str; 3] = ["untrusted", "on-request", "never"];
/// OS-level login-discovery names. CLI-specific credential names are
/// unobserved and therefore never allowlisted: passing them fails closed.
const LOGIN_ENVIRONMENT_ALLOWLIST: [&str; 4] =
    ["HOME", "PATH", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR"];

/// Credential-discovery boundary for a Muse invocation.
///
/// Reference-only: the adapter carries discovery paths and names but
/// never accepts, stores, or forwards secret material. Live runs rely on
/// ambient provider configuration outside `CodingMage`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MuseAuthentication {
    /// Use ambient provider configuration through validated discovery
    /// names only.
    AmbientReference,
}

/// Exact worker session bound to one coordinator-owned worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuseSession {
    /// Exact session identity passed as `--session-id`.
    pub session_id: codingmage_contracts::AttemptId,
    /// Coordinator-owned worktree the worker runs in.
    pub worktree: PathBuf,
}

impl MuseSession {
    /// Validates an absolute worktree without self or parent references.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::InvalidProfile`] for a relative worktree
    /// or one that can escape through `.` or `..` segments.
    pub fn validate(&self) -> Result<(), MuseSpecError> {
        if !self.worktree.is_absolute()
            || self.worktree.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        Ok(())
    }
}

/// Bounded coordinator-authored work packet for one worker turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuseWorkPacket {
    /// Coordinator-written prompt file passed as `--prompt-file`.
    pub prompt_file: PathBuf,
    /// Assigned role. Review, verification, and administrative roles must
    /// route to an independent reviewer, never to this adapter.
    pub role: AgentRole,
}

impl MuseWorkPacket {
    /// Validates the prompt file path. Role routing is enforced by
    /// [`MuseAdapter::plan_start`] and [`MuseAdapter::plan_resume`].
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::InvalidProfile`] for a relative prompt
    /// file or one that can escape through `.` or `..` segments.
    pub fn validate(&self) -> Result<(), MuseSpecError> {
        if !self.prompt_file.is_absolute()
            || self.prompt_file.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        Ok(())
    }
}

/// Exact CLI plan for one worker turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuseInvocationPlan {
    /// Literal CLI argument vector using only observed flags.
    pub arguments: Vec<String>,
    /// Exact worktree working directory.
    pub working_directory: PathBuf,
}

/// Typed Muse worker adapter without repository or task-state authority.
///
/// The adapter builds exact invocation plans, validates the bounded
/// environment, refuses version drift and reviewer roles, and normalizes
/// provider output. It never executes a subprocess itself: running a plan
/// requires the confinement proofs (31.1.1.4) and explicitly authorized
/// live qualification (Story 31.2).
#[derive(Clone, Debug)]
pub struct MuseAdapter {
    executable: PathBuf,
    expected_version: String,
    model: String,
    effort: String,
    provider: String,
    approval_mode: String,
    environment: BTreeMap<String, String>,
    invocation_deadline_millis: u64,
}

impl MuseAdapter {
    /// Creates a worker profile pinned to one exact CLI version.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError`] for a relative executable, an empty or
    /// unrecognized expected version, a noncanonical model, or provider,
    /// effort, or approval values outside the observed sets.
    pub fn new(
        executable: PathBuf,
        expected_version: &str,
        model: &str,
        effort: &str,
        provider: &str,
        approval_mode: &str,
    ) -> Result<Self, MuseSpecError> {
        if !executable.is_absolute() {
            return Err(MuseSpecError::InvalidProfile);
        }
        if expected_version.trim().is_empty()
            || !expected_version.trim_start().starts_with("Muse Code")
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        if model.is_empty()
            || model.len() > 128
            || !model
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        if !OBSERVED_EFFORTS.contains(&effort)
            || !OBSERVED_PROVIDERS.contains(&provider)
            || !OBSERVED_APPROVALS.contains(&approval_mode)
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        Ok(Self {
            executable,
            expected_version: expected_version.trim().to_owned(),
            model: model.to_owned(),
            effort: effort.to_owned(),
            provider: provider.to_owned(),
            approval_mode: approval_mode.to_owned(),
            environment: BTreeMap::new(),
            invocation_deadline_millis: DEFAULT_DEADLINE_MILLIS,
        })
    }

    /// Returns the pinned executable path.
    #[must_use]
    pub fn executable(&self) -> &std::path::Path {
        &self.executable
    }

    /// Returns the pinned expected CLI version.
    #[must_use]
    pub fn expected_version(&self) -> &str {
        &self.expected_version
    }

    /// Returns the bounded invocation deadline in milliseconds.
    #[must_use]
    pub const fn invocation_deadline_millis(&self) -> u64 {
        self.invocation_deadline_millis
    }

    /// Applies a bounded invocation deadline between one minute and one hour.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::InvalidProfile`] outside that window.
    pub fn with_invocation_deadline_millis(
        mut self,
        deadline_millis: u64,
    ) -> Result<Self, MuseSpecError> {
        if !(MINIMUM_DEADLINE_MILLIS..=MAXIMUM_DEADLINE_MILLIS).contains(&deadline_millis) {
            return Err(MuseSpecError::InvalidProfile);
        }
        self.invocation_deadline_millis = deadline_millis;
        Ok(self)
    }

    /// Supplies a minimal validated login-discovery environment without
    /// accepting secret material: only OS-level discovery names are
    /// allowlisted, values stay bounded, and `PATH` is pinned.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::InvalidProfile`] for an empty map,
    /// unknown names, empty or oversized values, or control characters.
    pub fn with_login_environment(
        mut self,
        environment: BTreeMap<String, String>,
    ) -> Result<Self, MuseSpecError> {
        if environment.is_empty()
            || environment.iter().any(|(name, value)| {
                !LOGIN_ENVIRONMENT_ALLOWLIST.contains(&name.as_str())
                    || value.is_empty()
                    || value.len() > 4096
                    || value.chars().any(char::is_control)
                    || name == "PATH" && value != "/usr/bin:/bin"
            })
        {
            return Err(MuseSpecError::InvalidProfile);
        }
        self.environment = environment;
        Ok(self)
    }

    /// Returns the bounded login-discovery environment.
    #[must_use]
    pub fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    /// Refuses to run when observed CLI identity differs from the pin.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::WrongProvider`] for unrecognized identity
    /// or [`MuseSpecError::VersionDrift`] for any other version.
    pub fn check_version(&self, observed_version: &str) -> Result<(), MuseSpecError> {
        if !observed_version.trim_start().starts_with("Muse Code") {
            return Err(MuseSpecError::WrongProvider);
        }
        if observed_version.trim() != self.expected_version {
            return Err(MuseSpecError::VersionDrift);
        }
        Ok(())
    }

    /// Builds a start plan for one exact new session.
    ///
    /// The fresh session id is generated by the CLI; `--no-session-log`
    /// keeps the anonymous probe hygienic, matching the observed rule
    /// that an explicit `--session-id` requires retained logging.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError`] for an invalid session or packet, or a
    /// role that must route to an independent reviewer.
    pub fn plan_start(
        &self,
        session: &MuseSession,
        packet: &MuseWorkPacket,
    ) -> Result<MuseInvocationPlan, MuseSpecError> {
        self.plan(session, packet, false)
    }

    /// Builds a resume plan for only the exact retained session.
    ///
    /// Retained logging stays on: the CLI refuses `--session-id` with
    /// `--no-session-log`, as observed.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError`] for an invalid session or packet, or a
    /// role that must route to an independent reviewer.
    pub fn plan_resume(
        &self,
        session: &MuseSession,
        packet: &MuseWorkPacket,
    ) -> Result<MuseInvocationPlan, MuseSpecError> {
        self.plan(session, packet, true)
    }

    fn plan(
        &self,
        session: &MuseSession,
        packet: &MuseWorkPacket,
        resume: bool,
    ) -> Result<MuseInvocationPlan, MuseSpecError> {
        session.validate()?;
        packet.validate()?;
        if !matches!(
            packet.role,
            AgentRole::Implementation | AgentRole::Correction
        ) {
            return Err(MuseSpecError::UnauthorizedRole);
        }
        // `--model` and `--reasoning-effort` require `--provider meta`,
        // as the CLI itself reports; echo fixture plans omit them.
        let mut arguments = vec![
            String::from("exec"),
            String::from("--provider"),
            self.provider.clone(),
            String::from("--json"),
        ];
        if self.provider == "meta" {
            arguments.push(String::from("--model"));
            arguments.push(self.model.clone());
            arguments.push(String::from("--reasoning-effort"));
            arguments.push(self.effort.clone());
        }
        arguments.extend([
            String::from("--approval-mode"),
            self.approval_mode.clone(),
            String::from("--prompt-file"),
            packet
                .prompt_file
                .to_str()
                .ok_or(MuseSpecError::InvalidProfile)?
                .to_owned(),
        ]);
        if resume {
            arguments.push(String::from("--session-id"));
            arguments.push(session.session_id.as_str().to_owned());
        } else {
            arguments.push(String::from("--no-session-log"));
        }
        Ok(MuseInvocationPlan {
            arguments,
            working_directory: session.worktree.clone(),
        })
    }

    /// Flags the adapter never emits, so nested workers, tools,
    /// worktrees, and retries stay under coordinator ownership: no
    /// approval/sandbox bypass, no unobserved parallelism, no
    /// worker-created worktrees, no workspace switching, no credential
    /// injection, and no tool-policy overrides. Coordinator defaults
    /// (approval and sandbox on) always apply.
    #[must_use]
    pub fn forbidden_flags() -> &'static [&'static str] {
        &[
            "--yolo",
            "--disable-approval",
            "--disable-sandbox",
            "--trust-workspace",
            "--parallel-tool-calls",
            "--allow-workspace-switch",
            "--api-key-stdin",
            "--disable-write",
            "--disable-shell",
            "--enable-shell-tool",
            "--sandbox-network",
            "-w",
            "--worktree",
            "--worktree-base",
            "--worktree-existing",
        ]
    }

    /// Active refusals keeping the provider out of use until execution
    /// behavior is proven. Plan-surface confinement is proven by the
    /// forbidden-flags and determinism fixtures below; execution-time
    /// confinement (child reaping, aggregate enforcement at runtime)
    /// needs the 31.1.1.5 fault fixtures, and any live run needs Story
    /// 31.2 authority — until then every entry here blocks use.
    #[must_use]
    pub fn execution_blockers() -> Vec<&'static str> {
        vec![
            "no execution path: plans are data until 31.1.1.4 confinement proofs land",
            "usage observation has no CLI source until 31.1.1.5 adapter proof",
            "cancellation has no CLI command until 31.1.1.4 process-level proof",
            "no live runs until Story 31.2 explicit live-run authority",
        ]
    }

    /// Normalizes one observed `exec --json` stdout stream into the
    /// provider-neutral transcript, treating every provider byte as
    /// untrusted data.
    ///
    /// Only the observed terminal shape maps to a claim
    /// (`run.terminal.completed` with `terminal: "completed"` and a null
    /// reason becomes `Completed` with empty claims); every other
    /// terminal shape fails closed so fault variants stay explicit work
    /// for 31.1.1.5. Records outside the planned session, missing or
    /// repeated terminal records, progress text beyond the bound, and
    /// workspace paths or other provider internals never survive:
    /// only truncated delta text reaches the transcript.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::InvalidOutput`] for empty, oversized,
    /// non-UTF-8, malformed, wrong-session, unterminated, or
    /// contradictory output.
    pub fn normalize_output(
        stdout: &[u8],
        session_id: &codingmage_contracts::AttemptId,
    ) -> Result<NormalizedTranscript, AdapterError> {
        if stdout.is_empty() || stdout.len() > MAX_OUTPUT_BYTES {
            return Err(AdapterError::InvalidOutput);
        }
        let text = std::str::from_utf8(stdout).map_err(|_| AdapterError::InvalidOutput)?;
        let mut summaries: Vec<String> = Vec::new();
        let mut terminal: Option<bool> = None;
        for line in text.lines() {
            if line.is_empty() {
                return Err(AdapterError::InvalidOutput);
            }
            let record: serde_json::Value =
                serde_json::from_str(line).map_err(|_| AdapterError::InvalidOutput)?;
            let stream_id = record
                .get("stream")
                .and_then(|stream| stream.get("id"))
                .and_then(serde_json::Value::as_str)
                .ok_or(AdapterError::InvalidOutput)?;
            if stream_id != session_id.as_str() {
                return Err(AdapterError::InvalidOutput);
            }
            let payload_type = record
                .get("payload_type")
                .and_then(serde_json::Value::as_str)
                .ok_or(AdapterError::InvalidOutput)?;
            if payload_type == "run.output.delta" {
                let delta = record
                    .get("payload")
                    .and_then(|payload| payload.get("text"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or(AdapterError::InvalidOutput)?;
                let mut truncated = delta.as_bytes();
                if truncated.len() > MAX_SUMMARY_BYTES {
                    truncated = &truncated[..MAX_SUMMARY_BYTES];
                }
                summaries.push(String::from_utf8_lossy(truncated).into_owned());
            } else if payload_type == "run.terminal.completed" {
                if terminal.is_some() {
                    return Err(AdapterError::InvalidOutput);
                }
                let payload = record.get("payload").ok_or(AdapterError::InvalidOutput)?;
                let completed = payload.get("terminal").and_then(serde_json::Value::as_str)
                    == Some("completed")
                    && payload
                        .get("reason")
                        .is_some_and(serde_json::Value::is_null);
                terminal = Some(completed);
            }
        }
        if terminal != Some(true) {
            return Err(AdapterError::InvalidOutput);
        }
        let mut lines: Vec<String> = Vec::with_capacity(summaries.len() + 2);
        lines.push(
            serde_json::to_string(&AgentEvent {
                version: 1,
                sequence: 0,
                session_id: session_id.clone(),
                event: AgentEventKind::Started,
            })
            .map_err(|_| AdapterError::InvalidOutput)?,
        );
        for (index, summary) in summaries.iter().enumerate() {
            lines.push(
                serde_json::to_string(&AgentEvent {
                    version: 1,
                    sequence: (index + 1) as u64,
                    session_id: session_id.clone(),
                    event: AgentEventKind::Progress {
                        summary: summary.clone(),
                    },
                })
                .map_err(|_| AdapterError::InvalidOutput)?,
            );
        }
        let final_sequence = (summaries.len() + 1) as u64;
        lines.push(
            serde_json::to_string(&AgentEvent {
                version: 1,
                sequence: final_sequence,
                session_id: session_id.clone(),
                event: AgentEventKind::Final {
                    result: AgentFinal {
                        status: AgentFinalStatus::Completed,
                        claims: ProviderClaims::default(),
                        blocker_code: None,
                    },
                },
            })
            .map_err(|_| AdapterError::InvalidOutput)?,
        );
        normalize_events(lines.join("\n").as_bytes(), Some(session_id))
    }
}

/// Checks reported capabilities against the Muse worker requirements.
///
/// # Errors
///
/// Returns [`MuseSpecError`] when the provider name, version, or any
/// required structured behavior is missing. Admission additionally
/// requires the exact CLI inspection and confinement proofs from the
/// later Story 31.1 sub-tasks.
pub fn admit(capabilities: &AgentCapabilities) -> Result<(), MuseSpecError> {
    if capabilities.provider != MUSE_PROVIDER {
        return Err(MuseSpecError::WrongProvider);
    }
    if capabilities.version.is_empty() {
        return Err(MuseSpecError::MissingVersion);
    }
    for required in required_capabilities() {
        if !capabilities.capabilities.contains(&required) {
            return Err(MuseSpecError::MissingCapability(required));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use codingmage_agent::{
        AdapterError, AgentAdapter, AgentEvent, AgentEventKind, AgentFinal, AgentFinalStatus,
        AgentOperation, AgentRequest, AgentRole, AgentUsage, FakeAdapter, FakeStep, ProviderClaims,
    };
    use codingmage_contracts::{AgentId, AttemptId, RunId, TaskId};

    use super::*;

    fn capabilities_without(missing: AgentCapability) -> AgentCapabilities {
        let mut required = required_capabilities();
        required.remove(&missing);
        AgentCapabilities {
            provider: MUSE_PROVIDER.to_owned(),
            version: String::from("fixture-cli-1"),
            capabilities: required,
        }
    }

    fn request(operation: AgentOperation, session_id: Option<AttemptId>) -> AgentRequest {
        AgentRequest {
            version: 1,
            run_id: RunId::new("run-1").expect("valid fixture"),
            task_id: TaskId::new("task-1").expect("valid fixture"),
            agent_id: AgentId::new("agent-muse").expect("valid fixture"),
            role: AgentRole::Implementation,
            operation,
            session_id,
            input_sha256: "b".repeat(64),
        }
    }

    fn completed_final() -> AgentFinal {
        AgentFinal {
            status: AgentFinalStatus::Completed,
            claims: ProviderClaims {
                commit: Some("b".repeat(40)),
                tests_passed: true,
                merge_completed: false,
                release_published: false,
            },
            blocker_code: None,
        }
    }

    fn transcript(session: &AttemptId, input_units: u64, output_units: u64) -> Vec<u8> {
        let events = [
            AgentEvent {
                version: 1,
                sequence: 0,
                session_id: session.clone(),
                event: AgentEventKind::Started,
            },
            AgentEvent {
                version: 1,
                sequence: 1,
                session_id: session.clone(),
                event: AgentEventKind::Usage {
                    input_units,
                    output_units,
                },
            },
            AgentEvent {
                version: 1,
                sequence: 2,
                session_id: session.clone(),
                event: AgentEventKind::Final {
                    result: completed_final(),
                },
            },
        ];
        events
            .iter()
            .map(|event| serde_json::to_string(event).expect("valid fixture"))
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    fn scripted_lifecycle() -> (FakeAdapter, AttemptId) {
        let session = AttemptId::new("attempt-1").expect("valid fixture");
        let adapter = FakeAdapter::new(
            MUSE_PROVIDER,
            vec![
                FakeStep {
                    operation: AgentOperation::Start,
                    response: Ok(transcript(&session, 10, 4)),
                },
                FakeStep {
                    operation: AgentOperation::Continue,
                    response: Ok(transcript(&session, 7, 3)),
                },
                FakeStep {
                    operation: AgentOperation::Cancel,
                    response: Ok(Vec::new()),
                },
            ],
        );
        (adapter, session)
    }

    #[test]
    fn admission_accepts_the_exact_required_behavior() {
        let adapter = FakeAdapter::new(MUSE_PROVIDER, Vec::new());
        let reported = adapter.probe().expect("valid fixture");
        assert_eq!(reported.provider, MUSE_PROVIDER);
        assert!(admit(&reported).is_ok());
    }

    #[test]
    fn admission_rejects_each_missing_behavior() {
        for missing in required_capabilities() {
            assert_eq!(
                admit(&capabilities_without(missing)),
                Err(MuseSpecError::MissingCapability(missing))
            );
        }
    }

    #[test]
    fn admission_rejects_wrong_provider_and_missing_version() {
        let adapter = FakeAdapter::new("other-provider", Vec::new());
        let reported = adapter.probe().expect("valid fixture");
        assert_eq!(admit(&reported), Err(MuseSpecError::WrongProvider));
        let unversioned = AgentCapabilities {
            provider: MUSE_PROVIDER.to_owned(),
            version: String::new(),
            capabilities: required_capabilities(),
        };
        assert_eq!(admit(&unversioned), Err(MuseSpecError::MissingVersion));
    }

    #[test]
    fn fake_transcripts_round_trip_probe_start_continue_usage_cancel() {
        let (mut adapter, session) = scripted_lifecycle();
        assert!(admit(&adapter.probe().expect("valid fixture")).is_ok());
        adapter
            .start(&request(AgentOperation::Start, None))
            .expect("valid fixture");
        adapter
            .continue_session(&request(AgentOperation::Continue, Some(session.clone())))
            .expect("valid fixture");
        assert_eq!(
            adapter.observe_usage(&request(
                AgentOperation::ObserveUsage,
                Some(session.clone())
            )),
            Ok(AgentUsage {
                input_units: 7,
                output_units: 3,
            })
        );
        adapter
            .cancel(&request(AgentOperation::Cancel, Some(session.clone())))
            .expect("valid fixture");
        assert_eq!(
            adapter.cancel(&request(AgentOperation::Cancel, Some(session))),
            Err(AdapterError::InvalidRequest)
        );
    }

    #[test]
    fn fake_transcripts_refuse_wrong_session_continuation() {
        let (mut adapter, session) = scripted_lifecycle();
        adapter
            .start(&request(AgentOperation::Start, None))
            .expect("valid fixture");
        let foreign = AttemptId::new("attempt-9").expect("valid fixture");
        assert_eq!(
            adapter.continue_session(&request(AgentOperation::Continue, Some(foreign))),
            Err(AdapterError::InvalidRequest)
        );
        adapter
            .continue_session(&request(AgentOperation::Continue, Some(session)))
            .expect("valid fixture");
    }

    /// Verbatim excerpts of the observed `--version`, root `--help`, and
    /// `exec --help` output. Paths are excluded; only the identity and
    /// protocol-declaration lines are pinned.
    fn observed_version() -> &'static str {
        "Muse Code 1.3.0 (1.3.0-R3401.1)\n"
    }

    fn observed_help() -> &'static str {
        "muse \u{2014} interactive terminal coding agent\n\nUsage: muse [OPTIONS] [PROMPT]\n       muse [OPTIONS] <COMMAND>\n\nCommands:\n  resume           Resume a previous session (--last or <session-ref>)\n  exec             Run one prompt non-interactively (headless)\n"
    }

    fn observed_exec_help() -> &'static str {
        "muse exec \u{2014} run one prompt non-interactively (headless)\n\nUsage: muse exec [OPTIONS] [PROMPT]\n\nOptions:\n      --json\n          Emit machine-readable JSONL events on stdout\n      --output-schema <FILE>\n          Shape the final answer with the JSON schema in FILE\n      --session-id <UUID>\n          Use a specific session id\n          Offer request_user_input and auto-cancel prompts (headless)\n"
    }

    #[test]
    fn observed_cli_identity_pins_exact_surface_and_blockers() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert_eq!(observed.version, "Muse Code 1.3.0 (1.3.0-R3401.1)");
        assert!(observed.headless_exec);
        assert!(observed.json_events);
        assert!(observed.session_resume);
        assert!(observed.output_schema);
        assert!(!observed.usage_reporting);
        assert!(!observed.cancel_command);
        assert!(observed.adapter_surface_ready());
        assert_eq!(observed.blockers().len(), 2);
    }

    #[test]
    fn cli_parse_rejects_unrecognized_identity() {
        assert_eq!(
            MuseCliCapabilities::parse("other tool 1.0", observed_help(), observed_exec_help()),
            Err(MuseSpecError::WrongProvider)
        );
    }

    #[test]
    fn cli_parse_does_not_mistake_prose_for_a_cancel_command() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert!(observed_exec_help().contains("auto-cancel"));
        assert!(!observed.cancel_command);
    }

    #[test]
    fn adapter_surface_requires_all_three_hooks() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert!(observed.adapter_surface_ready());
        let no_json = MuseCliCapabilities {
            json_events: false,
            ..observed
        };
        assert!(!no_json.adapter_surface_ready());
    }

    fn adapter() -> MuseAdapter {
        MuseAdapter::new(
            PathBuf::from("/usr/local/bin/muse"),
            "Muse Code 1.3.0 (1.3.0-R3401.1)",
            "test-model",
            "high",
            "echo",
            "untrusted",
        )
        .expect("valid fixture")
    }

    fn session() -> MuseSession {
        MuseSession {
            session_id: AttemptId::new("01a0c127-3333-4333-8333-333333333333")
                .expect("valid fixture"),
            worktree: PathBuf::from("/tmp/muse-worker-fixture"),
        }
    }

    fn packet(role: AgentRole) -> MuseWorkPacket {
        MuseWorkPacket {
            prompt_file: PathBuf::from("/tmp/muse-worker-fixture/prompt.md"),
            role,
        }
    }

    fn echo_record(session: &str, payload_type: &str, payload: &serde_json::Value) -> String {
        serde_json::json!({
            "stream": {"kind": "session", "id": session},
            "sequence": 1,
            "payload_type": payload_type,
            "payload": payload,
        })
        .to_string()
    }

    fn echo_completed_stream(session: &str, deltas: &[&str]) -> Vec<u8> {
        let mut lines: Vec<String> = deltas
            .iter()
            .map(|text| {
                echo_record(
                    session,
                    "run.output.delta",
                    &serde_json::json!({"kind": "run_output_delta", "text": text}),
                )
            })
            .collect();
        lines.push(echo_record(
            session,
            "run.lifecycle.started",
            &serde_json::json!({"kind": "run_started"}),
        ));
        lines.push(echo_record(
            session,
            "run.terminal.completed",
            &serde_json::json!({"kind": "run_terminal", "terminal": "completed", "reason": null}),
        ));
        lines.join("\n").into_bytes()
    }

    #[test]
    fn adapter_rejects_noncanonical_profiles() {
        let exe = PathBuf::from("/usr/local/bin/muse");
        let version = "Muse Code 1.3.0 (1.3.0-R3401.1)";
        assert!(MuseAdapter::new(exe.clone(), version, "m", "high", "echo", "untrusted").is_ok());
        assert_eq!(
            MuseAdapter::new(
                PathBuf::from("muse"),
                version,
                "m",
                "high",
                "echo",
                "untrusted"
            )
            .unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        assert_eq!(
            MuseAdapter::new(exe.clone(), "other 1.0", "m", "high", "echo", "untrusted")
                .unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        assert_eq!(
            MuseAdapter::new(
                exe.clone(),
                version,
                "bad model!",
                "high",
                "echo",
                "untrusted"
            )
            .unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        assert_eq!(
            MuseAdapter::new(exe.clone(), version, "m", "turbo", "echo", "untrusted").unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        assert_eq!(
            MuseAdapter::new(exe.clone(), version, "m", "high", "remote", "untrusted").unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        assert_eq!(
            MuseAdapter::new(exe, version, "m", "high", "echo", "always").unwrap_err(),
            MuseSpecError::InvalidProfile
        );
    }

    #[test]
    fn adapter_pins_version_and_refuses_drift() {
        let adapter = adapter();
        assert!(
            adapter
                .check_version("Muse Code 1.3.0 (1.3.0-R3401.1)")
                .is_ok()
        );
        assert_eq!(
            adapter.check_version("Muse Code 1.4.0 (1.4.0-R9999.9)"),
            Err(MuseSpecError::VersionDrift)
        );
        assert_eq!(
            adapter.check_version("other tool 1.0"),
            Err(MuseSpecError::WrongProvider)
        );
    }

    #[test]
    fn adapter_bounds_login_environment() {
        let adapter = adapter();
        assert!(adapter.environment().is_empty());
        let valid = BTreeMap::from([
            (String::from("HOME"), String::from("/tmp/muse-home")),
            (String::from("PATH"), String::from("/usr/bin:/bin")),
        ]);
        let configured = MuseAdapter::new(
            PathBuf::from("/usr/local/bin/muse"),
            "Muse Code 1.3.0 (1.3.0-R3401.1)",
            "test-model",
            "high",
            "echo",
            "untrusted",
        )
        .expect("valid fixture")
        .with_login_environment(valid)
        .expect("valid fixture");
        assert_eq!(configured.environment().len(), 2);
        assert!(
            adapter
                .with_login_environment(BTreeMap::from([(
                    String::from("PROVIDER_API_KEY"),
                    String::from("secret")
                )]))
                .is_err()
        );
        assert!(
            MuseAdapter::new(
                PathBuf::from("/usr/local/bin/muse"),
                "Muse Code 1.3.0 (1.3.0-R3401.1)",
                "test-model",
                "high",
                "echo",
                "untrusted",
            )
            .expect("valid fixture")
            .with_login_environment(BTreeMap::new())
            .is_err()
        );
        let _ = configured;
    }

    #[test]
    fn adapter_bounds_invocation_deadline() {
        let adapter = adapter();
        assert_eq!(adapter.invocation_deadline_millis(), 3_600_000);
        assert!(adapter.with_invocation_deadline_millis(60_000).is_ok());
        assert!(
            MuseAdapter::new(
                PathBuf::from("/usr/local/bin/muse"),
                "Muse Code 1.3.0 (1.3.0-R3401.1)",
                "test-model",
                "high",
                "echo",
                "untrusted",
            )
            .expect("valid fixture")
            .with_invocation_deadline_millis(1_000)
            .is_err()
        );
    }

    #[test]
    fn start_and_resume_plans_use_only_observed_flags() {
        let adapter = adapter();
        let start = adapter
            .plan_start(&session(), &packet(AgentRole::Implementation))
            .expect("valid fixture");
        assert_eq!(
            start.arguments,
            vec![
                "exec",
                "--provider",
                "echo",
                "--json",
                "--approval-mode",
                "untrusted",
                "--prompt-file",
                "/tmp/muse-worker-fixture/prompt.md",
                "--no-session-log",
            ]
        );
        assert_eq!(
            start.working_directory,
            PathBuf::from("/tmp/muse-worker-fixture")
        );
        let resume = adapter
            .plan_resume(&session(), &packet(AgentRole::Correction))
            .expect("valid fixture");
        assert!(resume.arguments.contains(&String::from("--session-id")));
        assert!(
            resume
                .arguments
                .contains(&String::from("01a0c127-3333-4333-8333-333333333333"))
        );
        assert!(!resume.arguments.contains(&String::from("--no-session-log")));
    }

    #[test]
    fn meta_plans_carry_model_and_effort() {
        let adapter = MuseAdapter::new(
            PathBuf::from("/usr/local/bin/muse"),
            "Muse Code 1.3.0 (1.3.0-R3401.1)",
            "test-model",
            "high",
            "meta",
            "untrusted",
        )
        .expect("valid fixture");
        let start = adapter
            .plan_start(&session(), &packet(AgentRole::Implementation))
            .expect("valid fixture");
        let position = |flag: &str| {
            start
                .arguments
                .iter()
                .position(|argument| argument == flag)
                .expect("valid fixture")
        };
        assert_eq!(start.arguments[position("--model") + 1], "test-model");
        assert_eq!(start.arguments[position("--reasoning-effort") + 1], "high");
    }

    #[test]
    fn reviewer_roles_route_away_from_the_worker() {
        let adapter = adapter();
        for role in [
            AgentRole::Review,
            AgentRole::Verification,
            AgentRole::Administrative,
        ] {
            assert_eq!(
                adapter.plan_start(&session(), &packet(role)).unwrap_err(),
                MuseSpecError::UnauthorizedRole
            );
            assert_eq!(
                adapter.plan_resume(&session(), &packet(role)).unwrap_err(),
                MuseSpecError::UnauthorizedRole
            );
        }
    }

    #[test]
    fn invalid_session_and_packet_paths_fail() {
        let adapter = adapter();
        let relative_worktree = MuseSession {
            session_id: AttemptId::new("attempt-1").expect("valid fixture"),
            worktree: PathBuf::from("relative/worktree"),
        };
        assert_eq!(
            adapter
                .plan_start(&relative_worktree, &packet(AgentRole::Implementation))
                .unwrap_err(),
            MuseSpecError::InvalidProfile
        );
        let relative_prompt = MuseWorkPacket {
            prompt_file: PathBuf::from("relative/prompt.md"),
            role: AgentRole::Implementation,
        };
        assert_eq!(
            adapter
                .plan_start(&session(), &relative_prompt)
                .unwrap_err(),
            MuseSpecError::InvalidProfile
        );
    }

    #[test]
    fn normalize_output_maps_the_observed_echo_shape() {
        let session_id =
            AttemptId::new("01a0c127-4444-4433-8433-444444444444").expect("valid fixture");
        let stdout = echo_completed_stream(session_id.as_str(), &["echo: ping"]);
        let transcript =
            MuseAdapter::normalize_output(&stdout, &session_id).expect("valid fixture");
        let events = transcript.events();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0].event, AgentEventKind::Started));
        assert!(matches!(
            &events[1].event,
            AgentEventKind::Progress { summary } if summary == "echo: ping"
        ));
        assert!(matches!(
            &events[2].event,
            AgentEventKind::Final { result } if result.status == AgentFinalStatus::Completed
        ));
        let rendered = serde_json::to_string(events).expect("valid fixture");
        assert!(!rendered.contains("workspace"));
    }

    #[test]
    fn normalize_output_fails_closed_on_unobserved_shapes() {
        let session_id =
            AttemptId::new("01a0c127-5555-4533-8533-555555555555").expect("valid fixture");
        assert_eq!(
            MuseAdapter::normalize_output(&[], &session_id),
            Err(AdapterError::InvalidOutput)
        );
        assert_eq!(
            MuseAdapter::normalize_output(b"not json", &session_id),
            Err(AdapterError::InvalidOutput)
        );
        let foreign = echo_completed_stream("foreign-session", &["echo: ping"]);
        assert_eq!(
            MuseAdapter::normalize_output(&foreign, &session_id),
            Err(AdapterError::InvalidOutput)
        );
        let unterminated = echo_record(
            session_id.as_str(),
            "run.output.delta",
            &serde_json::json!({"kind": "run_output_delta", "text": "echo: ping"}),
        )
        .into_bytes();
        assert_eq!(
            MuseAdapter::normalize_output(&unterminated, &session_id),
            Err(AdapterError::InvalidOutput)
        );
        let failed_terminal = [
            echo_record(
                session_id.as_str(),
                "run.output.delta",
                &serde_json::json!({"kind": "run_output_delta", "text": "echo: ping"}),
            ),
            echo_record(
                session_id.as_str(),
                "run.terminal.completed",
                &serde_json::json!({"kind": "run_terminal", "terminal": "failed", "reason": "boom"}),
            ),
        ]
        .join("\n")
        .into_bytes();
        assert_eq!(
            MuseAdapter::normalize_output(&failed_terminal, &session_id),
            Err(AdapterError::InvalidOutput)
        );
    }

    fn all_plans() -> Vec<MuseInvocationPlan> {
        let echo = adapter();
        let meta = MuseAdapter::new(
            PathBuf::from("/usr/local/bin/muse"),
            "Muse Code 1.3.0 (1.3.0-R3401.1)",
            "test-model",
            "high",
            "meta",
            "untrusted",
        )
        .expect("valid fixture");
        let start_packet = packet(AgentRole::Implementation);
        let resume_packet = packet(AgentRole::Correction);
        vec![
            echo.plan_start(&session(), &start_packet)
                .expect("valid fixture"),
            echo.plan_resume(&session(), &resume_packet)
                .expect("valid fixture"),
            meta.plan_start(&session(), &start_packet)
                .expect("valid fixture"),
            meta.plan_resume(&session(), &resume_packet)
                .expect("valid fixture"),
        ]
    }

    #[test]
    fn plans_never_emit_confinement_escape_hatches() {
        let forbidden = MuseAdapter::forbidden_flags();
        assert!(forbidden.contains(&"--yolo"));
        assert!(forbidden.contains(&"--disable-sandbox"));
        assert!(forbidden.contains(&"--worktree"));
        for plan in all_plans() {
            for argument in &plan.arguments {
                assert!(
                    !forbidden.contains(&argument.as_str()),
                    "escape hatch in plan: {argument}"
                );
            }
            assert_eq!(
                plan.working_directory,
                PathBuf::from("/tmp/muse-worker-fixture")
            );
        }
    }

    #[test]
    fn planning_is_deterministic_with_no_retry_state() {
        let adapter = adapter();
        let first = adapter
            .plan_start(&session(), &packet(AgentRole::Implementation))
            .expect("valid fixture");
        let second = adapter
            .plan_start(&session(), &packet(AgentRole::Implementation))
            .expect("valid fixture");
        assert_eq!(first, second);
    }

    #[test]
    fn provider_stays_refused_until_execution_proofs_land() {
        let blockers = MuseAdapter::execution_blockers();
        assert!(blockers.len() >= 4);
        assert!(blockers.iter().all(|blocker| !blocker.is_empty()));
    }

    #[test]
    fn spec_error_codes_are_stable() {
        assert_eq!(
            MuseSpecError::WrongProvider.to_string(),
            "codingmage.muse.wrong_provider"
        );
        assert_eq!(
            MuseSpecError::MissingVersion.to_string(),
            "codingmage.muse.missing_version"
        );
        assert_eq!(
            MuseSpecError::MissingCapability(AgentCapability::Cancellation).to_string(),
            "codingmage.muse.missing_capability.Cancellation"
        );
        assert_eq!(
            MuseSpecError::InvalidProfile.to_string(),
            "codingmage.muse.invalid_profile"
        );
        assert_eq!(
            MuseSpecError::UnauthorizedRole.to_string(),
            "codingmage.muse.unauthorized_role"
        );
        assert_eq!(
            MuseSpecError::VersionDrift.to_string(),
            "codingmage.muse.version_drift"
        );
    }
}
