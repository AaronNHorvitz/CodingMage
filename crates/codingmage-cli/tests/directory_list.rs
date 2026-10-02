//! Real-process directory snapshots for native file selection.

use std::{
    fs,
    os::unix::fs::symlink,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codingmage-directory-list-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn list(&self) -> std::process::Output {
        run(&["directory-list", "--directory", self.0.to_str().unwrap()])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_codingmage"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn lists_selected_directory_without_following_links_or_writing() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("sub")).unwrap();
    fs::write(fixture.0.join("b.toml"), b"b").unwrap();
    fs::write(fixture.0.join("a.toml"), b"a").unwrap();
    fs::write(fixture.0.join(".hidden"), b"hidden").unwrap();
    symlink(fixture.0.join("sub"), fixture.0.join("linked")).unwrap();
    let first = fixture.list();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = fixture.list();
    assert_eq!(first.stdout, second.stdout);
    let value: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["directory"], fixture.0.to_str().unwrap());
    assert_eq!(value["truncated"], false);
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0]["name"], "sub");
    assert_eq!(entries[0]["kind"], "directory");
    assert_eq!(entries[1]["name"], "a.toml");
    assert_eq!(entries[2]["name"], "b.toml");
    assert_eq!(entries[3]["name"], "linked");
    assert_eq!(entries[3]["kind"], "symlink");
    assert_eq!(fs::read(fixture.0.join("a.toml")).unwrap(), b"a");
}

#[test]
fn refuses_linked_directory_and_bad_arguments() {
    let fixture = Fixture::new();
    let alias = fixture.0.join("alias");
    symlink(&fixture.0, &alias).unwrap();
    let linked = run(&["directory-list", "--directory", alias.to_str().unwrap()]);
    assert!(!linked.status.success());
    assert_eq!(linked.stderr, b"codingmage.cli.invalid_argument\n");
    let relative = run(&["directory-list", "--directory", "relative"]);
    assert_eq!(relative.stderr, b"codingmage.cli.invalid_argument\n");
    let file = fixture.0.join("ordinary.toml");
    fs::write(&file, b"").unwrap();
    let regular = run(&["directory-list", "--directory", file.to_str().unwrap()]);
    assert_eq!(regular.stderr, b"codingmage.cli.invalid_argument\n");
    let unknown = run(&[
        "directory-list",
        "--directory",
        fixture.0.to_str().unwrap(),
        "--extra",
        "x",
    ]);
    assert_eq!(unknown.stderr, b"codingmage.cli.usage\n");
}

#[test]
fn truncates_large_directory_without_silently_claiming_completeness() {
    let fixture = Fixture::new();
    for index in 0..2_010 {
        fs::write(fixture.0.join(format!("{index:04}.toml")), b"").unwrap();
    }
    let output = fixture.list();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["truncated"], true);
    assert_eq!(value["entries"].as_array().unwrap().len(), 2_000);
    assert!(output.stdout.len() < 1_048_576);
}
