//! Request validation, guarded execution, and content-minimized results.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nix::{
    sys::{
        resource::{Resource, rlim_t, setrlimit},
        signal::{Signal, killpg},
    },
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_ARGUMENTS: usize = 1024;
const MAX_ARGUMENT_BYTES: usize = 1_048_576;
const MAX_ENVIRONMENT: usize = 128;
const MAX_STDIN_BYTES: usize = 1_048_576;
const MAX_OUTPUT_BOUND: u64 = 64 * 1024 * 1024;
const MAX_DEADLINE_MILLIS: u64 = 24 * 60 * 60 * 1000;
const MAX_OPEN_FILES: u64 = 1024;
const MAX_PROCESSES: u32 = 64;
const GUARD_TIMEOUT_EXIT: i32 = 120;
const GUARD_CANCEL_EXIT: i32 = 121;
const GUARD_PARENT_EXIT: i32 = 122;
const GUARD_INTERNAL_EXIT: i32 = 123;

/// Physical and content identity of one executable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableIdentity {
    /// Canonical executable path.
    pub path: PathBuf,
    /// Filesystem device.
    pub device: u64,
    /// Filesystem inode.
    pub inode: u64,
    /// Executable byte length.
    pub length: u64,
    /// SHA-256 of executable bytes.
    pub sha256: String,
}

/// Trusted profile defining exact command and environment authority.
#[derive(Clone, Debug)]
pub struct ProcessProfile {
    executable: ExecutableIdentity,
    allowed_argument_vectors: BTreeSet<Vec<String>>,
    allowed_environment_names: BTreeSet<String>,
}

impl ProcessProfile {
    /// Resolves an executable and freezes exact argument vectors and environment names.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessError`] if the executable is not an absolute, regular, nonsymlink file or
    /// any argument template or environment name violates a runtime bound.
    pub fn new(
        executable: &Path,
        allowed_argument_vectors: impl IntoIterator<Item = Vec<String>>,
        allowed_environment_names: impl IntoIterator<Item = String>,
    ) -> Result<Self, ProcessError> {
        let executable = executable_identity(executable)?;
        let allowed_argument_vectors: BTreeSet<Vec<String>> =
            allowed_argument_vectors.into_iter().collect();
        let allowed_environment_names: BTreeSet<String> =
            allowed_environment_names.into_iter().collect();
        if allowed_argument_vectors.is_empty()
            || allowed_argument_vectors
                .iter()
                .any(|arguments| validate_arguments(arguments).is_err())
            || allowed_environment_names.len() > MAX_ENVIRONMENT
            || allowed_environment_names
                .iter()
                .any(|name| !valid_environment_name(name))
        {
            return Err(ProcessError::InvalidProfile);
        }
        Ok(Self {
            executable,
            allowed_argument_vectors,
            allowed_environment_names,
        })
    }

    /// Returns the pinned executable identity.
    #[must_use]
    pub const fn executable(&self) -> &ExecutableIdentity {
        &self.executable
    }
}

/// One bounded invocation. It contains no shell string or ambient-environment option.
#[derive(Clone, Debug)]
pub struct ProcessRequest {
    /// Exact literal argument vector.
    pub arguments: Vec<String>,
    /// Explicit absolute working directory.
    pub working_directory: PathBuf,
    /// Explicit environment entries granted by the profile.
    pub environment: BTreeMap<String, String>,
    /// Bounded bytes written to standard input.
    pub stdin: Vec<u8>,
    /// Maximum retained bytes for each output stream.
    pub max_output_bytes: u64,
    /// Required wall-clock deadline.
    pub deadline_millis: u64,
    /// Maximum observed process-group members.
    pub max_processes: u32,
    /// Open-file limit inherited by the target.
    pub max_open_files: u64,
    /// Exit codes classified as success.
    pub expected_exit_codes: BTreeSet<i32>,
}

/// Shared cancellation signal observed without model or process authority.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Requests cancellation of the exact invocation using this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Complete stream digest with a bounded retained prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedStream {
    /// Retained prefix for immediate adapter parsing; callers must not persist it by default.
    pub retained: Vec<u8>,
    /// Total bytes observed before termination.
    pub total_bytes: u64,
    /// SHA-256 of every observed byte.
    pub sha256: String,
    /// Whether total bytes exceeded the retention bound.
    pub truncated: bool,
}

/// Terminal process classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    /// Target exited with an expected code.
    Succeeded,
    /// Target exited with a nonexpected code.
    Failed,
    /// Deadline expired and the target group was terminated.
    TimedOut,
    /// Operator cancellation terminated the target group.
    Cancelled,
    /// An output stream exceeded its configured bound.
    OutputLimit,
    /// Guard detected that its coordinator parent disappeared.
    ParentLost,
    /// Runtime or guard failed before a trustworthy target result existed.
    RuntimeFailure,
}

/// Result of exact descendant cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescendantCleanup {
    /// Target exited normally and no forced cleanup was needed.
    NotRequired,
    /// Guard terminated and reaped the target process group.
    Verified,
    /// Cleanup could not be established.
    Uncertain,
}

/// One truthful terminal process record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    /// Terminal classification.
    pub outcome: ProcessOutcome,
    /// Literal guard exit code or `-1` when signaled.
    pub exit_code: i32,
    /// Guard process identity and target process-group identity.
    pub process_group_id: u32,
    /// Complete standard-output record.
    pub stdout: CapturedStream,
    /// Complete standard-error record.
    pub stderr: CapturedStream,
    /// Elapsed wall time.
    pub elapsed: Duration,
    /// Descendant cleanup status.
    pub descendant_cleanup: DescendantCleanup,
}

/// Content-free process validation or startup failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    /// Profile was empty, oversized, or noncanonical.
    InvalidProfile,
    /// Request exceeded a bound or was incomplete.
    InvalidRequest,
    /// Arguments were not an exact trusted template.
    ArgumentsDenied,
    /// Environment requested a name outside the trusted profile.
    EnvironmentDenied,
    /// Executable or working-directory identity was unavailable or changed.
    Identity,
    /// Guard could not be spawned or initialized.
    Spawn,
    /// Runtime control state could not be created safely.
    Control,
}

impl fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProfile => "codingmage.process.invalid_profile",
            Self::InvalidRequest => "codingmage.process.invalid_request",
            Self::ArgumentsDenied => "codingmage.process.arguments_denied",
            Self::EnvironmentDenied => "codingmage.process.environment_denied",
            Self::Identity => "codingmage.process.identity",
            Self::Spawn => "codingmage.process.spawn",
            Self::Control => "codingmage.process.control",
        })
    }
}

impl std::error::Error for ProcessError {}

/// Executor bound to one guard binary and private control root.
#[derive(Clone, Debug)]
pub struct ProcessExecutor {
    guard: ExecutableIdentity,
    guard_arguments: Vec<String>,
    control_root: PathBuf,
}

impl ProcessExecutor {
    /// Pins the guard binary and prepares a private control root.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessError`] when guard or control-root identity and permissions are unsafe.
    pub fn new(guard: &Path, control_root: &Path) -> Result<Self, ProcessError> {
        Self::new_with_guard_arguments(guard, Vec::new(), control_root)
    }

    /// Pins a guard binary plus an exact literal argument vector and prepares a private control
    /// root.
    ///
    /// This permits a packaged application to expose its guard as a hidden subcommand without
    /// falling back to a shell, path search, or mutable wrapper script.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessError`] when guard identity, arguments, or control-root permissions are
    /// unsafe.
    pub fn new_with_guard_arguments(
        guard: &Path,
        guard_arguments: Vec<String>,
        control_root: &Path,
    ) -> Result<Self, ProcessError> {
        if guard_arguments.len() > MAX_ARGUMENTS
            || guard_arguments
                .iter()
                .any(|argument| argument.is_empty() || argument.contains('\0'))
        {
            return Err(ProcessError::InvalidRequest);
        }
        let guard = executable_identity(guard)?;
        prepare_private_directory(control_root)?;
        Ok(Self {
            guard,
            guard_arguments,
            control_root: fs::canonicalize(control_root).map_err(|_| ProcessError::Control)?,
        })
    }

    /// Executes one exact request and returns one terminal result.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessError`] only before a trustworthy guarded execution begins. Once the guard
    /// starts, every terminal path is represented by [`ProcessResult`].
    #[allow(clippy::too_many_lines)]
    pub fn execute(
        &self,
        profile: &ProcessProfile,
        request: &ProcessRequest,
        cancellation: &CancellationToken,
    ) -> Result<ProcessResult, ProcessError> {
        validate_request(profile, request)?;
        revalidate_identity(&self.guard)?;
        revalidate_identity(&profile.executable)?;
        let working_directory = checked_directory(&request.working_directory)?;
        let parent_pid = std::process::id();
        let parent_start = process_start_time(parent_pid).ok_or(ProcessError::Control)?;
        let control = create_control_directory(&self.control_root)?;
        let cancel_path = control.join("cancel");
        let target_pid_path = control.join("target-pid");
        let terminal_path = control.join("terminal");
        let envelope = LaunchEnvelope {
            version: 1,
            parent_pid,
            parent_start,
            executable: profile.executable.clone(),
            arguments: request.arguments.clone(),
            working_directory,
            environment: request.environment.clone(),
            stdin: request.stdin.clone(),
            deadline_millis: request.deadline_millis,
            max_processes: request.max_processes,
            max_open_files: request.max_open_files,
            cancel_path: cancel_path.clone(),
            target_pid_path: target_pid_path.clone(),
            terminal_path: terminal_path.clone(),
        };

        let mut command = Command::new(&self.guard.path);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command
            .args(&self.guard_arguments)
            .env_clear()
            .current_dir(&control)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        revalidate_identity(&self.guard)?;
        let started = Instant::now();
        let mut guard = command.spawn().map_err(|_| ProcessError::Spawn)?;
        let guard_pid = guard.id();
        let mut guard_stdin = guard.stdin.take().ok_or(ProcessError::Spawn)?;
        let encoded = serde_json::to_vec(&envelope).map_err(|_| ProcessError::Spawn)?;
        if encoded.len() > 2 * MAX_STDIN_BYTES {
            let _ = signal_group(guard_pid, Signal::SIGKILL);
            let _ = guard.wait();
            return Err(ProcessError::InvalidRequest);
        }
        guard_stdin
            .write_all(&encoded)
            .map_err(|_| ProcessError::Spawn)?;
        drop(guard_stdin);

        let stdout = guard.stdout.take().ok_or(ProcessError::Spawn)?;
        let stderr = guard.stderr.take().ok_or(ProcessError::Spawn)?;
        let overflow = Arc::new(AtomicBool::new(false));
        let stdout_overflow = Arc::clone(&overflow);
        let stderr_overflow = Arc::clone(&overflow);
        let output_bound = request.max_output_bytes;
        let stdout_reader = thread::spawn(move || capture(stdout, output_bound, &stdout_overflow));
        let stderr_reader = thread::spawn(move || capture(stderr, output_bound, &stderr_overflow));

        let mut requested_reason = None;
        let status = loop {
            if let Some(status) = guard.try_wait().map_err(|_| ProcessError::Spawn)? {
                break status;
            }
            if requested_reason.is_none() {
                if cancellation.is_cancelled() {
                    requested_reason = Some(ProcessOutcome::Cancelled);
                    write_cancel(&cancel_path, b"cancel")?;
                } else if overflow.load(Ordering::Acquire) {
                    requested_reason = Some(ProcessOutcome::OutputLimit);
                    write_cancel(&cancel_path, b"output")?;
                } else if started.elapsed() >= Duration::from_millis(request.deadline_millis) {
                    requested_reason = Some(ProcessOutcome::TimedOut);
                    write_cancel(&cancel_path, b"timeout")?;
                }
            }
            if requested_reason.is_some()
                && started.elapsed()
                    >= Duration::from_millis(request.deadline_millis)
                        .saturating_add(Duration::from_secs(2))
            {
                let _ = signal_group(guard_pid, Signal::SIGKILL);
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_reader.join().map_err(|_| ProcessError::Spawn)??;
        let stderr = stderr_reader.join().map_err(|_| ProcessError::Spawn)??;
        let exit_code = status.code().unwrap_or(-1);
        let terminal = fs::read_to_string(&terminal_path).ok();
        let (reported_exit_code, guard_outcome) =
            classify_terminal(terminal.as_deref(), exit_code, request);
        let outcome = requested_reason
            .or_else(|| {
                (stdout.truncated || stderr.truncated).then_some(ProcessOutcome::OutputLimit)
            })
            .unwrap_or(guard_outcome);
        let target_group_id = fs::read_to_string(&target_pid_path)
            .ok()
            .and_then(|value| value.parse::<u32>().ok());
        let descendant_cleanup = if matches!(
            outcome,
            ProcessOutcome::TimedOut | ProcessOutcome::Cancelled | ProcessOutcome::OutputLimit
        ) || (outcome == ProcessOutcome::RuntimeFailure
            && target_group_id.is_some())
        {
            if target_group_id.is_some_and(process_group_empty) {
                DescendantCleanup::Verified
            } else {
                DescendantCleanup::Uncertain
            }
        } else {
            DescendantCleanup::NotRequired
        };
        let _ = fs::remove_file(cancel_path);
        let _ = fs::remove_file(target_pid_path);
        let _ = fs::remove_file(terminal_path);
        let _ = fs::remove_dir(control);
        Ok(ProcessResult {
            outcome,
            exit_code: reported_exit_code,
            process_group_id: target_group_id.unwrap_or(0),
            stdout,
            stderr,
            elapsed: started.elapsed(),
            descendant_cleanup,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LaunchEnvelope {
    version: u16,
    parent_pid: u32,
    parent_start: u64,
    executable: ExecutableIdentity,
    arguments: Vec<String>,
    working_directory: PathBuf,
    environment: BTreeMap<String, String>,
    stdin: Vec<u8>,
    deadline_millis: u64,
    max_processes: u32,
    max_open_files: u64,
    cancel_path: PathBuf,
    target_pid_path: PathBuf,
    terminal_path: PathBuf,
}

/// Entry point used only by the sibling guard binary.
#[doc(hidden)]
#[must_use]
pub fn guard_entry() -> i32 {
    match run_guard() {
        Ok(code) | Err(code) => code,
    }
}

fn run_guard() -> Result<i32, i32> {
    let mut encoded = Vec::new();
    std::io::stdin()
        .take((2 * MAX_STDIN_BYTES + 1) as u64)
        .read_to_end(&mut encoded)
        .map_err(|_| GUARD_INTERNAL_EXIT)?;
    if encoded.len() > 2 * MAX_STDIN_BYTES {
        return Err(GUARD_INTERNAL_EXIT);
    }
    let envelope: LaunchEnvelope =
        serde_json::from_slice(&encoded).map_err(|_| GUARD_INTERNAL_EXIT)?;
    validate_envelope(&envelope).map_err(|_| GUARD_INTERNAL_EXIT)?;
    if process_start_time(envelope.parent_pid) != Some(envelope.parent_start) {
        return Err(GUARD_PARENT_EXIT);
    }
    setrlimit(
        Resource::RLIMIT_NOFILE,
        envelope.max_open_files as rlim_t,
        envelope.max_open_files as rlim_t,
    )
    .map_err(|_| GUARD_INTERNAL_EXIT)?;

    let mut command = Command::new(&envelope.executable.path);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .args(&envelope.arguments)
        .current_dir(&envelope.working_directory)
        .env_clear()
        .envs(&envelope.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    revalidate_identity(&envelope.executable).map_err(|_| GUARD_INTERNAL_EXIT)?;
    let mut target = command.spawn().map_err(|_| GUARD_INTERNAL_EXIT)?;
    let target_pid = target.id();
    fs::write(&envelope.target_pid_path, target_pid.to_string())
        .map_err(|_| GUARD_INTERNAL_EXIT)?;
    if let Some(mut stdin) = target.stdin.take() {
        stdin
            .write_all(&envelope.stdin)
            .map_err(|_| GUARD_INTERNAL_EXIT)?;
    }
    let started = Instant::now();
    loop {
        if let Some(status) = target.try_wait().map_err(|_| GUARD_INTERNAL_EXIT)? {
            let code = exit_code(status);
            write_terminal(&envelope.terminal_path, &format!("target:{code}"))?;
            return Ok(code);
        }
        if process_group_members(target_pid) > envelope.max_processes as usize {
            terminate_target_group(target_pid, &mut target);
            write_terminal(&envelope.terminal_path, "internal")?;
            return Ok(GUARD_INTERNAL_EXIT);
        }
        if envelope.cancel_path.exists() {
            let reason = fs::read(&envelope.cancel_path).unwrap_or_default();
            terminate_target_group(target_pid, &mut target);
            let (terminal, code) = if reason == b"timeout" {
                ("timeout", GUARD_TIMEOUT_EXIT)
            } else {
                ("cancel", GUARD_CANCEL_EXIT)
            };
            write_terminal(&envelope.terminal_path, terminal)?;
            return Ok(code);
        }
        if process_start_time(envelope.parent_pid) != Some(envelope.parent_start) {
            terminate_target_group(target_pid, &mut target);
            write_terminal(&envelope.terminal_path, "parent")?;
            return Ok(GUARD_PARENT_EXIT);
        }
        if started.elapsed() >= Duration::from_millis(envelope.deadline_millis) {
            terminate_target_group(target_pid, &mut target);
            write_terminal(&envelope.terminal_path, "timeout")?;
            return Ok(GUARD_TIMEOUT_EXIT);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn terminate_target_group(group: u32, target: &mut Child) {
    let _ = signal_group(group, Signal::SIGTERM);
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        if target.try_wait().ok().flatten().is_some() && process_group_empty(group) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = signal_group(group, Signal::SIGKILL);
    let _ = target.wait();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline && !process_group_empty(group) {
        thread::sleep(Duration::from_millis(5));
    }
}

fn validate_request(
    profile: &ProcessProfile,
    request: &ProcessRequest,
) -> Result<(), ProcessError> {
    if !profile
        .allowed_argument_vectors
        .contains(&request.arguments)
    {
        return Err(ProcessError::ArgumentsDenied);
    }
    validate_arguments(&request.arguments)?;
    if request.environment.len() > MAX_ENVIRONMENT
        || request.environment.iter().any(|(name, value)| {
            !profile.allowed_environment_names.contains(name)
                || !valid_environment_name(name)
                || value.contains('\0')
        })
    {
        return Err(ProcessError::EnvironmentDenied);
    }
    if request.stdin.len() > MAX_STDIN_BYTES
        || request.max_output_bytes == 0
        || request.max_output_bytes > MAX_OUTPUT_BOUND
        || request.deadline_millis == 0
        || request.deadline_millis > MAX_DEADLINE_MILLIS
        || request.max_processes == 0
        || request.max_processes > MAX_PROCESSES
        || request.max_open_files < 16
        || request.max_open_files > MAX_OPEN_FILES
        || request.expected_exit_codes.is_empty()
        || request.expected_exit_codes.len() > 16
    {
        return Err(ProcessError::InvalidRequest);
    }
    let _ = checked_directory(&request.working_directory)?;
    Ok(())
}

fn validate_envelope(envelope: &LaunchEnvelope) -> Result<(), ProcessError> {
    if envelope.version != 1
        || envelope.stdin.len() > MAX_STDIN_BYTES
        || envelope.deadline_millis == 0
        || envelope.deadline_millis > MAX_DEADLINE_MILLIS
        || envelope.max_processes == 0
        || envelope.max_processes > MAX_PROCESSES
        || envelope.max_open_files < 16
        || envelope.max_open_files > MAX_OPEN_FILES
        || !envelope.cancel_path.is_absolute()
        || !envelope.target_pid_path.is_absolute()
        || !envelope.terminal_path.is_absolute()
    {
        return Err(ProcessError::InvalidRequest);
    }
    validate_arguments(&envelope.arguments)?;
    for (name, value) in &envelope.environment {
        if !valid_environment_name(name) || value.contains('\0') {
            return Err(ProcessError::InvalidRequest);
        }
    }
    revalidate_identity(&envelope.executable)?;
    let directory = checked_directory(&envelope.working_directory)?;
    if directory != envelope.working_directory {
        return Err(ProcessError::Identity);
    }
    Ok(())
}

fn validate_arguments(arguments: &[String]) -> Result<(), ProcessError> {
    if arguments.len() > MAX_ARGUMENTS
        || arguments.iter().map(String::len).sum::<usize>() > MAX_ARGUMENT_BYTES
        || arguments
            .iter()
            .any(|argument| argument.contains('\0') || argument.starts_with('@'))
    {
        return Err(ProcessError::InvalidRequest);
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn executable_identity(path: &Path) -> Result<ExecutableIdentity, ProcessError> {
    if !path.is_absolute() {
        return Err(ProcessError::Identity);
    }
    let link = fs::symlink_metadata(path).map_err(|_| ProcessError::Identity)?;
    if link.file_type().is_symlink() || !link.is_file() {
        return Err(ProcessError::Identity);
    }
    let canonical = fs::canonicalize(path).map_err(|_| ProcessError::Identity)?;
    let metadata = fs::metadata(&canonical).map_err(|_| ProcessError::Identity)?;
    let bytes = fs::read(&canonical).map_err(|_| ProcessError::Identity)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(ExecutableIdentity {
            path: canonical,
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            sha256: hex_bytes(Sha256::digest(bytes).as_ref()),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (metadata, bytes);
        Err(ProcessError::Identity)
    }
}

fn revalidate_identity(expected: &ExecutableIdentity) -> Result<(), ProcessError> {
    if &executable_identity(&expected.path)? == expected {
        Ok(())
    } else {
        Err(ProcessError::Identity)
    }
}

fn checked_directory(path: &Path) -> Result<PathBuf, ProcessError> {
    if !path.is_absolute() {
        return Err(ProcessError::Identity);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| ProcessError::Identity)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ProcessError::Identity);
    }
    fs::canonicalize(path).map_err(|_| ProcessError::Identity)
}

fn prepare_private_directory(path: &Path) -> Result<(), ProcessError> {
    fs::create_dir_all(path).map_err(|_| ProcessError::Control)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| ProcessError::Control)?;
    }
    Ok(())
}

fn create_control_directory(root: &Path) -> Result<PathBuf, ProcessError> {
    static NEXT_CONTROL: AtomicU64 = AtomicU64::new(1);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProcessError::Control)?
        .as_nanos();
    let seed = format!(
        "{}:{now}:{}",
        std::process::id(),
        NEXT_CONTROL.fetch_add(1, Ordering::Relaxed)
    );
    let name = format!("run-{}", &hex_bytes(Sha256::digest(seed).as_ref())[..32]);
    let path = root.join(name);
    fs::create_dir(&path).map_err(|_| ProcessError::Control)?;
    prepare_private_directory(&path)?;
    Ok(path)
}

fn write_cancel(path: &Path, reason: &[u8]) -> Result<(), ProcessError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ProcessError::Control)?;
    file.write_all(reason)
        .and_then(|()| file.sync_all())
        .map_err(|_| ProcessError::Control)
}

fn write_terminal(path: &Path, terminal: &str) -> Result<(), i32> {
    fs::write(path, terminal).map_err(|_| GUARD_INTERNAL_EXIT)
}

fn capture(
    mut reader: impl Read,
    bound: u64,
    overflow: &AtomicBool,
) -> Result<CapturedStream, ProcessError> {
    let mut retained = Vec::new();
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).map_err(|_| ProcessError::Spawn)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        total = total.saturating_add(count as u64);
        let remaining = usize::try_from(bound)
            .unwrap_or(usize::MAX)
            .saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..count.min(remaining)]);
        if total > bound {
            overflow.store(true, Ordering::Release);
        }
    }
    Ok(CapturedStream {
        retained,
        total_bytes: total,
        sha256: hex_bytes(digest.finalize().as_ref()),
        truncated: total > bound,
    })
}

fn classify_terminal(
    terminal: Option<&str>,
    guard_exit_code: i32,
    request: &ProcessRequest,
) -> (i32, ProcessOutcome) {
    if let Some(code) = terminal.and_then(|value| value.strip_prefix("target:"))
        && let Ok(code) = code.parse::<i32>()
    {
        return (
            code,
            if request.expected_exit_codes.contains(&code) {
                ProcessOutcome::Succeeded
            } else {
                ProcessOutcome::Failed
            },
        );
    }
    let outcome = match terminal {
        Some("timeout") => ProcessOutcome::TimedOut,
        Some("cancel") => ProcessOutcome::Cancelled,
        Some("parent") => ProcessOutcome::ParentLost,
        _ => ProcessOutcome::RuntimeFailure,
    };
    (guard_exit_code, outcome)
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(128)
}

fn signal_group(group: u32, signal: Signal) -> Result<(), ProcessError> {
    let group = i32::try_from(group).map_err(|_| ProcessError::Control)?;
    killpg(Pid::from_raw(group), signal).map_err(|_| ProcessError::Control)
}

fn process_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    stat.get(end + 2..)?
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

fn process_group_members(group: u32) -> usize {
    let Ok(entries) = fs::read_dir("/proc") else {
        return usize::MAX;
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
        .filter(|pid| process_group(*pid) == Some(group))
        .count()
}

fn process_group(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    stat.get(end + 2..)?.split_whitespace().nth(2)?.parse().ok()
}

fn process_group_empty(group: u32) -> bool {
    process_group_members(group) == 0
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_names_are_canonical() {
        assert!(valid_environment_name("SAFE_NAME_1"));
        assert!(!valid_environment_name("1UNSAFE"));
        assert!(!valid_environment_name("BAD-NAME"));
        assert!(!valid_environment_name(""));
    }

    #[test]
    fn response_files_and_unbounded_requests_fail() {
        assert_eq!(
            validate_arguments(&["@response".to_owned()]),
            Err(ProcessError::InvalidRequest)
        );
        assert_eq!(
            validate_arguments(&vec!["x".to_owned(); MAX_ARGUMENTS + 1]),
            Err(ProcessError::InvalidRequest)
        );
    }

    #[test]
    fn parent_start_time_is_stable_for_current_process() {
        let first = process_start_time(std::process::id()).unwrap();
        let second = process_start_time(std::process::id()).unwrap();
        assert_eq!(first, second);
    }
}
