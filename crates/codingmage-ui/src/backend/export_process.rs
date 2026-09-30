//! Bounded, isolated writer for a report requested by the desktop owner.
//!
//! Filesystem operations can enter an uninterruptible kernel wait. The coordinator worker
//! therefore never executes or joins this writer. A one-shot child owns the filesystem call;
//! the supervisor can abandon a timed-out child after signaling it, without blocking controls
//! or native window shutdown.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::cli::BackendError;
use crate::report_export::{ExportRequest, ExportResult, MAX_EXPORT_INPUT};

const MAX_RESULT_BYTES: u64 = 16 * 1024;

struct BoundedInput {
    bytes: Vec<u8>,
    exceeded: bool,
    limit: usize,
}

impl BoundedInput {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            exceeded: false,
            limit,
        }
    }
}

impl std::io::Write for BoundedInput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(buffer.len())
            .is_none_or(|length| length > self.limit)
        {
            self.exceeded = true;
            return Err(std::io::Error::other("export request exceeds limit"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The installed desktop executable also hosts the private export process.
pub(super) fn helper_path() -> Result<PathBuf, BackendError> {
    let current = std::env::current_exe().map_err(|_| BackendError::Spawn)?;
    let parent = current.parent().ok_or(BackendError::Spawn)?;
    let sibling = if parent.file_name() == Some(std::ffi::OsStr::new("deps")) {
        parent
            .parent()
            .ok_or(BackendError::Spawn)?
            .join("codingmage-ui")
    } else {
        current
    };
    validate_helper(&sibling)?;
    Ok(sibling)
}

fn validate_helper(path: &Path) -> Result<(), BackendError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BackendError::Spawn)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackendError::Spawn);
    }
    Ok(())
}

/// Executes one immutable export with a deadline. `busy` is held until the child is reaped.
pub(super) fn run(
    helper: &Path,
    request: &ExportRequest,
    deadline: Duration,
    generation: u64,
    current: &AtomicU64,
    shutdown: &AtomicBool,
    busy: Arc<AtomicBool>,
) -> Result<Vec<u8>, BackendError> {
    if generation < current.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
        busy.store(false, Ordering::Release);
        return Err(BackendError::Cancelled);
    }
    let mut input = BoundedInput::new(MAX_EXPORT_INPUT);
    if serde_json::to_writer(&mut input, request).is_err() {
        busy.store(false, Ordering::Release);
        return if input.exceeded {
            Err(BackendError::Refused(
                "the report request exceeds the export limit".to_owned(),
            ))
        } else {
            Err(BackendError::Spawn)
        };
    }
    let input = input.bytes;
    if let Err(error) = validate_helper(helper) {
        busy.store(false, Ordering::Release);
        return Err(error);
    }
    let Ok(mut child) = Command::new(helper)
        .arg("--report-export-helper")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        busy.store(false, Ordering::Release);
        return Err(BackendError::Spawn);
    };
    let Some(mut stdin) = child.stdin.take() else {
        terminate(child, busy);
        return Err(BackendError::Spawn);
    };
    let feed = thread::Builder::new()
        .name("codingmage-ui-export-feed".to_owned())
        .spawn(move || stdin.write_all(&input));
    let Ok(feed) = feed else {
        terminate(child, busy);
        return Err(BackendError::Spawn);
    };
    let started = Instant::now();
    loop {
        let Ok(status) = child.try_wait() else {
            terminate(child, busy);
            return Err(BackendError::Spawn);
        };
        if let Some(status) = status {
            busy.store(false, Ordering::Release);
            let feed_result = feed.join().map_err(|_| BackendError::Spawn)?;
            if feed_result.is_err() {
                return Err(BackendError::Spawn);
            }
            let mut output = Vec::new();
            child
                .stdout
                .take()
                .ok_or(BackendError::Spawn)?
                .take(MAX_RESULT_BYTES + 1)
                .read_to_end(&mut output)
                .map_err(|_| BackendError::Spawn)?;
            if output.len() as u64 > MAX_RESULT_BYTES {
                return Err(BackendError::OutputTooLarge);
            }
            let response: ExportResult =
                serde_json::from_slice(&output).map_err(|_| BackendError::Spawn)?;
            return match (status.success(), response.error) {
                (true, None) => Ok(Vec::new()),
                (false, Some(error)) => Err(BackendError::Refused(error)),
                _ => Err(BackendError::Spawn),
            };
        }
        let reason =
            if shutdown.load(Ordering::Acquire) || current.load(Ordering::Acquire) > generation {
                Some(BackendError::Cancelled)
            } else if started.elapsed() >= deadline {
                Some(BackendError::Timeout)
            } else {
                None
            };
        if let Some(reason) = reason {
            // The feed thread owns only the private pipe. If the kernel stalls this child,
            // neither its reaper nor the feed thread may hold up coordinator controls.
            drop(feed);
            terminate(child, busy);
            return Err(reason);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn terminate(mut child: Child, busy: Arc<AtomicBool>) {
    let _ = child.kill();
    // A filesystem syscall may defer SIGKILL. Reap later without joining on UI shutdown.
    if thread::Builder::new()
        .name("codingmage-ui-export-reaper".to_owned())
        .spawn(move || {
            let _ = child.wait();
            busy.store(false, Ordering::Release);
        })
        .is_err()
    {
        // The process is already signaled. Keep the refusal conservative if reaping could
        // not be scheduled; this must not allow a second writer to start.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_request_encoding_refuses_bytes_beyond_its_limit() {
        let mut input = BoundedInput::new(4);
        input.write_all(b"abcd").unwrap();
        assert!(input.write_all(b"e").is_err());
        assert!(input.exceeded);
        assert_eq!(input.bytes, b"abcd");
    }
}
