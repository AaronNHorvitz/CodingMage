//! Content-minimized, read-only inventory of an authorized repository.

use std::{collections::BTreeSet, fmt, fs, path::Path, time::Duration};

use codingmage_core::RepositoryAuthorization;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::command::{CommandError, GitCommand, GitOutput, run_git, text};

const MAX_RECORDS: usize = 100_000;
const MAX_SNAPSHOT_FILES: usize = 100_000;
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Known in-progress Git operation or lock state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    /// No in-progress operation was detected.
    None,
    /// A merge is in progress.
    Merging,
    /// A rebase is in progress.
    Rebasing,
    /// A bisect is in progress.
    Bisecting,
    /// The index is locked.
    Locked,
}

/// Classified working-copy state without retaining file names.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct RepositoryCondition {
    /// Repository is on a detached `HEAD`.
    pub detached: bool,
    /// One or more staged changes exist.
    pub staged: bool,
    /// One or more unstaged changes exist.
    pub unstaged: bool,
    /// One or more untracked paths exist.
    pub untracked: bool,
    /// One or more unresolved entries exist.
    pub conflicted: bool,
}

impl RepositoryCondition {
    /// Returns whether no tracked, untracked, or conflicted change was observed.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        !(self.staged || self.unstaged || self.untracked || self.conflicted)
    }
}

/// Content-minimized evidence for one hardened Git invocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitInvocationEvidence {
    /// SHA-256 of complete standard output.
    pub stdout_sha256: String,
    /// SHA-256 of complete standard error.
    pub stderr_sha256: String,
    /// Literal terminal exit code.
    pub exit_code: i32,
    /// Elapsed milliseconds.
    pub elapsed_millis: u64,
}

/// Read-only inventory with names and paths minimized or fingerprinted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    /// Exact `HEAD` object identifier.
    pub head: String,
    /// Current branch, or `None` when detached.
    pub branch: Option<String>,
    /// Classified working-copy state.
    pub condition: RepositoryCondition,
    /// In-progress operation or lock.
    pub operation: OperationState,
    /// Digest of porcelain status, without retained path names.
    pub status_sha256: String,
    /// Digest of the index, if present.
    pub index_sha256: Option<String>,
    /// Number of refs returned by the bounded query.
    pub reference_count: usize,
    /// Number of local branch refs.
    pub branch_count: usize,
    /// Number of tag refs.
    pub tag_count: usize,
    /// Number of note refs.
    pub note_count: usize,
    /// Whether a stash ref exists.
    pub has_stash: bool,
    /// Digest of exact ref output.
    pub references_sha256: String,
    /// Number of registered worktrees.
    pub worktree_count: usize,
    /// Digest of exact worktree output.
    pub worktrees_sha256: String,
    /// Digest of local Git configuration.
    pub configuration_sha256: Option<String>,
    /// Digest of hook names and bytes.
    pub hooks_sha256: String,
    /// Number of redacted configured remotes.
    pub remote_count: usize,
    /// Digest of remote names and URL fingerprints.
    pub remotes_sha256: String,
    /// Whether filters, alternates, or attributes make checkout unsafe.
    pub unsafe_checkout_features: bool,
    /// Bounded invocation evidence in execution order.
    pub invocations: Vec<GitInvocationEvidence>,
}

/// Content-free inventory failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryError {
    /// Repository authorization was stale.
    StaleAuthorization,
    /// A hardened Git invocation failed.
    Command,
    /// Git output was malformed or exceeded a bound.
    InvalidOutput,
    /// Inventory changed protected repository state.
    PreservationMismatch,
    /// An unsupported in-progress or hostile state was found.
    UnsupportedState,
}

impl fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::StaleAuthorization => "codingmage.git.stale_authorization",
            Self::Command => "codingmage.git.command_failed",
            Self::InvalidOutput => "codingmage.git.invalid_output",
            Self::PreservationMismatch => "codingmage.git.preservation_mismatch",
            Self::UnsupportedState => "codingmage.git.unsupported_state",
        })
    }
}

impl std::error::Error for InventoryError {}

/// Captures repository state without retaining changed path names.
///
/// # Errors
///
/// Returns [`InventoryError`] if authorization is stale, output is malformed or excessive, an
/// invocation fails, or protected state changes during inventory.
pub fn inventory_repository(
    authorization: &RepositoryAuthorization,
) -> Result<Inventory, InventoryError> {
    authorization
        .revalidate()
        .map_err(|_| InventoryError::StaleAuthorization)?;
    let target = &authorization.identity().canonical_path;
    let git = &authorization.identity().git_directory;
    let before = protected_snapshot(git)?;

    let status = run_git(target, GitCommand::Status).map_err(map_command)?;
    let references = run_git(target, GitCommand::References).map_err(map_command)?;
    let worktrees = run_git(target, GitCommand::Worktrees).map_err(map_command)?;
    let head_output = run_git(target, GitCommand::Head).map_err(map_command)?;
    let tracked_paths = run_git(target, GitCommand::TrackedPaths).map_err(map_command)?;

    let (branch, condition) = parse_status(&status)?;
    let ref_counts = validate_references(&references)?;
    let worktree_count = validate_worktrees(&worktrees)?;
    let head = text(&head_output).map_err(map_command)?.trim().to_owned();
    if !valid_object_id(&head) {
        return Err(InventoryError::InvalidOutput);
    }

    let operation = operation_state(git);
    let index_sha256 = digest_optional_file(&git.join("index"))?;
    let configuration_sha256 = digest_optional_file(&git.join("config"))?;
    let hooks_sha256 = digest_tree(&git.join("hooks"))?;
    let unsafe_checkout_features =
        detect_unsafe_checkout_features(target, git)? || tracked_path_hazards(&tracked_paths)?;
    let remotes_sha256 = digest_remotes(&authorization.identity().remotes);
    let after = protected_snapshot(git)?;
    if before != after {
        return Err(InventoryError::PreservationMismatch);
    }

    Ok(Inventory {
        head,
        branch,
        condition,
        operation,
        status_sha256: status.stdout_sha256.clone(),
        index_sha256,
        reference_count: reference_count(&references)?,
        branch_count: ref_counts.branches,
        tag_count: ref_counts.tags,
        note_count: ref_counts.notes,
        has_stash: ref_counts.has_stash,
        references_sha256: references.stdout_sha256.clone(),
        worktree_count,
        worktrees_sha256: worktrees.stdout_sha256.clone(),
        configuration_sha256,
        hooks_sha256,
        remote_count: authorization.identity().remotes.len(),
        remotes_sha256,
        unsafe_checkout_features,
        invocations: [
            &status,
            &references,
            &worktrees,
            &head_output,
            &tracked_paths,
        ]
        .into_iter()
        .map(invocation_evidence)
        .collect(),
    })
}

fn tracked_path_hazards(output: &GitOutput) -> Result<bool, InventoryError> {
    let mut canonical = BTreeSet::new();
    let mut count = 0_usize;
    for path in output.stdout.split(|byte| *byte == 0) {
        if path.is_empty() {
            continue;
        }
        count = count.saturating_add(1);
        if count > MAX_RECORDS {
            return Err(InventoryError::InvalidOutput);
        }
        let text = std::str::from_utf8(path).map_err(|_| InventoryError::InvalidOutput)?;
        if !text.is_ascii() || matches!(text, ".gitmodules" | ".lfsconfig") {
            return Ok(true);
        }
        let folded = text.to_ascii_lowercase();
        if !canonical.insert(folded) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn condition_at(
    path: &Path,
) -> Result<(Option<String>, RepositoryCondition), InventoryError> {
    let status = run_git(path, GitCommand::Status).map_err(map_command)?;
    parse_status(&status)
}

fn invocation_evidence(output: &GitOutput) -> GitInvocationEvidence {
    GitInvocationEvidence {
        stdout_sha256: output.stdout_sha256.clone(),
        stderr_sha256: output.stderr_sha256.clone(),
        exit_code: output.exit_code,
        elapsed_millis: duration_millis(output.elapsed),
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn parse_status(
    output: &GitOutput,
) -> Result<(Option<String>, RepositoryCondition), InventoryError> {
    let mut branch = None;
    let mut condition = RepositoryCondition::default();
    let mut records = 0_usize;
    for line in text(output).map_err(map_command)?.lines() {
        records += 1;
        if records > MAX_RECORDS {
            return Err(InventoryError::InvalidOutput);
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            if value == "(detached)" {
                condition.detached = true;
            } else if valid_ref_component(value) {
                branch = Some(value.to_owned());
            } else {
                return Err(InventoryError::InvalidOutput);
            }
            continue;
        }
        if line.starts_with("# ") {
            continue;
        }
        if line.starts_with("? ") {
            condition.untracked = true;
            continue;
        }
        if line.starts_with("! ") {
            continue;
        }
        if line.starts_with("u ") {
            condition.conflicted = true;
            continue;
        }
        if line.starts_with("1 ") || line.starts_with("2 ") {
            let xy = line
                .as_bytes()
                .get(2..4)
                .ok_or(InventoryError::InvalidOutput)?;
            condition.staged |= xy[0] != b'.';
            condition.unstaged |= xy[1] != b'.';
            continue;
        }
        return Err(InventoryError::InvalidOutput);
    }
    Ok((branch, condition))
}

#[derive(Default)]
struct RefCounts {
    branches: usize,
    tags: usize,
    notes: usize,
    has_stash: bool,
}

fn validate_references(output: &GitOutput) -> Result<RefCounts, InventoryError> {
    let mut counts = RefCounts::default();
    for record in text(output).map_err(map_command)?.lines() {
        let (object, reference) = record
            .split_once('\0')
            .ok_or(InventoryError::InvalidOutput)?;
        if !valid_object_id(object) || !reference.starts_with("refs/") || reference.len() > 1024 {
            return Err(InventoryError::InvalidOutput);
        }
        if reference.starts_with("refs/heads/") {
            counts.branches += 1;
        } else if reference.starts_with("refs/tags/") {
            counts.tags += 1;
        } else if reference.starts_with("refs/notes/") {
            counts.notes += 1;
        } else if reference == "refs/stash" {
            counts.has_stash = true;
        }
    }
    let _ = reference_count(output)?;
    Ok(counts)
}

fn reference_count(output: &GitOutput) -> Result<usize, InventoryError> {
    let count = text(output).map_err(map_command)?.lines().count();
    if count > MAX_RECORDS {
        return Err(InventoryError::InvalidOutput);
    }
    Ok(count)
}

fn validate_worktrees(output: &GitOutput) -> Result<usize, InventoryError> {
    let mut count = 0_usize;
    for field in output.stdout.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if path.is_empty() {
                return Err(InventoryError::InvalidOutput);
            }
            count += 1;
            if count > MAX_RECORDS {
                return Err(InventoryError::InvalidOutput);
            }
        }
    }
    if count == 0 {
        return Err(InventoryError::InvalidOutput);
    }
    Ok(count)
}

fn operation_state(git: &Path) -> OperationState {
    if git.join("index.lock").exists() {
        OperationState::Locked
    } else if git.join("rebase-apply").exists() || git.join("rebase-merge").exists() {
        OperationState::Rebasing
    } else if git.join("MERGE_HEAD").exists() {
        OperationState::Merging
    } else if git.join("BISECT_LOG").exists() {
        OperationState::Bisecting
    } else {
        OperationState::None
    }
}

fn detect_unsafe_checkout_features(target: &Path, git: &Path) -> Result<bool, InventoryError> {
    let config = read_optional_bounded(&git.join("config"))?.unwrap_or_default();
    let lower = config.to_ascii_lowercase();
    let unsafe_config_markers = [
        "[alias",
        "[credential",
        "[diff ",
        "[filter ",
        "[include",
        "[merge ",
        "[url ",
        "askpass",
        "editor =",
        "fsmonitor =",
        "pager =",
        "program =",
    ];
    if unsafe_config_markers
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Ok(true);
    }
    if git.join("objects/info/alternates").exists() {
        return Ok(true);
    }
    if directory_has_entries(&git.join("refs/replace"))?
        || read_optional_bounded(&git.join("packed-refs"))?
            .is_some_and(|packed| packed.lines().any(|line| line.contains(" refs/replace/")))
        || directory_has_active_hooks(&git.join("hooks"))?
    {
        return Ok(true);
    }
    for attributes in [target.join(".gitattributes"), git.join("info/attributes")] {
        if let Some(content) = read_optional_bounded(&attributes)?
            && content.lines().any(|line| {
                let line = line.trim();
                !line.starts_with('#')
                    && (line.contains("filter=")
                        || line.contains("diff=")
                        || line.contains("merge="))
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn directory_has_entries(path: &Path) -> Result<bool, InventoryError> {
    match fs::read_dir(path) {
        Ok(mut entries) => Ok(entries.next().is_some()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(InventoryError::InvalidOutput),
    }
}

fn directory_has_active_hooks(path: &Path) -> Result<bool, InventoryError> {
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|_| InventoryError::InvalidOutput)?;
                let name = entry.file_name();
                let name = name.to_str().ok_or(InventoryError::InvalidOutput)?;
                if entry
                    .file_type()
                    .map_err(|_| InventoryError::InvalidOutput)?
                    .is_file()
                    && !name.ends_with(".sample")
                {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(InventoryError::InvalidOutput),
    }
}

#[derive(Eq, PartialEq)]
struct ProtectedSnapshot {
    digest: String,
    files: usize,
    bytes: u64,
}

fn protected_snapshot(git: &Path) -> Result<ProtectedSnapshot, InventoryError> {
    let mut digest = Sha256::new();
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    for relative in ["HEAD", "index", "packed-refs", "config"] {
        digest_snapshot_path(
            git,
            &git.join(relative),
            &mut digest,
            &mut files,
            &mut bytes,
        )?;
    }
    for relative in ["refs", "logs/refs", "hooks"] {
        digest_snapshot_tree(
            git,
            &git.join(relative),
            &mut digest,
            &mut files,
            &mut bytes,
        )?;
    }
    Ok(ProtectedSnapshot {
        digest: hex_bytes(digest.finalize().as_ref()),
        files,
        bytes,
    })
}

fn digest_snapshot_tree(
    root: &Path,
    path: &Path,
    digest: &mut Sha256,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), InventoryError> {
    if !path.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)
        .map_err(|_| InventoryError::InvalidOutput)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| InventoryError::InvalidOutput)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let entry_path = entry.path();
        let metadata =
            fs::symlink_metadata(&entry_path).map_err(|_| InventoryError::InvalidOutput)?;
        if metadata.is_dir() {
            digest_snapshot_tree(root, &entry_path, digest, files, bytes)?;
        } else if metadata.is_file() {
            digest_snapshot_path(root, &entry_path, digest, files, bytes)?;
        } else {
            return Err(InventoryError::UnsupportedState);
        }
    }
    Ok(())
}

fn digest_snapshot_path(
    root: &Path,
    path: &Path,
    digest: &mut Sha256,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), InventoryError> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read(path).map_err(|_| InventoryError::InvalidOutput)?;
    *files += 1;
    *bytes = bytes.saturating_add(content.len() as u64);
    if *files > MAX_SNAPSHOT_FILES || *bytes > MAX_SNAPSHOT_BYTES {
        return Err(InventoryError::InvalidOutput);
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| InventoryError::InvalidOutput)?;
    digest.update(relative.as_os_str().as_encoded_bytes());
    digest.update([0]);
    digest.update(&content);
    digest.update([0]);
    Ok(())
}

fn digest_optional_file(path: &Path) -> Result<Option<String>, InventoryError> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path).map_err(|_| InventoryError::InvalidOutput)?;
    if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
        return Err(InventoryError::InvalidOutput);
    }
    let content = fs::read(path).map_err(|_| InventoryError::InvalidOutput)?;
    Ok(Some(hex_bytes(Sha256::digest(content).as_ref())))
}

fn digest_tree(path: &Path) -> Result<String, InventoryError> {
    let root = path.parent().ok_or(InventoryError::InvalidOutput)?;
    let mut digest = Sha256::new();
    let mut files = 0;
    let mut bytes = 0;
    digest_snapshot_tree(root, path, &mut digest, &mut files, &mut bytes)?;
    Ok(hex_bytes(digest.finalize().as_ref()))
}

fn digest_remotes(remotes: &[codingmage_core::RemoteIdentity]) -> String {
    let mut digest = Sha256::new();
    for remote in remotes {
        digest.update(remote.name.as_bytes());
        digest.update([0]);
        digest.update(remote.url_sha256.as_bytes());
        digest.update([0]);
    }
    hex_bytes(digest.finalize().as_ref())
}

fn read_optional_bounded(path: &Path) -> Result<Option<String>, InventoryError> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path).map_err(|_| InventoryError::InvalidOutput)?;
    if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
        return Err(InventoryError::InvalidOutput);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|_| InventoryError::InvalidOutput)
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_ref_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
}

fn map_command(error: CommandError) -> InventoryError {
    match error {
        CommandError::InvalidOutput | CommandError::OutputLimit => InventoryError::InvalidOutput,
        _ => InventoryError::Command,
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

#[cfg(test)]
mod tests {
    use std::{
        fs, io::ErrorKind, net::TcpListener, os::unix::fs::PermissionsExt, thread, time::Duration,
    };

    use super::*;
    use crate::test_support::{GitFixture, run};

    #[test]
    fn clean_and_dirty_states_are_classified_without_path_retention() {
        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let clean = inventory_repository(&authorization).unwrap();
        assert!(clean.condition.is_clean());
        assert_eq!(clean.branch.as_deref(), Some("main"));

        fs::write(fixture.target.join("tracked-one.txt"), "staged\n").unwrap();
        run(&fixture.target, &["add", "tracked-one.txt"]);
        fs::write(fixture.target.join("tracked-two.txt"), "unstaged\n").unwrap();
        fs::write(
            fixture.target.join("untracked-private-name.txt"),
            "untracked\n",
        )
        .unwrap();
        let status_before = fixture.status();
        let dirty = inventory_repository(&authorization).unwrap();
        assert!(dirty.condition.staged);
        assert!(dirty.condition.unstaged);
        assert!(dirty.condition.untracked);
        assert!(!dirty.condition.conflicted);
        assert_eq!(fixture.status(), status_before);
        let encoded = toml::to_string(&dirty).unwrap();
        assert!(!encoded.contains("untracked-private-name"));
    }

    #[test]
    fn tracked_submodule_lfs_case_and_unicode_hazards_are_refused() {
        for (name, files) in [
            ("submodule", vec![(".gitmodules", "[submodule \"x\"]\n")]),
            (
                "lfs",
                vec![(".lfsconfig", "[lfs]\nurl = https://example.invalid\n")],
            ),
            ("case", vec![("Case.txt", "one\n"), ("case.txt", "two\n")]),
            ("unicode", vec![("caf\u{e9}.txt", "content\n")]),
        ] {
            let fixture = GitFixture::new();
            for (path, content) in files {
                fs::write(fixture.target.join(path), content).unwrap();
                run(&fixture.target, &["add", path]);
            }
            run(&fixture.target, &["commit", "-m", name]);
            let inventory = inventory_repository(&fixture.authorization()).unwrap();
            assert!(inventory.unsafe_checkout_features, "fixture={name}");
        }
    }

    #[test]
    fn detached_and_in_progress_states_are_classified() {
        let detached = GitFixture::new();
        run(&detached.target, &["checkout", "--detach", "HEAD"]);
        let inventory = inventory_repository(&detached.authorization()).unwrap();
        assert!(inventory.condition.detached);
        assert_eq!(inventory.branch, None);

        let markers = [
            ("MERGE_HEAD", OperationState::Merging),
            ("rebase-merge", OperationState::Rebasing),
            ("BISECT_LOG", OperationState::Bisecting),
            ("index.lock", OperationState::Locked),
        ];
        for (marker, expected) in markers {
            let fixture = GitFixture::new();
            let path = fixture.target.join(".git").join(marker);
            if marker == "rebase-merge" {
                fs::create_dir(&path).unwrap();
            } else {
                fs::write(&path, fixture.head()).unwrap();
            }
            assert_eq!(
                inventory_repository(&fixture.authorization())
                    .unwrap()
                    .operation,
                expected
            );
        }
    }

    #[test]
    fn conflicted_porcelain_is_classified_and_unknown_records_fail() {
        let output = |stdout: &[u8]| GitOutput {
            stdout: stdout.to_vec(),
            stdout_sha256: String::new(),
            stderr_sha256: String::new(),
            exit_code: 0,
            elapsed: Duration::ZERO,
        };
        let conflicted =
            output(b"# branch.head main\nu UU N... 100644 100644 100644 100644 a b c path\n");
        assert!(parse_status(&conflicted).unwrap().1.conflicted);
        let unknown = output(b"# branch.head main\nx unsupported\n");
        assert_eq!(
            parse_status(&unknown).unwrap_err(),
            InventoryError::InvalidOutput
        );
    }

    #[test]
    fn hostile_configuration_executes_no_canary_or_network() {
        let fixture = GitFixture::new();
        let canary = fixture.root.join("canary.sh");
        let fired = fixture.root.join("canary-fired");
        fs::write(
            &canary,
            format!("#!/bin/sh\nprintf fired > '{}'\n", fired.display()),
        )
        .unwrap();
        fs::set_permissions(&canary, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            fixture.target.join(".git/hooks/post-checkout"),
            fs::read(&canary).unwrap(),
        )
        .unwrap();
        fs::set_permissions(
            fixture.target.join(".git/hooks/post-checkout"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let hostile = format!(
            "\n[core]\n  fsmonitor = {}\n  pager = {}\n[alias]\n  status = !{}\n[credential]\n  helper = !{}\n[diff \"canary\"]\n  external = {}\n[gpg]\n  program = {}\n[filter \"canary\"]\n  clean = {}\n  smudge = {}\n[url \"http://{}\"]\n  insteadOf = fixture:\n",
            canary.display(),
            canary.display(),
            canary.display(),
            canary.display(),
            canary.display(),
            canary.display(),
            canary.display(),
            canary.display(),
            address
        );
        let config_path = fixture.target.join(".git/config");
        let mut config = fs::read_to_string(&config_path).unwrap();
        config.push_str(&hostile);
        fs::write(&config_path, config).unwrap();

        let inventory = inventory_repository(&fixture.authorization()).unwrap();
        assert!(inventory.unsafe_checkout_features);
        assert!(!fired.exists());
        thread::sleep(Duration::from_millis(50));
        match listener.accept() {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            other => panic!("unexpected network observation: {other:?}"),
        }
    }

    #[test]
    fn alternates_and_checkout_attributes_are_detected_without_execution() {
        let fixture = GitFixture::new();
        fs::create_dir_all(fixture.target.join(".git/objects/info")).unwrap();
        fs::write(
            fixture.target.join(".git/objects/info/alternates"),
            "/unapproved/objects\n",
        )
        .unwrap();
        fs::write(
            fixture.target.join(".gitattributes"),
            "* filter=unapproved\n",
        )
        .unwrap();
        let inventory = inventory_repository(&fixture.authorization()).unwrap();
        assert!(inventory.unsafe_checkout_features);
    }

    #[test]
    fn every_hostile_configuration_family_and_replacement_ref_is_refused() {
        let fragments = [
            "\n[alias]\n  status = !false\n",
            "\n[credential]\n  helper = !false\n",
            "\n[diff \"x\"]\n  external = false\n",
            "\n[filter \"x\"]\n  clean = false\n",
            "\n[include]\n  path = /tmp/hostile\n",
            "\n[merge \"x\"]\n  driver = false\n",
            "\n[url \"https://example.invalid\"]\n  insteadOf = fixture:\n",
            "\n[core]\n  askPass = false\n",
            "\n[core]\n  editor = false\n",
            "\n[core]\n  fsmonitor = false\n",
            "\n[core]\n  pager = false\n",
            "\n[gpg]\n  program = false\n",
        ];
        for fragment in fragments {
            let fixture = GitFixture::new();
            let authorization = fixture.authorization();
            let path = fixture.target.join(".git/config");
            let mut config = fs::read_to_string(&path).unwrap();
            config.push_str(fragment);
            fs::write(path, config).unwrap();
            assert!(
                inventory_repository(&authorization)
                    .unwrap()
                    .unsafe_checkout_features,
                "fragment={fragment:?}"
            );
        }

        let fixture = GitFixture::new();
        let authorization = fixture.authorization();
        let replacement = fixture.target.join(".git/refs/replace");
        fs::create_dir_all(&replacement).unwrap();
        fs::write(replacement.join(fixture.head()), fixture.head()).unwrap();
        assert!(
            inventory_repository(&authorization)
                .unwrap()
                .unsafe_checkout_features
        );
    }

    #[test]
    fn active_hook_alone_is_refused_but_sample_hooks_are_inert() {
        let fixture = GitFixture::new();
        let hooks = fixture.target.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::write(hooks.join("pre-commit.sample"), "sample\n").unwrap();
        assert!(
            !inventory_repository(&fixture.authorization())
                .unwrap()
                .unsafe_checkout_features
        );
        fs::write(hooks.join("pre-commit"), "active\n").unwrap();
        assert!(
            inventory_repository(&fixture.authorization())
                .unwrap()
                .unsafe_checkout_features
        );
    }
}
