//! Real-process project selection through the read-only coordinator contract.

use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use sha2::{Digest as _, Sha256};

struct Fixture {
    root: PathBuf,
    target: PathBuf,
    config: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-project-open-{}-{nonce}",
            std::process::id()
        ));
        let target = root.join("target");
        let config = root.join("config/codingmage.toml");
        let source = target.join("TASKS.md");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir(root.join("config")).unwrap();
        git(&target, &["init", "--initial-branch=main"]);
        git(&target, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &target,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(&source, task_source()).unwrap();
        git(&target, &["add", "TASKS.md"]);
        git(&target, &["commit", "-q", "-m", "fixture"]);
        let output = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args([
                "init",
                "--repo",
                target.to_str().unwrap(),
                "--config",
                config.to_str().unwrap(),
                "--scratch",
                root.join("scratch").to_str().unwrap(),
                "--state",
                root.join("state").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        Self {
            root,
            target,
            config,
            source,
        }
    }

    fn open(&self) -> Output {
        run(&["project-open", "--config", self.config.to_str().unwrap()])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(directory: &Path, arguments: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codingmage"))
        .args(arguments)
        .output()
        .unwrap()
}

fn task_source() -> &'static [u8] {
    b"# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the fixture task safely.\n"
}

fn digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

#[test]
fn project_open_reports_exact_validated_source_without_writing() {
    let fixture = Fixture::new();
    let before_config = fs::read(&fixture.config).unwrap();
    let before_source = fs::read(&fixture.source).unwrap();
    let output = fixture.open();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["config_path"], fixture.config.to_str().unwrap());
    assert_eq!(
        value["config"]["target_path"],
        fixture.target.to_str().unwrap()
    );
    assert_eq!(value["config_sha256"], digest(&before_config));
    assert_eq!(value["plan"]["state"], "loaded");
    assert_eq!(value["plan"]["byte_length"], before_source.len());
    assert_eq!(value["plan"]["source_sha256"], digest(&before_source));
    assert_eq!(value["plan"]["plan"]["items"][1]["id"], "0.1.1.1");
    assert_eq!(fs::read(&fixture.config).unwrap(), before_config);
    assert_eq!(fs::read(&fixture.source).unwrap(), before_source);
}

#[test]
fn project_open_keeps_plan_failures_distinct_and_refuses_bad_arguments() {
    let fixture = Fixture::new();
    fs::write(&fixture.source, b"not a plan").unwrap();
    let invalid: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(invalid["plan"]["state"], "invalid");
    assert_eq!(invalid["plan"]["code"], "codingmage.plan.missing_goal");
    let mut duplicate = task_source().to_vec();
    duplicate.extend_from_slice(b"  - [ ] **Sub-task 0.1.1.1:** Duplicate fixture task.\n");
    fs::write(&fixture.source, duplicate).unwrap();
    let duplicate: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(duplicate["plan"]["state"], "invalid");
    assert_eq!(duplicate["plan"]["code"], "codingmage.plan.duplicate_id");
    fs::remove_file(&fixture.source).unwrap();
    symlink(&fixture.config, &fixture.source).unwrap();
    let linked: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(linked["plan"]["state"], "unavailable");
    fs::remove_file(&fixture.source).unwrap();
    let absent: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(absent["plan"]["state"], "unavailable");
    let outside = fixture.root.join("outside.md");
    fs::write(&outside, task_source()).unwrap();
    fs::hard_link(&outside, &fixture.source).unwrap();
    let linked_inode: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(linked_inode["plan"]["state"], "unavailable");
    fs::remove_file(&fixture.source).unwrap();
    fs::write(&fixture.source, vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
    let oversized: Value = serde_json::from_slice(&fixture.open().stdout).unwrap();
    assert_eq!(oversized["plan"]["state"], "unavailable");
    let extra = run(&[
        "project-open",
        "--config",
        fixture.config.to_str().unwrap(),
        "--unknown",
        "x",
    ]);
    assert!(!extra.status.success());
    assert_eq!(
        String::from_utf8_lossy(&extra.stderr).trim(),
        "codingmage.cli.usage"
    );
    let relative = run(&["project-open", "--config", "relative.toml"]);
    assert!(!relative.status.success());
    assert_eq!(
        String::from_utf8_lossy(&relative.stderr).trim(),
        "codingmage.cli.invalid_argument"
    );
}

#[test]
fn project_open_keeps_configuration_when_a_parsed_plan_exceeds_the_output_bound() {
    let fixture = Fixture::new();
    let mut source = String::from(
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n",
    );
    let title = "x".repeat(360);
    for index in 1..=18_000 {
        use std::fmt::Write as _;
        writeln!(&mut source, "  - [ ] **Sub-task 0.1.1.{index}:** {title}").unwrap();
    }
    assert!(source.len() < 8 * 1024 * 1024);
    fs::write(&fixture.source, source).unwrap();
    let output = fixture.open();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() < 8 * 1024 * 1024);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["plan"]["state"], "projection_too_large");
    assert_eq!(
        value["config"]["target_path"],
        fixture.target.to_str().unwrap()
    );
}
