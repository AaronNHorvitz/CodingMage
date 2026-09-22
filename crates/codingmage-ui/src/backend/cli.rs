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
/// Fixed Git executable used for read-only object reads, matching the coordinator's own choice.
pub const GIT_EXECUTABLE: &str = "/usr/bin/git";

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
        let mut child = command.spawn().map_err(|_| BackendError::Spawn)?;
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

/// Runs one read-only Git command against a repository with a cleared environment.
///
/// Only object and reference reads are issued by the interface (`show`, `diff`, `log`,
/// `rev-parse`); the command never receives a pager, hooks, aliases or ambient configuration.
///
/// # Errors
///
/// Returns [`BackendError`] for spawn failure, timeout, cancellation, oversized output or a
/// nonzero exit, which is reported as `codingmage.ui.git_read`.
pub fn run_git(
    repository: &Path,
    arguments: &[String],
    deadline: Duration,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<u8>, BackendError> {
    let mut command = Command::new(GIT_EXECUTABLE);
    command
        .arg("--no-pager")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PATH", "/usr/bin:/bin");
    let mut child = command.spawn().map_err(|_| BackendError::Spawn)?;
    let stdout = child.stdout.take().ok_or(BackendError::Spawn)?;
    let stderr = child.stderr.take().ok_or(BackendError::Spawn)?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let status = wait_bounded(&mut child, deadline, cancel)?;
    let stdout = stdout_reader.join().map_err(|_| BackendError::Spawn)??;
    let _stderr = stderr_reader.join().map_err(|_| BackendError::Spawn)??;
    if status.success() {
        Ok(stdout)
    } else {
        Err(BackendError::Command {
            code: "codingmage.ui.git_read".to_owned(),
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
            Self::Timeout => "codingmage.ui.timeout".to_owned(),
            Self::Cancelled => "codingmage.ui.cancelled".to_owned(),
            Self::OutputTooLarge => "codingmage.ui.output_too_large".to_owned(),
            Self::Command { code, .. } => code.clone(),
            Self::Contract(_) => "codingmage.ui.contract".to_owned(),
            Self::Refused(_) => "codingmage.ui.refused".to_owned(),
        }
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
            "Choose a new configuration path or existing empty scratch and state roots.",
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
        "codingmage.provider.codex.authentication"
        | "codingmage.provider.claude.authentication" => (
            "A provider login is missing or expired.",
            "Log in to the provider CLI outside CodingMage, then resume.",
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
        "codingmage.ui.binary_unavailable" => (
            "The coordinator executable is not installed next to this interface.",
            "Install codingmage in the same directory or select its path in Setup.",
        ),
        "codingmage.ui.timeout" => (
            "The coordinator command did not finish within the interface deadline.",
            "Retry; if it repeats, run the same command in a terminal to inspect it.",
        ),
        "codingmage.ui.git_read" => (
            "A read-only Git object read failed.",
            "The campaign head or initial commit may be missing from this repository; refresh after the coordinator checkpoints.",
        ),
        "codingmage.ui.contract" => (
            "The coordinator output does not match the contract this interface was built for.",
            "Install matching versions of codingmage and codingmage-ui.",
        ),
        _ => (
            "The coordinator reported a stable failure code.",
            "Look the code up in the operations troubleshooting guide.",
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
            "codingmage.unknown.code",
        ] {
            let (what, action) = explain_code(code);
            assert!(!what.is_empty() && !action.is_empty());
        }
    }
}
