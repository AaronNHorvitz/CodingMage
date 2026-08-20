use std::{collections::BTreeSet, fmt, path::PathBuf};

use codingmage_core::RepositoryAuthorization;

use crate::{
    OwnedWorktree,
    command::{GitCommand, run_git, run_git_with_codes, text},
    inventory::condition_at,
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

    use codingmage_contracts::{RunId, TaskId};

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
}
