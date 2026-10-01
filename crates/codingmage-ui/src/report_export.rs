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
    fcntl::{AtFlags, OFlag, RenameFlags, open, renameat2},
    sys::stat::{Mode, mkdirat},
    unistd::{geteuid, linkat},
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

    fn recovery_leaf_path(&self) -> Option<PathBuf> {
        fs::canonicalize(self.fd_path())
            .ok()
            .map(|directory| directory.join(&self.leaf))
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

fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn remove_if_same(path: &Path, expected: &fs::Metadata) {
    if fs::symlink_metadata(path).is_ok_and(|current| same_inode(&current, expected)) {
        let _ = fs::remove_file(path);
    }
}

// The candidate and the file displaced by an overwrite live in a directory
// inaccessible to other users. A held directory handle keeps the exchange
// source bound even when a writable parent is changed concurrently.
struct PrivateStage {
    directory: File,
    path: PathBuf,
    identity: fs::Metadata,
}

impl PrivateStage {
    fn create(
        destination: &Destination,
        before_open: impl FnOnce(PathBuf),
    ) -> Result<Self, WriteError> {
        let name = format!("{}.staging", temporary_name());
        mkdirat(
            &destination.directory,
            name.as_str(),
            Mode::from_bits_truncate(0o700),
        )
        .map_err(|_| WriteError::Io)?;
        let path = destination.fd_path().join(name);
        before_open(path.clone());
        let directory = File::from(
            open(
                &path,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| {
                field_error("private staging directory changed; inspect the destination and retry")
            })?,
        );
        let identity = directory.metadata().map_err(|_| WriteError::Io)?;
        // mkdirat does not return a handle. A parent-directory writer can
        // substitute its name before open, so verify the held object before
        // creating any candidate inside it. Later parent renames cannot give
        // that writer access to a directory owned by us with mode 0700.
        if !private_stage_metadata(&identity, geteuid().as_raw())
            || !fs::symlink_metadata(&path).is_ok_and(|named| same_inode(&named, &identity))
        {
            return Err(field_error(
                "private staging directory changed; inspect the destination and retry",
            ));
        }
        Ok(Self {
            directory,
            path,
            identity,
        })
    }

    fn file_path(&self) -> PathBuf {
        PathBuf::from(format!(
            "/proc/self/fd/{}/candidate",
            self.directory.as_raw_fd()
        ))
    }

    fn recovery_path(&self) -> Option<PathBuf> {
        fs::canonicalize(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))
            .ok()
            .map(|directory| directory.join("candidate"))
    }

    fn remove_directory(&self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|named| same_inode(&named, &self.identity)) {
            let _ = fs::remove_dir(&self.path);
        }
    }

    fn publish_over_existing(
        &self,
        destination: &Destination,
        written: &fs::Metadata,
        old: &fs::Metadata,
        retain_stage: &mut bool,
        before_exchange: impl FnOnce(),
        after_exchange: impl FnOnce(),
    ) -> Result<PathBuf, WriteError> {
        let path = &destination.requested_path;
        let leaf = destination.leaf_path();
        let temporary = self.file_path();
        before_exchange();
        destination.check_parent()?;
        // Exchange is atomic: no check-then-unlink can remove a different
        // writer's leaf. The displaced entry stays inside our private stage.
        exchange_with_destination(self, destination)?;
        after_exchange();
        let Ok(displaced) = fs::symlink_metadata(&temporary) else {
            *retain_stage = true;
            return Err(WriteError::Uncertain {
                destination: path.clone(),
                retained: None,
            });
        };
        if !same_inode(&displaced, old) {
            if restore_displaced_entry(self, destination, written) {
                return Err(field_error(
                    "destination changed during overwrite; inspect and retry",
                ));
            }
            *retain_stage = true;
            return Err(WriteError::Uncertain {
                destination: path.clone(),
                retained: self.recovery_path(),
            });
        }
        let named = fs::symlink_metadata(&leaf).ok();
        let parent_check = destination.check_parent();
        if !named
            .as_ref()
            .is_some_and(|entry| entry.is_file() && same_inode(entry, written))
            || parent_check.is_err()
        {
            if restore_displaced_entry(self, destination, written) {
                return parent_check.and(Err(WriteError::Io));
            }
            *retain_stage = true;
            return Err(WriteError::Uncertain {
                destination: path.clone(),
                retained: self.recovery_path(),
            });
        }
        // Other users cannot replace this entry inside the 0700 stage.
        fs::remove_file(&temporary).map_err(|_| {
            *retain_stage = true;
            WriteError::Uncertain {
                destination: path.clone(),
                retained: self.recovery_path(),
            }
        })?;
        Ok(path.clone())
    }
}

fn private_stage_metadata(metadata: &fs::Metadata, effective_uid: u32) -> bool {
    metadata.is_dir() && metadata.uid() == effective_uid && metadata.mode() & 0o777 == 0o700
}

fn exchange_with_destination(
    stage: &PrivateStage,
    destination: &Destination,
) -> Result<(), WriteError> {
    renameat2(
        &stage.directory,
        "candidate",
        &destination.directory,
        Path::new(&destination.leaf),
        RenameFlags::RENAME_EXCHANGE,
    )
    .map_err(|_| WriteError::Io)
}

fn restore_displaced_entry(
    stage: &PrivateStage,
    destination: &Destination,
    written: &fs::Metadata,
) -> bool {
    let leaf = destination.leaf_path();
    if !fs::symlink_metadata(&leaf).is_ok_and(|named| same_inode(&named, written)) {
        return false;
    }
    if exchange_with_destination(stage, destination).is_err() {
        return false;
    }
    fs::symlink_metadata(stage.file_path()).is_ok_and(|named| same_inode(&named, written))
}

fn link_file_descriptor(
    file: &File,
    destination: &Path,
    requested_path: &Path,
) -> Result<(), WriteError> {
    let source = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
    linkat(file, &source, file, destination, AtFlags::AT_SYMLINK_FOLLOW).map_err(|error| {
        if error == nix::errno::Errno::EEXIST {
            WriteError::Exists(requested_path.to_path_buf())
        } else {
            WriteError::Io
        }
    })
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

struct WriteHooks<W, P, B, A, S> {
    before_write: W,
    before_publish: P,
    before_exchange: B,
    after_exchange: A,
    before_stage_open: S,
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
    write_validated_document_with_exchange_hook(
        path,
        repository,
        bytes,
        overwrite,
        validate,
        WriteHooks {
            before_write,
            before_publish,
            before_exchange: || {},
            after_exchange: || {},
            before_stage_open: |_| {},
        },
    )
}

fn write_validated_document_with_exchange_hook<W, P, B, A, S>(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl FnOnce(&Path) -> Result<(), String>,
    hooks: WriteHooks<W, P, B, A, S>,
) -> Result<PathBuf, WriteError>
where
    W: FnOnce(),
    P: FnOnce(),
    B: FnOnce(),
    A: FnOnce(),
    S: FnOnce(PathBuf),
{
    let WriteHooks {
        before_write,
        before_publish,
        before_exchange,
        after_exchange,
        before_stage_open,
    } = hooks;
    let destination = Destination::open(path, repository)?;
    before_write();
    destination.check_parent()?;
    let stage = PrivateStage::create(&destination, before_stage_open)?;
    let temporary = stage.file_path();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let Ok(mut file) = options.open(&temporary) else {
        stage.remove_directory();
        return Err(WriteError::Io);
    };
    let mut retain_stage = false;
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
        // The named candidate is used only by the existing loaders. Publication uses
        // the retained descriptor, so replacing its directory entry cannot publish
        // bytes that the loader never accepted.
        let current = fs::symlink_metadata(&temporary).map_err(|_| WriteError::Io)?;
        if !current.is_file()
            || !same_inode(&current, &written)
            || fs::read(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .map_err(|_| WriteError::Io)?
                != bytes
        {
            return Err(WriteError::Io);
        }
        let old = if overwrite {
            match fs::symlink_metadata(&leaf) {
                Ok(old) if old.is_file() => Some(old),
                Ok(_) => return Err(WriteError::Exists(path.to_path_buf())),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                Err(_) => return Err(WriteError::Io),
            }
        } else {
            None
        };
        if let Some(old) = old.as_ref() {
            return stage.publish_over_existing(
                &destination,
                &written,
                old,
                &mut retain_stage,
                before_exchange,
                after_exchange,
            );
        }
        link_file_descriptor(&file, &leaf, path)?;
        let named = fs::symlink_metadata(&leaf).ok();
        let parent_check = destination.check_parent();
        if !named
            .as_ref()
            .is_some_and(|entry| entry.is_file() && same_inode(entry, &written))
            || parent_check.is_err()
        {
            // An unlink here would race another directory writer. Leave the
            // destination for explicit reconciliation when our link is visible.
            if named
                .as_ref()
                .is_some_and(|entry| same_inode(entry, &written))
            {
                return Err(WriteError::Uncertain {
                    destination: path.to_path_buf(),
                    retained: destination.recovery_leaf_path(),
                });
            }
            return parent_check.and(Err(WriteError::Io));
        }
        Ok(path.to_path_buf())
    })();
    if !retain_stage {
        if let Ok(written) = file.metadata() {
            remove_if_same(&temporary, &written);
        }
        stage.remove_directory();
    }
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

    #[test]
    fn replaced_validated_candidate_never_reaches_destination() {
        for replacement in ["regular", "symlink", "modified"] {
            for overwrite in [false, true] {
                let root = root(&format!("candidate-{replacement}-{overwrite}"));
                let destination = root.join("outside/document.toml");
                if overwrite {
                    fs::write(&destination, b"previous document").unwrap();
                }
                let outside = root.join("outside");
                let result = write_validated_document(
                    &destination,
                    &root.join("repo"),
                    b"accepted candidate",
                    overwrite,
                    |path| {
                        if fs::read(path).unwrap() == b"accepted candidate" {
                            Ok(())
                        } else {
                            Err("rejected candidate".to_owned())
                        }
                    },
                    || {},
                    || {
                        let staged = fs::read_dir(&outside)
                            .unwrap()
                            .map(|entry| entry.unwrap().path())
                            .find(|path| path.is_dir())
                            .unwrap()
                            .join("candidate");
                        if replacement == "modified" {
                            fs::write(&staged, b"rejected replacement").unwrap();
                        } else {
                            fs::remove_file(&staged).unwrap();
                            if replacement == "regular" {
                                fs::write(&staged, b"rejected replacement").unwrap();
                            } else {
                                symlink(root.join("repo"), &staged).unwrap();
                            }
                        }
                    },
                );
                assert_eq!(result, Err(WriteError::Io));
                if overwrite {
                    assert_eq!(fs::read(&destination).unwrap(), b"previous document");
                } else {
                    assert!(!destination.exists());
                }
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn descriptor_bound_overwrite_replaces_valid_regular_file() {
        let root = root("descriptor-overwrite");
        let path = root.join("outside/report.json");
        fs::write(&path, b"previous").unwrap();
        assert_eq!(
            write_report(&path, &root.join("repo"), b"accepted", true, || {}, || {}),
            Ok(path.clone())
        );
        assert_eq!(fs::read(&path).unwrap(), b"accepted");
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_stage_requires_effective_owner_and_private_mode() {
        let root = root("stage-metadata");
        let stage = root.join("outside/stage");
        fs::create_dir(&stage).unwrap();
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).unwrap();
        let metadata = fs::metadata(&stage).unwrap();
        let writer = geteuid().as_raw();
        assert!(private_stage_metadata(&metadata, writer));
        assert!(!private_stage_metadata(&metadata, writer.wrapping_add(1)));
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(!private_stage_metadata(
            &fs::metadata(&stage).unwrap(),
            writer
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn substituted_open_stage_is_rejected_before_candidate_creation() {
        let root = root("substituted-stage");
        let path = root.join("outside/report.json");
        let retained = root.join("outside/retained-stage");
        fs::write(&path, b"old document").unwrap();
        let result = write_validated_document_with_exchange_hook(
            &path,
            &root.join("repo"),
            b"validated document",
            true,
            |_| panic!("candidate validation must not run in a substituted stage"),
            WriteHooks {
                before_write: || {},
                before_publish: || panic!("publication must not run in a substituted stage"),
                before_exchange: || {},
                after_exchange: || {},
                before_stage_open: |stage: PathBuf| {
                    fs::rename(&stage, &retained).unwrap();
                    fs::create_dir(&stage).unwrap();
                    fs::set_permissions(&stage, fs::Permissions::from_mode(0o777)).unwrap();
                },
            },
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert_eq!(fs::read(&path).unwrap(), b"old document");
        assert_eq!(fs::read_dir(&retained).unwrap().count(), 0);
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_exchange_preserves_a_concurrent_regular_replacement() {
        let root = root("overwrite-exchange-race");
        let path = root.join("outside/report.json");
        let moved_old = root.join("outside/moved-old.json");
        fs::write(&path, b"original").unwrap();
        let result = write_validated_document_with_exchange_hook(
            &path,
            &root.join("repo"),
            b"validated",
            true,
            |_| Ok(()),
            WriteHooks {
                before_write: || {},
                before_publish: || {},
                before_exchange: || {
                    fs::rename(&path, &moved_old).unwrap();
                    fs::write(&path, b"concurrent replacement").unwrap();
                },
                after_exchange: || {},
                before_stage_open: |_| {},
            },
        );
        assert!(matches!(result, Err(WriteError::Fields(_))));
        assert_eq!(fs::read(&path).unwrap(), b"concurrent replacement");
        assert_eq!(fs::read(&moved_old).unwrap(), b"original");
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uncertain_exchange_retains_displaced_file_after_second_leaf_change() {
        let root = root("overwrite-exchange-uncertain");
        let path = root.join("outside/report.json");
        let moved_validated = root.join("outside/moved-validated.json");
        fs::write(&path, b"original").unwrap();
        let result = write_validated_document_with_exchange_hook(
            &path,
            &root.join("repo"),
            b"validated",
            true,
            |_| Ok(()),
            WriteHooks {
                before_write: || {},
                before_publish: || {},
                before_exchange: || {},
                after_exchange: || {
                    fs::rename(&path, &moved_validated).unwrap();
                    fs::write(&path, b"later writer").unwrap();
                },
                before_stage_open: |_| {},
            },
        );
        let Err(WriteError::Uncertain {
            destination,
            retained: Some(retained),
        }) = result
        else {
            panic!("expected a recoverable uncertainty result");
        };
        assert_eq!(destination, path);
        assert_eq!(fs::read(&path).unwrap(), b"later writer");
        assert_eq!(fs::read(&moved_validated).unwrap(), b"validated");
        assert_eq!(fs::read(&retained).unwrap(), b"original");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_displaced_entry_never_triggers_a_public_leaf_unlink() {
        let root = root("overwrite-missing-displaced");
        let path = root.join("outside/report.json");
        let outside = root.join("outside");
        fs::write(&path, b"original").unwrap();
        let result = write_validated_document_with_exchange_hook(
            &path,
            &root.join("repo"),
            b"validated",
            true,
            |_| Ok(()),
            WriteHooks {
                before_write: || {},
                before_publish: || {},
                before_exchange: || {},
                after_exchange: || {
                    let stage = fs::read_dir(&outside)
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .find(|entry| entry.is_dir())
                        .unwrap();
                    fs::remove_file(stage.join("candidate")).unwrap();
                },
                before_stage_open: |_| {},
            },
        );
        assert_eq!(
            result,
            Err(WriteError::Uncertain {
                destination: path.clone(),
                retained: None,
            })
        );
        assert_eq!(fs::read(path).unwrap(), b"validated");
        fs::remove_dir_all(root).unwrap();
    }
}
