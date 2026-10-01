//! Linux directory-handle writer for reports and guided Setup documents.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read as _, Write as _},
    os::{
        fd::AsRawFd as _,
        unix::fs::{MetadataExt as _, OpenOptionsExt as _},
    },
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use crate::setup::{FieldError, WriteError};

/// Maximum bytes accepted by the isolated export process, including the destination.
pub(crate) const MAX_EXPORT_INPUT: usize = 32 * 1024 * 1024;

/// Immutable request transferred over the private child stdin pipe.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportRequest {
    pub(crate) report: crate::report::OutcomeReport,
    pub(crate) destination: PathBuf,
    pub(crate) repository: PathBuf,
    pub(crate) overwrite: bool,
}

/// Small result transferred over the private child stdout pipe.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportResult {
    pub(crate) error: Option<String>,
}

/// Executes one export in the isolated `codingmage-ui --report-export-helper` process.
/// This entry point is an internal process boundary, not an owner-facing command.
pub(crate) fn helper_main() -> std::process::ExitCode {
    let mut input = Vec::new();
    let result = std::io::stdin()
        .take((MAX_EXPORT_INPUT + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|_| "the report request could not be read".to_owned())
        .and_then(|_| {
            if input.len() > MAX_EXPORT_INPUT {
                return Err("the report request exceeds the export limit".to_owned());
            }
            let request: ExportRequest = serde_json::from_slice(&input)
                .map_err(|_| "the report request is malformed".to_owned())?;
            if request.report.schema_version != crate::report::REPORT_SCHEMA_VERSION {
                return Err("the report schema is unsupported".to_owned());
            }
            request
                .report
                .export(&request.destination, &request.repository, request.overwrite)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    let response = ExportResult {
        error: result.err(),
    };
    if serde_json::to_writer(std::io::stdout(), &response).is_err() {
        return std::process::ExitCode::from(2);
    }
    if response.error.is_some() {
        std::process::ExitCode::from(1)
    } else {
        std::process::ExitCode::SUCCESS
    }
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

struct Destination {
    directory: File,
    repository_directory: File,
    requested_parent: PathBuf,
    repository_path: PathBuf,
    repository: PathBuf,
    repository_identity: (u64, u64),
    leaf: std::ffi::OsString,
    requested_path: PathBuf,
}

impl Destination {
    fn open(path: &Path, repository: &Path) -> Result<Self, WriteError> {
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(field_error(
                "choose an absolute path without parent-directory components",
            ));
        }
        let requested_parent = path.parent().ok_or(WriteError::Io)?.to_path_buf();
        let leaf = path
            .file_name()
            .ok_or_else(|| field_error("choose a file name"))?;
        let resolved_parent = fs::canonicalize(&requested_parent).map_err(|_| {
            field_error("create an existing, readable parent directory before writing")
        })?;
        let resolved_repository = fs::canonicalize(repository).map_err(|_| WriteError::Io)?;
        let repository_directory = File::from(
            open(
                &resolved_repository,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| WriteError::Io)?,
        );
        let repository_metadata = repository_directory
            .metadata()
            .map_err(|_| WriteError::Io)?;
        if resolved_parent.starts_with(&resolved_repository) {
            return Err(WriteError::InsideRepository(path.to_path_buf()));
        }
        let directory = File::from(
            open(
                &resolved_parent,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| WriteError::Io)?,
        );
        let destination = Self {
            directory,
            repository_directory,
            requested_parent,
            repository_path: repository.to_path_buf(),
            repository: resolved_repository,
            repository_identity: (repository_metadata.dev(), repository_metadata.ino()),
            leaf: leaf.to_os_string(),
            requested_path: path.to_path_buf(),
        };
        destination.check_parent()?;
        Ok(destination)
    }

    fn fd_path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))
    }

    fn leaf_path(&self) -> PathBuf {
        self.fd_path().join(&self.leaf)
    }

    fn check_parent(&self) -> Result<(), WriteError> {
        let repository_path = File::open(&self.repository_path)
            .map_err(|_| field_error_at("repository", "target repository changed; reopen it"))?;
        let repository = repository_path
            .metadata()
            .map_err(|_| field_error_at("repository", "target repository changed; reopen it"))?;
        if (repository.dev(), repository.ino()) != self.repository_identity {
            return Err(field_error_at(
                "repository",
                "target repository changed; reopen it",
            ));
        }
        let repository_mount = mount_id(&self.repository_directory)?;
        if mount_id(&repository_path)? != repository_mount {
            return Err(field_error_at(
                "repository",
                "target repository mount changed; reopen it",
            ));
        }
        if mount_id(&self.directory)? != repository_mount {
            return Err(field_error(
                "choose a destination on the same mount as the target repository",
            ));
        }
        let repository_path =
            fs::canonicalize(&self.repository_path).map_err(|_| WriteError::Io)?;
        let held = self.directory.metadata().map_err(|_| WriteError::Io)?;
        let requested_parent = File::open(&self.requested_parent)
            .map_err(|_| field_error("destination directory changed; choose it again"))?;
        let requested = requested_parent
            .metadata()
            .map_err(|_| field_error("destination directory changed; choose it again"))?;
        if held.dev() != requested.dev() || held.ino() != requested.ino() {
            return Err(field_error(
                "destination directory changed; choose it again",
            ));
        }
        if mount_id(&requested_parent)? != repository_mount {
            return Err(field_error(
                "destination directory mount changed; choose it again",
            ));
        }
        let current = fs::canonicalize(self.fd_path()).map_err(|_| WriteError::Io)?;
        if current.starts_with(&self.repository) || current.starts_with(repository_path) {
            return Err(WriteError::InsideRepository(self.requested_path.clone()));
        }
        Ok(())
    }
}

fn mount_id(directory: &File) -> Result<u64, WriteError> {
    let fdinfo = fs::read_to_string(format!("/proc/self/fdinfo/{}", directory.as_raw_fd()))
        .map_err(|_| WriteError::Io)?;
    let mut ids = fdinfo
        .lines()
        .filter_map(|line| line.strip_prefix("mnt_id:"))
        .map(|value| value.trim().parse::<u64>().ok());
    match (ids.next(), ids.next()) {
        (Some(Some(id)), None) if id != 0 => Ok(id),
        _ => Err(WriteError::Io),
    }
}

fn field_error(message: &str) -> WriteError {
    field_error_at("destination", message)
}

fn field_error_at(field: &'static str, message: &str) -> WriteError {
    WriteError::Fields(vec![FieldError {
        field,
        message: message.to_owned(),
    }])
}

fn temporary_name() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    format!(
        ".codingmage-report-{}-{time}-{sequence}.candidate",
        std::process::id()
    )
}

/// Refuses an invalid destination before formatting the report. The writer repeats these
/// checks after formatting because the filesystem can change in between.
pub(super) fn validate_report_destination(
    path: &Path,
    repository: &Path,
) -> Result<(), WriteError> {
    Destination::open(path, repository).map(|_| ())
}

/// Writes bytes through a retained parent descriptor. Both hooks are empty in production and
/// permit deterministic parent and leaf changes in focused tests.
pub(super) fn write_report(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    before_write: impl FnOnce(),
    before_publish: impl FnOnce(),
) -> Result<PathBuf, WriteError> {
    write_validated_document(
        path,
        repository,
        bytes,
        overwrite,
        |_| Ok(()),
        before_write,
        before_publish,
    )
}

/// Publishes a local document only after its exact candidate bytes pass the existing loader.
/// The parent directory and repository identity stay bound to held descriptors throughout.
pub(crate) fn write_validated_document(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl FnOnce(&Path) -> Result<(), String>,
    before_write: impl FnOnce(),
    before_publish: impl FnOnce(),
) -> Result<PathBuf, WriteError> {
    let destination = Destination::open(path, repository)?;
    before_write();
    destination.check_parent()?;
    let temporary = destination.fd_path().join(temporary_name());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(&temporary).map_err(|_| WriteError::Io)?;
    let result = (|| {
        file.write_all(bytes).map_err(|_| WriteError::Io)?;
        file.sync_all().map_err(|_| WriteError::Io)?;
        validate(&temporary).map_err(WriteError::Rejected)?;
        let written = file.metadata().map_err(|_| WriteError::Io)?;
        destination.check_parent()?;
        let leaf = destination.leaf_path();
        match fs::symlink_metadata(&leaf) {
            Ok(metadata) if !overwrite || !metadata.is_file() => {
                return Err(WriteError::Exists(path.to_path_buf()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(WriteError::Io),
        }
        before_publish();
        destination.check_parent()?;
        if overwrite {
            // Rename replaces the directory entry, not the target of a raced leaf link.
            fs::rename(&temporary, &leaf).map_err(|_| WriteError::Io)?;
        } else {
            // Hard-link publication is atomic and fails if another writer created the leaf.
            fs::hard_link(&temporary, &leaf).map_err(|error| {
                if error.kind() == ErrorKind::AlreadyExists {
                    WriteError::Exists(path.to_path_buf())
                } else {
                    WriteError::Io
                }
            })?;
        }
        let named = fs::symlink_metadata(&leaf).map_err(|_| WriteError::Io)?;
        let parent_check = destination.check_parent();
        if !named.is_file()
            || named.dev() != written.dev()
            || named.ino() != written.ino()
            || parent_check.is_err()
        {
            if named.dev() == written.dev() && named.ino() == written.ino() {
                let _ = fs::remove_file(&leaf);
            }
            return parent_check.and(Err(WriteError::Io));
        }
        Ok(path.to_path_buf())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    fn root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codingmage-report-handle-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("repo")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        root
    }

    #[test]
    fn bind_mounted_repository_root_and_child_are_refused() {
        const CASE: &str = "CODINGMAGE_TEST_UI_REPORT_BIND_CASE";
        const ROOT: &str = "CODINGMAGE_TEST_UI_REPORT_BIND_ROOT";
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
                assert!(matches!(
                    write_report(
                        &alias.join("report.json"),
                        &root.join("repo"),
                        b"private",
                        overwrite,
                        || {},
                        || {}
                    ),
                    Err(WriteError::Fields(_))
                ));
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
                    "report_export::tests::bind_mounted_repository_root_and_child_are_refused",
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
    fn changed_requested_parent_is_refused_before_any_write() {
        let root = root("swapped-link");
        let alias = root.join("alias");
        symlink(root.join("outside"), &alias).unwrap();
        let result = write_report(
            &alias.join("report.json"),
            &root.join("repo"),
            b"private",
            false,
            || {
                fs::remove_file(&alias).unwrap();
                symlink(root.join("repo"), &alias).unwrap();
            },
            || {},
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert!(!root.join("repo/report.json").exists());
        assert!(!root.join("outside/report.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moved_open_parent_into_repository_is_refused() {
        let root = root("moved-parent");
        let outside = root.join("outside");
        let result = write_report(
            &outside.join("report.json"),
            &root.join("repo"),
            b"private",
            false,
            || fs::rename(&outside, root.join("repo/moved")).unwrap(),
            || {},
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert!(!root.join("repo/moved/report.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_parent_after_staging_is_refused_and_staged_bytes_are_removed() {
        let root = root("swap-before-publication");
        let alias = root.join("alias");
        symlink(root.join("outside"), &alias).unwrap();
        let result = write_report(
            &alias.join("report.json"),
            &root.join("repo"),
            b"private",
            false,
            || {},
            || {
                fs::remove_file(&alias).unwrap();
                symlink(root.join("repo"), &alias).unwrap();
            },
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert!(!root.join("repo/report.json").exists());
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replaced_target_identity_is_refused_before_any_write() {
        let root = root("replaced-target");
        let repository = root.join("repo");
        let path = root.join("outside/report.json");
        let result = write_report(
            &path,
            &repository,
            b"private",
            false,
            || {
                fs::rename(&repository, root.join("old-repo")).unwrap();
                fs::create_dir(&repository).unwrap();
            },
            || {},
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_regular_export_never_follows_a_leaf_symlink() {
        let root = root("leaf-link");
        let path = root.join("outside/report.json");
        let sensitive = root.join("repo/sensitive.json");
        fs::write(&sensitive, b"original").unwrap();
        symlink(&sensitive, &path).unwrap();
        assert!(matches!(
            write_report(&path, &root.join("repo"), b"private", true, || {}, || {}),
            Err(WriteError::Exists(_))
        ));
        assert_eq!(fs::read(sensitive).unwrap(), b"original");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_leaf_creation_is_not_overwritten_without_consent() {
        let root = root("new-leaf");
        let path = root.join("outside/report.json");
        let result = write_report(
            &path,
            &root.join("repo"),
            b"report",
            false,
            || {},
            || fs::write(&path, b"other writer").unwrap(),
        );
        assert_eq!(result, Err(WriteError::Exists(path.clone())));
        assert_eq!(fs::read(path).unwrap(), b"other writer");
        fs::remove_dir_all(root).unwrap();
    }
}
