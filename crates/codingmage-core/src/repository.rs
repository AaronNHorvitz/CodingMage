//! Filesystem-bound authorization for one external Git repository.

use std::{
    fmt, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use codingmage_contracts::RepositoryId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Config;

const MAX_HEAD_BYTES: u64 = 4096;
const MAX_CONFIG_BYTES: u64 = 1_048_576;

/// Stable filesystem identity for a held directory.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemIdentity {
    device: u64,
    inode: u64,
}

/// Redacted identity of a configured remote.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteIdentity {
    /// Bounded remote name.
    pub name: String,
    /// SHA-256 fingerprint of the URL; the URL itself is not retained.
    pub url_sha256: String,
}

/// Immutable repository facts recorded when authority is granted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryIdentity {
    /// Coordinator identifier derived from filesystem identity.
    pub repository_id: RepositoryId,
    /// Canonical target path.
    pub canonical_path: PathBuf,
    /// Physical target-directory identity.
    pub target: FilesystemIdentity,
    /// Canonical Git metadata path.
    pub git_directory: PathBuf,
    /// Physical Git-directory identity.
    pub git: FilesystemIdentity,
    /// Initial symbolic or detached head and its resolved object when available.
    pub initial_head: String,
    /// Redacted remote identities.
    pub remotes: Vec<RemoteIdentity>,
}

/// Held repository authority that must be revalidated before a mutation.
pub struct RepositoryAuthorization {
    identity: RepositoryIdentity,
    target_handle: File,
    git_handle: File,
}

impl fmt::Debug for RepositoryAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepositoryAuthorization")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// Content-free repository authorization failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryAuthorizationError {
    /// A configured path was unavailable or unsupported.
    Unavailable,
    /// A configured root was a symbolic link.
    Symlink,
    /// The repository was bare.
    Bare,
    /// A parent repository made the target ambiguous.
    Nested,
    /// The repository metadata format was unsupported or malformed.
    UnsupportedFormat,
    /// The repository or Git metadata was not owned by the current user.
    UnsafeOwnership,
    /// Source, target, scratch, or state authority overlapped.
    Overlap,
    /// The target was `CodingMage` itself.
    SelfTarget,
    /// The authorized physical identity or initial head changed.
    IdentityChanged,
}

impl RepositoryAuthorizationError {
    /// Returns the stable public error code for this refusal.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "codingmage.repository.unavailable",
            Self::Symlink => "codingmage.repository.symlink",
            Self::Bare => "codingmage.repository.bare",
            Self::Nested => "codingmage.repository.nested",
            Self::UnsupportedFormat => "codingmage.repository.unsupported_format",
            Self::UnsafeOwnership => "codingmage.repository.unsafe_ownership",
            Self::Overlap => "codingmage.repository.authority_overlap",
            Self::SelfTarget => "codingmage.repository.self_target",
            Self::IdentityChanged => "codingmage.repository.identity_changed",
        }
    }
}

impl fmt::Display for RepositoryAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for RepositoryAuthorizationError {}

impl RepositoryAuthorization {
    /// Authorizes the configured target and holds its physical identity.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryAuthorizationError`] before granting authority when any configured root,
    /// repository format, ownership, or overlap check fails.
    pub fn authorize(
        config: &Config,
        codingmage_source: &Path,
    ) -> Result<Self, RepositoryAuthorizationError> {
        let source = checked_root(codingmage_source)?;
        let target = checked_root(&config.target_path)?;
        let scratch = checked_root(&config.scratch_root)?;
        let state = checked_root(&config.state_root)?;
        let roots = [&source, &target, &scratch, &state];

        if source.path == target.path || source.identity == target.identity {
            return Err(RepositoryAuthorizationError::SelfTarget);
        }
        for (index, left) in roots.iter().enumerate() {
            for right in roots.iter().skip(index + 1) {
                if roots_overlap(left, right) {
                    return Err(RepositoryAuthorizationError::Overlap);
                }
            }
        }

        reject_bare_or_nested(&target.path)?;
        let git_path = resolve_git_directory(&target.path)?;
        let git_root = checked_root(&git_path)?;
        ensure_owned(&target.metadata)?;
        ensure_owned(&git_root.metadata)?;

        let target_handle =
            File::open(&target.path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
        let git_handle =
            File::open(&git_root.path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
        let initial_head = read_head(&git_root.path)?;
        let remotes = read_remote_identities(&git_root.path)?;
        let repository_id = RepositoryId::new(format!(
            "repo-{:x}-{:x}",
            target.identity.device, target.identity.inode
        ))
        .map_err(|_| RepositoryAuthorizationError::UnsupportedFormat)?;

        Ok(Self {
            identity: RepositoryIdentity {
                repository_id,
                canonical_path: target.path,
                target: target.identity,
                git_directory: git_root.path,
                git: git_root.identity,
                initial_head,
                remotes,
            },
            target_handle,
            git_handle,
        })
    }

    /// Returns the immutable authorization facts.
    #[must_use]
    pub const fn identity(&self) -> &RepositoryIdentity {
        &self.identity
    }

    /// Revalidates path, held handles, Git metadata, and initial head.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryAuthorizationError::IdentityChanged`] if any authority-bearing identity
    /// no longer matches the authorization snapshot.
    pub fn revalidate(&self) -> Result<(), RepositoryAuthorizationError> {
        let target = checked_root(&self.identity.canonical_path)
            .map_err(|_| RepositoryAuthorizationError::IdentityChanged)?;
        let git = checked_root(&self.identity.git_directory)
            .map_err(|_| RepositoryAuthorizationError::IdentityChanged)?;
        let held_target = filesystem_identity(
            &self
                .target_handle
                .metadata()
                .map_err(|_| RepositoryAuthorizationError::IdentityChanged)?,
        );
        let held_git = filesystem_identity(
            &self
                .git_handle
                .metadata()
                .map_err(|_| RepositoryAuthorizationError::IdentityChanged)?,
        );
        let head = read_head(&self.identity.git_directory)
            .map_err(|_| RepositoryAuthorizationError::IdentityChanged)?;

        if target.identity != self.identity.target
            || git.identity != self.identity.git
            || held_target != self.identity.target
            || held_git != self.identity.git
            || head != self.identity.initial_head
        {
            return Err(RepositoryAuthorizationError::IdentityChanged);
        }
        Ok(())
    }
}

struct CheckedRoot {
    path: PathBuf,
    metadata: fs::Metadata,
    identity: FilesystemIdentity,
}

fn checked_root(path: &Path) -> Result<CheckedRoot, RepositoryAuthorizationError> {
    let link_metadata =
        fs::symlink_metadata(path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    if link_metadata.file_type().is_symlink() {
        return Err(RepositoryAuthorizationError::Symlink);
    }
    if !link_metadata.is_dir() {
        return Err(RepositoryAuthorizationError::Unavailable);
    }
    let canonical =
        fs::canonicalize(path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    let metadata =
        fs::metadata(&canonical).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    let identity = filesystem_identity(&metadata);
    Ok(CheckedRoot {
        path: canonical,
        metadata,
        identity,
    })
}

fn roots_overlap(left: &CheckedRoot, right: &CheckedRoot) -> bool {
    left.identity == right.identity
        || left.path.starts_with(&right.path)
        || right.path.starts_with(&left.path)
}

fn reject_bare_or_nested(target: &Path) -> Result<(), RepositoryAuthorizationError> {
    if !target.join(".git").exists()
        && target.join("HEAD").is_file()
        && target.join("objects").is_dir()
    {
        return Err(RepositoryAuthorizationError::Bare);
    }
    for parent in target.ancestors().skip(1) {
        if parent.join(".git").exists() {
            return Err(RepositoryAuthorizationError::Nested);
        }
    }
    Ok(())
}

fn resolve_git_directory(target: &Path) -> Result<PathBuf, RepositoryAuthorizationError> {
    let marker = target.join(".git");
    let metadata =
        fs::symlink_metadata(&marker).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(RepositoryAuthorizationError::Symlink);
    }
    if metadata.is_dir() {
        return Ok(marker);
    }
    if !metadata.is_file() || metadata.len() > MAX_HEAD_BYTES {
        return Err(RepositoryAuthorizationError::UnsupportedFormat);
    }
    let content = read_bounded(&marker, MAX_HEAD_BYTES)?;
    let raw = content
        .strip_prefix("gitdir:")
        .ok_or(RepositoryAuthorizationError::UnsupportedFormat)?
        .trim();
    if raw.is_empty() || raw.contains('\0') {
        return Err(RepositoryAuthorizationError::UnsupportedFormat);
    }
    let path = Path::new(raw);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        target.join(path)
    })
}

fn read_head(git: &Path) -> Result<String, RepositoryAuthorizationError> {
    let head = read_bounded(&git.join("HEAD"), MAX_HEAD_BYTES)?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        if !valid_ref(reference) {
            return Err(RepositoryAuthorizationError::UnsupportedFormat);
        }
        let object = read_ref(git, reference)?.unwrap_or_else(|| "unborn".to_owned());
        return Ok(format!("ref:{reference}@{object}"));
    }
    if valid_object_id(head) {
        return Ok(format!("oid:{head}"));
    }
    Err(RepositoryAuthorizationError::UnsupportedFormat)
}

fn read_ref(git: &Path, reference: &str) -> Result<Option<String>, RepositoryAuthorizationError> {
    let loose = git.join(reference);
    if loose.is_file() {
        let value = read_bounded(&loose, MAX_HEAD_BYTES)?;
        let value = value.trim();
        if !valid_object_id(value) {
            return Err(RepositoryAuthorizationError::UnsupportedFormat);
        }
        return Ok(Some(value.to_owned()));
    }
    let packed = git.join("packed-refs");
    if !packed.exists() {
        return Ok(None);
    }
    for line in read_bounded(&packed, MAX_CONFIG_BYTES)?.lines() {
        if line.starts_with(['#', '^']) {
            continue;
        }
        if let Some((object, name)) = line.split_once(' ')
            && name == reference
            && valid_object_id(object)
        {
            return Ok(Some(object.to_owned()));
        }
    }
    Ok(None)
}

fn valid_ref(reference: &str) -> bool {
    reference.starts_with("refs/")
        && reference.len() <= 1024
        && !reference.contains("..")
        && !reference.contains("@{")
        && !reference.ends_with(['.', '/'])
        && !reference
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_remote_identities(git: &Path) -> Result<Vec<RemoteIdentity>, RepositoryAuthorizationError> {
    let path = git.join("config");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let content = read_bounded(&path, MAX_CONFIG_BYTES)?;
    let mut current_remote: Option<String> = None;
    let mut remotes = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            if line.starts_with("[include") {
                return Err(RepositoryAuthorizationError::UnsupportedFormat);
            }
            current_remote = parse_remote_section(line);
            continue;
        }
        let Some(name) = &current_remote else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("url") {
            let value = value.trim();
            let digest = Sha256::digest(value.as_bytes());
            remotes.push(RemoteIdentity {
                name: name.clone(),
                url_sha256: hex_bytes(digest.as_ref()),
            });
        }
    }
    remotes.sort_by(|left, right| {
        (&left.name, &left.url_sha256).cmp(&(&right.name, &right.url_sha256))
    });
    Ok(remotes)
}

fn parse_remote_section(line: &str) -> Option<String> {
    let body = line.strip_prefix("[remote \"")?.strip_suffix("\"]")?;
    if body.is_empty()
        || body.len() > 128
        || !body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return None;
    }
    Some(body.to_owned())
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

fn read_bounded(path: &Path, limit: u64) -> Result<String, RepositoryAuthorizationError> {
    let metadata = fs::metadata(path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    if metadata.len() > limit {
        return Err(RepositoryAuthorizationError::UnsupportedFormat);
    }
    let file = File::open(path).map_err(|_| RepositoryAuthorizationError::Unavailable)?;
    let mut content = String::new();
    file.take(limit + 1)
        .read_to_string(&mut content)
        .map_err(|_| RepositoryAuthorizationError::UnsupportedFormat)?;
    if content.len() as u64 > limit {
        return Err(RepositoryAuthorizationError::UnsupportedFormat);
    }
    Ok(content)
}

#[cfg(unix)]
fn filesystem_identity(metadata: &fs::Metadata) -> FilesystemIdentity {
    use std::os::unix::fs::MetadataExt;

    FilesystemIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(not(unix))]
compile_error!("Sprint 2 repository identity currently requires a Unix platform adapter");

#[cfg(target_os = "linux")]
fn ensure_owned(metadata: &fs::Metadata) -> Result<(), RepositoryAuthorizationError> {
    use std::os::unix::fs::MetadataExt;

    let status = fs::read_to_string("/proc/self/status")
        .map_err(|_| RepositoryAuthorizationError::UnsafeOwnership)?;
    let effective = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(RepositoryAuthorizationError::UnsafeOwnership)?;
    if !ownership_matches(metadata.uid(), effective) {
        return Err(RepositoryAuthorizationError::UnsafeOwnership);
    }
    Ok(())
}

const fn ownership_matches(owner: u32, effective: u32) -> bool {
    owner == effective
}

#[cfg(all(unix, not(target_os = "linux")))]
fn ensure_owned(_metadata: &fs::Metadata) -> Result<(), RepositoryAuthorizationError> {
    Err(RepositoryAuthorizationError::UnsafeOwnership)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::{AgentProfile, CapabilityPolicy, CommandSpec, PublicationMode, PublicationPolicy};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        target: PathBuf,
        scratch: PathBuf,
        state: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "codingmage-repository-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let source = root.join("source");
            let target = root.join("target");
            let scratch = root.join("scratch");
            let state = root.join("state");
            for path in [&source, &target, &scratch, &state] {
                fs::create_dir_all(path).unwrap();
            }
            fs::create_dir_all(target.join(".git/refs/heads")).unwrap();
            fs::write(target.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
            fs::write(
                target.join(".git/refs/heads/main"),
                "0123456789abcdef0123456789abcdef01234567\n",
            )
            .unwrap();
            Self {
                root,
                source,
                target,
                scratch,
                state,
            }
        }

        fn config(&self) -> Config {
            Config {
                version: 1,
                target_path: self.target.clone(),
                task_source: PathBuf::from("TASKS.md"),
                default_branch: "main".to_owned(),
                integration_branch: "codingmage/integration".to_owned(),
                scratch_root: self.scratch.clone(),
                state_root: self.state.clone(),
                agent_profiles: vec![AgentProfile {
                    id: codingmage_contracts::AgentId::new("agent-1").unwrap(),
                    provider: "fake".to_owned(),
                    model: "fixture".to_owned(),
                }],
                correction_limit: 3,
                gate_commands: vec![CommandSpec {
                    executable: PathBuf::from("/usr/bin/true"),
                    args: Vec::new(),
                }],
                capabilities: CapabilityPolicy::default(),
                publication: PublicationPolicy {
                    mode: PublicationMode::LocalOnly,
                },
                allow_parent_discovery: false,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn authorizes_held_identity_and_redacts_remote_url() {
        let fixture = Fixture::new();
        let sensitive = ["credential", "-fixture"].concat();
        fs::write(
            fixture.target.join(".git/config"),
            format!("[remote \"origin\"]\n  url = https://user:{sensitive}@example.invalid/repo\n"),
        )
        .unwrap();
        let authorization =
            RepositoryAuthorization::authorize(&fixture.config(), &fixture.source).unwrap();
        authorization.revalidate().unwrap();
        let encoded = toml::to_string(authorization.identity()).unwrap();
        assert!(!encoded.contains(&sensitive));
        assert_eq!(authorization.identity().remotes.len(), 1);
        assert_eq!(authorization.identity().remotes[0].url_sha256.len(), 64);
    }

    #[test]
    fn renamed_and_replaced_targets_fail_revalidation() {
        let fixture = Fixture::new();
        let authorization =
            RepositoryAuthorization::authorize(&fixture.config(), &fixture.source).unwrap();
        let moved = fixture.root.join("moved-target");
        fs::rename(&fixture.target, &moved).unwrap();
        assert_eq!(
            authorization.revalidate().unwrap_err(),
            RepositoryAuthorizationError::IdentityChanged
        );
        fs::create_dir_all(&fixture.target).unwrap();
        assert_eq!(
            authorization.revalidate().unwrap_err(),
            RepositoryAuthorizationError::IdentityChanged
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_is_rejected() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let alias = fixture.root.join("target-alias");
        symlink(&fixture.target, &alias).unwrap();
        let mut config = fixture.config();
        config.target_path = alias;
        assert_eq!(
            RepositoryAuthorization::authorize(&config, &fixture.source).unwrap_err(),
            RepositoryAuthorizationError::Symlink
        );
    }

    #[test]
    fn self_target_and_overlapping_roots_are_rejected() {
        let fixture = Fixture::new();
        let mut self_config = fixture.config();
        self_config.target_path = fixture.source.clone();
        assert_eq!(
            RepositoryAuthorization::authorize(&self_config, &fixture.source).unwrap_err(),
            RepositoryAuthorizationError::SelfTarget
        );

        let mut overlap = fixture.config();
        overlap.scratch_root = fixture.target.join("scratch");
        fs::create_dir_all(&overlap.scratch_root).unwrap();
        assert_eq!(
            RepositoryAuthorization::authorize(&overlap, &fixture.source).unwrap_err(),
            RepositoryAuthorizationError::Overlap
        );
    }

    #[test]
    fn physical_identity_detects_synthetic_bind_alias() {
        let fixture = Fixture::new();
        let target = checked_root(&fixture.target).unwrap();
        let mut alias = checked_root(&fixture.scratch).unwrap();
        alias.identity = target.identity;
        assert!(roots_overlap(&target, &alias));
    }

    #[test]
    fn unsafe_owner_identity_is_rejected() {
        assert!(ownership_matches(1000, 1000));
        assert!(!ownership_matches(1001, 1000));
    }

    #[test]
    fn bare_and_nested_repositories_are_rejected() {
        let fixture = Fixture::new();
        fs::remove_dir_all(fixture.target.join(".git")).unwrap();
        fs::create_dir_all(fixture.target.join("objects")).unwrap();
        fs::write(fixture.target.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(
            RepositoryAuthorization::authorize(&fixture.config(), &fixture.source).unwrap_err(),
            RepositoryAuthorizationError::Bare
        );

        let nested = Fixture::new();
        fs::create_dir_all(nested.root.join(".git")).unwrap();
        assert_eq!(
            RepositoryAuthorization::authorize(&nested.config(), &nested.source).unwrap_err(),
            RepositoryAuthorizationError::Nested
        );
    }

    #[test]
    fn changed_head_fails_revalidation() {
        let fixture = Fixture::new();
        let authorization =
            RepositoryAuthorization::authorize(&fixture.config(), &fixture.source).unwrap();
        fs::write(
            fixture.target.join(".git/refs/heads/main"),
            "ffffffffffffffffffffffffffffffffffffffff\n",
        )
        .unwrap();
        assert_eq!(
            authorization.revalidate().unwrap_err(),
            RepositoryAuthorizationError::IdentityChanged
        );
    }
}
