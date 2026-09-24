//! Detached coordinator launches and their liveness.
//!
//! The interface starts `codingmage campaign` in its own process group with private output
//! files and records the process identity. It never waits for, signals or adopts that process;
//! liveness is observed from `/proc` and the recorded kernel start time, and the final JSON
//! outcome is read from the private stdout file once the process has exited.

use std::{
    fs,
    os::unix::process::CommandExt as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    admission::now_ms,
    backend::{
        CoordinatorBinary,
        models::{CampaignOutcome, parse_campaign_outcome},
    },
    state_dir::{StateError, ensure_private_dir, read_private, write_private},
};

/// One recorded coordinator launch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRecord {
    /// Document version.
    pub version: u16,
    /// Launch identity.
    pub launch_id: String,
    /// Campaign identity.
    pub campaign_id: String,
    /// Campaign authority digest at launch.
    pub authority_sha256: String,
    /// Configuration path passed to the coordinator.
    pub config_path: PathBuf,
    /// Specification path passed to the coordinator.
    pub spec_path: PathBuf,
    /// Coordinator executable.
    pub executable: PathBuf,
    /// Process identifier.
    pub pid: u32,
    /// Kernel start time of the process in clock ticks since boot.
    pub start_ticks: u64,
    /// Unix milliseconds when launched.
    pub launched_at_ms: u64,
    /// Private stdout capture.
    pub stdout_path: PathBuf,
    /// Private stderr capture.
    pub stderr_path: PathBuf,
}

/// Observed state of a launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchState {
    /// The recorded process is alive.
    Live,
    /// The process exited and wrote a terminal outcome.
    Exited(CampaignOutcome),
    /// The process exited without a parseable outcome; the stable code from stderr, if any.
    ExitedWithoutOutcome {
        /// First stderr line that looks like a stable code.
        code: Option<String>,
    },
}

/// Failure to launch or observe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchError {
    /// The process could not be spawned.
    Spawn,
    /// The process identity could not be read after spawning.
    Identity,
    /// Private state could not be written or read.
    State(StateError),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn => formatter.write_str("the coordinator process could not be started"),
            Self::Identity => {
                formatter.write_str("the coordinator process identity could not be read")
            }
            Self::State(error) => write!(formatter, "launch state failure: {error}"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// A launch owned by this interface process until it is dropped; dropping never kills it.
#[derive(Debug)]
pub struct OwnedLaunch {
    /// Recorded identity.
    pub record: LaunchRecord,
    child: Option<Child>,
}

impl OwnedLaunch {
    /// Reaps the child if it has exited so it does not linger as a zombie; never kills it.
    pub fn reap(&mut self) {
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(Some(_)))
        {
            self.child = None;
        }
    }
}

/// Starts `codingmage campaign` detached for one exact configuration and specification.
///
/// # Errors
///
/// Returns [`LaunchError`] when the process cannot be started or recorded.
pub fn launch_campaign(
    binary: &CoordinatorBinary,
    config_path: &Path,
    spec_path: &Path,
    campaign_id: &str,
    authority_sha256: &str,
    launch_dir: &Path,
) -> Result<OwnedLaunch, LaunchError> {
    ensure_private_dir(launch_dir).map_err(LaunchError::State)?;
    let launch_id = format!("{}-{}", now_ms(), std::process::id());
    let stdout_path = launch_dir.join(format!("{launch_id}.stdout"));
    let stderr_path = launch_dir.join(format!("{launch_id}.stderr"));
    let stdout = private_file(&stdout_path)?;
    let stderr = private_file(&stderr_path)?;
    let mut command = Command::new(binary.path());
    command
        .arg("campaign")
        .arg("--config")
        .arg(config_path)
        .arg("--campaign")
        .arg(spec_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .env_clear()
        .process_group(0);
    for name in [
        "HOME",
        "PATH",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let child = command.spawn().map_err(|_| LaunchError::Spawn)?;
    let pid = child.id();
    let start_ticks = process_start_ticks(pid).ok_or(LaunchError::Identity)?;
    let record = LaunchRecord {
        version: 1,
        launch_id,
        campaign_id: campaign_id.to_owned(),
        authority_sha256: authority_sha256.to_owned(),
        config_path: config_path.to_path_buf(),
        spec_path: spec_path.to_path_buf(),
        executable: binary.path().to_path_buf(),
        pid,
        start_ticks,
        launched_at_ms: now_ms(),
        stdout_path,
        stderr_path,
    };
    record.save(launch_dir).map_err(LaunchError::State)?;
    Ok(OwnedLaunch {
        record,
        child: Some(child),
    })
}

fn private_file(path: &Path) -> Result<fs::File, LaunchError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| LaunchError::Spawn)
}

impl LaunchRecord {
    fn path(launch_dir: &Path) -> PathBuf {
        launch_dir.join("current.json")
    }

    /// Persists this record as the current launch for the campaign.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when persistence fails.
    pub fn save(&self, launch_dir: &Path) -> Result<(), StateError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| StateError::Invalid)?;
        write_private(&Self::path(launch_dir), &bytes)
    }

    /// Loads the current launch record for a campaign, if any.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the document exists but is invalid.
    pub fn load(launch_dir: &Path) -> Result<Option<Self>, StateError> {
        let path = Self::path(launch_dir);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_private(&path)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
        if value.version != 1 || value.pid == 0 {
            return Err(StateError::Invalid);
        }
        Ok(Some(value))
    }

    /// Observes the launch without signalling it.
    #[must_use]
    pub fn observe(&self) -> LaunchState {
        if process_start_ticks(self.pid).is_some_and(|ticks| ticks == self.start_ticks)
            && !process_is_zombie(self.pid)
        {
            return LaunchState::Live;
        }
        match fs::read(&self.stdout_path) {
            Ok(bytes) if !bytes.trim_ascii().is_empty() => match parse_campaign_outcome(&bytes) {
                Ok(outcome) => LaunchState::Exited(outcome),
                Err(_) => LaunchState::ExitedWithoutOutcome {
                    code: stderr_code(&self.stderr_path),
                },
            },
            _ => LaunchState::ExitedWithoutOutcome {
                code: stderr_code(&self.stderr_path),
            },
        }
    }

    /// Last lines of the content-minimized progress stream.
    #[must_use]
    pub fn progress_tail(&self, lines: usize) -> Vec<String> {
        let Ok(text) = fs::read_to_string(&self.stderr_path) else {
            return Vec::new();
        };
        let all = text.lines().collect::<Vec<_>>();
        all.iter()
            .rev()
            .take(lines)
            .rev()
            .map(|line| (*line).to_owned())
            .collect()
    }
}

fn stderr_code(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("codingmage.") && !line.contains(' '))
        .map(str::to_owned)
}

/// Reads the kernel start time of a process from `/proc/<pid>/stat`.
#[must_use]
pub fn process_start_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    // Fields after the command name start at field 3 (state); starttime is field 22.
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

fn process_is_zombie(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().next().map(str::to_owned))
        })
        .is_some_and(|state| state == "Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_ticks_are_read_for_the_current_process_and_absent_for_none() {
        assert!(process_start_ticks(std::process::id()).is_some());
        assert_eq!(process_start_ticks(u32::MAX - 1), None);
    }

    #[test]
    fn exited_launch_reports_outcome_or_stable_code() {
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-launch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let record = LaunchRecord {
            version: 1,
            launch_id: "l".to_owned(),
            campaign_id: "c".to_owned(),
            authority_sha256: "a".repeat(64),
            config_path: root.join("config.toml"),
            spec_path: root.join("campaign.toml"),
            executable: root.join("codingmage"),
            pid: u32::MAX - 1,
            start_ticks: 1,
            launched_at_ms: 1,
            stdout_path: root.join("out"),
            stderr_path: root.join("err"),
        };
        fs::write(
            &record.stderr_path,
            "[codingmage +00:00] coordinator validating\ncodingmage.runtime.orchestration\n",
        )
        .unwrap();
        assert_eq!(
            record.observe(),
            LaunchState::ExitedWithoutOutcome {
                code: Some("codingmage.runtime.orchestration".to_owned())
            }
        );
        fs::write(
            &record.stdout_path,
            r#"{"campaign_id":"c","state":"paused","branch":"codingmage/c","head":"h","completed_units":1,"stop_reason":"operator_pause","last_task_id":null,"blocker_code":"codingmage.campaign.control.paused"}"#,
        )
        .unwrap();
        assert!(
            matches!(record.observe(), LaunchState::Exited(outcome) if outcome.completed_units == 1)
        );
        assert_eq!(
            record.progress_tail(1),
            vec!["codingmage.runtime.orchestration".to_owned()]
        );
        record.save(&root).unwrap();
        assert_eq!(LaunchRecord::load(&root).unwrap(), Some(record));
        fs::remove_dir_all(root).unwrap();
    }
}
