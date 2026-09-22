//! One bounded worker thread that runs backend requests off the interface thread.
//!
//! Every request is bound to the project and campaign it was issued for plus a generation
//! counter. The interface discards any response whose binding or generation no longer matches
//! the current selection, so a slow response for a previously opened repository can never be
//! rendered as the current one.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SendError, Sender, SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::Duration,
};

use super::cli::{BackendError, CoordinatorBinary};

/// Maximum queued requests before new requests are refused.
pub const QUEUE_CAPACITY: usize = 8;

/// Exact identities a request was issued for.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Binding {
    /// Absolute configuration file.
    pub config_path: Option<PathBuf>,
    /// Repository identity known from `doctor`, when already observed.
    pub repository_id: Option<String>,
    /// Campaign identity, when a campaign is selected.
    pub campaign_id: Option<String>,
}

/// Monotonic selection generation; bumped whenever the selection changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Generation(pub u64);

/// Work the worker can execute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Job {
    /// Run the coordinator with the exact argument vector and deadline.
    Command {
        /// Stable label for the interface, such as `doctor`.
        label: &'static str,
        /// Exact argument vector.
        arguments: Vec<String>,
        /// Deadline after which the command is killed.
        deadline: Duration,
    },
    /// Run one read-only Git object read against a repository.
    GitRead {
        /// Stable label for the interface.
        label: &'static str,
        /// Repository to read.
        repository: PathBuf,
        /// Exact argument vector after `--no-pager -C <repository>`.
        arguments: Vec<String>,
        /// Deadline after which the command is killed.
        deadline: Duration,
    },
}

impl Job {
    /// Stable label of the job.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Command { label, .. } | Self::GitRead { label, .. } => label,
        }
    }
}

/// One queued request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    /// Generation at issue time.
    pub generation: Generation,
    /// Binding at issue time.
    pub binding: Binding,
    /// Work to perform.
    pub job: Job,
    /// Caller correlation identity (used for control requests).
    pub request_id: Option<String>,
}

/// One completed request.
#[derive(Debug)]
pub struct Response {
    /// Generation at issue time.
    pub generation: Generation,
    /// Binding at issue time.
    pub binding: Binding,
    /// Label of the job.
    pub label: &'static str,
    /// Caller correlation identity when supplied.
    pub request_id: Option<String>,
    /// Raw stdout or the failure.
    pub result: Result<Vec<u8>, BackendError>,
}

/// Why a request could not be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    /// The bounded queue is full.
    Full,
    /// The worker thread has stopped.
    Stopped,
}

/// Handle used by the interface thread.
#[derive(Debug)]
pub struct Worker {
    sender: SyncSender<Request>,
    receiver: Receiver<Response>,
    current: Arc<AtomicU64>,
    cancel_flag: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Starts the worker for one coordinator executable.
    ///
    /// `wake` is invoked after each response so the interface can request a repaint.
    #[must_use]
    pub fn start(binary: CoordinatorBinary, wake: impl Fn() + Send + 'static) -> Self {
        let (sender, requests) = sync_channel::<Request>(QUEUE_CAPACITY);
        let (responses, receiver) = std::sync::mpsc::channel::<Response>();
        let current = Arc::new(AtomicU64::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_current = Arc::clone(&current);
        let worker_cancel = Arc::clone(&cancel_flag);
        let handle = thread::Builder::new()
            .name("codingmage-ui-backend".to_owned())
            .spawn(move || {
                run_loop(
                    &binary,
                    &requests,
                    &responses,
                    &worker_current,
                    &worker_cancel,
                    &wake,
                );
            })
            .ok();
        Self {
            sender,
            receiver,
            current,
            cancel_flag,
            handle,
        }
    }

    /// Queues one request.
    ///
    /// # Errors
    ///
    /// Returns [`QueueError`] when the bounded queue is full or the worker stopped.
    pub fn submit(&self, request: Request) -> Result<(), QueueError> {
        match self.sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(QueueError::Full),
            Err(TrySendError::Disconnected(_)) => Err(QueueError::Stopped),
        }
    }

    /// Advances the current generation, cancelling any in-flight or queued older request.
    pub fn advance(&self, generation: Generation) {
        self.current.store(generation.0, Ordering::Release);
        self.cancel_flag.store(true, Ordering::Release);
    }

    /// Drains completed responses without blocking.
    #[must_use]
    pub fn drain(&self) -> Vec<Response> {
        let mut responses = Vec::new();
        while let Ok(response) = self.receiver.try_recv() {
            responses.push(response);
        }
        responses
    }

    /// Waits up to `timeout` for the next response; used by tests and shutdown paths.
    #[must_use]
    pub fn wait(&self, timeout: Duration) -> Option<Response> {
        self.receiver.recv_timeout(timeout).ok()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel_flag.store(true, Ordering::Release);
        self.current.store(u64::MAX, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            // Dropping the sender ends the loop; joining keeps no subprocess orphaned.
            drop(std::mem::replace(&mut self.sender, sync_channel(1).0));
            let _ = handle.join();
        }
    }
}

fn run_loop(
    binary: &CoordinatorBinary,
    requests: &Receiver<Request>,
    responses: &Sender<Response>,
    current: &Arc<AtomicU64>,
    cancel_flag: &Arc<AtomicBool>,
    wake: &(impl Fn() + Send),
) {
    while let Ok(request) = requests.recv() {
        let label = request.job.label();
        let result = if request.generation.0 < current.load(Ordering::Acquire) {
            Err(BackendError::Cancelled)
        } else {
            cancel_flag.store(false, Ordering::Release);
            let cancel = Arc::clone(cancel_flag);
            let watcher_current = Arc::clone(current);
            let issued = request.generation.0;
            let stop = Arc::new(AtomicBool::new(false));
            let watcher_stop = Arc::clone(&stop);
            let watcher_cancel = Arc::clone(&cancel);
            let watcher = thread::spawn(move || {
                while !watcher_stop.load(Ordering::Acquire) {
                    if watcher_current.load(Ordering::Acquire) > issued {
                        watcher_cancel.store(true, Ordering::Release);
                        break;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            });
            let result = match &request.job {
                Job::Command {
                    arguments,
                    deadline,
                    ..
                } => binary.run(arguments, *deadline, &cancel),
                Job::GitRead {
                    repository,
                    arguments,
                    deadline,
                    ..
                } => super::cli::run_git(repository, arguments, *deadline, &cancel),
            };
            stop.store(true, Ordering::Release);
            let _ = watcher.join();
            result
        };
        let response = Response {
            generation: request.generation,
            binding: request.binding,
            label,
            request_id: request.request_id,
            result,
        };
        if let Err(SendError(_)) = responses.send(response) {
            return;
        }
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_binary(root: &std::path::Path) -> CoordinatorBinary {
        use std::os::unix::fs::PermissionsExt as _;
        fs::create_dir_all(root).unwrap();
        let script = root.join("codingmage");
        fs::write(
            &script,
            "#!/bin/sh\ncase \"$1\" in\n  slow) sleep 3; echo slow;;\n  *) echo \"$1\";;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        CoordinatorBinary::at(&script).unwrap()
    }

    fn request(generation: u64, label: &'static str, argument: &str) -> Request {
        Request {
            generation: Generation(generation),
            binding: Binding {
                config_path: Some(PathBuf::from("/tmp/config.toml")),
                repository_id: None,
                campaign_id: None,
            },
            job: Job::Command {
                label,
                arguments: vec![argument.to_owned()],
                deadline: Duration::from_secs(10),
            },
            request_id: None,
        }
    }

    #[test]
    fn responses_carry_their_issue_binding_and_stale_generations_are_cancelled() {
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-worker-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let worker = Worker::start(fake_binary(&root), || {});
        worker.submit(request(1, "slow", "slow")).unwrap();
        worker.submit(request(1, "second", "later")).unwrap();
        thread::sleep(Duration::from_millis(200));
        worker.advance(Generation(2));
        worker.submit(request(2, "fresh", "fresh")).unwrap();
        let first = worker.wait(Duration::from_secs(10)).unwrap();
        assert_eq!(first.label, "slow");
        assert_eq!(first.result, Err(BackendError::Cancelled));
        let second = worker.wait(Duration::from_secs(10)).unwrap();
        assert_eq!(second.label, "second");
        assert_eq!(second.result, Err(BackendError::Cancelled));
        let third = worker.wait(Duration::from_secs(10)).unwrap();
        assert_eq!(third.label, "fresh");
        assert_eq!(third.generation, Generation(2));
        assert_eq!(third.result.unwrap(), b"fresh\n");
        drop(worker);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn queue_is_bounded() {
        let root = std::env::temp_dir().join(format!("codingmage-ui-queue-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let worker = Worker::start(fake_binary(&root), || {});
        let mut refused = false;
        for index in 0..(QUEUE_CAPACITY + 4) {
            let outcome = worker.submit(request(1, "slow", if index == 0 { "slow" } else { "x" }));
            if outcome == Err(QueueError::Full) {
                refused = true;
                break;
            }
        }
        assert!(refused);
        worker.advance(Generation(9));
        drop(worker);
        fs::remove_dir_all(root).unwrap();
    }
}
