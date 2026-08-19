//! Exact immutable commit and source-location verification for model review.

use std::{collections::BTreeSet, fmt, path::Path};

use crate::command::{GitCommand, run_git, text};

const MAX_PATHS: usize = 100_000;

/// One provider-referenced source location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewLocation {
    /// Relative repository path.
    pub path: String,
    /// One-based line number.
    pub line: u64,
}

/// Exact base, target, checkout, and tree observed around a review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewScope {
    base: String,
    target: String,
    head: String,
    tree_sha256: String,
    paths: BTreeSet<String>,
}

impl ReviewScope {
    /// Captures and validates an exact review scope from a read-only checkout.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewScopeError`] unless both commits exist, base is an ancestor of target,
    /// checkout HEAD equals target, and the target tree has bounded canonical paths.
    pub fn capture(worktree: &Path, base: &str, target: &str) -> Result<Self, ReviewScopeError> {
        if !worktree.is_absolute() || !worktree.is_dir() || !object_id(base) || !object_id(target) {
            return Err(ReviewScopeError::InvalidBinding);
        }
        run_git(worktree, GitCommand::VerifyCommit(base)).map_err(|_| ReviewScopeError::Git)?;
        run_git(worktree, GitCommand::VerifyCommit(target)).map_err(|_| ReviewScopeError::Git)?;
        run_git(
            worktree,
            GitCommand::IsAncestor {
                ancestor: base,
                child: target,
            },
        )
        .map_err(|_| ReviewScopeError::InvalidBinding)?;
        let head_output = run_git(worktree, GitCommand::Head).map_err(|_| ReviewScopeError::Git)?;
        let head = text(&head_output)
            .map_err(|_| ReviewScopeError::Git)?
            .trim()
            .to_owned();
        if head != target {
            return Err(ReviewScopeError::Stale);
        }
        let tree =
            run_git(worktree, GitCommand::TreePaths(target)).map_err(|_| ReviewScopeError::Git)?;
        let mut paths = BTreeSet::new();
        for raw in tree
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            if paths.len() >= MAX_PATHS {
                return Err(ReviewScopeError::InvalidTree);
            }
            let path = std::str::from_utf8(raw).map_err(|_| ReviewScopeError::InvalidTree)?;
            if !safe_path(path) || !paths.insert(path.to_owned()) {
                return Err(ReviewScopeError::InvalidTree);
            }
        }
        Ok(Self {
            base: base.to_owned(),
            target: target.to_owned(),
            head,
            tree_sha256: tree.stdout_sha256,
            paths,
        })
    }

    /// Verifies every referenced file and line against the captured target commit.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewScopeError::InvalidLocation`] for absent, escaping, zero, or out-of-range
    /// locations.
    pub fn verify_locations(
        &self,
        worktree: &Path,
        locations: &[ReviewLocation],
    ) -> Result<(), ReviewScopeError> {
        for location in locations {
            if location.line == 0
                || !safe_path(&location.path)
                || !self.paths.contains(&location.path)
            {
                return Err(ReviewScopeError::InvalidLocation);
            }
            let blob = run_git(
                worktree,
                GitCommand::Blob {
                    commit: &self.target,
                    path: &location.path,
                },
            )
            .map_err(|_| ReviewScopeError::InvalidLocation)?;
            let line_count = if blob.stdout.is_empty() {
                0
            } else {
                blob.stdout.split(|byte| *byte == b'\n').count()
                    - usize::from(blob.stdout.ends_with(b"\n"))
            };
            if usize::try_from(location.line).map_or(true, |line| line > line_count) {
                return Err(ReviewScopeError::InvalidLocation);
            }
        }
        Ok(())
    }

    /// Requires the exact base, target, checkout HEAD, and tree to remain unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewScopeError::Stale`] when any scope identity changed.
    pub fn revalidate(&self, worktree: &Path) -> Result<(), ReviewScopeError> {
        let observed = Self::capture(worktree, &self.base, &self.target)?;
        if observed == *self {
            Ok(())
        } else {
            Err(ReviewScopeError::Stale)
        }
    }
}

fn object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|component| !matches!(component, "" | "." | ".."))
}

/// Stable review-scope failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewScopeError {
    /// Commit or worktree binding is invalid.
    InvalidBinding,
    /// Hardened Git observation failed.
    Git,
    /// Target tree is excessive or malformed.
    InvalidTree,
    /// Provider finding references an absent or invalid source location.
    InvalidLocation,
    /// Review scope changed before completion.
    Stale,
}

impl fmt::Display for ReviewScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBinding => "codingmage.git.review.invalid_binding",
            Self::Git => "codingmage.git.review.git",
            Self::InvalidTree => "codingmage.git.review.invalid_tree",
            Self::InvalidLocation => "codingmage.git.review.invalid_location",
            Self::Stale => "codingmage.git.review.stale",
        })
    }
}

impl std::error::Error for ReviewScopeError {}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_support::{GitFixture, run};

    #[test]
    fn exact_files_lines_and_scope_are_verified() {
        let fixture = GitFixture::new();
        let base = fixture.head();
        fs::write(fixture.target.join("review.txt"), "one\ntwo\nthree\n").unwrap();
        run(&fixture.target, &["add", "review.txt"]);
        run(&fixture.target, &["commit", "-m", "review target"]);
        let target = fixture.head();
        let scope = ReviewScope::capture(&fixture.target, &base, &target).unwrap();
        scope
            .verify_locations(
                &fixture.target,
                &[ReviewLocation {
                    path: "review.txt".to_owned(),
                    line: 3,
                }],
            )
            .unwrap();
        for location in [
            ReviewLocation {
                path: "missing.txt".to_owned(),
                line: 1,
            },
            ReviewLocation {
                path: "review.txt".to_owned(),
                line: 4,
            },
            ReviewLocation {
                path: "../escape".to_owned(),
                line: 1,
            },
        ] {
            assert_eq!(
                scope.verify_locations(&fixture.target, &[location]),
                Err(ReviewScopeError::InvalidLocation)
            );
        }
        scope.revalidate(&fixture.target).unwrap();
        run(&fixture.target, &["checkout", &base]);
        assert_eq!(
            scope.revalidate(&fixture.target),
            Err(ReviewScopeError::Stale)
        );
    }

    #[test]
    fn nonancestor_and_wrong_head_are_rejected() {
        let fixture = GitFixture::new();
        let target = fixture.head();
        run(&fixture.target, &["checkout", "--orphan", "unrelated"]);
        fs::write(fixture.target.join("unrelated.txt"), "unrelated\n").unwrap();
        run(&fixture.target, &["add", "unrelated.txt"]);
        run(&fixture.target, &["commit", "-m", "unrelated"]);
        let unrelated = fixture.head();
        assert_eq!(
            ReviewScope::capture(&fixture.target, &unrelated, &target),
            Err(ReviewScopeError::InvalidBinding)
        );
    }
}
