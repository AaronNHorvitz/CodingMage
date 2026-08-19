//! Exact lifecycle for coordinator-owned worktrees and manifests.

use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{RunId, TaskId, WorktreeId};
use codingmage_core::{Config, RepositoryAuthorization};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    command::{CommandError, GitCommand, path_from_bytes, run_git, run_git_with_codes, text},
    inventory::{InventoryError, OperationState, condition_at, inventory_repository},
};

static NEXT_WORKTREE: AtomicU64 = AtomicU64::new(1);

/// Lifecycle state retained in the ownership manifest.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeStatus {
    /// Worktree exists and may be assigned to one implementation agent.
    Active,
    /// Worktree was removed after exact identity and cleanliness checks.
    Removed,
}

/// Physical identity of the owned worktree directory.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeFilesystemIdentity {
    device: u64,
    inode: u64,
}

/// Durable, private ownership record for one worktree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeManifest {
    /// Manifest schema version.
    pub version: u16,
    /// Exact owned worktree identity.
    pub worktree_id: WorktreeId,
    /// Repository authorized for this worktree.
    pub repository_id: codingmage_contracts::RepositoryId,
    /// Run that created the worktree.
    pub run_id: RunId,
    /// Task assigned to the worktree.
    pub task_id: TaskId,
    /// Absolute destination under the configured scratch root.
    pub path: PathBuf,
    /// Physical directory identity captured after creation.
    pub filesystem: WorktreeFilesystemIdentity,
    /// Exact source commit.
    pub source_commit: String,
    /// Exact coordinator-owned branch.
    pub branch: String,
    /// Process that created the manifest.
    pub owner_process_id: u32,
    /// Current lifecycle status.
    pub status: WorktreeStatus,
}

/// Loaded worktree ownership authority.
#[derive(Clone, Debug)]
pub struct OwnedWorktree {
    manifest: WorktreeManifest,
    manifest_path: PathBuf,
}

impl OwnedWorktree {
    /// Returns the immutable ownership manifest.
    #[must_use]
    pub const fn manifest(&self) -> &WorktreeManifest {
        &self.manifest
    }

    /// Loads one exact manifest selected by a validated identifier.
    ///
    /// # Errors
    ///
    /// Returns [`WorktreeError`] if the manifest is absent, malformed, oversized, or does not match
    /// the selected identity.
    pub fn load(config: &Config, worktree_id: &WorktreeId) -> Result<Self, WorktreeError> {
        let manifest_path = manifest_path(config, worktree_id);
        let metadata = fs::symlink_metadata(&manifest_path).map_err(|_| WorktreeError::Manifest)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1_048_576 {
            return Err(WorktreeError::Manifest);
        }
        let content = fs::read_to_string(&manifest_path).map_err(|_| WorktreeError::Manifest)?;
        let manifest: WorktreeManifest =
            toml::from_str(&content).map_err(|_| WorktreeError::Manifest)?;
        if &manifest.worktree_id != worktree_id {
            return Err(WorktreeError::Manifest);
        }
        Ok(Self {
            manifest,
            manifest_path,
        })
    }
}

pub(crate) fn revalidate_active_worktree(
    authorization: &RepositoryAuthorization,
    owned: &OwnedWorktree,
    expected_head: &str,
) -> Result<(), WorktreeError> {
    authorization
        .revalidate()
        .map_err(|_| WorktreeError::StaleAuthorization)?;
    if owned.manifest.status != WorktreeStatus::Active
        || owned.manifest.repository_id != authorization.identity().repository_id
        || !valid_object_id(expected_head)
    {
        return Err(WorktreeError::Manifest);
    }
    let metadata =
        fs::symlink_metadata(&owned.manifest.path).map_err(|_| WorktreeError::Identity)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || filesystem_identity(&metadata) != owned.manifest.filesystem
        || !registered_worktree(
            &authorization.identity().canonical_path,
            &owned.manifest.path,
        )?
    {
        return Err(WorktreeError::Identity);
    }
    let head_output =
        run_git(&owned.manifest.path, GitCommand::Head).map_err(|_| WorktreeError::Identity)?;
    if text(&head_output)
        .map_err(|_| WorktreeError::Identity)?
        .trim()
        != expected_head
    {
        return Err(WorktreeError::Identity);
    }
    let ancestry = run_git_with_codes(
        &owned.manifest.path,
        GitCommand::IsAncestor {
            ancestor: &owned.manifest.source_commit,
            child: expected_head,
        },
        &[0, 1],
    )
    .map_err(|_| WorktreeError::Identity)?;
    let (branch, _) = condition_at(&owned.manifest.path).map_err(map_inventory)?;
    if ancestry.exit_code != 0 || branch.as_deref() != Some(owned.manifest.branch.as_str()) {
        return Err(WorktreeError::Identity);
    }
    Ok(())
}

/// Content-free worktree lifecycle failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorktreeError {
    /// Repository authorization became stale.
    StaleAuthorization,
    /// Source commit was invalid or unavailable.
    InvalidSource,
    /// Repository state is unsupported for checkout.
    UnsupportedRepository,
    /// Destination or branch already exists.
    Collision,
    /// Hardened Git operation failed.
    Command,
    /// Ownership manifest could not be created or validated.
    Manifest,
    /// Worktree path, registration, branch, or lineage changed.
    Identity,
    /// Owned worktree contains uncommitted or conflicted state.
    Dirty,
    /// Removal completed incompletely or could not be verified.
    RemovalUncertain,
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StaleAuthorization => "codingmage.git.stale_authorization",
            Self::InvalidSource => "codingmage.git.invalid_source",
            Self::UnsupportedRepository => "codingmage.git.unsupported_repository",
            Self::Collision => "codingmage.git.worktree_collision",
            Self::Command => "codingmage.git.command_failed",
            Self::Manifest => "codingmage.git.manifest_invalid",
            Self::Identity => "codingmage.git.worktree_identity",
            Self::Dirty => "codingmage.git.worktree_dirty",
            Self::RemovalUncertain => "codingmage.git.removal_uncertain",
        })
    }
}

impl std::error::Error for WorktreeError {}

/// Creates one exact branch and worktree from a validated source commit.
///
/// # Errors
///
/// Returns [`WorktreeError`] before adoption if repository state, source identity, destination,
/// command execution, postflight identity, or private manifest persistence fails.
pub fn create_owned_worktree(
    authorization: &RepositoryAuthorization,
    config: &Config,
    run_id: RunId,
    task_id: TaskId,
    source_commit: &str,
) -> Result<OwnedWorktree, WorktreeError> {
    authorization
        .revalidate()
        .map_err(|_| WorktreeError::StaleAuthorization)?;
    if !valid_object_id(source_commit) {
        return Err(WorktreeError::InvalidSource);
    }
    let inventory = inventory_repository(authorization).map_err(map_inventory)?;
    if inventory.operation != OperationState::None || inventory.unsafe_checkout_features {
        return Err(WorktreeError::UnsupportedRepository);
    }
    run_git(
        &authorization.identity().canonical_path,
        GitCommand::VerifyCommit(source_commit),
    )
    .map_err(|_| WorktreeError::InvalidSource)?;

    prepare_private_directory(&config.scratch_root)?;
    let manifest_root = config.state_root.join("worktrees");
    prepare_private_directory(&manifest_root)?;
    let worktree_id = generate_worktree_id(run_id.as_str(), task_id.as_str())?;
    let destination = config.scratch_root.join(worktree_id.as_str());
    if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
        return Err(WorktreeError::Collision);
    }
    let branch = format!(
        "{}/{}-{}-{}",
        config.integration_branch,
        task_id,
        run_id,
        &worktree_id.as_str()[3..]
    );
    if branch.len() > 255 {
        return Err(WorktreeError::Collision);
    }

    run_git(
        &authorization.identity().canonical_path,
        GitCommand::AddWorktree {
            destination: &destination,
            branch: &branch,
            source: source_commit,
        },
    )
    .map_err(|_| WorktreeError::Command)?;
    set_private_permissions(&destination)?;

    let head = run_git(&destination, GitCommand::Head).map_err(|_| WorktreeError::Identity)?;
    if text(&head).map_err(|_| WorktreeError::Identity)?.trim() != source_commit {
        return Err(WorktreeError::Identity);
    }
    let (observed_branch, condition) = condition_at(&destination).map_err(map_inventory)?;
    if observed_branch.as_deref() != Some(branch.as_str()) || !condition.is_clean() {
        return Err(WorktreeError::Identity);
    }
    if !registered_worktree(&authorization.identity().canonical_path, &destination)? {
        return Err(WorktreeError::Identity);
    }

    let manifest = WorktreeManifest {
        version: 1,
        worktree_id: worktree_id.clone(),
        repository_id: authorization.identity().repository_id.clone(),
        run_id,
        task_id,
        path: destination,
        filesystem: filesystem_identity(
            &fs::metadata(config.scratch_root.join(worktree_id.as_str()))
                .map_err(|_| WorktreeError::Identity)?,
        ),
        source_commit: source_commit.to_owned(),
        branch,
        owner_process_id: std::process::id(),
        status: WorktreeStatus::Active,
    };
    let manifest_path = manifest_path(config, &worktree_id);
    write_manifest(&manifest_path, &manifest, true)?;
    Ok(OwnedWorktree {
        manifest,
        manifest_path,
    })
}

/// Removes only a clean, registered worktree matching its exact private manifest.
///
/// # Errors
///
/// Returns [`WorktreeError`] without invoking removal when authorization, ownership, path,
/// registration, branch, lineage, or cleanliness differs from the manifest.
pub fn remove_owned_worktree(
    authorization: &RepositoryAuthorization,
    owned: &mut OwnedWorktree,
) -> Result<(), WorktreeError> {
    authorization
        .revalidate()
        .map_err(|_| WorktreeError::StaleAuthorization)?;
    if owned.manifest.status != WorktreeStatus::Active
        || owned.manifest.repository_id != authorization.identity().repository_id
    {
        return Err(WorktreeError::Manifest);
    }
    let metadata =
        fs::symlink_metadata(&owned.manifest.path).map_err(|_| WorktreeError::Identity)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || filesystem_identity(&metadata) != owned.manifest.filesystem
        || !registered_worktree(
            &authorization.identity().canonical_path,
            &owned.manifest.path,
        )?
    {
        return Err(WorktreeError::Identity);
    }

    let head_output =
        run_git(&owned.manifest.path, GitCommand::Head).map_err(|_| WorktreeError::Identity)?;
    let head = text(&head_output)
        .map_err(|_| WorktreeError::Identity)?
        .trim();
    if !valid_object_id(head) {
        return Err(WorktreeError::Identity);
    }
    let ancestry = run_git_with_codes(
        &owned.manifest.path,
        GitCommand::IsAncestor {
            ancestor: &owned.manifest.source_commit,
            child: head,
        },
        &[0, 1],
    )
    .map_err(|_| WorktreeError::Identity)?;
    if ancestry.exit_code != 0 {
        return Err(WorktreeError::Identity);
    }
    let (branch, condition) = condition_at(&owned.manifest.path).map_err(map_inventory)?;
    if branch.as_deref() != Some(owned.manifest.branch.as_str()) {
        return Err(WorktreeError::Identity);
    }
    if !condition.is_clean() {
        return Err(WorktreeError::Dirty);
    }

    run_git(
        &authorization.identity().canonical_path,
        GitCommand::RemoveWorktree {
            destination: &owned.manifest.path,
        },
    )
    .map_err(|_| WorktreeError::Command)?;
    if owned.manifest.path.exists()
        || registered_worktree(
            &authorization.identity().canonical_path,
            &owned.manifest.path,
        )?
    {
        return Err(WorktreeError::RemovalUncertain);
    }
    owned.manifest.status = WorktreeStatus::Removed;
    write_manifest(&owned.manifest_path, &owned.manifest, false)?;
    Ok(())
}

fn registered_worktree(repository: &Path, expected: &Path) -> Result<bool, WorktreeError> {
    let output = run_git(repository, GitCommand::Worktrees).map_err(|_| WorktreeError::Command)?;
    let expected = fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    for field in output.stdout.split(|byte| *byte == 0) {
        if let Some(raw_path) = field.strip_prefix(b"worktree ") {
            let path = path_from_bytes(raw_path);
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            if canonical == expected {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn generate_worktree_id(run: &str, task: &str) -> Result<WorktreeId, WorktreeError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorktreeError::Manifest)?
        .as_nanos();
    let counter = NEXT_WORKTREE.fetch_add(1, Ordering::Relaxed);
    let input = format!("{}:{now}:{counter}:{run}:{task}", std::process::id());
    let digest = Sha256::digest(input.as_bytes());
    let suffix = hex_bytes(&digest[..16]);
    WorktreeId::new(format!("wt-{suffix}")).map_err(|_| WorktreeError::Manifest)
}

fn manifest_path(config: &Config, worktree_id: &WorktreeId) -> PathBuf {
    config
        .state_root
        .join("worktrees")
        .join(format!("{}.toml", worktree_id.as_str()))
}

fn write_manifest(
    destination: &Path,
    manifest: &WorktreeManifest,
    create_new: bool,
) -> Result<(), WorktreeError> {
    let parent = destination.parent().ok_or(WorktreeError::Manifest)?;
    prepare_private_directory(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        manifest.worktree_id,
        NEXT_WORKTREE.fetch_add(1, Ordering::Relaxed)
    ));
    let content = toml::to_string(manifest).map_err(|_| WorktreeError::Manifest)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&temporary)
        .map_err(|_| WorktreeError::Manifest)?;
    set_file_private(&file)?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| WorktreeError::Manifest)?;
    if create_new && destination.exists() {
        let _ = fs::remove_file(&temporary);
        return Err(WorktreeError::Collision);
    }
    fs::rename(&temporary, destination).map_err(|_| WorktreeError::Manifest)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| WorktreeError::Manifest)?;
    Ok(())
}

fn prepare_private_directory(path: &Path) -> Result<(), WorktreeError> {
    fs::create_dir_all(path).map_err(|_| WorktreeError::Manifest)?;
    set_private_permissions(path)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), WorktreeError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| WorktreeError::Manifest)
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), WorktreeError> {
    Err(WorktreeError::UnsupportedRepository)
}

#[cfg(unix)]
fn set_file_private(file: &File) -> Result<(), WorktreeError> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| WorktreeError::Manifest)
}

#[cfg(not(unix))]
fn set_file_private(_file: &File) -> Result<(), WorktreeError> {
    Err(WorktreeError::UnsupportedRepository)
}

#[cfg(unix)]
fn filesystem_identity(metadata: &fs::Metadata) -> WorktreeFilesystemIdentity {
    use std::os::unix::fs::MetadataExt;

    WorktreeFilesystemIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(not(unix))]
fn filesystem_identity(_metadata: &fs::Metadata) -> WorktreeFilesystemIdentity {
    WorktreeFilesystemIdentity {
        device: 0,
        inode: 0,
    }
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn map_inventory(error: InventoryError) -> WorktreeError {
    match error {
        InventoryError::StaleAuthorization => WorktreeError::StaleAuthorization,
        InventoryError::UnsupportedState => WorktreeError::UnsupportedRepository,
        InventoryError::Command
        | InventoryError::InvalidOutput
        | InventoryError::PreservationMismatch => WorktreeError::Command,
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

impl From<CommandError> for WorktreeError {
    fn from(_: CommandError) -> Self {
        Self::Command
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::thread;

    use codingmage_contracts::{RunId, TaskId};

    use super::*;
    use crate::test_support::{GitFixture, output, run};

    fn create(fixture: &GitFixture) -> (RepositoryAuthorization, Config, OwnedWorktree) {
        let authorization = fixture.authorization();
        let config = fixture.config();
        let owned = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-1").unwrap(),
            TaskId::new("task-1").unwrap(),
            &fixture.head(),
        )
        .unwrap();
        (authorization, config, owned)
    }

    #[test]
    fn lifecycle_preserves_active_checkout_and_unrelated_git_state() {
        let fixture = GitFixture::new();
        run(&fixture.target, &["tag", "user-tag"]);
        run(&fixture.target, &["notes", "add", "-m", "user-note"]);
        fs::write(fixture.target.join("tracked-one.txt"), "stash-change\n").unwrap();
        run(&fixture.target, &["stash", "push", "-m", "user-stash"]);
        fs::write(fixture.target.join("active-untracked.txt"), "keep\n").unwrap();

        let status_before = fixture.status();
        let index_before = fs::read(fixture.target.join(".git/index")).unwrap();
        let file_before = fs::read(fixture.target.join("active-untracked.txt")).unwrap();
        let notes_before = output(&fixture.target, &["rev-parse", "refs/notes/commits"]);
        let stash_before = output(&fixture.target, &["rev-parse", "refs/stash"]);
        let tag_before = output(&fixture.target, &["rev-parse", "refs/tags/user-tag"]);
        let similar = fixture.scratch.join("wt-similar-user-owned");
        fs::create_dir(&similar).unwrap();
        fs::write(similar.join("keep.txt"), "keep\n").unwrap();

        let (authorization, config, owned) = create(&fixture);
        assert_eq!(owned.manifest.status, WorktreeStatus::Active);
        assert!(owned.manifest.path.starts_with(&fixture.scratch));
        assert_eq!(
            fs::metadata(&owned.manifest.path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let expected_manifest = owned.manifest.clone();
        let worktree_id = owned.manifest.worktree_id.clone();
        drop(owned);
        let mut owned = OwnedWorktree::load(&config, &worktree_id).unwrap();
        assert_eq!(owned.manifest, expected_manifest);

        remove_owned_worktree(&authorization, &mut owned).unwrap();
        assert_eq!(owned.manifest.status, WorktreeStatus::Removed);
        assert!(!owned.manifest.path.exists());
        assert!(similar.join("keep.txt").is_file());
        let owned_ref = format!("refs/heads/{}", owned.manifest.branch);
        assert!(
            output(&fixture.target, &["show-ref", "--verify", &owned_ref])
                .starts_with(&fixture.head())
        );
        let manifest_file = config
            .state_root
            .join("worktrees")
            .join(format!("{}.toml", owned.manifest.worktree_id));
        assert_eq!(
            fs::metadata(manifest_file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fixture.status(), status_before);
        assert_eq!(
            fs::read(fixture.target.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(
            fs::read(fixture.target.join("active-untracked.txt")).unwrap(),
            file_before
        );
        assert_eq!(
            output(&fixture.target, &["rev-parse", "refs/notes/commits"]),
            notes_before
        );
        assert_eq!(
            output(&fixture.target, &["rev-parse", "refs/stash"]),
            stash_before
        );
        assert_eq!(
            output(&fixture.target, &["rev-parse", "refs/tags/user-tag"]),
            tag_before
        );
    }

    #[test]
    fn dirty_owned_worktree_is_not_removed() {
        let fixture = GitFixture::new();
        let (authorization, _config, mut owned) = create(&fixture);
        fs::write(owned.manifest.path.join("uncommitted.txt"), "keep\n").unwrap();
        assert_eq!(
            remove_owned_worktree(&authorization, &mut owned).unwrap_err(),
            WorktreeError::Dirty
        );
        assert!(owned.manifest.path.join("uncommitted.txt").is_file());
    }

    #[test]
    fn renamed_or_replaced_worktree_fails_closed() {
        let fixture = GitFixture::new();
        let (authorization, _config, mut owned) = create(&fixture);
        let renamed = fixture.scratch.join("renamed-by-user");
        fs::rename(&owned.manifest.path, &renamed).unwrap();
        fs::create_dir(&owned.manifest.path).unwrap();
        assert_eq!(
            remove_owned_worktree(&authorization, &mut owned).unwrap_err(),
            WorktreeError::Identity
        );
        assert!(renamed.is_dir());
    }

    #[test]
    fn hostile_checkout_features_refuse_creation() {
        let fixture = GitFixture::new();
        fs::write(fixture.target.join(".gitattributes"), "* filter=hostile\n").unwrap();
        let result = create_owned_worktree(
            &fixture.authorization(),
            &fixture.config(),
            RunId::new("run-1").unwrap(),
            TaskId::new("task-1").unwrap(),
            &fixture.head(),
        );
        assert_eq!(result.unwrap_err(), WorktreeError::UnsupportedRepository);
        assert!(fs::read_dir(&fixture.scratch).unwrap().next().is_none());
    }

    #[test]
    fn malformed_source_is_rejected_before_git() {
        let fixture = GitFixture::new();
        let result = create_owned_worktree(
            &fixture.authorization(),
            &fixture.config(),
            RunId::new("run-1").unwrap(),
            TaskId::new("task-1").unwrap(),
            "HEAD;touch-side-effect",
        );
        assert_eq!(result.unwrap_err(), WorktreeError::InvalidSource);
    }

    #[test]
    fn concurrent_user_file_activity_survives_lifecycle() {
        let fixture = GitFixture::new();
        let active_file = fixture.target.join("concurrent-user-edit.txt");
        let running = Arc::new(AtomicBool::new(true));
        let iterations = Arc::new(AtomicU64::new(0));
        let writer_running = Arc::clone(&running);
        let writer_iterations = Arc::clone(&iterations);
        let writer = thread::spawn(move || {
            while writer_running.load(Ordering::Acquire) {
                let value = writer_iterations.fetch_add(1, Ordering::AcqRel);
                fs::write(&active_file, format!("user-edit-{value}\n")).unwrap();
                thread::yield_now();
            }
            active_file
        });
        while iterations.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }

        let (authorization, _config, mut owned) = create(&fixture);
        remove_owned_worktree(&authorization, &mut owned).unwrap();
        running.store(false, Ordering::Release);
        let active_file = writer.join().unwrap();

        assert!(iterations.load(Ordering::Acquire) > 1);
        let content = fs::read_to_string(active_file).unwrap();
        assert!(content.starts_with("user-edit-"));
    }
}
