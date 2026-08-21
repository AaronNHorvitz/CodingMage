//! Coordinator-owned staging and commit creation for one bounded implementation result.

use std::{collections::BTreeSet, fmt, path::PathBuf};

use codingmage_core::RepositoryAuthorization;

use crate::{
    command::{GitCommand, path_from_bytes, run_git, text},
    inventory::condition_at,
    worktree::{OwnedWorktree, revalidate_active_worktree},
};

/// Immutable result of one coordinator-owned commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    /// Exact parent observed before staging.
    pub parent: String,
    /// Exact commit observed after the commit operation.
    pub commit: String,
    /// Sorted changed paths accepted by the ownership policy.
    pub changed_paths: Vec<PathBuf>,
    /// Digest of bounded Git commit standard output.
    pub stdout_sha256: String,
    /// Digest of bounded Git commit standard error.
    pub stderr_sha256: String,
}

/// Content-free controlled-commit failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitError {
    /// Worktree, authorization, branch, or expected parent changed.
    Identity,
    /// The worktree has no changed path to commit.
    Empty,
    /// A changed or declared path is malformed or outside packet ownership.
    PathAuthority,
    /// Repository operation state is unsafe or changed during commit.
    RepositoryState,
    /// A hardened literal Git command failed.
    Command,
}

impl fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.git.commit.identity",
            Self::Empty => "codingmage.git.commit.empty",
            Self::PathAuthority => "codingmage.git.commit.path_authority",
            Self::RepositoryState => "codingmage.git.commit.repository_state",
            Self::Command => "codingmage.git.commit.command",
        })
    }
}

impl std::error::Error for CommitError {}

/// Revalidates an owned worktree, stages only packet-owned changes, and creates one exact commit.
///
/// The commit author, message, hook policy, signer policy, executable, environment, and argument
/// vectors are coordinator-owned constants. Provider output cannot supply Git arguments.
///
/// # Errors
///
/// Returns [`CommitError`] before staging when identity, repository state, changed paths, or owned
/// paths disagree. A Git failure is reported without retrying the state-changing operation.
pub fn commit_owned_changes(
    authorization: &RepositoryAuthorization,
    owned: &OwnedWorktree,
    expected_parent: &str,
    owned_paths: &[PathBuf],
) -> Result<CommitReceipt, CommitError> {
    commit_owned_changes_inner(authorization, owned, expected_parent, owned_paths, || {})
}

/// Reobserves a possibly completed coordinator commit without creating or changing Git state.
///
/// The current worktree head must be a clean direct child of `expected_parent`, and every changed
/// path must remain inside the original packet authority. Synthetic stream digests distinguish a
/// reobserved receipt from one returned directly by [`commit_owned_changes`].
///
/// # Errors
///
/// Returns [`CommitError`] when any identity, lineage, cleanliness, or path check fails.
pub fn reobserve_owned_commit(
    authorization: &RepositoryAuthorization,
    owned: &OwnedWorktree,
    expected_parent: &str,
    owned_paths: &[PathBuf],
) -> Result<CommitReceipt, CommitError> {
    let head_output =
        run_git(&owned.manifest().path, GitCommand::Head).map_err(|_| CommitError::Command)?;
    let head = text(&head_output)
        .map_err(|_| CommitError::Command)?
        .trim()
        .to_owned();
    if head == expected_parent {
        return Err(CommitError::Empty);
    }
    let (_, condition) = condition_at(&owned.manifest().path).map_err(|_| CommitError::Command)?;
    if !condition.is_clean() {
        return Err(CommitError::RepositoryState);
    }
    observe_owned_child_commit(authorization, owned, expected_parent, &head, owned_paths)
}

/// Observes one exact commit as a clean direct child within the original path authority.
///
/// Unlike [`reobserve_owned_commit`], `commit` need not be the current head. The active owned
/// worktree is still revalidated at its current head before the immutable child is inspected.
///
/// # Errors
///
/// Returns [`CommitError`] for stale worktree identity, non-direct lineage, or path escape.
pub fn observe_owned_child_commit(
    authorization: &RepositoryAuthorization,
    owned: &OwnedWorktree,
    expected_parent: &str,
    commit: &str,
    owned_paths: &[PathBuf],
) -> Result<CommitReceipt, CommitError> {
    let head_output =
        run_git(&owned.manifest().path, GitCommand::Head).map_err(|_| CommitError::Command)?;
    let head = text(&head_output).map_err(|_| CommitError::Command)?.trim();
    revalidate_active_worktree(authorization, owned, head).map_err(|_| CommitError::Identity)?;
    let parent_output = run_git(&owned.manifest().path, GitCommand::Parent(commit))
        .map_err(|_| CommitError::Identity)?;
    let parent = text(&parent_output).map_err(|_| CommitError::Identity)?;
    if parent.trim() != expected_parent {
        return Err(CommitError::Identity);
    }
    let metadata = run_git(&owned.manifest().path, GitCommand::CommitMetadata(commit))
        .map_err(|_| CommitError::Identity)?;
    let fields = metadata
        .stdout
        .split(|byte| *byte == 0)
        .take(3)
        .collect::<Vec<_>>();
    let expected_message = format!("codingmage: complete {}", owned.manifest().task_id);
    if fields
        != [
            b"CodingMage Coordinator".as_slice(),
            b"codingmage@localhost.invalid".as_slice(),
            expected_message.as_bytes(),
        ]
    {
        return Err(CommitError::Identity);
    }
    let output = run_git(
        &owned.manifest().path,
        GitCommand::DiffPaths {
            base: expected_parent,
            target: commit,
        },
    )
    .map_err(|_| CommitError::Command)?;
    let mut changed_paths = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(path_from_bytes)
        .collect::<Vec<_>>();
    changed_paths.sort();
    changed_paths.dedup();
    if changed_paths.is_empty()
        || changed_paths.iter().any(|changed| {
            !safe_relative(changed)
                || !owned_paths
                    .iter()
                    .any(|owned| path_is_owned(changed, owned))
        })
    {
        return Err(CommitError::PathAuthority);
    }
    Ok(CommitReceipt {
        parent: expected_parent.to_owned(),
        commit: commit.to_owned(),
        changed_paths,
        stdout_sha256: output.stdout_sha256,
        stderr_sha256: output.stderr_sha256,
    })
}

fn commit_owned_changes_inner(
    authorization: &RepositoryAuthorization,
    owned: &OwnedWorktree,
    expected_parent: &str,
    owned_paths: &[PathBuf],
    after_preflight: impl FnOnce(),
) -> Result<CommitReceipt, CommitError> {
    revalidate_active_worktree(authorization, owned, expected_parent)
        .map_err(|_| CommitError::Identity)?;
    if owned_paths.is_empty() || owned_paths.iter().any(|path| !safe_relative(path)) {
        return Err(CommitError::PathAuthority);
    }
    let (_, before) = condition_at(&owned.manifest().path).map_err(|_| CommitError::Command)?;
    if before.conflicted {
        return Err(CommitError::RepositoryState);
    }

    let changed_paths = changed_paths(&owned.manifest().path)?;
    if changed_paths.is_empty() {
        return Err(CommitError::Empty);
    }
    if changed_paths.iter().any(|changed| {
        !owned_paths
            .iter()
            .any(|owned| path_is_owned(changed, owned))
    }) {
        return Err(CommitError::PathAuthority);
    }
    after_preflight();

    let paths: Vec<PathBuf> = changed_paths.iter().cloned().collect();
    run_git(
        &owned.manifest().path,
        GitCommand::StagePaths { paths: &paths },
    )
    .map_err(|_| CommitError::Command)?;
    revalidate_active_worktree(authorization, owned, expected_parent)
        .map_err(|_| CommitError::Identity)?;
    let message = format!("codingmage: complete {}", owned.manifest().task_id);
    let output = run_git(
        &owned.manifest().path,
        GitCommand::Commit { message: &message },
    )
    .map_err(|_| CommitError::Command)?;

    let head =
        run_git(&owned.manifest().path, GitCommand::Head).map_err(|_| CommitError::Command)?;
    let commit = text(&head)
        .map_err(|_| CommitError::Command)?
        .trim()
        .to_owned();
    if commit == expected_parent {
        return Err(CommitError::Identity);
    }
    let (_, after) = condition_at(&owned.manifest().path).map_err(|_| CommitError::Command)?;
    if !after.is_clean() {
        return Err(CommitError::RepositoryState);
    }
    let parent_check = run_git(&owned.manifest().path, GitCommand::Parent(&commit))
        .map_err(|_| CommitError::Identity)?;
    if text(&parent_check)
        .map_err(|_| CommitError::Identity)?
        .trim()
        != expected_parent
    {
        return Err(CommitError::Identity);
    }

    Ok(CommitReceipt {
        parent: expected_parent.to_owned(),
        commit,
        changed_paths: paths,
        stdout_sha256: output.stdout_sha256,
        stderr_sha256: output.stderr_sha256,
    })
}

fn changed_paths(worktree: &std::path::Path) -> Result<BTreeSet<PathBuf>, CommitError> {
    let tracked =
        run_git(worktree, GitCommand::ChangedTrackedPaths).map_err(|_| CommitError::Command)?;
    let untracked =
        run_git(worktree, GitCommand::UntrackedPaths).map_err(|_| CommitError::Command)?;
    let mut paths = BTreeSet::new();
    for value in tracked
        .stdout
        .split(|byte| *byte == 0)
        .chain(untracked.stdout.split(|byte| *byte == 0))
        .filter(|value| !value.is_empty())
    {
        let path = path_from_bytes(value);
        if !safe_relative(&path) {
            return Err(CommitError::PathAuthority);
        }
        paths.insert(path);
    }
    Ok(paths)
}

fn safe_relative(path: &std::path::Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn path_is_owned(changed: &std::path::Path, owned: &std::path::Path) -> bool {
    changed == owned || changed.starts_with(owned)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use codingmage_contracts::{RunId, TaskId};

    use super::*;
    use crate::{create_owned_worktree, test_support::GitFixture};

    fn create(fixture: &GitFixture) -> (RepositoryAuthorization, OwnedWorktree, String) {
        let authorization = fixture.authorization();
        let parent = fixture.head();
        let owned = create_owned_worktree(
            &authorization,
            &fixture.config(),
            RunId::new("run-commit").unwrap(),
            TaskId::new("task-commit").unwrap(),
            &parent,
        )
        .unwrap();
        (authorization, owned, parent)
    }

    #[test]
    fn coordinator_commits_only_declared_changes_and_preserves_active_checkout() {
        let fixture = GitFixture::new();
        let active_status = fixture.status();
        let (authorization, owned, parent) = create(&fixture);
        fs::write(owned.manifest().path.join("tracked-one.txt"), "changed\n").unwrap();
        fs::create_dir(owned.manifest().path.join("src")).unwrap();
        fs::write(
            owned.manifest().path.join("src/new.rs"),
            "pub fn value() {}\n",
        )
        .unwrap();

        let receipt = commit_owned_changes(
            &authorization,
            &owned,
            &parent,
            &[PathBuf::from("tracked-one.txt"), PathBuf::from("src")],
        )
        .unwrap();
        assert_eq!(receipt.parent, parent);
        assert_ne!(receipt.commit, receipt.parent);
        assert_eq!(
            receipt.changed_paths,
            [
                PathBuf::from("src/new.rs"),
                PathBuf::from("tracked-one.txt")
            ]
        );
        assert_eq!(fixture.status(), active_status);

        let reobserved = reobserve_owned_commit(
            &authorization,
            &owned,
            &receipt.parent,
            &[PathBuf::from("tracked-one.txt"), PathBuf::from("src")],
        )
        .unwrap();
        assert_eq!(reobserved.parent, receipt.parent);
        assert_eq!(reobserved.commit, receipt.commit);
        assert_eq!(reobserved.changed_paths, receipt.changed_paths);
    }

    #[test]
    fn reobservation_refuses_unowned_or_nondirect_commit() {
        let fixture = GitFixture::new();
        let (authorization, owned, parent) = create(&fixture);
        fs::write(owned.manifest().path.join("tracked-one.txt"), "candidate\n").unwrap();
        let receipt = commit_owned_changes(
            &authorization,
            &owned,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();
        assert_eq!(
            reobserve_owned_commit(
                &authorization,
                &owned,
                &parent,
                &[PathBuf::from("tracked-two.txt")],
            ),
            Err(CommitError::PathAuthority)
        );
        assert_eq!(
            reobserve_owned_commit(
                &authorization,
                &owned,
                &"f".repeat(40),
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(CommitError::Identity)
        );
        assert_ne!(receipt.commit, parent);
    }

    #[test]
    fn reobservation_refuses_a_noncoordinator_commit() {
        let fixture = GitFixture::new();
        let (authorization, owned, parent) = create(&fixture);
        fs::write(owned.manifest().path.join("tracked-one.txt"), "candidate\n").unwrap();
        let status = std::process::Command::new("/usr/bin/git")
            .current_dir(&owned.manifest().path)
            .args(["add", "tracked-one.txt"])
            .status()
            .unwrap();
        assert!(status.success());
        let status = std::process::Command::new("/usr/bin/git")
            .current_dir(&owned.manifest().path)
            .args(["commit", "-m", "unowned commit"])
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            reobserve_owned_commit(
                &authorization,
                &owned,
                &parent,
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(CommitError::Identity)
        );
    }

    #[test]
    fn undeclared_change_fails_before_staging() {
        let fixture = GitFixture::new();
        let (authorization, owned, parent) = create(&fixture);
        fs::write(owned.manifest().path.join("tracked-one.txt"), "declared\n").unwrap();
        fs::write(owned.manifest().path.join("tracked-two.txt"), "outside\n").unwrap();

        assert_eq!(
            commit_owned_changes(
                &authorization,
                &owned,
                &parent,
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(CommitError::PathAuthority)
        );
        let index = std::process::Command::new("/usr/bin/git")
            .current_dir(&owned.manifest().path)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .unwrap();
        assert!(index.status.success());
        assert!(index.stdout.is_empty());
    }

    #[test]
    fn active_checkout_edit_during_commit_survives_exactly() {
        let fixture = GitFixture::new();
        let (authorization, owned, parent) = create(&fixture);
        fs::write(owned.manifest().path.join("tracked-one.txt"), "candidate\n").unwrap();
        let active = fixture.target.join("tracked-two.txt");

        let receipt = commit_owned_changes_inner(
            &authorization,
            &owned,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
            || fs::write(&active, "concurrent-user-edit\n").unwrap(),
        )
        .unwrap();

        assert_ne!(receipt.commit, parent);
        assert_eq!(
            fs::read_to_string(active).unwrap(),
            "concurrent-user-edit\n"
        );
        assert!(fixture.status().windows(2).any(|bytes| bytes == b".M"));
    }

    #[test]
    fn stale_parent_empty_and_escaping_authority_fail_closed() {
        let fixture = GitFixture::new();
        let (authorization, owned, parent) = create(&fixture);
        assert_eq!(
            commit_owned_changes(
                &authorization,
                &owned,
                &"f".repeat(40),
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(CommitError::Identity)
        );
        assert_eq!(
            commit_owned_changes(
                &authorization,
                &owned,
                &parent,
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(CommitError::Empty)
        );
        assert_eq!(
            commit_owned_changes(
                &authorization,
                &owned,
                &parent,
                &[PathBuf::from("../outside")],
            ),
            Err(CommitError::PathAuthority)
        );
    }
}
