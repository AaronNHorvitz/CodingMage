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
    fcntl::{AtFlags, OFlag, RenameFlags, open, renameat2},
    sys::stat::{Mode, mkdirat},
    unistd::{geteuid, linkat},
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
        check_stable_parent_chain(&resolved_parent)?;
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
        check_stable_parent_chain(&current)?;
        if current.starts_with(&self.repository) || current.starts_with(current_repository) {
            return Err(CliError::Refused);
        }
        Ok(())
    }
}

// Linux pathname publication is not atomic with a separate parent-name check.
// Admit only a chain whose directory entries cannot be renamed by another
// unprivileged UID. A sticky public parent protects an entry owned by this UID
// (or root); every other parent must deny group and other writes. This is an
// ownership boundary, not a defence against a hostile process with our UID.
fn check_stable_parent_chain(path: &Path) -> Result<(), CliError> {
    let uid = geteuid().as_raw();
    let mut prefix = PathBuf::from("/");
    let mut parent = fs::symlink_metadata(&prefix).map_err(|_| CliError::Refused)?;
    if !trusted_directory(&parent, uid) {
        return Err(CliError::Refused);
    }
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => prefix.push(name),
            _ => return Err(CliError::Refused),
        }
        let child = fs::symlink_metadata(&prefix).map_err(|_| CliError::Refused)?;
        if !trusted_directory(&child, uid)
            || (parent.mode() & 0o022 != 0 && parent.mode() & 0o1000 == 0)
        {
            return Err(CliError::Refused);
        }
        parent = child;
    }
    // The final directory itself owns the public leaf and the private stage.
    // Even a sticky directory permits another UID to rename a leaf it owns,
    // so require exclusive mutation authority here.
    if parent.mode() & 0o022 != 0 {
        return Err(CliError::Refused);
    }
    Ok(())
}

fn trusted_directory(metadata: &fs::Metadata, uid: u32) -> bool {
    metadata.is_dir() && (metadata.uid() == uid || metadata.uid() == 0)
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

fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn remove_if_same(path: &Path, expected: &fs::Metadata) {
    if fs::symlink_metadata(path).is_ok_and(|named| same_inode(&named, expected)) {
        let _ = fs::remove_file(path);
    }
}

fn private_stage_metadata(metadata: &fs::Metadata, effective_uid: u32) -> bool {
    metadata.is_dir() && metadata.uid() == effective_uid && metadata.mode() & 0o777 == 0o700
}

// A held, owner-private directory keeps the candidate and displaced file
// inaccessible to another principal that can write only the destination parent.
struct PrivateStage {
    directory: File,
    path: PathBuf,
    identity: fs::Metadata,
}

impl PrivateStage {
    fn create(
        destination: &Destination,
        before_open: impl FnOnce(PathBuf),
    ) -> Result<Self, CliError> {
        let name = format!("{}.staging", candidate_name());
        mkdirat(
            &destination.directory,
            name.as_str(),
            Mode::from_bits_truncate(0o700),
        )
        .map_err(|_| CliError::Refused)?;
        let path = destination.fd_path().join(name);
        before_open(path.clone());
        let directory = File::from(
            open(
                &path,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| CliError::Refused)?,
        );
        let identity = directory.metadata().map_err(|_| CliError::Refused)?;
        if !private_stage_metadata(&identity, geteuid().as_raw())
            || !fs::symlink_metadata(&path).is_ok_and(|named| same_inode(&named, &identity))
        {
            // The name may now belong to a different writer. Never remove it.
            return Err(CliError::Refused);
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

    fn remove_directory(&self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|named| same_inode(&named, &self.identity)) {
            let _ = fs::remove_dir(&self.path);
        }
    }

    fn exchange(&self, destination: &Destination) -> Result<(), CliError> {
        renameat2(
            &self.directory,
            "candidate",
            &destination.directory,
            Path::new(&destination.leaf),
            RenameFlags::RENAME_EXCHANGE,
        )
        .map_err(|_| CliError::Refused)
    }

    fn restore_if_owned(&self, destination: &Destination, written: &fs::Metadata) -> bool {
        let leaf = destination.leaf_path();
        if !fs::symlink_metadata(&leaf).is_ok_and(|named| same_inode(&named, written)) {
            return false;
        }
        self.exchange(destination).is_ok()
            && fs::symlink_metadata(self.file_path()).is_ok_and(|named| same_inode(&named, written))
    }
}

struct ExchangeEntries<'a> {
    previous: &'a fs::Metadata,
    candidate: &'a fs::Metadata,
}

fn publish_overwrite<B, A, C>(
    stage: &PrivateStage,
    destination: &Destination,
    entries: &ExchangeEntries<'_>,
    retain_stage: &mut bool,
    before_publish_call: B,
    publication_check: &mut C,
    after_publication: A,
) -> Result<(), CliError>
where
    B: FnOnce(),
    A: FnOnce(),
    C: FnMut() -> Result<(), CliError>,
{
    before_publish_call();
    destination.check()?;
    publication_check()?;
    stage.exchange(destination)?;
    after_publication();
    let temporary = stage.file_path();
    let Ok(displaced) = fs::symlink_metadata(&temporary) else {
        *retain_stage = true;
        return Err(CliError::UncertainWrite);
    };
    if !same_inode(&displaced, entries.previous) {
        if stage.restore_if_owned(destination, entries.candidate) {
            return Err(CliError::Refused);
        }
        *retain_stage = true;
        return Err(CliError::UncertainWrite);
    }
    let named = fs::symlink_metadata(destination.leaf_path()).ok();
    let parent_check = destination.check();
    if !named
        .as_ref()
        .is_some_and(|entry| entry.is_file() && same_inode(entry, entries.candidate))
        || parent_check.is_err()
    {
        if stage.restore_if_owned(destination, entries.candidate) {
            return parent_check.and(Err(CliError::Refused));
        }
        *retain_stage = true;
        return Err(CliError::UncertainWrite);
    }
    if publication_check().is_err() {
        *retain_stage = true;
        return Err(CliError::UncertainWrite);
    }
    fs::remove_file(&temporary).map_err(|_| {
        *retain_stage = true;
        CliError::UncertainWrite
    })
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
    write_validated(path, repository, bytes, overwrite, |_| Ok(()))
}

/// Publishes exact bytes after a caller-supplied loader accepts the private candidate.
pub(super) fn write_validated(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl FnOnce(&Path) -> Result<(), CliError>,
) -> Result<(), CliError> {
    write_with_injections(
        path,
        repository,
        bytes,
        overwrite,
        validate,
        Hooks {
            before_write: || {},
            before_publish: || {},
            before_stage_open: |_| {},
            before_publish_call: || {},
            after_publication: || {},
        },
    )
}

/// Publishes exact bytes only while a caller's authority holds at both publication checks.
pub(super) fn write_guarded(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    publication_check: impl FnMut() -> Result<(), CliError>,
) -> Result<(), CliError> {
    write_with_publication_check(
        path,
        repository,
        bytes,
        overwrite,
        |_| Ok(()),
        publication_check,
        Hooks {
            before_write: || {},
            before_publish: || {},
            before_stage_open: |_| {},
            before_publish_call: || {},
            after_publication: || {},
        },
    )
}

#[cfg(test)]
fn write_with_hooks(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    before_write: impl FnOnce(),
    before_publish: impl FnOnce(),
) -> Result<(), CliError> {
    write_with_injections(
        path,
        repository,
        bytes,
        overwrite,
        |_| Ok(()),
        Hooks {
            before_write,
            before_publish,
            before_stage_open: |_| {},
            before_publish_call: || {},
            after_publication: || {},
        },
    )
}

struct Hooks<W, P, S, B, A> {
    before_write: W,
    before_publish: P,
    before_stage_open: S,
    before_publish_call: B,
    after_publication: A,
}

fn write_with_injections<W, P, S, B, A>(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl FnOnce(&Path) -> Result<(), CliError>,
    hooks: Hooks<W, P, S, B, A>,
) -> Result<(), CliError>
where
    W: FnOnce(),
    P: FnOnce(),
    S: FnOnce(PathBuf),
    B: FnOnce(),
    A: FnOnce(),
{
    write_with_publication_check(
        path,
        repository,
        bytes,
        overwrite,
        validate,
        || Ok(()),
        hooks,
    )
}

fn write_with_publication_check<W, P, S, B, A, C>(
    path: &Path,
    repository: &Path,
    bytes: &[u8],
    overwrite: bool,
    validate: impl FnOnce(&Path) -> Result<(), CliError>,
    mut publication_check: C,
    hooks: Hooks<W, P, S, B, A>,
) -> Result<(), CliError>
where
    W: FnOnce(),
    P: FnOnce(),
    S: FnOnce(PathBuf),
    B: FnOnce(),
    A: FnOnce(),
    C: FnMut() -> Result<(), CliError>,
{
    let Hooks {
        before_write,
        before_publish,
        before_stage_open,
        before_publish_call,
        after_publication,
    } = hooks;
    let destination = Destination::open(path, repository)?;
    before_write();
    destination.check()?;
    let stage = PrivateStage::create(&destination, before_stage_open)?;
    let temporary = stage.file_path();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let Ok(mut file) = options.open(&temporary) else {
        stage.remove_directory();
        return Err(CliError::Refused);
    };
    let mut retain_stage = false;
    let result = (|| {
        file.write_all(bytes).map_err(|_| CliError::Internal)?;
        file.sync_all().map_err(|_| CliError::Internal)?;
        validate(&temporary)?;
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
        let candidate = fs::symlink_metadata(&temporary).map_err(|_| CliError::Refused)?;
        if !candidate.is_file()
            || !same_inode(&candidate, &written)
            || fs::read(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .map_err(|_| CliError::Refused)?
                != bytes
        {
            return Err(CliError::Refused);
        }
        let old = if overwrite {
            match fs::symlink_metadata(&leaf) {
                Ok(old) if old.is_file() => Some(old),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                _ => return Err(CliError::Refused),
            }
        } else {
            None
        };
        if let Some(old) = old.as_ref() {
            return publish_overwrite(
                &stage,
                &destination,
                &ExchangeEntries {
                    previous: old,
                    candidate: &written,
                },
                &mut retain_stage,
                before_publish_call,
                &mut publication_check,
                after_publication,
            );
        }
        before_publish_call();
        destination.check()?;
        publication_check()?;
        let source = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        linkat(&file, &source, &file, &leaf, AtFlags::AT_SYMLINK_FOLLOW)
            .map_err(|_| CliError::Refused)?;
        after_publication();
        let named = fs::symlink_metadata(&leaf).ok();
        let parent_check = destination.check();
        if !named
            .as_ref()
            .is_some_and(|entry| entry.is_file() && same_inode(entry, &written))
            || parent_check.is_err()
        {
            // A successful link may still have published the accepted bytes.
            // Preserve the private candidate for reconciliation and never unlink
            // a public name that another writer may now own.
            retain_stage = true;
            return Err(CliError::UncertainWrite);
        }
        if publication_check().is_err() {
            retain_stage = true;
            return Err(CliError::UncertainWrite);
        }
        Ok(())
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
    use codingmage_core::{
        CapabilityPolicy, Config, PublicationMode, PublicationPolicy, RepositoryAuthorization,
    };
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

    fn authorized_head_fixture(
        label: &str,
    ) -> (PathBuf, PathBuf, RepositoryAuthorization, PathBuf) {
        let root = root(label);
        let repository = root.join("repo");
        let source = root.join("source");
        let scratch = root.join("scratch");
        let state = root.join("state");
        for path in [&source, &scratch, &state] {
            fs::create_dir(path).unwrap();
        }
        let refs = repository.join(".git/refs/heads");
        fs::create_dir_all(&refs).unwrap();
        fs::write(repository.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let head = refs.join("main");
        fs::write(&head, "0123456789abcdef0123456789abcdef01234567\n").unwrap();
        let config = Config {
            version: 1,
            target_path: repository.clone(),
            task_source: PathBuf::from("TASKS.md"),
            default_branch: "main".to_owned(),
            integration_branch: "codingmage/integration".to_owned(),
            scratch_root: scratch,
            state_root: state,
            agent_profiles: Vec::new(),
            correction_limit: 3,
            gate_commands: Vec::new(),
            capabilities: CapabilityPolicy::default(),
            publication: PublicationPolicy {
                mode: PublicationMode::LocalOnly,
            },
            allow_parent_discovery: false,
        };
        let authority = RepositoryAuthorization::authorize(&config, &source).unwrap();
        (root, repository, authority, head)
    }

    #[test]
    fn changed_head_at_publication_never_reports_success() {
        for (overwrite, after_publication) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let label = match (overwrite, after_publication) {
                (false, false) => "head-create-before",
                (true, false) => "head-overwrite-before",
                (false, true) => "head-create-after",
                (true, true) => "head-overwrite-after",
            };
            let (root, repository, authority, head) = authorized_head_fixture(label);
            let destination = root.join("outside/authorization.txt");
            if overwrite {
                fs::write(&destination, b"old").unwrap();
            }
            let result = write_with_publication_check(
                &destination,
                &repository,
                b"new",
                overwrite,
                |_| Ok(()),
                || {
                    authority
                        .revalidate()
                        .map_err(|_| CliError::StaleObservation)
                },
                Hooks {
                    before_write: || {},
                    before_publish: || {},
                    before_stage_open: |_| {},
                    before_publish_call: || {
                        if !after_publication {
                            fs::write(&head, "abcdef0123456789abcdef0123456789abcdef01\n").unwrap();
                        }
                    },
                    after_publication: || {
                        if after_publication {
                            fs::write(&head, "abcdef0123456789abcdef0123456789abcdef01\n").unwrap();
                        } else {
                            panic!("stale authority must stop publication");
                        }
                    },
                },
            );
            if after_publication {
                assert_eq!(result, Err(CliError::UncertainWrite));
                assert_eq!(fs::read(&destination).unwrap(), b"new");
                let outside = root.join("outside");
                assert_eq!(fs::read_dir(&outside).unwrap().count(), 2);
                let stage = fs::read_dir(&outside)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|entry| entry.is_dir())
                    .unwrap();
                assert_eq!(
                    fs::read(stage.join("candidate")).unwrap(),
                    if overwrite {
                        b"old".as_slice()
                    } else {
                        b"new".as_slice()
                    }
                );
            } else {
                assert_eq!(result, Err(CliError::StaleObservation));
                if overwrite {
                    assert_eq!(fs::read(&destination).unwrap(), b"old");
                } else {
                    assert!(!destination.exists());
                }
                assert_eq!(
                    fs::read_dir(root.join("outside")).unwrap().count(),
                    usize::from(overwrite)
                );
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn stage_metadata_requires_effective_owner_and_private_mode() {
        let root = root("stage-metadata");
        let stage = root.join("outside/stage");
        fs::create_dir(&stage).unwrap();
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).unwrap();
        let metadata = fs::metadata(&stage).unwrap();
        assert!(private_stage_metadata(&metadata, geteuid().as_raw()));
        assert!(!private_stage_metadata(
            &metadata,
            geteuid().as_raw().wrapping_add(1)
        ));
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(!private_stage_metadata(
            &fs::metadata(&stage).unwrap(),
            geteuid().as_raw()
        ));
        fs::remove_dir_all(root).unwrap();
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
    fn renameable_parent_chain_is_refused_before_candidate_creation() {
        let root = root("renameable-chain");
        let outside = root.join("outside");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        let path = outside.join("report.json");
        assert_eq!(
            write(&path, &root.join("repo"), b"accepted", false),
            Err(CliError::Refused)
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(
            write(&path, &root.join("repo"), b"accepted", false),
            Err(CliError::Refused)
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parent_move_immediately_before_publication_call_is_refused() {
        for overwrite in [false, true] {
            let root = root(if overwrite {
                "move-exchange"
            } else {
                "move-link"
            });
            let outside = root.join("outside");
            let moved = root.join("moved-outside");
            let path = outside.join("report.json");
            if overwrite {
                fs::write(&path, b"old").unwrap();
            }
            let result = write_with_injections(
                &path,
                &root.join("repo"),
                b"accepted",
                overwrite,
                |_| Ok(()),
                Hooks {
                    before_write: || {},
                    before_publish: || {},
                    before_stage_open: |_| {},
                    before_publish_call: || {
                        fs::rename(&outside, &moved).unwrap();
                        fs::create_dir(&outside).unwrap();
                    },
                    after_publication: || panic!("moved parent must not receive a report"),
                },
            );
            assert_eq!(result, Err(CliError::Refused));
            assert!(!path.exists());
            if overwrite {
                assert_eq!(fs::read(moved.join("report.json")).unwrap(), b"old");
            } else {
                assert!(!moved.join("report.json").exists());
            }
            fs::remove_dir_all(root).unwrap();
        }
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

    #[test]
    fn substituted_stage_is_rejected_before_candidate_creation() {
        let root = root("substituted-stage");
        let path = root.join("outside/report.json");
        let retained = root.join("outside/retained-stage");
        fs::write(&path, b"old").unwrap();
        let result = write_with_injections(
            &path,
            &root.join("repo"),
            b"new",
            true,
            |_| panic!("a substituted stage must not reach validation"),
            Hooks {
                before_write: || {},
                before_publish: || panic!("a substituted stage must not reach publication"),
                before_stage_open: |stage: PathBuf| {
                    fs::rename(&stage, &retained).unwrap();
                    fs::create_dir(&stage).unwrap();
                    fs::set_permissions(&stage, fs::Permissions::from_mode(0o777)).unwrap();
                },
                before_publish_call: || {},
                after_publication: || {},
            },
        );
        assert_eq!(result, Err(CliError::Refused));
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(&retained).unwrap().count(), 0);
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replaced_validated_candidate_never_reaches_the_public_leaf() {
        let root = root("replaced-candidate");
        let path = root.join("outside/report.json");
        fs::write(&path, b"old").unwrap();
        let outside = root.join("outside");
        let result = write_with_injections(
            &path,
            &root.join("repo"),
            b"accepted",
            true,
            |candidate| {
                assert_eq!(fs::read(candidate).unwrap(), b"accepted");
                Ok(())
            },
            Hooks {
                before_write: || {},
                before_publish: || {
                    let stage = fs::read_dir(&outside)
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .find(|entry| entry.is_dir())
                        .unwrap();
                    fs::remove_file(stage.join("candidate")).unwrap();
                    fs::write(stage.join("candidate"), b"rejected").unwrap();
                },
                before_stage_open: |_| {},
                before_publish_call: || {},
                after_publication: || {},
            },
        );
        assert_eq!(result, Err(CliError::Refused));
        assert_eq!(fs::read(&path).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejected_candidate_is_not_published() {
        let root = root("validator-rejection");
        let path = root.join("outside/report.json");
        fs::write(&path, b"old").unwrap();
        let result = write_validated(&path, &root.join("repo"), b"invalid", true, |candidate| {
            assert_eq!(fs::read(candidate).unwrap(), b"invalid");
            Err(CliError::Config)
        });
        assert_eq!(result, Err(CliError::Config));
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overwrite_restores_a_concurrent_regular_replacement() {
        let root = root("overwrite-exchange");
        let path = root.join("outside/report.json");
        let moved = root.join("outside/moved-old.json");
        fs::write(&path, b"old").unwrap();
        let result = write_with_injections(
            &path,
            &root.join("repo"),
            b"accepted",
            true,
            |_| Ok(()),
            Hooks {
                before_write: || {},
                before_publish: || {},
                before_stage_open: |_| {},
                before_publish_call: || {
                    fs::rename(&path, &moved).unwrap();
                    fs::write(&path, b"concurrent").unwrap();
                },
                after_publication: || {},
            },
        );
        assert_eq!(result, Err(CliError::Refused));
        assert_eq!(fs::read(&path).unwrap(), b"concurrent");
        assert_eq!(fs::read(&moved).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn post_exchange_change_is_uncertain_and_retains_old_entry() {
        let root = root("overwrite-uncertain");
        let path = root.join("outside/report.json");
        let moved = root.join("outside/moved-written.json");
        let outside = root.join("outside");
        fs::write(&path, b"old").unwrap();
        let result = write_with_injections(
            &path,
            &root.join("repo"),
            b"accepted",
            true,
            |_| Ok(()),
            Hooks {
                before_write: || {},
                before_publish: || {},
                before_stage_open: |_| {},
                before_publish_call: || {},
                after_publication: || {
                    fs::rename(&path, &moved).unwrap();
                    fs::write(&path, b"later").unwrap();
                },
            },
        );
        assert_eq!(result, Err(CliError::UncertainWrite));
        assert_eq!(fs::read(&path).unwrap(), b"later");
        assert_eq!(fs::read(&moved).unwrap(), b"accepted");
        let stage = fs::read_dir(outside)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|entry| entry.is_dir())
            .unwrap();
        assert_eq!(fs::read(stage.join("candidate")).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn post_link_change_is_uncertain_and_retains_candidate() {
        let root = root("link-uncertain");
        let path = root.join("outside/report.json");
        let moved = root.join("outside/moved-written.json");
        let outside = root.join("outside");
        let result = write_with_injections(
            &path,
            &root.join("repo"),
            b"accepted",
            false,
            |_| Ok(()),
            Hooks {
                before_write: || {},
                before_publish: || {},
                before_stage_open: |_| {},
                before_publish_call: || {},
                after_publication: || {
                    fs::rename(&path, &moved).unwrap();
                    fs::write(&path, b"later").unwrap();
                },
            },
        );
        assert_eq!(result, Err(CliError::UncertainWrite));
        assert_eq!(fs::read(&path).unwrap(), b"later");
        assert_eq!(fs::read(&moved).unwrap(), b"accepted");
        let stage = fs::read_dir(outside)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|entry| entry.is_dir())
            .unwrap();
        assert_eq!(fs::read(stage.join("candidate")).unwrap(), b"accepted");
        fs::remove_dir_all(root).unwrap();
    }
}
