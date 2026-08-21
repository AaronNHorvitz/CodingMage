//! Explicit five-unit live-provider qualification over disposable repositories.

use codingmage_soak::materialize_supervised_pilot_unit_fixture;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const UNIT_COUNT: u8 = 5;

fn main() {
    if let Err(error) = run() {
        eprintln!("supervised pilot failed: {error}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 8 {
        return Err("usage: supervised_live_pilot OUTPUT_ROOT CODINGMAGE CLAUDE CODEX CLAUDE_MODEL CLAUDE_EFFORT CODEX_MODEL CODEX_EFFORT".into());
    }
    let output_root = PathBuf::from(&arguments[0]);
    if output_root.exists() {
        return Err("output root must not already exist".into());
    }
    fs::create_dir(&output_root)?;
    let output_root = fs::canonicalize(output_root)?;
    let codingmage = canonical_file(Path::new(&arguments[1]))?;
    let claude = canonical_file(Path::new(&arguments[2]))?;
    let codex = canonical_file(Path::new(&arguments[3]))?;
    let claude_model = text_argument(&arguments[4])?;
    let claude_effort = text_argument(&arguments[5])?;
    let codex_model = text_argument(&arguments[6])?;
    let codex_effort = text_argument(&arguments[7])?;
    let mut units = Vec::with_capacity(usize::from(UNIT_COUNT));

    for unit in 1..=UNIT_COUNT {
        eprintln!("[supervised-pilot] starting disposable unit {unit}/{UNIT_COUNT}");
        let unit_root = output_root.join(format!("unit-{unit:02}"));
        let target = unit_root.join("target");
        let config_root = unit_root.join("config");
        fs::create_dir_all(&target)?;
        fs::create_dir(&config_root)?;
        let fixture = materialize_supervised_pilot_unit_fixture(&target, unit)?;
        let config = config_root.join("codingmage.toml");
        let scratch = unit_root.join("scratch");
        let state = unit_root.join("state");
        let initialization = Command::new(&codingmage)
            .args(["init", "--repo"])
            .arg(&fixture.root)
            .arg("--config")
            .arg(&config)
            .arg("--scratch")
            .arg(&scratch)
            .arg("--state")
            .arg(&state)
            .output()?;
        require_success(&initialization, "configuration initialization")?;
        let mut config_file = fs::OpenOptions::new().append(true).open(&config)?;
        config_file.write_all(
            b"\n[[gate_commands]]\nexecutable = \"/usr/bin/cmp\"\nargs = [\"--silent\", \"artifact.txt\", \"expected.txt\"]\n",
        )?;
        config_file.sync_all()?;
        let spec = unit_root.join("run.toml");
        fs::write(
            &spec,
            format!(
                "version = 2\n\
                 task_id = \"0.1.1.1\"\n\
                 owned_paths = [\"artifact.txt\"]\n\
                 completion_policy = \"close_task\"\n\n\
                 [implementer]\n\
                 executable = \"{}\"\n\
                 model = \"{claude_model}\"\n\
                 effort = \"{claude_effort}\"\n\
                 authentication = \"existing_login\"\n\n\
                 [reviewer]\n\
                 executable = \"{}\"\n\
                 model = \"{codex_model}\"\n\
                 effort = \"{codex_effort}\"\n",
                claude.display(),
                codex.display()
            ),
        )?;
        let diagnosis = Command::new(&codingmage)
            .args(["doctor", "--config"])
            .arg(&config)
            .output()?;
        require_success(&diagnosis, "preflight diagnosis")?;

        let head_before = git_output(&fixture.root, &["rev-parse", "HEAD"])?;
        let status_before = git_output(&fixture.root, &["status", "--porcelain=v2", "--branch"])?;
        let worktrees_before = git_output(&fixture.root, &["worktree", "list", "--porcelain"])?;
        let tasks_before = fs::read(fixture.root.join("TASKS.md"))?;
        let artifact_before = fs::read(fixture.root.join("artifact.txt"))?;

        let output = Command::new(&codingmage)
            .args(["run", "--config"])
            .arg(&config)
            .arg("--spec")
            .arg(&spec)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(format!("CodingMage run failed for supervised unit {unit}").into());
        }
        let outcome: Value = serde_json::from_slice(&output.stdout)?;
        let branch = required_text(&outcome, "branch")?;
        let candidate = required_text(&outcome, "candidate_commit")?;
        let completion = required_text(&outcome, "completion_commit")?;
        if outcome["state"] != "complete"
            || outcome["review_verdict"] != "pass"
            || candidate == completion
            || !valid_object_id(candidate)
            || !valid_object_id(completion)
        {
            return Err(format!("invalid terminal outcome for supervised unit {unit}").into());
        }

        require_equal(
            &head_before,
            &git_output(&fixture.root, &["rev-parse", "HEAD"])?,
            "active HEAD",
        )?;
        require_equal(
            &status_before,
            &git_output(&fixture.root, &["status", "--porcelain=v2", "--branch"])?,
            "active status",
        )?;
        require_equal(
            &worktrees_before,
            &git_output(&fixture.root, &["worktree", "list", "--porcelain"])?,
            "worktree inventory",
        )?;
        require_equal(
            &tasks_before,
            &fs::read(fixture.root.join("TASKS.md"))?,
            "active task source",
        )?;
        require_equal(
            &artifact_before,
            &fs::read(fixture.root.join("artifact.txt"))?,
            "active artifact",
        )?;
        if fs::read_dir(&scratch)?.next().is_some() {
            return Err(format!("scratch residue remained after supervised unit {unit}").into());
        }
        let expected = fs::read(fixture.root.join("expected.txt"))?;
        require_equal(
            &expected,
            &git_output(&fixture.root, &["show", &format!("{branch}:artifact.txt")])?,
            "completed artifact",
        )?;
        require_equal(
            &tasks_before,
            &git_output(&fixture.root, &["show", &format!("{candidate}:TASKS.md")])?,
            "candidate task source",
        )?;
        let completed_tasks =
            git_output(&fixture.root, &["show", &format!("{completion}:TASKS.md")])?;
        if !String::from_utf8(completed_tasks)?.contains("- [x] **Sub-task 0.1.1.1:**") {
            return Err(format!("completion marker missing for supervised unit {unit}").into());
        }
        require_equal(
            format!("{candidate}\n").as_bytes(),
            &git_output(&fixture.root, &["rev-parse", &format!("{completion}^")])?,
            "completion lineage",
        )?;
        let refs = git_output(
            &fixture.root,
            &["for-each-ref", "--format=%(refname)", "refs/heads"],
        )?;
        if refs
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count()
            != 2
        {
            return Err(format!("unexpected branch inventory for supervised unit {unit}").into());
        }

        units.push(json!({
            "unit": unit,
            "run_id": outcome["run_id"],
            "state": outcome["state"],
            "review_verdict": outcome["review_verdict"],
            "candidate_commit": candidate,
            "completion_commit": completion,
            "correction_rounds": outcome["correction_rounds"],
            "utilization": outcome["utilization"],
            "active_checkout_preserved": true,
            "owned_worktree_released": true,
            "scratch_empty": true
        }));
        eprintln!("[supervised-pilot] unit {unit}/{UNIT_COUNT} passed");
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "qualification": "sprint-17-five-supervised-units",
            "unit_count": UNIT_COUNT,
            "publication": "local_only",
            "all_units_passed": true,
            "fixtures_retained": true,
            "units": units
        }))?
    );
    Ok(())
}

fn canonical_file(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("provider and CodingMage paths must name regular files, not links".into());
    }
    Ok(fs::canonicalize(path)?)
}

fn text_argument(value: &std::ffi::OsStr) -> Result<&str, Box<dyn std::error::Error>> {
    value
        .to_str()
        .ok_or_else(|| "profile arguments must be UTF-8".into())
}

fn require_success(
    output: &std::process::Output,
    operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{operation} failed").into())
    }
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_clear()
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["--no-pager", "-c", "core.hooksPath=/dev/null"])
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err("Git evidence observation failed".into());
    }
    Ok(output.stdout)
}

fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("missing terminal field {field}").into())
}

fn valid_object_id(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require_equal(
    expected: &[u8],
    actual: &[u8],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!("{label} was not preserved exactly").into())
    }
}
