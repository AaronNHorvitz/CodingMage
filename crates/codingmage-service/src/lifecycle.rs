use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use codingmage_contracts::RepositoryId;
use serde::{Deserialize, Serialize};

/// Explicit resource limits for the unprivileged coordinator service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceLimits {
    /// Maximum resident memory bytes.
    pub memory_bytes: u64,
    /// Maximum service tasks.
    pub tasks: u32,
    /// CPU quota percent in the range 1 through 1000.
    pub cpu_percent: u16,
}

/// Validated user-service inputs.
#[derive(Clone, Debug)]
pub struct ServiceSpec {
    executable: PathBuf,
    configuration: PathBuf,
    state_root: PathBuf,
    scratch_root: PathBuf,
    limits: ServiceLimits,
}

impl ServiceSpec {
    /// Validates absolute, existing, nonsymlink service paths and resource limits.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidSpec`] for unsafe paths or limits.
    pub fn new(
        executable: &Path,
        configuration: &Path,
        state_root: &Path,
        scratch_root: &Path,
        limits: ServiceLimits,
    ) -> Result<Self, LifecycleError> {
        let executable = validate_file(executable)?;
        let configuration = validate_file(configuration)?;
        let state_root = validate_directory(state_root)?;
        let scratch_root = validate_directory(scratch_root)?;
        if limits.memory_bytes < 64 * 1024 * 1024
            || limits.tasks < 4
            || !(1..=1000).contains(&limits.cpu_percent)
        {
            return Err(LifecycleError::InvalidSpec);
        }
        Ok(Self {
            executable,
            configuration,
            state_root,
            scratch_root,
            limits,
        })
    }

    /// Renders a deterministic `systemd --user` unit without enabling lingering or root access.
    #[must_use]
    pub fn render_unit(&self) -> String {
        format!(
            "[Unit]\nDescription=CodingMage local development coordinator\nAfter=default.target\n\n[Service]\nType=simple\nExecStart={} run --config {}\nRestart=on-failure\nRestartSec=10s\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nProtectHome=read-only\nReadWritePaths={} {}\nMemoryMax={}\nTasksMax={}\nCPUQuota={}%\n\n[Install]\nWantedBy=default.target\n",
            systemd_argument(&self.executable),
            systemd_argument(&self.configuration),
            systemd_argument(&self.state_root),
            systemd_argument(&self.scratch_root),
            self.limits.memory_bytes,
            self.limits.tasks,
            self.limits.cpu_percent,
        )
    }

    /// Creates a side-effect-free lifecycle preview for an operator command.
    #[must_use]
    pub fn plan(&self, action: ServiceAction, unit_root: &Path) -> ServicePlan {
        let unit_path = unit_root.join("codingmage.service");
        let steps = match action {
            ServiceAction::Install => vec![
                "write-user-unit",
                "systemctl-user-daemon-reload",
                "systemctl-user-start",
            ],
            ServiceAction::Verify => vec!["systemd-analyze-user-verify"],
            ServiceAction::Start => vec!["systemctl-user-start"],
            ServiceAction::Stop => vec!["systemctl-user-stop"],
            ServiceAction::Uninstall => vec![
                "systemctl-user-stop",
                "remove-user-unit",
                "systemctl-user-daemon-reload",
            ],
        };
        ServicePlan {
            action,
            unit_path,
            unit_contents: (action == ServiceAction::Install).then(|| self.render_unit()),
            steps,
            requires_root: false,
            enables_lingering: false,
            enables_at_boot: false,
        }
    }

    /// Atomically installs only the `CodingMage` user-unit file without enabling it.
    ///
    /// # Errors
    ///
    /// Returns a path, symlink, drift, or durable I/O error.
    pub fn install_unit(&self, unit_root: &Path) -> Result<PathBuf, LifecycleError> {
        prepare_unit_root(unit_root)?;
        let current = unit_root.join("codingmage.service");
        reject_symlink_if_present(&current)?;
        let temporary = unit_root.join("codingmage.service.tmp");
        if temporary.exists() {
            return Err(LifecycleError::Drift);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| LifecycleError::Io)?;
        file.write_all(self.render_unit().as_bytes())
            .map_err(|_| LifecycleError::Io)?;
        file.sync_all().map_err(|_| LifecycleError::Io)?;
        fs::rename(&temporary, &current).map_err(|_| LifecycleError::Io)?;
        sync_directory(unit_root)?;
        Ok(current)
    }

    /// Verifies the exact installed unit bytes.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::Drift`] when the exact unit is missing or changed.
    pub fn verify_installed(&self, unit_root: &Path) -> Result<(), LifecycleError> {
        let current = unit_root.join("codingmage.service");
        reject_symlink_if_present(&current)?;
        let contents = fs::read_to_string(current).map_err(|_| LifecycleError::Drift)?;
        if contents != self.render_unit() {
            return Err(LifecycleError::Drift);
        }
        Ok(())
    }

    /// Removes only an unchanged `CodingMage` user unit; missing is idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::Drift`] instead of deleting a changed or symlinked unit.
    pub fn uninstall_unit(&self, unit_root: &Path) -> Result<(), LifecycleError> {
        let current = unit_root.join("codingmage.service");
        if !current.exists() {
            return Ok(());
        }
        self.verify_installed(unit_root)?;
        fs::remove_file(current).map_err(|_| LifecycleError::Io)?;
        sync_directory(unit_root)
    }
}

/// Supported explicit user-service lifecycle operations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    /// Write but do not enable the user unit, then start it.
    Install,
    /// Verify unit syntax and referenced paths.
    Verify,
    /// Start the existing user unit.
    Start,
    /// Stop the existing user unit.
    Stop,
    /// Stop and remove only the exact user unit.
    Uninstall,
}

/// Operator-visible preview with no executable shell text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePlan {
    /// Requested action.
    pub action: ServiceAction,
    /// Exact user-unit path.
    pub unit_path: PathBuf,
    /// Unit bytes only when installation writes them.
    pub unit_contents: Option<String>,
    /// Stable allowlisted step identifiers.
    pub steps: Vec<&'static str>,
    /// Always false for this user service.
    pub requires_root: bool,
    /// Always false; host lingering is outside authority.
    pub enables_lingering: bool,
    /// Always false; install does not implicitly enable the unit.
    pub enables_at_boot: bool,
}

/// Kernel-owned single-coordinator lock for one exact repository.
#[derive(Debug)]
pub struct CoordinatorLock {
    file: File,
}

impl CoordinatorLock {
    /// Acquires one nonblocking lock scoped to the target repository identity.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::AlreadyOwned`] when another live process owns the target.
    pub fn acquire(
        lock_root: &Path,
        repository_id: &RepositoryId,
        owner_identity: &str,
    ) -> Result<Self, LifecycleError> {
        validate_owner(owner_identity)?;
        fs::create_dir_all(lock_root).map_err(|_| LifecycleError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(lock_root, fs::Permissions::from_mode(0o700))
                .map_err(|_| LifecycleError::Io)?;
        }
        let path = lock_root.join(format!("{}.lock", repository_id.as_str()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| LifecycleError::Io)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => LifecycleError::AlreadyOwned,
            std::fs::TryLockError::Error(_) => LifecycleError::Io,
        })?;
        file.set_len(0).map_err(|_| LifecycleError::Io)?;
        file.write_all(owner_identity.as_bytes())
            .map_err(|_| LifecycleError::Io)?;
        file.sync_all().map_err(|_| LifecycleError::Io)?;
        Ok(Self { file })
    }
}

impl Drop for CoordinatorLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.file);
    }
}

fn validate_file(path: &Path) -> Result<PathBuf, LifecycleError> {
    validate_path_text(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| LifecycleError::InvalidSpec)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(LifecycleError::InvalidSpec);
    }
    fs::canonicalize(path).map_err(|_| LifecycleError::InvalidSpec)
}

fn validate_directory(path: &Path) -> Result<PathBuf, LifecycleError> {
    validate_path_text(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| LifecycleError::InvalidSpec)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LifecycleError::InvalidSpec);
    }
    fs::canonicalize(path).map_err(|_| LifecycleError::InvalidSpec)
}

fn validate_path_text(path: &Path) -> Result<(), LifecycleError> {
    let text = path.to_str().ok_or(LifecycleError::InvalidSpec)?;
    if !path.is_absolute() || text.contains(['\n', '\r', '\0']) {
        return Err(LifecycleError::InvalidSpec);
    }
    Ok(())
}

fn validate_owner(value: &str) -> Result<(), LifecycleError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(LifecycleError::InvalidSpec);
    }
    Ok(())
}

fn systemd_argument(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{value}\"")
}

fn prepare_unit_root(path: &Path) -> Result<(), LifecycleError> {
    if !path.is_absolute() {
        return Err(LifecycleError::InvalidSpec);
    }
    fs::create_dir_all(path).map_err(|_| LifecycleError::Io)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| LifecycleError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LifecycleError::InvalidSpec);
    }
    Ok(())
}

fn reject_symlink_if_present(path: &Path) -> Result<(), LifecycleError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(LifecycleError::Drift)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LifecycleError::Io),
    }
}

fn sync_directory(path: &Path) -> Result<(), LifecycleError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| LifecycleError::Io)
}

/// User-service configuration or ownership failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleError {
    /// Service path, resource limit, or owner identity is unsafe.
    InvalidSpec,
    /// Another live process owns the exact repository.
    AlreadyOwned,
    /// Local filesystem operation failed.
    Io,
    /// Existing unit or temporary state differs from CodingMage-owned bytes.
    Drift,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSpec => "codingmage.service.invalid_spec",
            Self::AlreadyOwned => "codingmage.service.already_owned",
            Self::Io => "codingmage.service.io",
            Self::Drift => "codingmage.service.drift",
        })
    }
}

impl std::error::Error for LifecycleError {}
