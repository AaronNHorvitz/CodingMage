//! Private literal Git command templates and bounded output capture.

use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

const GIT_EXECUTABLE: &str = "/usr/bin/git";
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
pub(crate) enum GitCommand<'a> {
    Status,
    References,
    Worktrees,
    Head,
    TrackedPaths,
    VerifyCommit(&'a str),
    IsAncestor {
        ancestor: &'a str,
        child: &'a str,
    },
    AddWorktree {
        destination: &'a Path,
        branch: &'a str,
        source: &'a str,
    },
    RemoveWorktree {
        destination: &'a Path,
    },
}

pub(crate) struct GitOutput {
    pub stdout: Vec<u8>,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub exit_code: i32,
    pub elapsed: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandError {
    ExecutableUnavailable,
    ExecutableChanged,
    Spawn,
    Timeout,
    OutputLimit,
    Failed,
    InvalidOutput,
}

pub(crate) fn run_git(
    working_directory: &Path,
    request: GitCommand<'_>,
) -> Result<GitOutput, CommandError> {
    run_git_with_codes(working_directory, request, &[0])
}

pub(crate) fn run_git_with_codes(
    working_directory: &Path,
    request: GitCommand<'_>,
    allowed_exit_codes: &[i32],
) -> Result<GitOutput, CommandError> {
    let executable = PathBuf::from(GIT_EXECUTABLE);
    let before = executable_identity(&executable)?;
    let arguments = arguments(request);
    let mut command = Command::new(&executable);
    command
        .current_dir(working_directory)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/usr/bin/false")
        .env("SSH_ASKPASS", "/usr/bin/false")
        .env("GIT_EDITOR", "/usr/bin/false")
        .env("GIT_SEQUENCE_EDITOR", "/usr/bin/false")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .args([
            OsStr::new("--no-pager"),
            OsStr::new("--no-optional-locks"),
            OsStr::new("-c"),
            OsStr::new("core.hooksPath=/dev/null"),
            OsStr::new("-c"),
            OsStr::new("core.fsmonitor=false"),
            OsStr::new("-c"),
            OsStr::new("credential.helper="),
            OsStr::new("-c"),
            OsStr::new("core.askPass=/usr/bin/false"),
            OsStr::new("-c"),
            OsStr::new("diff.external="),
            OsStr::new("-c"),
            OsStr::new("core.pager=cat"),
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if executable_identity(&executable)? != before {
        return Err(CommandError::ExecutableChanged);
    }
    let started = Instant::now();
    let mut child = command.spawn().map_err(|_| CommandError::Spawn)?;
    let stdout = child.stdout.take().ok_or(CommandError::Spawn)?;
    let stderr = child.stderr.take().ok_or(CommandError::Spawn)?;
    let stdout_reader = thread::spawn(move || capture(stdout));
    let stderr_reader = thread::spawn(move || capture(stderr));
    let status = wait_bounded(&mut child, COMMAND_TIMEOUT)?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| CommandError::InvalidOutput)??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CommandError::InvalidOutput)??;
    if stdout.truncated || stderr.truncated {
        return Err(CommandError::OutputLimit);
    }
    let exit_code = status.code().unwrap_or(-1);
    if !allowed_exit_codes.contains(&exit_code) {
        return Err(CommandError::Failed);
    }
    Ok(GitOutput {
        stdout: stdout.bytes,
        stdout_sha256: stdout.sha256,
        stderr_sha256: stderr.sha256,
        exit_code,
        elapsed: started.elapsed(),
    })
}

fn arguments(request: GitCommand<'_>) -> Vec<OsString> {
    match request {
        GitCommand::Status => [
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        GitCommand::References => [
            "for-each-ref",
            "--format=%(objectname)%00%(refname)",
            "refs/heads",
            "refs/tags",
            "refs/notes",
            "refs/stash",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        GitCommand::Worktrees => ["worktree", "list", "--porcelain", "-z"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        GitCommand::Head => ["rev-parse", "--verify", "HEAD"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        GitCommand::TrackedPaths => ["ls-files", "-z"].into_iter().map(OsString::from).collect(),
        GitCommand::VerifyCommit(object) => {
            vec![
                "cat-file".into(),
                "-e".into(),
                format!("{object}^{{commit}}").into(),
            ]
        }
        GitCommand::IsAncestor { ancestor, child } => vec![
            "merge-base".into(),
            "--is-ancestor".into(),
            ancestor.into(),
            child.into(),
        ],
        GitCommand::AddWorktree {
            destination,
            branch,
            source,
        } => vec![
            "worktree".into(),
            "add".into(),
            "-b".into(),
            branch.into(),
            destination.as_os_str().to_owned(),
            source.into(),
        ],
        GitCommand::RemoveWorktree { destination } => vec![
            "worktree".into(),
            "remove".into(),
            destination.as_os_str().to_owned(),
        ],
    }
}

fn wait_bounded(
    child: &mut Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, CommandError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|_| CommandError::Failed)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CommandError::Timeout);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct Captured {
    bytes: Vec<u8>,
    sha256: String,
    truncated: bool,
}

fn capture(mut reader: impl Read) -> Result<Captured, CommandError> {
    let mut retained = Vec::new();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    let mut total = 0_usize;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| CommandError::InvalidOutput)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..count.min(remaining)]);
        total = total.saturating_add(count);
    }
    Ok(Captured {
        bytes: retained,
        sha256: hex_bytes(digest.finalize().as_ref()),
        truncated: total > MAX_OUTPUT_BYTES,
    })
}

fn executable_identity(path: &Path) -> Result<(u64, u64), CommandError> {
    let metadata = fs::metadata(path).map_err(|_| CommandError::ExecutableUnavailable)?;
    if !metadata.is_file() {
        return Err(CommandError::ExecutableUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(CommandError::ExecutableUnavailable)
    }
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

pub(crate) fn text(output: &GitOutput) -> Result<&str, CommandError> {
    std::str::from_utf8(&output.stdout).map_err(|_| CommandError::InvalidOutput)
}

pub(crate) fn path_from_bytes(value: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(OsStr::from_bytes(value))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(value).into_owned())
    }
}

impl From<io::Error> for CommandError {
    fn from(_: io::Error) -> Self {
        Self::InvalidOutput
    }
}
