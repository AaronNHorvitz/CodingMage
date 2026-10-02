//! Real-process guided configuration publication through the public coordinator command.

use std::{
    fs,
    io::Write as _,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_core::{Config, load_config};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

struct Fixture {
    root: PathBuf,
    repository: PathBuf,
    config: PathBuf,
    scratch: PathBuf,
    state: PathBuf,
    bytes: Vec<u8>,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-setup-config-{label}-{}-{nonce}",
            std::process::id()
        ));
        let repository = root.join("repository");
        let workspace = root.join("workspace");
        let config = workspace.join("codingmage.toml");
        let scratch = workspace.join("scratch");
        let state = workspace.join("state");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir(&workspace).unwrap();
        git(&repository, &["init", "--initial-branch=main"]);
        git(&repository, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &repository,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(repository.join("TASKS.md"), "# Tasks\n").unwrap();
        git(&repository, &["add", "TASKS.md"]);
        git(&repository, &["commit", "-m", "fixture"]);
        let init = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args([
                "init",
                "--repo",
                repository.to_str().unwrap(),
                "--config",
                config.to_str().unwrap(),
                "--scratch",
                scratch.to_str().unwrap(),
                "--state",
                state.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let bytes = fs::read(&config).unwrap();
        fs::remove_file(&config).unwrap();
        fs::remove_dir(&scratch).unwrap();
        fs::remove_dir(&state).unwrap();
        Self {
            root,
            repository,
            config,
            scratch,
            state,
            bytes,
        }
    }

    fn write(&self, output: &Path, bytes: &[u8], overwrite: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args([
                "setup-write-config",
                "--repo",
                self.repository.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--overwrite",
                overwrite,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(bytes);
        child.wait_with_output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn guided_configuration_bootstraps_private_roots_and_requires_explicit_overwrite() {
    let fixture = Fixture::new("bootstrap");
    let written = fixture.write(&fixture.config, &fixture.bytes, "false");
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    assert_eq!(fs::read(&fixture.config).unwrap(), fixture.bytes);
    assert!(fixture.scratch.is_dir() && fixture.state.is_dir());
    assert_eq!(
        fs::metadata(&fixture.scratch).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&fixture.state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let receipt: Value = serde_json::from_slice(&written.stdout).unwrap();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["written"], true);
    assert_eq!(receipt["bytes"], fixture.bytes.len());
    let mut digest = String::new();
    for byte in Sha256::digest(&fixture.bytes) {
        use std::fmt::Write as _;
        write!(&mut digest, "{byte:02x}").unwrap();
    }
    assert_eq!(receipt["sha256"], digest);
    assert!(
        receipt["repository_id"]
            .as_str()
            .unwrap()
            .starts_with("repo-")
    );
    load_config(&fixture.config).unwrap();

    let refused = fixture.write(&fixture.config, &fixture.bytes, "false");
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "codingmage.cli.refused"
    );
    assert_eq!(fs::read(&fixture.config).unwrap(), fixture.bytes);

    let mut amended: Config = toml::from_str(std::str::from_utf8(&fixture.bytes).unwrap()).unwrap();
    amended.correction_limit = 4;
    let amended = toml::to_string_pretty(&amended).unwrap();
    let replaced = fixture.write(&fixture.config, amended.as_bytes(), "true");
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    assert_eq!(load_config(&fixture.config).unwrap().correction_limit, 4);
    assert_eq!(fs::read_to_string(&fixture.config).unwrap(), amended);

    let foreign_repository = fixture.root.join("foreign-repository");
    fs::create_dir(&foreign_repository).unwrap();
    let mut foreign = load_config(&fixture.config).unwrap();
    foreign.target_path = foreign_repository;
    let foreign_bytes = toml::to_string_pretty(&foreign).unwrap();
    fs::write(&fixture.config, &foreign_bytes).unwrap();
    let refused = fixture.write(&fixture.config, &fixture.bytes, "true");
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "codingmage.cli.refused"
    );
    assert_eq!(fs::read_to_string(&fixture.config).unwrap(), foreign_bytes);
}

#[test]
fn malformed_or_cross_repository_configuration_has_no_publication() {
    let fixture = Fixture::new("refusals");
    let malformed = fixture.write(&fixture.config, b"not = [toml", "false");
    assert!(!malformed.status.success());
    assert!(!fixture.config.exists() && !fixture.scratch.exists() && !fixture.state.exists());

    let mut unknown = fixture.bytes.clone();
    unknown.extend_from_slice(b"\nunexpected = true\n");
    let unknown = fixture.write(&fixture.config, &unknown, "false");
    assert!(!unknown.status.success());
    assert!(!fixture.config.exists() && !fixture.scratch.exists());

    let oversized = fixture.write(&fixture.config, &vec![b'x'; 1024 * 1024 + 1], "false");
    assert!(!oversized.status.success());
    assert!(!fixture.config.exists() && !fixture.scratch.exists());

    let inside = fixture.repository.join("codingmage.toml");
    let refused = fixture.write(&inside, &fixture.bytes, "false");
    assert!(!refused.status.success());
    assert!(!inside.exists() && !fixture.scratch.exists());

    symlink(fixture.repository.join("TASKS.md"), &fixture.config).unwrap();
    let linked_output = fixture.write(&fixture.config, &fixture.bytes, "true");
    assert!(!linked_output.status.success());
    assert!(!fixture.scratch.exists() && !fixture.state.exists());
    fs::remove_file(&fixture.config).unwrap();

    let mut changed: Config = toml::from_str(std::str::from_utf8(&fixture.bytes).unwrap()).unwrap();
    changed.target_path = fixture.root.join("another-repository");
    let changed = toml::to_string_pretty(&changed).unwrap();
    let stale = fixture.write(&fixture.config, changed.as_bytes(), "false");
    assert_eq!(
        String::from_utf8_lossy(&stale.stderr).trim(),
        "codingmage.cli.stale_observation"
    );
    assert!(!fixture.config.exists() && !fixture.scratch.exists());

    let mut distant: Config = toml::from_str(std::str::from_utf8(&fixture.bytes).unwrap()).unwrap();
    distant.scratch_root = fixture.root.join("other/scratch");
    let distant = toml::to_string_pretty(&distant).unwrap();
    let refused = fixture.write(&fixture.config, distant.as_bytes(), "false");
    assert!(!refused.status.success());
    assert!(!fixture.config.exists() && !fixture.scratch.exists());

    symlink(&fixture.repository, &fixture.scratch).unwrap();
    let linked = fixture.write(&fixture.config, &fixture.bytes, "false");
    assert!(!linked.status.success());
    assert!(!fixture.config.exists() && !fixture.state.exists());
}
