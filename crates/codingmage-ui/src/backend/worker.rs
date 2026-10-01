//! One bounded worker thread that runs backend requests off the interface thread.
//!
//! Every request is bound to the project and campaign it was issued for plus a generation
//! counter. The interface discards any response whose binding or generation no longer matches
//! the current selection, so a slow response for a previously opened repository can never be
//! rendered as the current one.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SendError, Sender, SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::Duration,
};

use super::{
    cli::{BackendError, CoordinatorBinary},
    export_process,
};
use crate::{report::OutcomeReport, report_export::ExportRequest};

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
    /// Validate a fresh support-bundle destination off the UI thread, then run the coordinator.
    SupportBundle {
        /// Exact command arguments displayed by the UI.
        arguments: Vec<String>,
        /// New output directory requested by the owner.
        destination: PathBuf,
        /// Repository that must remain untouched by the bundle.
        repository: PathBuf,
        /// Deadline for the coordinator command.
        deadline: Duration,
    },
    /// Export an already assembled observation off the render thread.
    ReportExport {
        /// Snapshot captured when the owner requested export.
        report: Box<OutcomeReport>,
        /// Exact destination selected by the owner.
        destination: PathBuf,
        /// Repository that the destination must remain outside.
        repository: PathBuf,
        /// Whether replacement was explicitly requested.
        overwrite: bool,
        /// Maximum time allowed before the isolated writer is terminated.
        deadline: Duration,
    },
    /// Export a fresh source-bound report through the public coordinator command.
    SourceReportExport {
        /// Exact argument vector shown beside the native action.
        arguments: Vec<String>,
        /// Maximum time allowed for the isolated command.
        deadline: Duration,
    },
    /// Inspect a fresh source-bound report through the public coordinator command.
    SourceReportInspect {
        /// Exact read-only command arguments displayed by the UI.
        arguments: Vec<String>,
        /// Maximum time allowed for the isolated command.
        deadline: Duration,
    },
}

impl Job {
    /// Stable label of the job.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Command { label, .. } => label,
            Self::SupportBundle { .. } => "support-bundle",
            Self::ReportExport { .. } | Self::SourceReportExport { .. } => "report-export",
            Self::SourceReportInspect { .. } => "campaign-outcome-report",
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
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

struct WorkerContext {
    current: Arc<AtomicU64>,
    cancel_flag: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    export_busy: Arc<AtomicBool>,
    inspect_busy: Arc<AtomicBool>,
    helper: Result<PathBuf, BackendError>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Worker {
    /// Starts the worker for one coordinator executable.
    ///
    /// `wake` is invoked after each response so the interface can request a repaint.
    #[must_use]
    pub fn start(binary: CoordinatorBinary, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::start_with_helper(binary, export_process::helper_path(), wake)
    }

    fn start_with_helper(
        binary: CoordinatorBinary,
        helper: Result<PathBuf, BackendError>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let (sender, requests) = sync_channel::<Request>(QUEUE_CAPACITY);
        let (responses, receiver) = std::sync::mpsc::channel::<Response>();
        let current = Arc::new(AtomicU64::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let export_busy = Arc::new(AtomicBool::new(false));
        let inspect_busy = Arc::new(AtomicBool::new(false));
        let context = WorkerContext {
            current: Arc::clone(&current),
            cancel_flag: Arc::clone(&cancel_flag),
            shutdown: Arc::clone(&shutdown),
            export_busy,
            inspect_busy,
            helper,
            wake: Arc::new(wake),
        };
        let handle = thread::Builder::new()
            .name("codingmage-ui-backend".to_owned())
            .spawn(move || run_loop(&binary, &requests, &responses, &context))
            .ok();
        Self {
            sender,
            receiver,
            current,
            cancel_flag,
            shutdown,
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
        self.shutdown.store(true, Ordering::Release);
        self.current.store(u64::MAX, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            // The coordinator thread stops; isolated export children are signaled and
            // reaped separately, so a wedged filesystem cannot hold this join.
            drop(std::mem::replace(&mut self.sender, sync_channel(1).0));
            let _ = handle.join();
        }
    }
}

fn run_loop(
    binary: &CoordinatorBinary,
    requests: &Receiver<Request>,
    responses: &Sender<Response>,
    context: &WorkerContext,
) {
    while let Ok(request) = requests.recv() {
        if matches!(request.job, Job::SourceReportInspect { .. }) {
            dispatch_source_inspection(binary, request, responses, context);
            continue;
        }
        if matches!(
            request.job,
            Job::ReportExport { .. } | Job::SourceReportExport { .. }
        ) {
            dispatch_export(binary, request, responses, context);
            continue;
        }
        let label = request.job.label();
        let result = if request.generation.0 < context.current.load(Ordering::Acquire) {
            Err(BackendError::Cancelled)
        } else {
            context.cancel_flag.store(false, Ordering::Release);
            let cancel = Arc::clone(&context.cancel_flag);
            let watcher_current = Arc::clone(&context.current);
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
                Job::SupportBundle {
                    arguments,
                    destination,
                    repository,
                    deadline,
                } => validate_support_destination(destination, repository)
                    .and_then(|()| binary.run(arguments, *deadline, &cancel)),
                Job::ReportExport { .. }
                | Job::SourceReportExport { .. }
                | Job::SourceReportInspect { .. } => {
                    unreachable!("report commands are dispatched separately")
                }
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
        (context.wake)();
    }
}

fn dispatch_export(
    binary: &CoordinatorBinary,
    request: Request,
    responses: &Sender<Response>,
    context: &WorkerContext,
) {
    let failed = if request.generation.0 < context.current.load(Ordering::Acquire) {
        Some(BackendError::Cancelled)
    } else if context
        .export_busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        Some(BackendError::Refused(
            "a previous report writer has not exited; try again after it stops".to_owned(),
        ))
    } else {
        None
    };
    if let Some(error) = failed {
        let _ = responses.send(Response {
            generation: request.generation,
            binding: request.binding,
            label: "report-export",
            request_id: request.request_id,
            result: Err(error),
        });
        (context.wake)();
        return;
    }
    let response_fallback = Response {
        generation: request.generation,
        binding: request.binding.clone(),
        label: "report-export",
        request_id: request.request_id.clone(),
        result: Err(BackendError::Spawn),
    };
    let completion_sender = responses.clone();
    let current = Arc::clone(&context.current);
    let shutdown = Arc::clone(&context.shutdown);
    let busy_for_thread = Arc::clone(&context.export_busy);
    let wake_for_thread = Arc::clone(&context.wake);
    let helper = context.helper.clone();
    let binary = binary.clone();
    let spawn = thread::Builder::new()
        .name("codingmage-ui-export-supervisor".to_owned())
        .spawn(move || {
            let result = match request.job {
                Job::ReportExport {
                    report,
                    destination,
                    repository,
                    overwrite,
                    deadline,
                } => match helper {
                    Ok(helper) => export_process::run(
                        &helper,
                        &ExportRequest {
                            report: *report,
                            destination,
                            repository,
                            overwrite,
                        },
                        deadline,
                        request.generation.0,
                        &current,
                        &shutdown,
                        Arc::clone(&busy_for_thread),
                    ),
                    Err(error) => Err(error),
                },
                Job::SourceReportExport {
                    arguments,
                    deadline,
                } => run_source_report_command(
                    &binary,
                    &arguments,
                    deadline,
                    request.generation.0,
                    Arc::clone(&current),
                    Arc::clone(&shutdown),
                ),
                Job::Command { .. }
                | Job::SupportBundle { .. }
                | Job::SourceReportInspect { .. } => {
                    unreachable!("only report exports enter the export supervisor")
                }
            };
            busy_for_thread.store(false, Ordering::Release);
            let _ = completion_sender.send(Response {
                generation: request.generation,
                binding: request.binding,
                label: "report-export",
                request_id: request.request_id,
                result,
            });
            wake_for_thread();
        });
    if spawn.is_err() {
        context.export_busy.store(false, Ordering::Release);
        // A failed thread spawn cannot have started a writer.
        // The response receiver may already have gone away during shutdown.
        let _ = responses.send(response_fallback);
        (context.wake)();
    }
}

fn dispatch_source_inspection(
    binary: &CoordinatorBinary,
    request: Request,
    responses: &Sender<Response>,
    context: &WorkerContext,
) {
    let label = "campaign-outcome-report";
    let failed = if request.generation.0 < context.current.load(Ordering::Acquire) {
        Some(BackendError::Cancelled)
    } else if context
        .inspect_busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        Some(BackendError::Refused(
            "a previous source inspection has not exited; try again after it stops".to_owned(),
        ))
    } else {
        None
    };
    if let Some(error) = failed {
        let _ = responses.send(Response {
            generation: request.generation,
            binding: request.binding,
            label,
            request_id: request.request_id,
            result: Err(error),
        });
        (context.wake)();
        return;
    }
    let response_fallback = Response {
        generation: request.generation,
        binding: request.binding.clone(),
        label,
        request_id: request.request_id.clone(),
        result: Err(BackendError::Spawn),
    };
    let completion_sender = responses.clone();
    let current = Arc::clone(&context.current);
    let shutdown = Arc::clone(&context.shutdown);
    let busy = Arc::clone(&context.inspect_busy);
    let wake = Arc::clone(&context.wake);
    let binary = binary.clone();
    let spawn = thread::Builder::new()
        .name("codingmage-ui-report-inspection".to_owned())
        .spawn(move || {
            let result = match request.job {
                Job::SourceReportInspect {
                    arguments,
                    deadline,
                } => run_source_report_command(
                    &binary,
                    &arguments,
                    deadline,
                    request.generation.0,
                    current,
                    shutdown,
                ),
                _ => unreachable!("only source inspection enters this supervisor"),
            };
            busy.store(false, Ordering::Release);
            let _ = completion_sender.send(Response {
                generation: request.generation,
                binding: request.binding,
                label,
                request_id: request.request_id,
                result,
            });
            wake();
        });
    if spawn.is_err() {
        context.inspect_busy.store(false, Ordering::Release);
        let _ = responses.send(response_fallback);
        (context.wake)();
    }
}

fn run_source_report_command(
    binary: &CoordinatorBinary,
    arguments: &[String],
    deadline: Duration,
    generation: u64,
    current: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
) -> Result<Vec<u8>, BackendError> {
    if shutdown.load(Ordering::Acquire) || current.load(Ordering::Acquire) > generation {
        return Err(BackendError::Cancelled);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_monitor = Arc::clone(&cancel);
    let monitor = thread::spawn(move || {
        while !cancel_for_monitor.load(Ordering::Acquire) {
            if shutdown.load(Ordering::Acquire) || current.load(Ordering::Acquire) > generation {
                cancel_for_monitor.store(true, Ordering::Release);
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
    });
    let result = binary.run(arguments, deadline, &cancel);
    cancel.store(true, Ordering::Release);
    let _ = monitor.join();
    result
}

fn validate_support_destination(destination: &Path, repository: &Path) -> Result<(), BackendError> {
    let refuse = |message: &str| BackendError::Refused(message.to_owned());
    if !destination.is_absolute()
        || destination
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(refuse(
            "choose an absolute new directory without parent-path components",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| refuse("choose a directory with an existing parent"))?;
    let parent = fs::canonicalize(parent)
        .map_err(|_| refuse("the destination parent must already exist and be readable"))?;
    let repository = fs::canonicalize(repository)
        .map_err(|_| refuse("the repository path must be readable before creating diagnostics"))?;
    if parent.starts_with(repository) {
        return Err(refuse(
            "the support bundle destination must be outside the target repository",
        ));
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(refuse("the support bundle destination already exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(refuse(
            "the support bundle destination could not be checked safely",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    use crate::report::{ChangeCoverage, OutcomeReport, ReportInputs};

    #[test]
    fn support_destination_refuses_repository_alias_and_existing_output() {
        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-support-destination-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("repo")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        let repository = root.join("repo");
        let outside = root.join("outside/new");
        assert!(validate_support_destination(&outside, &repository).is_ok());
        assert!(validate_support_destination(&repository.join("new"), &repository).is_err());
        std::os::unix::fs::symlink(&repository, root.join("repo-alias")).unwrap();
        assert!(validate_support_destination(&root.join("repo-alias/new"), &repository).is_err());
        fs::create_dir(&outside).unwrap();
        assert!(validate_support_destination(&outside, &repository).is_err());
        fs::remove_dir_all(root).unwrap();
    }

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

    fn empty_report() -> OutcomeReport {
        OutcomeReport::assemble(
            &ReportInputs {
                campaign_id: "campaign",
                repository_id: "repository",
                authority_sha256: "authority",
                initial_commit: "initial",
                publication: "local_only".to_owned(),
                admission: None,
                status: None,
                blockers: None,
                final_report: None,
                last_invocation: None,
                commits: &[],
                files: &[],
                change_coverage: ChangeCoverage {
                    observed: false,
                    commits_truncated: false,
                    files_truncated: false,
                },
                runs: &[],
                run_records_observed: false,
                run_records_truncated: false,
            },
            false,
        )
    }

    #[test]
    fn stalled_export_helper_does_not_starve_control_or_window_shutdown() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-stalled-export-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("repo")).unwrap();
        let helper = root.join("stalled-helper");
        let marker = root.join("started");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf started > '{}'\nexec sleep 30\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let worker = Worker::start_with_helper(fake_binary(&root), Ok(helper), || {});
        worker
            .submit(Request {
                generation: Generation(0),
                binding: Binding::default(),
                job: Job::ReportExport {
                    report: Box::new(empty_report()),
                    destination: root.join("report.json"),
                    repository: root.join("repo"),
                    overwrite: false,
                    deadline: Duration::from_secs(5),
                },
                request_id: Some("export-1".to_owned()),
            })
            .unwrap();
        let started = Instant::now();
        while !marker.exists() && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "the controlled helper never started");
        worker.submit(request(0, "stop", "stop")).unwrap();
        let control = worker.wait(Duration::from_secs(2)).unwrap();
        assert_eq!(control.label, "stop");
        assert_eq!(control.result.unwrap(), b"stop\n");
        assert!(!root.join("report.json").exists());
        let closing = Instant::now();
        drop(worker);
        assert!(closing.elapsed() < Duration::from_secs(1));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stalled_export_helper_returns_a_finite_timeout() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-export-deadline-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("repo")).unwrap();
        let helper = root.join("stalled-helper");
        fs::write(&helper, "#!/bin/sh\nexec sleep 30\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let worker = Worker::start_with_helper(fake_binary(&root), Ok(helper), || {});
        worker
            .submit(Request {
                generation: Generation(0),
                binding: Binding::default(),
                job: Job::ReportExport {
                    report: Box::new(empty_report()),
                    destination: root.join("report.json"),
                    repository: root.join("repo"),
                    overwrite: false,
                    deadline: Duration::from_millis(150),
                },
                request_id: Some("export-timeout".to_owned()),
            })
            .unwrap();
        let response = worker.wait(Duration::from_secs(2)).unwrap();
        assert_eq!(response.label, "report-export");
        assert_eq!(response.request_id.as_deref(), Some("export-timeout"));
        assert_eq!(response.result, Err(BackendError::Timeout));
        assert!(!root.join("report.json").exists());
        worker.submit(request(0, "stop", "stop")).unwrap();
        assert_eq!(
            worker.wait(Duration::from_secs(2)).unwrap().result.unwrap(),
            b"stop\n"
        );
        drop(worker);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stalled_source_report_command_does_not_hold_controls_or_shutdown() {
        for inspect in [false, true] {
            stalled_source_report_case(inspect);
        }
    }

    fn stalled_source_report_case(inspect: bool) {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-source-report-stall-{}-{inspect}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("codingmage");
        let marker = root.join("started");
        fs::write(
            &binary,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n report-export|campaign-outcome-report) printf started > '{}'; while :; do :; done;;\n stop) printf 'stop\\n';;\nesac\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let worker = Worker::start(CoordinatorBinary::at(&binary).unwrap(), || {});
        let (job, label) = if inspect {
            (
                Job::SourceReportInspect {
                    arguments: vec!["campaign-outcome-report".to_owned()],
                    deadline: Duration::from_millis(250),
                },
                "campaign-outcome-report",
            )
        } else {
            (
                Job::SourceReportExport {
                    arguments: vec!["report-export".to_owned()],
                    deadline: Duration::from_millis(250),
                },
                "report-export",
            )
        };
        worker
            .submit(Request {
                generation: Generation(0),
                binding: Binding::default(),
                job,
                request_id: Some("source-export-1".to_owned()),
            })
            .unwrap();
        let started = Instant::now();
        while !marker.exists() && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "the controlled coordinator never started");
        worker.submit(request(0, "stop", "stop")).unwrap();
        let control = worker.wait(Duration::from_secs(2)).unwrap();
        assert_eq!(control.label, "stop");
        assert_eq!(control.result.unwrap(), b"stop\n");
        let export = worker.wait(Duration::from_secs(2)).unwrap();
        assert_eq!(export.label, label);
        assert_eq!(export.request_id.as_deref(), Some("source-export-1"));
        assert_eq!(export.result, Err(BackendError::Timeout));
        let closing = Instant::now();
        drop(worker);
        assert!(closing.elapsed() < Duration::from_secs(1));
        fs::remove_dir_all(root).unwrap();
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
