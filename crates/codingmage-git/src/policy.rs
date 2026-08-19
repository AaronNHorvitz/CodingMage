//! Public pre-spawn policy for configured command requests.

use std::{ffi::OsStr, fmt};

use codingmage_core::CommandSpec;

/// Reason a configured Git command is not an approved literal template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitPolicyError {
    /// A Git invocation was not the one approved read-only gate template.
    ProhibitedGit,
}

impl fmt::Display for GitPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("codingmage.git.operation_prohibited")
    }
}

impl std::error::Error for GitPolicyError {}

/// Rejects configured Git commands except the exact read-only `git diff --check` gate.
///
/// Internal worktree operations do not pass through this function; they are private literal
/// variants that cannot be populated from model, task, or configuration text.
///
/// # Errors
///
/// Returns [`GitPolicyError::ProhibitedGit`] for any unrecognized Git argument vector.
pub fn validate_requested_command(command: &CommandSpec) -> Result<(), GitPolicyError> {
    if command.executable.file_name() != Some(OsStr::new("git")) {
        return Ok(());
    }
    if command.args == ["diff", "--check"] {
        Ok(())
    } else {
        Err(GitPolicyError::ProhibitedGit)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn git(args: &[&str]) -> CommandSpec {
        CommandSpec {
            executable: PathBuf::from("/usr/bin/git"),
            args: args.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn exact_read_only_gate_is_allowed() {
        assert_eq!(
            validate_requested_command(&git(&["diff", "--check"])),
            Ok(())
        );
    }

    #[test]
    fn destructive_and_publication_operations_are_denied() {
        let requests = [
            vec!["reset", "--hard"],
            vec!["clean", "-fdx"],
            vec!["checkout", "--", "."],
            vec!["branch", "-D", "work"],
            vec!["worktree", "prune"],
            vec!["gc"],
            vec!["rebase", "main"],
            vec!["commit", "--amend"],
            vec!["push", "--force"],
            vec!["push", "origin", "main"],
            vec!["merge", "feature"],
            vec!["diff", "--check", "--output=/tmp/side-effect"],
        ];
        for request in requests {
            assert_eq!(
                validate_requested_command(&git(&request)),
                Err(GitPolicyError::ProhibitedGit),
                "request={request:?}"
            );
        }
    }
}
