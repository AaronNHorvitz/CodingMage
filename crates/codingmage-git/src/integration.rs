use std::{collections::BTreeSet, fmt, path::PathBuf};

use codingmage_contracts::{RunId, TaskId};
use codingmage_core::{Config, RepositoryAuthorization};

use crate::{
    OwnedWorktree,
    command::{GitCommand, run_git, run_git_with_codes, run_git_with_input, text},
    commit_owned_changes, create_owned_worktree,
    inventory::condition_at,
    remove_owned_worktree,
    worktree::revalidate_active_worktree,
};

const MAX_CHANGED_PATHS: usize = 100_000;

/// Exact coordinator-owned campaign-head advancement receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationReceipt {
    /// Head observed before mutation.
    pub previous_head: String,
    /// Reviewed descendant installed as the new head.
    pub integrated_head: String,
    /// Number of unique changed paths checked against authority.
    pub changed_path_count: usize,
    /// Digest of the hardened fast-forward command output.
    pub stdout_sha256: String,
}

/// Exact receipt for a reviewed stale-base delta transferred onto the current campaign head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationTransferReceipt {
    /// Campaign head observed before transfer.
    pub previous_head: String,
    /// Original immutable candidate base.
    pub candidate_base: String,
    /// Original independently reviewed candidate.
    pub reviewed_head: String,
    /// Coordinator-created commit installed on the campaign branch.
    pub integrated_head: String,
    /// Number of unique candidate paths checked against task authority.
    pub changed_path_count: usize,
    /// Digest of the exact bounded binary patch.
    pub patch_sha256: String,
}

/// Advances one owned campaign worktree to an exact reviewed descendant.
///
/// The coordinator validates ancestry, current head, cleanliness, and every changed path before the
/// fixed fast-forward command runs. Model output cannot supply a command, ref, option, or path.
///
/// # Errors
///
/// Returns [`IntegrationError`] without mutation for stale identity, non-descendants, dirty state,
/// path escape, or malformed Git output. A command failure is not retried automatically.
pub fn integrate_reviewed_descendant(
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    expected_head: &str,
    reviewed_head: &str,
    allowed_paths: &[PathBuf],
) -> Result<IntegrationReceipt, IntegrationError> {
    revalidate_active_worktree(authorization, campaign, expected_head)
        .map_err(|_| IntegrationError::Identity)?;
    if expected_head == reviewed_head
        || !valid_object_id(reviewed_head)
        || allowed_paths.is_empty()
        || allowed_paths.iter().any(|path| !safe_relative(path))
    {
        return Err(IntegrationError::Authority);
    }
    let ancestry = run_git_with_codes(
        &campaign.manifest().path,
        GitCommand::IsAncestor {
            ancestor: expected_head,
            child: reviewed_head,
        },
        &[0, 1],
    )
    .map_err(|_| IntegrationError::Command)?;
    if ancestry.exit_code != 0 {
        return Err(IntegrationError::NonDescendant);
    }
    let (_, condition) =
        condition_at(&campaign.manifest().path).map_err(|_| IntegrationError::RepositoryState)?;
    if !condition.is_clean() {
        return Err(IntegrationError::RepositoryState);
    }
    let changed = run_git(
        &campaign.manifest().path,
        GitCommand::DiffPaths {
            base: expected_head,
            target: reviewed_head,
        },
    )
    .map_err(|_| IntegrationError::Command)?;
    let paths = parse_paths(&changed.stdout)?;
    if paths.is_empty()
        || paths.iter().any(|changed| {
            !allowed_paths
                .iter()
                .any(|allowed| path_is_owned(changed, allowed))
        })
    {
        return Err(IntegrationError::Authority);
    }

    revalidate_active_worktree(authorization, campaign, expected_head)
        .map_err(|_| IntegrationError::Identity)?;
    let result = run_git(
        &campaign.manifest().path,
        GitCommand::FastForward {
            target: reviewed_head,
        },
    )
    .map_err(|_| IntegrationError::Command)?;
    let head = run_git(&campaign.manifest().path, GitCommand::Head)
        .map_err(|_| IntegrationError::Uncertain)?;
    let observed = text(&head).map_err(|_| IntegrationError::Uncertain)?.trim();
    let (_, after) =
        condition_at(&campaign.manifest().path).map_err(|_| IntegrationError::Uncertain)?;
    if observed != reviewed_head || !after.is_clean() {
        return Err(IntegrationError::Uncertain);
    }
    Ok(IntegrationReceipt {
        previous_head: expected_head.to_owned(),
        integrated_head: reviewed_head.to_owned(),
        changed_path_count: paths.len(),
        stdout_sha256: result.stdout_sha256,
    })
}

/// Transfers one reviewed stale-base delta onto the exact current campaign head.
///
/// The candidate remains immutable. A clean temporary worktree is created from `expected_head`,
/// the exact `candidate_base..reviewed_head` binary patch is checked before application, and only
/// coordinator-committed authorized paths may advance the campaign. The temporary worktree is
/// removed before the campaign fast-forward, so cleanup failure cannot follow branch mutation.
///
/// # Errors
///
/// Returns [`IntegrationError`] without changing the campaign for stale identities, unowned paths,
/// patch conflicts, temporary-worktree uncertainty, or malformed Git output.
#[allow(clippy::too_many_arguments)]
pub fn integrate_reviewed_delta(
    authorization: &RepositoryAuthorization,
    config: &Config,
    campaign: &OwnedWorktree,
    integration_run_id: RunId,
    task_id: TaskId,
    expected_head: &str,
    candidate_base: &str,
    reviewed_head: &str,
    allowed_paths: &[PathBuf],
) -> Result<IntegrationTransferReceipt, IntegrationError> {
    revalidate_active_worktree(authorization, campaign, expected_head)
        .map_err(|_| IntegrationError::Identity)?;
    if expected_head == reviewed_head
        || candidate_base == reviewed_head
        || !valid_object_id(candidate_base)
        || !valid_object_id(reviewed_head)
        || allowed_paths.is_empty()
        || allowed_paths.iter().any(|path| !safe_relative(path))
    {
        return Err(IntegrationError::Authority);
    }
    verify_ancestry(&campaign.manifest().path, candidate_base, reviewed_head)?;
    let changed = run_git(
        &campaign.manifest().path,
        GitCommand::DiffPaths {
            base: candidate_base,
            target: reviewed_head,
        },
    )
    .map_err(|_| IntegrationError::Command)?;
    let paths = parse_paths(&changed.stdout)?;
    if paths.is_empty()
        || paths.iter().any(|changed| {
            !allowed_paths
                .iter()
                .any(|allowed| path_is_owned(changed, allowed))
        })
    {
        return Err(IntegrationError::Authority);
    }
    let patch = run_git(
        &campaign.manifest().path,
        GitCommand::BinaryDiff {
            base: candidate_base,
            target: reviewed_head,
        },
    )
    .map_err(|_| IntegrationError::Command)?;
    if patch.stdout.is_empty() {
        return Err(IntegrationError::InvalidOutput);
    }

    let mut transfer = create_owned_worktree(
        authorization,
        config,
        integration_run_id,
        task_id,
        expected_head,
    )
    .map_err(|_| IntegrationError::Command)?;
    run_git_with_input(
        &transfer.manifest().path,
        GitCommand::ApplyCheck,
        &patch.stdout,
    )
    .map_err(|_| IntegrationError::Conflict)?;
    run_git_with_input(&transfer.manifest().path, GitCommand::Apply, &patch.stdout)
        .map_err(|_| IntegrationError::Uncertain)?;
    let committed = commit_owned_changes(authorization, &transfer, expected_head, allowed_paths)
        .map_err(|_| IntegrationError::Uncertain)?;
    remove_owned_worktree(authorization, &mut transfer).map_err(|_| IntegrationError::Uncertain)?;
    let installed = integrate_reviewed_descendant(
        authorization,
        campaign,
        expected_head,
        &committed.commit,
        allowed_paths,
    )?;
    Ok(IntegrationTransferReceipt {
        previous_head: installed.previous_head,
        candidate_base: candidate_base.to_owned(),
        reviewed_head: reviewed_head.to_owned(),
        integrated_head: installed.integrated_head,
        changed_path_count: paths.len(),
        patch_sha256: patch.stdout_sha256,
    })
}

fn verify_ancestry(
    repository: &std::path::Path,
    ancestor: &str,
    child: &str,
) -> Result<(), IntegrationError> {
    let ancestry = run_git_with_codes(
        repository,
        GitCommand::IsAncestor { ancestor, child },
        &[0, 1],
    )
    .map_err(|_| IntegrationError::Command)?;
    if ancestry.exit_code == 0 {
        Ok(())
    } else {
        Err(IntegrationError::NonDescendant)
    }
}

fn parse_paths(bytes: &[u8]) -> Result<BTreeSet<PathBuf>, IntegrationError> {
    let mut paths = BTreeSet::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        if paths.len() >= MAX_CHANGED_PATHS {
            return Err(IntegrationError::InvalidOutput);
        }
        let value = std::str::from_utf8(raw).map_err(|_| IntegrationError::InvalidOutput)?;
        let path = PathBuf::from(value);
        if !safe_relative(&path) || !paths.insert(path) {
            return Err(IntegrationError::InvalidOutput);
        }
    }
    Ok(paths)
}

fn safe_relative(path: &std::path::Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn path_is_owned(changed: &std::path::Path, owned: &std::path::Path) -> bool {
    changed == owned || changed.starts_with(owned)
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Content-free deterministic integration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationError {
    /// Campaign worktree identity or expected head changed.
    Identity,
    /// Proposed commit or path authority was invalid.
    Authority,
    /// Proposed head did not descend from the exact campaign head.
    NonDescendant,
    /// Reviewed stale-base delta does not apply cleanly to the current campaign head.
    Conflict,
    /// Campaign checkout was not clean and quiescent.
    RepositoryState,
    /// Hardened Git invocation failed before a known effect.
    Command,
    /// Git output was malformed or excessive.
    InvalidOutput,
    /// Mutation returned without a provable terminal state.
    Uncertain,
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.git.integration.identity",
            Self::Authority => "codingmage.git.integration.authority",
            Self::NonDescendant => "codingmage.git.integration.non_descendant",
            Self::Conflict => "codingmage.git.integration.conflict",
            Self::RepositoryState => "codingmage.git.integration.repository_state",
            Self::Command => "codingmage.git.integration.command",
            Self::InvalidOutput => "codingmage.git.integration.invalid_output",
            Self::Uncertain => "codingmage.git.integration.uncertain",
        })
    }
}

impl std::error::Error for IntegrationError {}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{commit_owned_changes, create_owned_worktree, test_support::GitFixture};

    #[test]
    fn exact_reviewed_descendant_fast_forwards_campaign_only() {
        let fixture = GitFixture::new();
        let active_before = fixture.status();
        let authorization = fixture.authorization();
        let parent = fixture.head();
        let campaign = create_owned_worktree(
            &authorization,
            &fixture.config(),
            RunId::new("run-campaign").unwrap(),
            TaskId::new("task-campaign").unwrap(),
            &parent,
        )
        .unwrap();
        let candidate = create_owned_worktree(
            &authorization,
            &fixture.config(),
            RunId::new("run-candidate").unwrap(),
            TaskId::new("task-candidate").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(
            candidate.manifest().path.join("tracked-one.txt"),
            "reviewed\n",
        )
        .unwrap();
        let reviewed = commit_owned_changes(
            &authorization,
            &candidate,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();

        let receipt = integrate_reviewed_descendant(
            &authorization,
            &campaign,
            &parent,
            &reviewed.commit,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();
        assert_eq!(receipt.previous_head, parent);
        assert_eq!(receipt.integrated_head, reviewed.commit);
        assert_eq!(receipt.changed_path_count, 1);
        assert_eq!(fixture.status(), active_before);
    }

    #[test]
    fn non_descendant_and_unowned_changes_refuse_before_mutation() {
        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let parent = fixture.head();
        let campaign = create_owned_worktree(
            &authorization,
            &fixture.config(),
            RunId::new("run-campaign-refusal").unwrap(),
            TaskId::new("task-campaign-refusal").unwrap(),
            &parent,
        )
        .unwrap();
        let candidate = create_owned_worktree(
            &authorization,
            &fixture.config(),
            RunId::new("run-candidate-refusal").unwrap(),
            TaskId::new("task-candidate-refusal").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(
            candidate.manifest().path.join("tracked-two.txt"),
            "outside\n",
        )
        .unwrap();
        let reviewed = commit_owned_changes(
            &authorization,
            &candidate,
            &parent,
            &[PathBuf::from("tracked-two.txt")],
        )
        .unwrap();
        assert_eq!(
            integrate_reviewed_descendant(
                &authorization,
                &campaign,
                &parent,
                &reviewed.commit,
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(IntegrationError::Authority)
        );
        let head = run_git(&campaign.manifest().path, GitCommand::Head).unwrap();
        assert_eq!(text(&head).unwrap().trim(), parent);
    }

    #[test]
    fn stale_reviewed_delta_transfers_onto_current_campaign_head() {
        let fixture = GitFixture::new();
        let active_before = fixture.status();
        let authorization = fixture.authorization();
        let config = fixture.config();
        let parent = fixture.head();
        let campaign = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-campaign-transfer").unwrap(),
            TaskId::new("task-campaign-transfer").unwrap(),
            &parent,
        )
        .unwrap();
        let first = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-first-transfer").unwrap(),
            TaskId::new("task-first-transfer").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(first.manifest().path.join("tracked-one.txt"), "first\n").unwrap();
        let first_commit = commit_owned_changes(
            &authorization,
            &first,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();
        integrate_reviewed_descendant(
            &authorization,
            &campaign,
            &parent,
            &first_commit.commit,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();

        let stale = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-stale-transfer").unwrap(),
            TaskId::new("task-stale-transfer").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(stale.manifest().path.join("tracked-two.txt"), "second\n").unwrap();
        let stale_commit = commit_owned_changes(
            &authorization,
            &stale,
            &parent,
            &[PathBuf::from("tracked-two.txt")],
        )
        .unwrap();
        let receipt = integrate_reviewed_delta(
            &authorization,
            &config,
            &campaign,
            RunId::new("run-delta-transfer").unwrap(),
            TaskId::new("task-delta-transfer").unwrap(),
            &first_commit.commit,
            &parent,
            &stale_commit.commit,
            &[PathBuf::from("tracked-two.txt")],
        )
        .unwrap();
        assert_eq!(receipt.previous_head, first_commit.commit);
        assert_eq!(receipt.reviewed_head, stale_commit.commit);
        assert_ne!(receipt.integrated_head, receipt.reviewed_head);
        assert_eq!(receipt.changed_path_count, 1);
        assert_eq!(
            fs::read_to_string(campaign.manifest().path.join("tracked-one.txt")).unwrap(),
            "first\n"
        );
        assert_eq!(
            fs::read_to_string(campaign.manifest().path.join("tracked-two.txt")).unwrap(),
            "second\n"
        );
        assert_eq!(fixture.status(), active_before);
    }

    #[test]
    fn stale_delta_conflict_preserves_campaign_and_candidate() {
        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let config = fixture.config();
        let parent = fixture.head();
        let campaign = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-campaign-conflict").unwrap(),
            TaskId::new("task-campaign-conflict").unwrap(),
            &parent,
        )
        .unwrap();
        let current = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-current-conflict").unwrap(),
            TaskId::new("task-current-conflict").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(current.manifest().path.join("tracked-one.txt"), "current\n").unwrap();
        let current_commit = commit_owned_changes(
            &authorization,
            &current,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();
        integrate_reviewed_descendant(
            &authorization,
            &campaign,
            &parent,
            &current_commit.commit,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();

        let stale = create_owned_worktree(
            &authorization,
            &config,
            RunId::new("run-stale-conflict").unwrap(),
            TaskId::new("task-stale-conflict").unwrap(),
            &parent,
        )
        .unwrap();
        fs::write(stale.manifest().path.join("tracked-one.txt"), "stale\n").unwrap();
        let stale_commit = commit_owned_changes(
            &authorization,
            &stale,
            &parent,
            &[PathBuf::from("tracked-one.txt")],
        )
        .unwrap();
        assert_eq!(
            integrate_reviewed_delta(
                &authorization,
                &config,
                &campaign,
                RunId::new("run-delta-conflict").unwrap(),
                TaskId::new("task-delta-conflict").unwrap(),
                &current_commit.commit,
                &parent,
                &stale_commit.commit,
                &[PathBuf::from("tracked-one.txt")],
            ),
            Err(IntegrationError::Conflict)
        );
        let campaign_head = run_git(&campaign.manifest().path, GitCommand::Head).unwrap();
        let stale_head = run_git(&stale.manifest().path, GitCommand::Head).unwrap();
        assert_eq!(text(&campaign_head).unwrap().trim(), current_commit.commit);
        assert_eq!(text(&stale_head).unwrap().trim(), stale_commit.commit);
    }
}
