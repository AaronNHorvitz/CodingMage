//! Bounded invocation of the sibling `codingmage` executable.
//!
//! The interface never links the runtime; it runs the installed coordinator binary with exact
//! arguments, reads its machine-readable stdout and maps the stable error code from stderr.

use std::{
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Largest stdout or stderr capture retained from one command.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// File name of the coordinator executable.
pub const COORDINATOR_EXECUTABLE: &str = "codingmage";

/// Location of the coordinator executable relative to this interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinatorBinary {
    path: PathBuf,
}

impl CoordinatorBinary {
    /// Resolves the `codingmage` executable installed next to the running interface.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::BinaryUnavailable`] with the expected path when the sibling
    /// executable is missing, linked or not a regular file.
    pub fn sibling() -> Result<Self, BackendError> {
        let current = std::env::current_exe().map_err(|_| BackendError::BinaryUnavailable {
            expected: PathBuf::from(COORDINATOR_EXECUTABLE),
        })?;
        let expected = current.parent().map_or_else(
            || PathBuf::from(COORDINATOR_EXECUTABLE),
            |parent| parent.join(COORDINATOR_EXECUTABLE),
        );
        Self::at(&expected)
    }

    /// Uses one explicitly selected absolute executable.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::BinaryUnavailable`] when the path is relative, missing, linked or
    /// not a regular file.
    pub fn at(path: &Path) -> Result<Self, BackendError> {
        let unavailable = || BackendError::BinaryUnavailable {
            expected: path.to_path_buf(),
        };
        if !path.is_absolute() {
            return Err(unavailable());
        }
        let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(unavailable());
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Returns the executable path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Runs one command with the given argument vector under a deadline.
    ///
    /// The subprocess receives no stdin, a cleared environment except `HOME` and `PATH` for
    /// provider probes, and is killed when the deadline passes or `cancel` is set.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for spawn failure, timeout, cancellation, oversized output or a
    /// nonzero exit with its stable code.
    pub fn run(
        &self,
        arguments: &[String],
        deadline: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<u8>, BackendError> {
        let mut command = Command::new(&self.path);
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for name in ["HOME", "PATH", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                BackendError::PermissionDenied
            } else {
                BackendError::Spawn
            }
        })?;
        let stdout = child.stdout.take().ok_or(BackendError::Spawn)?;
        let stderr = child.stderr.take().ok_or(BackendError::Spawn)?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout));
        let stderr_reader = thread::spawn(move || read_bounded(stderr));
        let status = wait_bounded(&mut child, deadline, cancel)?;
        let stdout = stdout_reader.join().map_err(|_| BackendError::Spawn)??;
        let stderr = stderr_reader.join().map_err(|_| BackendError::Spawn)??;
        if status.success() {
            return Ok(stdout);
        }
        let code = String::from_utf8_lossy(&stderr)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        Err(BackendError::Command {
            code: if valid_code(&code) {
                code
            } else {
                "codingmage.ui.unreadable_failure".to_owned()
            },
            exit_code: status.code(),
        })
    }
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 160
        && code.starts_with("codingmage.")
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn read_bounded(mut stream: impl Read) -> Result<Vec<u8>, BackendError> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk).map_err(|_| BackendError::Spawn)?;
        if read == 0 {
            return Ok(buffer);
        }
        if buffer.len() + read > MAX_OUTPUT_BYTES {
            return Err(BackendError::OutputTooLarge);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn wait_bounded(
    child: &mut Child,
    deadline: Duration,
    cancel: &Arc<AtomicBool>,
) -> Result<std::process::ExitStatus, BackendError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| BackendError::Spawn)? {
            return Ok(status);
        }
        let reason = if cancel.load(Ordering::Acquire) {
            Some(BackendError::Cancelled)
        } else if started.elapsed() >= deadline {
            Some(BackendError::Timeout)
        } else {
            None
        };
        if let Some(reason) = reason {
            let _ = child.kill();
            let _ = child.wait();
            return Err(reason);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Failure of one backend invocation, without command output content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendError {
    /// The coordinator executable is not installed next to the interface.
    BinaryUnavailable {
        /// Path that was expected to hold the executable.
        expected: PathBuf,
    },
    /// The subprocess could not be started or its streams could not be read.
    Spawn,
    /// The coordinator file exists but the operating system denied execution.
    PermissionDenied,
    /// The command exceeded its deadline and was terminated.
    Timeout,
    /// The request was cancelled before completion.
    Cancelled,
    /// The command produced more output than the interface retains.
    OutputTooLarge,
    /// The command exited nonzero with a stable code.
    Command {
        /// Stable `codingmage.*` code from stderr.
        code: String,
        /// Process exit code when available.
        exit_code: Option<i32>,
    },
    /// The command succeeded but its output did not match the expected contract.
    Contract(super::models::ModelError),
    /// The interface refused the request before invoking the backend.
    Refused(String),
}

impl BackendError {
    /// Stable code used in status lines and evidence.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::BinaryUnavailable { .. } => "codingmage.ui.binary_unavailable".to_owned(),
            Self::Spawn => "codingmage.ui.spawn".to_owned(),
            Self::PermissionDenied => "codingmage.ui.permission_denied".to_owned(),
            Self::Timeout => "codingmage.ui.timeout".to_owned(),
            Self::Cancelled => "codingmage.ui.cancelled".to_owned(),
            Self::OutputTooLarge => "codingmage.ui.output_too_large".to_owned(),
            Self::Command { code, .. } => code.clone(),
            Self::Contract(_) => "codingmage.ui.contract".to_owned(),
            Self::Refused(_) => "codingmage.ui.refused".to_owned(),
        }
    }

    /// Presentation state derived only from the known failure kind or stable coordinator code.
    #[must_use]
    pub fn failure_state(&self) -> FailureState {
        match self {
            Self::BinaryUnavailable { .. } => FailureState::ExecutableMissing,
            Self::PermissionDenied => FailureState::PermissionDenied,
            Self::Contract(error) => match error {
                super::models::ModelError::Malformed => FailureState::MalformedOutput,
                super::models::ModelError::AuthorityMismatch => FailureState::IdentityMismatch,
                super::models::ModelError::UnsupportedSchema { .. } => {
                    FailureState::UnsupportedSchema
                }
            },
            Self::OutputTooLarge => FailureState::OutputTooLarge,
            Self::Spawn => FailureState::CoordinatorUnavailable,
            Self::Timeout => FailureState::CoordinatorTimedOut,
            Self::Command { code, .. } => provider_failure_state(code),
            Self::Cancelled | Self::Refused(_) => FailureState::RequestFailed,
        }
    }
}

/// Truthful, user-visible failure category; the stable code remains available for detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureState {
    /// The expected sibling coordinator file is absent or unusable.
    ExecutableMissing,
    /// Execution was denied by the operating system.
    PermissionDenied,
    /// A configured provider needs its own login renewed.
    AuthenticationRequired,
    /// A known provider process or capability is unavailable.
    ProviderUnavailable,
    /// Successful backend output violated the expected schema.
    MalformedOutput,
    /// Backend output named a different selected campaign or authority.
    IdentityMismatch,
    /// Backend output uses a schema this interface does not support.
    UnsupportedSchema,
    /// Backend output exceeded the bounded capture size.
    OutputTooLarge,
    /// The coordinator could not be started or contacted.
    CoordinatorUnavailable,
    /// The coordinator did not respond before the request deadline.
    CoordinatorTimedOut,
    /// A refusal or unclassified stable error occurred.
    RequestFailed,
}

fn provider_failure_state(code: &str) -> FailureState {
    match code {
        "codingmage.provider.codex.authentication"
        | "codingmage.provider.claude.authentication" => FailureState::AuthenticationRequired,
        "codingmage.provider.codex.capability_missing"
        | "codingmage.provider.claude.capability_missing"
        | "codingmage.provider.codex.unsupported_version"
        | "codingmage.provider.claude.unsupported_version"
        | "codingmage.provider.codex.invalid_profile"
        | "codingmage.provider.claude.invalid_profile"
        | "codingmage.provider.codex.process"
        | "codingmage.provider.claude.process"
        | "codingmage.provider.codex.failed"
        | "codingmage.provider.claude.failed"
        | "codingmage.provider.codex.timeout"
        | "codingmage.provider.claude.timeout"
        | "codingmage.provider.codex.quota"
        | "codingmage.provider.claude.quota" => FailureState::ProviderUnavailable,
        _ => FailureState::RequestFailed,
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryUnavailable { expected } => write!(
                formatter,
                "the coordinator executable was not found at {}",
                expected.display()
            ),
            Self::Spawn => formatter.write_str("the coordinator process could not be started"),
            Self::PermissionDenied => {
                formatter.write_str("the operating system denied execution of the coordinator")
            }
            Self::Timeout => formatter.write_str("the coordinator command exceeded its deadline"),
            Self::Cancelled => formatter.write_str("the request was cancelled"),
            Self::OutputTooLarge => {
                formatter.write_str("the coordinator produced more output than can be shown")
            }
            Self::Command { code, exit_code } => match exit_code {
                Some(exit) => write!(formatter, "{code} (exit {exit})"),
                None => write!(formatter, "{code} (terminated by signal)"),
            },
            Self::Contract(error) => write!(formatter, "{error}"),
            Self::Refused(reason) => write!(formatter, "{reason}"),
        }
    }
}

impl std::error::Error for BackendError {}

impl From<super::models::ModelError> for BackendError {
    fn from(error: super::models::ModelError) -> Self {
        Self::Contract(error)
    }
}

/// Human explanation and next action for a stable coordinator code.
#[must_use]
pub fn explain_code(code: &str) -> (&'static str, &'static str) {
    match code {
        "codingmage.cli.usage" | "codingmage.cli.invalid_argument" => (
            "The interface sent arguments the coordinator rejected.",
            "Check that every selected path is absolute, exists and is not a symbolic link.",
        ),
        "codingmage.cli.config" => (
            "The configuration file could not be loaded.",
            "Open Setup to validate the configuration; roots must be absolute existing directories and policies must agree.",
        ),
        "codingmage.cli.repository" | "codingmage.runtime.authority" => (
            "Repository authorization was refused.",
            "The target must be a clean, user-owned, non-bare Git checkout that is not nested, not CodingMage itself, and whose head, branch and identity match the campaign authority.",
        ),
        "codingmage.cli.plan" | "codingmage.runtime.plan" => (
            "The task source could not be used.",
            "The task source must parse with the strict checklist grammar, match the campaign digest and keep at least ten open sub-tasks for preflight.",
        ),
        "codingmage.cli.no_ready_work" => (
            "No dependency-ready sub-task exists.",
            "Complete or unblock prerequisite items in the task source.",
        ),
        "codingmage.cli.refused" => (
            "The coordinator refused to overwrite or broaden authority.",
            "Choose a new destination outside the repository or confirm overwrite of an existing regular file; for Setup, check the selected roots.",
        ),
        "codingmage.cli.uncertain_write" => (
            "The report destination changed during publication.",
            "Inspect the destination and retained .codingmage-cli-report-*.staging directory in its parent before retrying; keep the stage until the files are reconciled.",
        ),
        "codingmage.cli.stale_observation" => (
            "The campaign head changed while task states were requested.",
            "Refresh the campaign to observe its current reconciled head.",
        ),
        "codingmage.runtime.spec"
        | "codingmage.runtime.campaign.spec"
        | "codingmage.runtime.campaign.authority" => (
            "The campaign specification is invalid.",
            "Open Setup to revalidate the campaign authority fields.",
        ),
        "codingmage.runtime.repository" => (
            "The repository state is unsafe for the requested operation.",
            "The active checkout must be clean and free of unsupported features.",
        ),
        "codingmage.runtime.state" => (
            "Durable campaign state is missing, complete, cancelled or unreadable.",
            "Controls apply only to an existing, unfinished campaign under the same authority.",
        ),
        "codingmage.runtime.orchestration" => (
            "The coordinator lock for this repository is held by another live process.",
            "Wait for the running coordinator to reach a boundary or stop it through its own controls.",
        ),
        "codingmage.runtime.process" => (
            "The guarded process runtime is unavailable.",
            "Check that the coordinator executable can start its process guard.",
        ),
        code if code.starts_with("codingmage.provider.") => explain_provider_code(code),
        code if code.starts_with("codingmage.ui.") => explain_interface_code(code),
        _ => (
            "The coordinator reported a stable failure code.",
            "Look the code up in the operations troubleshooting guide.",
        ),
    }
}

fn explain_provider_code(code: &str) -> (&'static str, &'static str) {
    match code {
        "codingmage.provider.codex.authentication"
        | "codingmage.provider.claude.authentication" => (
            "A provider login is missing or expired.",
            "Log in to the configured provider CLI outside CodingMage, then rerun preflight or resume the campaign.",
        ),
        "codingmage.provider.codex.quota" | "codingmage.provider.claude.quota" => (
            "A provider reported exhausted quota.",
            "Wait for the provider reset, then resume; CodingMage never acquires capacity.",
        ),
        "codingmage.provider.codex.capability_missing"
        | "codingmage.provider.claude.capability_missing"
        | "codingmage.provider.codex.unsupported_version"
        | "codingmage.provider.claude.unsupported_version" => (
            "A configured provider executable lacks a required capability or version.",
            "Install a supported provider version at the configured path; CodingMage never substitutes another provider or model.",
        ),
        "codingmage.provider.codex.invalid_profile"
        | "codingmage.provider.claude.invalid_profile" => (
            "A configured provider profile is invalid.",
            "Check the executable path, model selector and effort in the campaign authority.",
        ),
        "codingmage.provider.codex.process"
        | "codingmage.provider.claude.process"
        | "codingmage.provider.codex.failed"
        | "codingmage.provider.claude.failed" => (
            "A configured provider executable could not be probed.",
            "Run the provider's own version command in a terminal to confirm it starts.",
        ),
        "codingmage.provider.codex.timeout" | "codingmage.provider.claude.timeout" => (
            "A provider did not answer within its bounded deadline.",
            "Retry; persistent timeouts indicate a provider or network problem outside CodingMage.",
        ),
        _ => (
            "A configured provider reported a stable failure code.",
            "Run the provider's own version command in a terminal and check its login state.",
        ),
    }
}

fn explain_interface_code(code: &str) -> (&'static str, &'static str) {
    match code {
        "codingmage.ui.binary_unavailable" => (
            "The coordinator executable is not installed next to this interface.",
            "Install codingmage next to codingmage-ui, then restart the app.",
        ),
        "codingmage.ui.timeout" => (
            "The coordinator command did not finish within the interface deadline.",
            "Retry; if it repeats, run the same command in a terminal to inspect it.",
        ),
        "codingmage.ui.permission_denied" => (
            "The coordinator file exists, but the operating system denied execution.",
            "Restore execute permission for the installed codingmage binary outside the app, then retry the action.",
        ),
        "codingmage.ui.spawn" => (
            "The coordinator process could not be started or read.",
            "Check the installed coordinator and retry the read. Refresh campaign status before repeating a control whose outcome is unknown.",
        ),
        "codingmage.ui.output_too_large" => (
            "The coordinator returned more output than this interface can safely retain.",
            "Inspect the same command with the coordinator CLI; reduce the requested scope before retrying.",
        ),
        "codingmage.ui.unreadable_failure" => (
            "The coordinator exited without a readable stable failure code.",
            "Check that codingmage and codingmage-ui are matching versions, then rerun the exact shown command in a terminal.",
        ),
        "codingmage.ui.git_read" => (
            "A read-only Git object read failed.",
            "The campaign head or initial commit may be missing from this repository; refresh after the coordinator checkpoints.",
        ),
        "codingmage.ui.outcome_unknown" => (
            "The interface lost the outcome of a control request.",
            "Issue the same control again; the coordinator replays the exact request identity idempotently.",
        ),
        "codingmage.ui.contract" => (
            "The coordinator output does not match the contract this interface was built for.",
            "Refresh this observation; if the mismatch repeats, install matching versions of codingmage and codingmage-ui.",
        ),
        _ => (
            "The interface could not confirm this request's outcome.",
            "Refresh coordinator status before retrying any control; an unknown outcome does not prove no effect.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_missing_and_linked_binaries_are_unavailable() {
        assert!(matches!(
            CoordinatorBinary::at(Path::new("relative")),
            Err(BackendError::BinaryUnavailable { .. })
        ));
        assert!(matches!(
            CoordinatorBinary::at(Path::new("/nonexistent/codingmage")),
            Err(BackendError::BinaryUnavailable { .. })
        ));
        let root = std::env::temp_dir().join(format!("codingmage-ui-cli-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let real = root.join("codingmage");
        fs::write(&real, b"#!/bin/sh\nexit 0\n").unwrap();
        std::os::unix::fs::symlink(&real, root.join("alias")).unwrap();
        assert!(matches!(
            CoordinatorBinary::at(&root.join("alias")),
            Err(BackendError::BinaryUnavailable { .. })
        ));
        assert!(CoordinatorBinary::at(&real).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stderr_codes_timeouts_and_cancellation_are_reported() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!("codingmage-ui-run-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let script = root.join("codingmage");
        fs::write(
            &script,
            "#!/bin/sh\ncase \"$1\" in\n  fail) echo codingmage.cli.config >&2; exit 1;;\n  garbage) echo 'stack trace: /home/x' >&2; exit 1;;\n  sleep) sleep 5;;\n  *) echo '{\"ok\":true}';;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let binary = CoordinatorBinary::at(&script).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        assert_eq!(
            binary
                .run(&["ok".to_owned()], Duration::from_secs(5), &cancel)
                .unwrap(),
            b"{\"ok\":true}\n"
        );
        assert_eq!(
            binary.run(&["fail".to_owned()], Duration::from_secs(5), &cancel),
            Err(BackendError::Command {
                code: "codingmage.cli.config".to_owned(),
                exit_code: Some(1)
            })
        );
        assert_eq!(
            binary.run(&["garbage".to_owned()], Duration::from_secs(5), &cancel),
            Err(BackendError::Command {
                code: "codingmage.ui.unreadable_failure".to_owned(),
                exit_code: Some(1)
            })
        );
        let started = Instant::now();
        assert_eq!(
            binary.run(&["sleep".to_owned()], Duration::from_millis(200), &cancel),
            Err(BackendError::Timeout)
        );
        assert!(started.elapsed() < Duration::from_secs(4));
        cancel.store(true, Ordering::Release);
        assert_eq!(
            binary.run(&["sleep".to_owned()], Duration::from_secs(5), &cancel),
            Err(BackendError::Cancelled)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_explanation_has_an_action() {
        for code in [
            "codingmage.cli.config",
            "codingmage.runtime.authority",
            "codingmage.cli.uncertain_write",
            "codingmage.unknown.code",
        ] {
            let (what, action) = explain_code(code);
            assert!(!what.is_empty() && !action.is_empty());
        }
        let (what, action) = explain_code("codingmage.cli.uncertain_write");
        assert!(what.contains("changed during publication"));
        assert!(action.contains("retained") && action.contains("before retrying"));
    }

    #[test]
    fn failure_states_follow_known_error_kinds_and_codes_only() {
        let command = |code: &str| BackendError::Command {
            code: code.to_owned(),
            exit_code: Some(1),
        };
        assert_eq!(
            BackendError::BinaryUnavailable {
                expected: PathBuf::from("/missing/codingmage"),
            }
            .failure_state(),
            FailureState::ExecutableMissing
        );
        assert_eq!(
            BackendError::PermissionDenied.failure_state(),
            FailureState::PermissionDenied
        );
        assert_eq!(
            BackendError::Contract(super::super::models::ModelError::Malformed).failure_state(),
            FailureState::MalformedOutput
        );
        assert_eq!(
            BackendError::Contract(super::super::models::ModelError::AuthorityMismatch)
                .failure_state(),
            FailureState::IdentityMismatch
        );
        assert_eq!(
            BackendError::Contract(super::super::models::ModelError::UnsupportedSchema {
                observed: 9,
                supported: 2,
            })
            .failure_state(),
            FailureState::UnsupportedSchema
        );
        assert_eq!(
            command("codingmage.provider.codex.authentication").failure_state(),
            FailureState::AuthenticationRequired
        );
        assert_eq!(
            command("codingmage.provider.claude.capability_missing").failure_state(),
            FailureState::ProviderUnavailable
        );
        assert_eq!(
            command("codingmage.provider.codex.quota").failure_state(),
            FailureState::ProviderUnavailable
        );
        assert_eq!(
            BackendError::Spawn.failure_state(),
            FailureState::CoordinatorUnavailable
        );
        assert_eq!(
            BackendError::OutputTooLarge.failure_state(),
            FailureState::OutputTooLarge
        );
    }
}
