use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use codingmage_contracts::AgentId;
use codingmage_core::{
    AgentProfile, CapabilityPolicy, CommandSpec, Config, PublicationMode, PublicationPolicy,
    RepositoryAuthorization,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

pub(crate) struct GitFixture {
    pub root: PathBuf,
    pub source: PathBuf,
    pub target: PathBuf,
    pub scratch: PathBuf,
    pub state: PathBuf,
}

impl GitFixture {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-git-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("codingmage-source");
        let target = root.join("target");
        let scratch = root.join("scratch");
        let state = root.join("state");
        for path in [&source, &target, &scratch, &state] {
            fs::create_dir_all(path).unwrap();
        }
        run(&target, &["init", "-b", "main"]);
        fs::write(target.join("tracked-one.txt"), "one\n").unwrap();
        fs::write(target.join("tracked-two.txt"), "two\n").unwrap();
        run(&target, &["add", "tracked-one.txt", "tracked-two.txt"]);
        run(
            &target,
            &[
                "-c",
                "user.name=CodingMage Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        );
        Self {
            root,
            source,
            target,
            scratch,
            state,
        }
    }

    pub fn config(&self) -> Config {
        Config {
            version: 1,
            target_path: self.target.clone(),
            task_source: PathBuf::from("TASKS.md"),
            default_branch: "main".to_owned(),
            integration_branch: "codingmage/integration".to_owned(),
            scratch_root: self.scratch.clone(),
            state_root: self.state.clone(),
            agent_profiles: vec![AgentProfile {
                id: AgentId::new("fixture-agent").unwrap(),
                provider: "fake".to_owned(),
                model: "fixture".to_owned(),
            }],
            correction_limit: 3,
            gate_commands: vec![CommandSpec {
                executable: PathBuf::from("/usr/bin/git"),
                args: vec!["diff".to_owned(), "--check".to_owned()],
            }],
            capabilities: CapabilityPolicy::default(),
            publication: PublicationPolicy {
                mode: PublicationMode::LocalOnly,
            },
            allow_parent_discovery: false,
        }
    }

    pub fn authorization(&self) -> RepositoryAuthorization {
        RepositoryAuthorization::authorize(&self.config(), &self.source).unwrap()
    }

    pub fn head(&self) -> String {
        output(&self.target, &["rev-parse", "HEAD"])
            .trim()
            .to_owned()
    }

    pub fn status(&self) -> Vec<u8> {
        Command::new("/usr/bin/git")
            .current_dir(&self.target)
            .args([
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ])
            .output()
            .unwrap()
            .stdout
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn run(directory: &Path, arguments: &[&str]) {
    let status = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "git fixture command failed: {arguments:?}"
    );
}

pub(crate) fn output(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git fixture command failed: {arguments:?}"
    );
    String::from_utf8(output.stdout).unwrap()
}
