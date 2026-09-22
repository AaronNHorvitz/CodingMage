//! Disposable repository fixtures and a display-less harness for the native workspace.
#![allow(dead_code)]

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use codingmage_ui::{App, backend::CoordinatorBinary};
use egui_kittest::Harness;

/// Resolves the real `codingmage` executable built by this workspace.
pub fn coordinator_binary() -> PathBuf {
    if let Some(explicit) = std::env::var_os("CODINGMAGE_TEST_BINARY") {
        return PathBuf::from(explicit);
    }
    let current = std::env::current_exe().unwrap();
    let target_dir = current.parent().unwrap().parent().unwrap();
    let candidate = target_dir.join("codingmage");
    assert!(
        candidate.is_file(),
        "the coordinator executable {} is missing; build it with `cargo build -p codingmage-cli` before running the interface tests",
        candidate.display()
    );
    candidate
}

/// One disposable Git repository with a configuration written by `codingmage init`.
pub struct Fixture {
    pub root: PathBuf,
    pub target: PathBuf,
    pub config: PathBuf,
    pub scratch: PathBuf,
    pub state: PathBuf,
}

impl Fixture {
    /// Creates a repository with `open_subtasks` open sub-tasks and initializes a configuration.
    pub fn new(label: &str, open_subtasks: usize) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = root.join("target");
        fs::create_dir_all(target.join("src")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        git(&target, &["init", "--initial-branch=main"]);
        git(&target, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &target,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(target.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
        fs::write(target.join("TASKS.md"), task_source(open_subtasks)).unwrap();
        git(&target, &["add", "TASKS.md", "src/lib.rs"]);
        git(&target, &["commit", "-q", "-m", "fixture"]);
        let config = root.join("config/codingmage.toml");
        let scratch = root.join("scratch");
        let state = root.join("state");
        let output = Command::new(coordinator_binary())
            .args([
                "init",
                "--repo",
                target.to_str().unwrap(),
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
            output.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self {
            root,
            target,
            config,
            scratch,
            state,
        }
    }

    /// Writes an executable fixture script.
    pub fn executable(&self, name: &str, content: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = self.root.join(name);
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    /// Current head of the target.
    pub fn head(&self) -> String {
        git_output(&self.target, &["rev-parse", "HEAD"])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Strict task source with the requested number of open sub-tasks.
pub fn task_source(open_subtasks: usize) -> String {
    let mut source = String::from(
        "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n",
    );
    for index in 1..=open_subtasks {
        let _ = writeln!(
            source,
            "  - [ ] **Sub-task 0.1.1.{index}:** Complete fixture operation number {index} safely."
        );
    }
    source
        .push_str("\n- [ ] **AC 0.1:** Given the fixture, when it runs, then the value changes.\n");
    source
}

pub fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// System fonts for the display-less harness.
pub fn fonts() -> egui::FontDefinitions {
    let selected = codingmage_ui::fonts::select_fonts().expect("system fonts");
    codingmage_ui::fonts::font_definitions(&selected).expect("font bytes")
}

/// Builds a harness around the application with the given coordinator resolution.
pub fn harness(
    binary: Result<CoordinatorBinary, codingmage_ui::backend::BackendError>,
    size: [f32; 2],
) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::Vec2::new(size[0], size[1]))
        .with_max_steps(4)
        .build_eframe(move |creation| {
            creation.egui_ctx.set_fonts(fonts());
            App::with_binary(&creation.egui_ctx, binary)
        })
}

/// Steps the harness until `done` holds or the timeout passes.
pub fn settle(
    harness: &mut Harness<'static, App>,
    timeout: Duration,
    done: impl Fn(&App) -> bool,
) -> bool {
    let started = Instant::now();
    loop {
        harness.step();
        if done(harness.state()) {
            return true;
        }
        if started.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Snapshot of every regular file under a directory for change detection.
pub fn tree_digest(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out
}
