//! Real CLI boundary for bounded, private Setup authorization records.

use std::{
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

struct Fixture {
    root: PathBuf,
    config: PathBuf,
    repository_id: String,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-setup-authorization-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repository = root.join("repository");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir(root.join("workspace")).unwrap();
        git(&repository, &["init", "--initial-branch=main"]);
        git(&repository, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &repository,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(repository.join("TASKS.md"), "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the fixture operation.\n").unwrap();
        git(&repository, &["add", "TASKS.md"]);
        git(&repository, &["commit", "-m", "fixture"]);
        let config = root.join("workspace/codingmage.toml");
        let init = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args([
                "init",
                "--repo",
                repository.to_str().unwrap(),
                "--config",
                config.to_str().unwrap(),
                "--scratch",
                root.join("scratch").to_str().unwrap(),
                "--state",
                root.join("state").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let diagnosis = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args(["doctor", "--config", config.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            diagnosis.status.success(),
            "{}",
            String::from_utf8_lossy(&diagnosis.stderr)
        );
        let diagnosis: Value = serde_json::from_slice(&diagnosis.stdout).unwrap();
        let repository_id = diagnosis["repository_id"].as_str().unwrap().to_owned();
        Self {
            root,
            config,
            repository_id,
        }
    }

    fn write(&self, output: &Path, content: &[u8], overwrite: &str) -> Output {
        self.write_with_repository_id(output, content, overwrite, &self.repository_id)
    }

    fn write_with_repository_id(
        &self,
        output: &Path,
        content: &[u8],
        overwrite: &str,
        repository_id: &str,
    ) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codingmage"))
            .args([
                "setup-write-authorization",
                "--config",
                self.config.to_str().unwrap(),
                "--repository-id",
                repository_id,
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
        let _ = child.stdin.take().unwrap().write_all(content);
        child.wait_with_output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn authorization_write_is_private_bound_and_explicitly_replaceable() {
    let fixture = Fixture::new();
    let destination = fixture.root.join("workspace/authorization.txt");
    let first = b"Owner authorizes only this disposable local campaign.";
    let outcome = fixture.write(&destination, first, "false");
    assert!(
        outcome.status.success(),
        "{}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    assert_eq!(fs::read(&destination).unwrap(), first);
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o077,
        0
    );
    let receipt: Value = serde_json::from_slice(&outcome.stdout).unwrap();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["written"], true);
    assert_eq!(receipt["bytes"], first.len());
    let digest = Command::new("/usr/bin/sha256sum")
        .arg(&destination)
        .output()
        .unwrap();
    assert!(digest.status.success());
    let digest = String::from_utf8(digest.stdout).unwrap();
    assert_eq!(receipt["sha256"], digest.split_whitespace().next().unwrap());
    assert_eq!(receipt["repository_id"], fixture.repository_id);
    assert!(
        !outcome
            .stdout
            .windows(first.len())
            .any(|window| window == first)
    );
    let second = b"Owner authorizes a changed local scope.";
    let refused = fixture.write(&destination, second, "false");
    assert!(!refused.status.success());
    assert_eq!(refused.stderr, b"codingmage.cli.refused\n");
    assert_eq!(fs::read(&destination).unwrap(), first);
    let replaced = fixture.write(&destination, second, "true");
    assert!(replaced.status.success());
    assert_eq!(fs::read(&destination).unwrap(), second);
}

#[test]
fn invalid_private_input_and_unsafe_destinations_never_publish() {
    let fixture = Fixture::new();
    let destination = fixture.root.join("workspace/authorization.txt");
    let stale = fixture.write_with_repository_id(&destination, b"private text", "false", "other");
    assert_eq!(stale.stderr, b"codingmage.cli.stale_observation\n");
    assert!(!destination.exists());
    for input in [b" \n".as_slice(), b"\xff".as_slice(), b"a\0b".as_slice()] {
        let outcome = fixture.write(&destination, input, "false");
        assert!(!outcome.status.success());
        assert_eq!(outcome.stderr, b"codingmage.cli.invalid_argument\n");
        assert!(!destination.exists());
    }
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let outcome = fixture.write(&destination, &oversized, "false");
    assert!(!outcome.status.success());
    assert_eq!(outcome.stderr, b"codingmage.cli.invalid_argument\n");
    assert!(!destination.exists());
    let inside = fixture.root.join("repository/authorization.txt");
    let outcome = fixture.write(&inside, b"private text", "false");
    assert!(!outcome.status.success());
    assert_eq!(outcome.stderr, b"codingmage.cli.refused\n");
    assert!(!inside.exists());
    let invalid_flag = fixture.write(&destination, b"private text", "maybe");
    assert_eq!(invalid_flag.stderr, b"codingmage.cli.usage\n");
    assert!(!destination.exists());
}
