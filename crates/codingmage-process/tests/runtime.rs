//! Live integration tests for process limits and Linux descendant cleanup.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use codingmage_process::{
    CancellationToken, DescendantCleanup, ProcessError, ProcessExecutor, ProcessOutcome,
    ProcessProfile, ProcessRequest,
};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
    guard: PathBuf,
    executable: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-process-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self {
            root,
            guard: PathBuf::from(env!("CARGO_BIN_EXE_codingmage-process-guard")),
            executable: PathBuf::from(env!("CARGO_BIN_EXE_codingmage-process-fixture")),
        }
    }

    fn executor(&self) -> ProcessExecutor {
        ProcessExecutor::new(&self.guard, &self.root.join("control")).unwrap()
    }

    fn request(&self, arguments: Vec<String>) -> ProcessRequest {
        ProcessRequest {
            arguments,
            working_directory: self.root.clone(),
            environment: BTreeMap::new(),
            stdin: Vec::new(),
            max_output_bytes: 4096,
            deadline_millis: 5000,
            max_processes: 4,
            max_open_files: 64,
            expected_exit_codes: BTreeSet::from([0]),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists(), "timed out waiting for fixture file");
}

fn wait_absent(pid: u32) {
    let path = PathBuf::from(format!("/proc/{pid}"));
    let deadline = Instant::now() + Duration::from_secs(3);
    while path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!path.exists(), "process {pid} was not reaped");
}

fn wait_group_absent(group: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let present = fs::read_dir("/proc")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
            .any(|pid| {
                let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                    return false;
                };
                let Some(end) = stat.rfind(')') else {
                    return false;
                };
                stat.get(end + 2..)
                    .and_then(|rest| rest.split_whitespace().nth(2))
                    .and_then(|value| value.parse::<u32>().ok())
                    == Some(group)
            });
        if !present {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "process group {group} was not reaped"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn literal_arguments_have_no_shell_or_response_file_interpretation() {
    let fixture = Fixture::new();
    let vector = strings(&[
        "args",
        "; touch should-not-exist",
        "$(uname)",
        "*",
        "--unknown",
    ]);
    let profile = ProcessProfile::new(&fixture.executable, [vector.clone()], []).unwrap();
    let result = fixture
        .executor()
        .execute(
            &profile,
            &fixture.request(vector),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Succeeded);
    let output = String::from_utf8(result.stdout.retained).unwrap();
    assert!(output.contains("; touch should-not-exist"));
    assert!(output.contains("$(uname)"));
    assert!(!fixture.root.join("should-not-exist").exists());

    let denied = strings(&["args", "different"]);
    assert_eq!(
        fixture.executor().execute(
            &profile,
            &fixture.request(denied),
            &CancellationToken::default()
        ),
        Err(ProcessError::ArgumentsDenied)
    );
    assert_eq!(
        ProcessProfile::new(&fixture.executable, [strings(&["args", "@response"])], [])
            .unwrap_err(),
        ProcessError::InvalidProfile
    );
}

#[test]
fn environment_and_working_directory_are_explicit() {
    let fixture = Fixture::new();
    let vector = strings(&["env", "SAFE_VALUE"]);
    let profile = ProcessProfile::new(
        &fixture.executable,
        [vector.clone()],
        ["SAFE_VALUE".to_owned()],
    )
    .unwrap();
    let mut request = fixture.request(vector);
    request
        .environment
        .insert("SAFE_VALUE".to_owned(), "visible".to_owned());
    let result = fixture
        .executor()
        .execute(&profile, &request, &CancellationToken::default())
        .unwrap();
    let output = String::from_utf8(result.stdout.retained).unwrap();
    assert!(output.contains("requested=visible"));
    assert!(output.contains("home=absent"));
    assert!(output.contains(&format!("cwd={}", fixture.root.display())));

    request
        .environment
        .insert("UNGRANTED".to_owned(), "hidden".to_owned());
    assert_eq!(
        fixture
            .executor()
            .execute(&profile, &request, &CancellationToken::default()),
        Err(ProcessError::EnvironmentDenied)
    );
}

#[test]
fn stdin_exit_and_stream_digests_are_truthful() {
    let fixture = Fixture::new();
    let stdin_vector = strings(&["stdin"]);
    let stdin_profile =
        ProcessProfile::new(&fixture.executable, [stdin_vector.clone()], []).unwrap();
    let mut request = fixture.request(stdin_vector);
    request.stdin = b"bounded input\n".to_vec();
    let result = fixture
        .executor()
        .execute(&stdin_profile, &request, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.stdout.retained, request.stdin);
    assert_eq!(result.stdout.total_bytes, request.stdin.len() as u64);
    assert_eq!(result.stdout.sha256.len(), 64);

    let fail_vector = strings(&["fail", "7"]);
    let fail_profile = ProcessProfile::new(&fixture.executable, [fail_vector.clone()], []).unwrap();
    let result = fixture
        .executor()
        .execute(
            &fail_profile,
            &fixture.request(fail_vector),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Failed);
    assert_eq!(result.exit_code, 7);

    let reserved_vector = strings(&["fail", "120"]);
    let reserved_profile =
        ProcessProfile::new(&fixture.executable, [reserved_vector.clone()], []).unwrap();
    let mut reserved_request = fixture.request(reserved_vector);
    reserved_request.expected_exit_codes = BTreeSet::from([120]);
    let reserved = fixture
        .executor()
        .execute(
            &reserved_profile,
            &reserved_request,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(reserved.outcome, ProcessOutcome::Succeeded);
    assert_eq!(reserved.exit_code, 120);
}

#[test]
fn output_limit_terminates_the_target_group() {
    let fixture = Fixture::new();
    let vector = strings(&["output", "1000000"]);
    let profile = ProcessProfile::new(&fixture.executable, [vector.clone()], []).unwrap();
    let mut request = fixture.request(vector);
    request.max_output_bytes = 128;
    let result = fixture
        .executor()
        .execute(&profile, &request, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.outcome, ProcessOutcome::OutputLimit);
    assert!(result.stdout.truncated || result.stderr.truncated);
    assert!(result.stdout.retained.len() <= 128);
    assert_eq!(result.descendant_cleanup, DescendantCleanup::Verified);
}

#[test]
fn timeout_and_cancellation_reap_descendants() {
    for cancelled in [false, true] {
        let fixture = Fixture::new();
        let child_pid_path = fixture.root.join(if cancelled {
            "cancel-child.pid"
        } else {
            "timeout-child.pid"
        });
        let vector = vec![
            "spawn-child".to_owned(),
            child_pid_path.display().to_string(),
            "10000".to_owned(),
        ];
        let profile = ProcessProfile::new(&fixture.executable, [vector.clone()], []).unwrap();
        let mut request = fixture.request(vector);
        request.deadline_millis = if cancelled { 5000 } else { 150 };
        let cancellation = CancellationToken::default();
        if cancelled {
            let token = cancellation.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(100));
                token.cancel();
            });
        }
        let result = fixture
            .executor()
            .execute(&profile, &request, &cancellation)
            .unwrap();
        assert_eq!(
            result.outcome,
            if cancelled {
                ProcessOutcome::Cancelled
            } else {
                ProcessOutcome::TimedOut
            }
        );
        assert_eq!(result.descendant_cleanup, DescendantCleanup::Verified);
        wait_for(&child_pid_path);
        let child_pid = fs::read_to_string(child_pid_path)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        wait_absent(child_pid);
    }
}

#[test]
fn process_count_limit_reaps_the_excess_group() {
    let fixture = Fixture::new();
    let child_pid_path = fixture.root.join("process-limit-child.pid");
    let vector = vec![
        "spawn-child".to_owned(),
        child_pid_path.display().to_string(),
        "10000".to_owned(),
    ];
    let profile = ProcessProfile::new(&fixture.executable, [vector.clone()], []).unwrap();
    let mut request = fixture.request(vector);
    request.max_processes = 1;
    let result = fixture
        .executor()
        .execute(&profile, &request, &CancellationToken::default())
        .unwrap();
    assert_eq!(result.outcome, ProcessOutcome::RuntimeFailure);
    assert_eq!(result.descendant_cleanup, DescendantCleanup::Verified);
    wait_group_absent(result.process_group_id);
}

#[test]
fn executable_replacement_and_missing_deadline_fail_before_spawn() {
    let fixture = Fixture::new();
    let copied = fixture.root.join("copied-fixture");
    fs::copy(&fixture.executable, &copied).unwrap();
    let vector = strings(&["args", "safe"]);
    let profile = ProcessProfile::new(&copied, [vector.clone()], []).unwrap();
    fs::remove_file(&copied).unwrap();
    fs::copy(&fixture.executable, &copied).unwrap();
    assert_eq!(
        fixture.executor().execute(
            &profile,
            &fixture.request(vector.clone()),
            &CancellationToken::default()
        ),
        Err(ProcessError::Identity)
    );

    let live_profile = ProcessProfile::new(&fixture.executable, [vector.clone()], []).unwrap();
    let mut no_deadline = fixture.request(vector);
    no_deadline.deadline_millis = 0;
    assert_eq!(
        fixture
            .executor()
            .execute(&live_profile, &no_deadline, &CancellationToken::default()),
        Err(ProcessError::InvalidRequest)
    );
}

#[test]
fn parent_failure_guard_reaps_the_descendant_group() {
    let fixture = Fixture::new();
    let driver = PathBuf::from(env!("CARGO_BIN_EXE_codingmage-process-driver"));
    let child_pid_path = fixture.root.join("parent-loss-child.pid");
    let mut parent = Command::new(driver)
        .args([
            fixture.guard.as_os_str(),
            fixture.executable.as_os_str(),
            fixture.root.join("driver-control").as_os_str(),
            child_pid_path.as_os_str(),
        ])
        .spawn()
        .unwrap();
    wait_for(&child_pid_path);
    let child_pid = fs::read_to_string(&child_pid_path)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    kill(
        Pid::from_raw(i32::try_from(parent.id()).unwrap()),
        Signal::SIGKILL,
    )
    .unwrap();
    let _ = parent.wait();
    wait_absent(child_pid);
}
