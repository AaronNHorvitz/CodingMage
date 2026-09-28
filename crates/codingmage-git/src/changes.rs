//! Bounded, exact-commit change summaries under held repository identity.

use std::fmt;

use codingmage_core::RepositoryAuthorization;
use serde::Serialize;

use crate::command::{GitCommand, run_git};

const MAX_ENTRIES: usize = 500;

/// One commit in the candidate range, newest first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangeCommit {
    /// Full object identity.
    pub id: String,
    /// Untrusted, inert subject text.
    pub subject: String,
    /// Unix seconds reported by Git.
    pub timestamp: u64,
}

/// One changed repository-relative path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangedFile {
    /// UTF-8 path from the repository object.
    pub path: String,
    /// Added lines, absent for a binary change.
    pub added: Option<u64>,
    /// Deleted lines, absent for a binary change.
    pub deleted: Option<u64>,
}

/// Bounded summary of an exact ancestor-to-descendant range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthorizedChanges {
    /// Commit summaries, newest first.
    pub commits: Vec<ChangeCommit>,
    /// Changed path summaries.
    pub files: Vec<ChangedFile>,
    /// More commits exist than were returned.
    pub commits_truncated: bool,
    /// More changed paths exist than were returned.
    pub files_truncated: bool,
}

/// Why an exact change summary could not be observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeReadError {
    /// A commit binding was not a full object ID or the base was not an ancestor.
    InvalidBinding,
    /// Held repository identity changed.
    Identity,
    /// A hardened Git read failed or exceeded its byte/deadline bound.
    Command,
    /// Git returned malformed data; the summary was withheld.
    Malformed,
}

impl fmt::Display for ChangeReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBinding => "codingmage.git.changes.invalid_binding",
            Self::Identity => "codingmage.git.changes.identity",
            Self::Command => "codingmage.git.changes.command",
            Self::Malformed => "codingmage.git.changes.malformed",
        })
    }
}

impl std::error::Error for ChangeReadError {}

/// Reads bounded commit and file summaries for two exact commits in an authorized repository.
///
/// The base must be an ancestor of the head. Neither range nor path text reaches a shell.
/// Repository identity is revalidated around every Git observation. A malformed or oversized
/// observation fails closed rather than showing a partial result as a complete one.
///
/// # Errors
///
/// Returns [`ChangeReadError`] for invalid bindings, changed identity, Git failures or malformed
/// output.
pub fn read_authorized_changes(
    authorization: &RepositoryAuthorization,
    base: &str,
    head: &str,
) -> Result<AuthorizedChanges, ChangeReadError> {
    if !full_id(base) || !full_id(head) || base.len() != head.len() {
        return Err(ChangeReadError::InvalidBinding);
    }
    let repository = &authorization.identity().canonical_path;
    authorization
        .revalidate()
        .map_err(|_| ChangeReadError::Identity)?;
    run_git(
        repository,
        GitCommand::IsAncestor {
            ancestor: base,
            child: head,
        },
    )
    .map_err(|_| ChangeReadError::InvalidBinding)?;
    authorization
        .revalidate()
        .map_err(|_| ChangeReadError::Identity)?;
    let log = run_git(repository, GitCommand::ChangeLog { base, target: head })
        .map_err(|_| ChangeReadError::Command)?;
    authorization
        .revalidate()
        .map_err(|_| ChangeReadError::Identity)?;
    let numstat = run_git(repository, GitCommand::ChangeNumstat { base, target: head })
        .map_err(|_| ChangeReadError::Command)?;
    authorization
        .revalidate()
        .map_err(|_| ChangeReadError::Identity)?;
    let mut commits = parse_log(&log.stdout)?;
    let mut files = parse_numstat(&numstat.stdout)?;
    let commits_truncated = commits.len() > MAX_ENTRIES;
    let files_truncated = files.len() > MAX_ENTRIES;
    commits.truncate(MAX_ENTRIES);
    files.truncate(MAX_ENTRIES);
    Ok(AuthorizedChanges {
        commits,
        files,
        commits_truncated,
        files_truncated,
    })
}

fn full_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_log(bytes: &[u8]) -> Result<Vec<ChangeCommit>, ChangeReadError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .strip_suffix(&[0])
        .ok_or(ChangeReadError::Malformed)?
        .split(|byte| *byte == 0)
        .map(|entry| {
            let text = std::str::from_utf8(entry).map_err(|_| ChangeReadError::Malformed)?;
            let mut parts = text.split('\u{1f}');
            let (Some(id), Some(subject), Some(timestamp), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(ChangeReadError::Malformed);
            };
            if !full_id(id) || subject.len() > 4096 || subject.chars().any(char::is_control) {
                return Err(ChangeReadError::Malformed);
            }
            Ok(ChangeCommit {
                id: id.to_owned(),
                subject: subject.to_owned(),
                timestamp: timestamp.parse().map_err(|_| ChangeReadError::Malformed)?,
            })
        })
        .collect()
}

fn parse_numstat(bytes: &[u8]) -> Result<Vec<ChangedFile>, ChangeReadError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .strip_suffix(&[0])
        .ok_or(ChangeReadError::Malformed)?
        .split(|byte| *byte == 0)
        .map(|entry| {
            let text = std::str::from_utf8(entry).map_err(|_| ChangeReadError::Malformed)?;
            let mut parts = text.splitn(3, '\t');
            let (Some(added), Some(deleted), Some(path)) =
                (parts.next(), parts.next(), parts.next())
            else {
                return Err(ChangeReadError::Malformed);
            };
            if path.is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
                return Err(ChangeReadError::Malformed);
            }
            let counts = if added == "-" && deleted == "-" {
                (None, None)
            } else {
                (
                    Some(added.parse().map_err(|_| ChangeReadError::Malformed)?),
                    Some(deleted.parse().map_err(|_| ChangeReadError::Malformed)?),
                )
            };
            Ok(ChangedFile {
                path: path.to_owned(),
                added: counts.0,
                deleted: counts.1,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{GitFixture, run};
    use std::fs;

    #[test]
    fn exact_range_is_bounded_and_repository_identity_is_held() {
        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let base = fixture.head();
        fs::write(fixture.target.join("tracked-one.txt"), "new\n").unwrap();
        run(&fixture.target, &["add", "tracked-one.txt"]);
        run(&fixture.target, &["commit", "-m", "new candidate"]);
        // Holding the original authorization detects the changed target head.
        assert_eq!(
            read_authorized_changes(&authorization, &base, &fixture.head()),
            Err(ChangeReadError::Identity)
        );
        let current = fixture.authorization();
        let head = fixture.head();
        let observed = read_authorized_changes(&current, &base, &head).unwrap();
        assert_eq!(observed.commits.len(), 1);
        assert_eq!(observed.commits[0].id, head);
        assert_eq!(observed.files[0].path, "tracked-one.txt");
        assert_eq!(observed.files[0].added, Some(1));
        assert!(!observed.commits_truncated);
        assert_eq!(
            read_authorized_changes(&current, "HEAD", &head),
            Err(ChangeReadError::InvalidBinding)
        );
        assert_eq!(
            read_authorized_changes(&current, &head, &base),
            Err(ChangeReadError::InvalidBinding)
        );
    }

    #[test]
    fn malformed_observations_never_become_empty_success() {
        let binary = parse_numstat(b"-\t-\timage.png\0").unwrap();
        assert_eq!(binary[0].added, None);
        assert_eq!(binary[0].deleted, None);
        assert_eq!(parse_log(b"bad\0"), Err(ChangeReadError::Malformed));
        assert_eq!(parse_log(b"unfinished"), Err(ChangeReadError::Malformed));
        assert_eq!(parse_numstat(b"bad\0"), Err(ChangeReadError::Malformed));
        assert_eq!(parse_numstat(b"1\t-\ta\0"), Err(ChangeReadError::Malformed));
        assert_eq!(parse_numstat(b"1\t2\ta"), Err(ChangeReadError::Malformed));
    }
}
