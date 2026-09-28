//! Bounded immutable blob observation under held repository authority.

use std::{fmt, path::Path};

use codingmage_core::RepositoryAuthorization;

use crate::command::{GitCommand, run_git};

/// Why an exact repository blob could not be observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlobReadError {
    /// Commit or relative path binding is malformed.
    InvalidBinding,
    /// Held repository identity changed during observation.
    Identity,
    /// Hardened Git observation failed, timed out or exceeded its byte cap.
    Command,
}

impl fmt::Display for BlobReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBinding => "codingmage.git.blob.invalid_binding",
            Self::Identity => "codingmage.git.blob.identity",
            Self::Command => "codingmage.git.blob.command",
        })
    }
}

impl std::error::Error for BlobReadError {}

/// Reads at most eight MiB from one exact commit and relative repository path.
///
/// The held repository authority is revalidated before and after the hardened, timed Git
/// observation. The requested commit and path are never interpreted by a shell.
///
/// # Errors
///
/// Returns [`BlobReadError`] for malformed binding, changed identity or failed observation.
pub fn read_authorized_blob(
    authorization: &RepositoryAuthorization,
    commit: &str,
    path: &Path,
) -> Result<Vec<u8>, BlobReadError> {
    if !object_id(commit) || !safe_relative_path(path) {
        return Err(BlobReadError::InvalidBinding);
    }
    authorization
        .revalidate()
        .map_err(|_| BlobReadError::Identity)?;
    let output = run_git(
        &authorization.identity().canonical_path,
        GitCommand::TaskSourceBlob {
            commit,
            path: path.to_str().ok_or(BlobReadError::InvalidBinding)?,
        },
    )
    .map_err(|_| BlobReadError::Command)?;
    authorization
        .revalidate()
        .map_err(|_| BlobReadError::Identity)?;
    Ok(output.stdout)
}

fn object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_relative_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    !text.is_empty()
        && !path.is_absolute()
        && !text.chars().any(char::is_control)
        && text.split('/').all(|part| !matches!(part, "" | "." | ".."))
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{GitFixture, run};

    #[test]
    fn blob_bindings_require_full_object_and_relative_normal_path() {
        let sha = "a".repeat(40);
        assert!(object_id(&sha));
        assert!(object_id(&"b".repeat(64)));
        assert!(safe_relative_path(Path::new("docs/TASKS.md")));
        let short = "a".repeat(39);
        for bad in ["HEAD", short.as_str()] {
            assert!(!object_id(bad));
        }
        for bad in ["", "/TASKS.md", "../TASKS.md", "a/./b", "a//b", "a\nb"] {
            assert!(!safe_relative_path(Path::new(bad)));
        }
    }

    #[test]
    fn authorized_blob_reads_exact_commit_and_refuses_changed_repository_head() {
        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let head = fixture.head();
        assert_eq!(
            read_authorized_blob(&authorization, &head, Path::new("tracked-one.txt")).unwrap(),
            b"one\n".to_vec()
        );
        assert_eq!(
            read_authorized_blob(&authorization, "HEAD", Path::new("tracked-one.txt")),
            Err(BlobReadError::InvalidBinding)
        );
        std::fs::write(fixture.target.join("tracked-one.txt"), b"changed\n").unwrap();
        run(&fixture.target, &["add", "tracked-one.txt"]);
        run(&fixture.target, &["commit", "-m", "advance fixture"]);
        assert_eq!(
            read_authorized_blob(&authorization, &head, Path::new("tracked-one.txt")),
            Err(BlobReadError::Identity)
        );
    }
}
