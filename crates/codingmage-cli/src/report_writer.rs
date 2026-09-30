//! Guarded Linux file publication for an explicitly requested CLI outcome report.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write as _},
    os::{
        fd::AsRawFd as _,
        unix::fs::{MetadataExt as _, OpenOptionsExt as _},
    },
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use crate::CliError;

static NEXT_CANDIDATE: AtomicU64 = AtomicU64::new(0);

struct Destination {
    directory: File,
    repository_directory: File,
    requested_parent: PathBuf,
    repository_path: PathBuf,
    repository: PathBuf,
    repository_identity: (u64, u64),
    leaf: std::ffi::OsString,
}

impl Destination {
    fn open(path: &Path, repository: &Path) -> Result<Self, CliError> {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CliError::InvalidArgument);
        }
        let requested_parent = path.parent().ok_or(CliError::InvalidArgument)?;
        let leaf = path.file_name().ok_or(CliError::InvalidArgument)?;
        let resolved_parent = fs::canonicalize(requested_parent).map_err(|_| CliError::Refused)?;
        let resolved_repository = fs::canonicalize(repository).map_err(|_| CliError::Repository)?;
        let repository_directory = File::from(
            open(
                &resolved_repository,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| CliError::Repository)?,
        );
        let repository_metadata = repository_directory
            .metadata()
            .map_err(|_| CliError::Repository)?;
        if resolved_parent.starts_with(&resolved_repository) {
            return Err(CliError::Refused);
        }
        let directory = File::from(
            open(
                &resolved_parent,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| CliError::Refused)?,
        );
        let destination = Self {
            directory,
            repository_directory,
            requested_parent: requested_parent.to_path_buf(),
            repository_path: repository.to_path_buf(),
            repository: resolved_repository,
            repository_identity: (repository_metadata.dev(), repository_metadata.ino()),
            leaf: leaf.to_os_string(),
        };
        destination.check()?;
        Ok(destination)
    }

    fn fd_path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))
    }

    fn leaf_path(&self) -> PathBuf {
        self.fd_path().join(&self.leaf)
    }

    fn check(&self) -> Result<(), CliError> {
        let repository_path = File::open(&self.repository_path).map_err(|_| CliError::Refused)?;
        let repository_metadata = repository_path.metadata().map_err(|_| CliError::Refused)?;
        if (repository_metadata.dev(), repository_metadata.ino()) != self.repository_identity {
            return Err(CliError::Refused);
        }
        let repository_mount = mount_id(&self.repository_directory)?;
        if mount_id(&repository_path)? != repository_mount
            || mount_id(&self.directory)? != repository_mount
        {
            // The same directory can be reached through a bind mount with a different
            // pathname. Restrict exports to the repository root's mount; this also
            // refuses aliases of nested mounts inside the repository.
            return Err(CliError::Refused);
        }
        let current_repository =
            fs::canonicalize(&self.repository_path).map_err(|_| CliError::Refused)?;
        let held = self.directory.metadata().map_err(|_| CliError::Refused)?;
        let requested_parent = File::open(&self.requested_parent).map_err(|_| CliError::Refused)?;
        let requested = requested_parent.metadata().map_err(|_| CliError::Refused)?;
        if held.dev() != requested.dev() || held.ino() != requested.ino() {
            return Err(CliError::Refused);
        }
        if mount_id(&requested_parent)? != repository_mount {
            return Err(CliError::Refused);
        }
        let current = fs::canonicalize(self.fd_path()).map_err(|_| CliError::Refused)?;
        if current.starts_with(&self.repository) || current.starts_with(current_repository) {
            return Err(CliError::Refused);
        }
        Ok(())
    }
}

fn mount_id(directory: &File) -> Result<u64, CliError> {
    let fdinfo = fs::read_to_string(format!("/proc/self/fdinfo/{}", directory.as_raw_fd()))
        .map_err(|_| CliError::Refused)?;
    let mut ids = fdinfo
        .lines()
        .filter_map(|line| line.strip_prefix("mnt_id:"))
        .map(|value| value.trim().parse::<u64>().ok());
    match (ids.next(), ids.next()) {
        (Some(Some(id)), None) if id != 0 => Ok(id),
        _ => Err(CliError::Refused),
    }
}

fn candidate_name() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_CANDIDATE.fetch_add(1, Ordering::Relaxed);
    format!(
        ".codingmage-cli-report-{}-{time}-{sequence}.candidate",
        std::process::id()
    )
}

/// Validates the requested destination before expensive source reads.
pub(super) fn validate(path: &Path, repository: &Path) -> Result<(), CliError> {
    Destination::open(path, repository).map(|_| ())
}

/// Writes an already assembled report through a checked parent descriptor.
pub(super) fn write(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), CliError> {
    write_with_hooks(path, repository, bytes, overwrite, || {}, || {})
}

fn write_with_hooks(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    before_write: impl FnOnce(),
    before_publish: impl FnOnce(),
) -> Result<(), CliError> {
    let destination = Destination::open(path, repository)?;
    before_write();
    destination.check()?;
    let temporary = destination.fd_path().join(candidate_name());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temporary).map_err(|_| CliError::Refused)?;
    let result = (|| {
        file.write_all(bytes).map_err(|_| CliError::Internal)?;
        file.sync_all().map_err(|_| CliError::Internal)?;
        let written = file.metadata().map_err(|_| CliError::Internal)?;
        destination.check()?;
        let leaf = destination.leaf_path();
        match fs::symlink_metadata(&leaf) {
            Ok(metadata) if !overwrite || !metadata.is_file() => return Err(CliError::Refused),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(CliError::Refused),
        }
        before_publish();
        destination.check()?;
        if overwrite {
            fs::rename(&temporary, &leaf).map_err(|_| CliError::Refused)?;
        } else {
            fs::hard_link(&temporary, &leaf).map_err(|_| CliError::Refused)?;
        }
        let named = fs::symlink_metadata(&leaf).map_err(|_| CliError::Refused)?;
        let parent_check = destination.check();
        if !named.is_file()
            || named.dev() != written.dev()
            || named.ino() != written.ino()
            || parent_check.is_err()
        {
            if named.dev() == written.dev() && named.ino() == written.ino() {
                let _ = fs::remove_file(&leaf);
            }
            return parent_check.and(Err(CliError::Refused));
        }
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{PermissionsExt as _, symlink},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    fn root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codingmage-cli-report-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("repo")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        root
    }

    #[test]
    fn linked_parent_into_repository_is_refused() {
        let root = root("linked-parent");
        symlink(root.join("repo"), root.join("alias")).unwrap();
        let path = root.join("alias/report.json");
        assert_eq!(
            write(&path, &root.join("repo"), b"report", false),
            Err(CliError::Refused)
        );
        assert!(!root.join("repo/report.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bind_mounted_repository_root_and_child_are_refused() {
        const CASE: &str = "CODINGMAGE_TEST_REPORT_BIND_CASE";
        const ROOT: &str = "CODINGMAGE_TEST_REPORT_BIND_ROOT";
        if let Some(case) = std::env::var_os(CASE) {
            let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
            let source = match case.to_str() {
                Some("root") => root.join("repo"),
                Some("child") => root.join("repo/child"),
                _ => panic!("unknown bind test case"),
            };
            let alias = root.join("alias");
            assert!(
                Command::new("mount")
                    .arg("--bind")
                    .arg(&source)
                    .arg(&alias)
                    .status()
                    .unwrap()
                    .success()
            );
            assert_ne!(
                mount_id(&File::open(root.join("repo")).unwrap()).unwrap(),
                mount_id(&File::open(&alias).unwrap()).unwrap()
            );
            for overwrite in [false, true] {
                assert_eq!(
                    write(
                        &alias.join("report.json"),
                        &root.join("repo"),
                        b"report",
                        overwrite
                    ),
                    Err(CliError::Refused)
                );
                assert!(!source.join("report.json").exists());
            }
            return;
        }
        for case in ["root", "child"] {
            let root = root(&format!("bind-{case}"));
            fs::create_dir(root.join("repo/child")).unwrap();
            fs::create_dir(root.join("alias")).unwrap();
            let mut child = Command::new("unshare")
                .args(["-Urnm", "--"])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "report_writer::tests::bind_mounted_repository_root_and_child_are_refused",
                    "--nocapture",
                ])
                .env(CASE, case)
                .env(ROOT, &root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            loop {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                if start.elapsed() >= Duration::from_secs(15) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("bind-mount test child timed out");
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "bind case {case} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("1 passed"),
                "bind case {case} did not run the child assertion"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn changed_parent_and_competing_leaf_are_refused() {
        let root = root("changed-parent");
        symlink(root.join("outside"), root.join("alias")).unwrap();
        let path = root.join("alias/report.json");
        let result = write_with_hooks(
            &path,
            &root.join("repo"),
            b"report",
            false,
            || {
                fs::remove_file(root.join("alias")).unwrap();
                symlink(root.join("repo"), root.join("alias")).unwrap();
            },
            || {},
        );
        assert_eq!(result, Err(CliError::Refused));
        assert!(!root.join("repo/report.json").exists());
        fs::remove_file(root.join("alias")).unwrap();
        symlink(root.join("outside"), root.join("alias")).unwrap();
        let path = root.join("outside/report.json");
        let result = write_with_hooks(
            &path,
            &root.join("repo"),
            b"report",
            false,
            || {},
            || {
                fs::write(&path, b"other writer").unwrap();
            },
        );
        assert_eq!(result, Err(CliError::Refused));
        assert_eq!(fs::read(path).unwrap(), b"other writer");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moved_open_parent_is_refused_before_publication() {
        let root = root("moved-parent");
        let outside = root.join("outside");
        let path = outside.join("report.json");
        let result = write_with_hooks(
            &path,
            &root.join("repo"),
            b"report",
            false,
            || {},
            || {
                fs::rename(&outside, root.join("repo/moved")).unwrap();
            },
        );
        assert_eq!(result, Err(CliError::Refused));
        assert!(!root.join("repo/moved/report.json").exists());
        assert_eq!(fs::read_dir(root.join("repo/moved")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overwrite_replaces_only_a_regular_leaf() {
        let root = root("overwrite");
        let path = root.join("outside/report.json");
        let sensitive = root.join("repo/sensitive.json");
        fs::write(&sensitive, b"original").unwrap();
        symlink(&sensitive, &path).unwrap();
        assert_eq!(
            write(&path, &root.join("repo"), b"report", true),
            Err(CliError::Refused)
        );
        assert_eq!(fs::read(&sensitive).unwrap(), b"original");
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"old").unwrap();
        write(&path, &root.join("repo"), b"report", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"report");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        fs::remove_dir_all(root).unwrap();
    }
}
