//! Fail-closed Windows volume, path, and executable-identity validation.
//!
//! Every helper validates explicit caller-supplied values without touching the
//! filesystem, so the hostile fixtures run on any host. Native NTFS probing
//! (reparse-point enumeration, link-target resolution, live volume observation)
//! remains unimplemented until genuine Windows guest evidence exists; the
//! [`ReparseKind`] policy hook marks exactly where that probing will attach.

use std::collections::HashMap;

use super::PlatformError;

/// Maximum Windows path length in characters.
pub const MAX_WINDOWS_PATH_CHARS: usize = 32_767;

/// Maximum opaque volume-fingerprint length accepted by [`RepositoryIdentity`].
pub const MAX_VOLUME_FINGERPRINT_CHARS: usize = 256;

/// Reserved Win32 device stems, matched case-insensitively before any extension.
const RESERVED_DEVICE_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Returns true when `segment` names a reserved Win32 device, with or without
/// an extension and in any ASCII case.
///
/// # Examples
///
/// ```rust
/// use codingmage_platform::is_reserved_device_stem;
///
/// assert!(is_reserved_device_stem("NUL"));
/// assert!(is_reserved_device_stem("com1.txt"));
/// assert!(!is_reserved_device_stem("console"));
/// ```
#[must_use]
pub fn is_reserved_device_stem(segment: &str) -> bool {
    segment.split('.').next().is_some_and(|stem| {
        !stem.is_empty()
            && stem.len() <= 4
            && RESERVED_DEVICE_STEMS
                .iter()
                .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    })
}

/// Normalizes a Windows path without collapsing distinct identities.
///
/// Drive letters become uppercase and `/` separators become `\` outside the
/// `\\?\` namespace. The `\\?\` prefix forms keep literal segment semantics.
/// Each namespace normalizes only within itself: a drive-absolute path never
/// equals its `\\?\` counterpart.
///
/// Duplicate separators collapse, but `.` and `..` segments are refused rather
/// than resolved: only already-canonical inputs normalize, so a hostile
/// noncanonical spelling can never alias a bound identity.
///
/// Supported namespaces are drive-absolute (`C:\...`), long drive-absolute
/// (`\\?\C:\...`), and long UNC (`\\?\UNC\server\share\...`). Drive-relative,
/// bare UNC, `\\.\` device, rooted, and relative inputs are refused as
/// [`PlatformError::Unsupported`]; malformed content inside a supported
/// namespace fails as [`PlatformError::InvalidPath`].
///
/// # Errors
///
/// Returns [`PlatformError::InvalidPath`] for empty, overlong, control-bearing,
/// or otherwise malformed paths, and [`PlatformError::Unsupported`] for
/// namespaces without an implementation.
pub fn normalize_windows_path(raw: &str) -> Result<String, PlatformError> {
    if raw.is_empty()
        || raw.chars().count() > MAX_WINDOWS_PATH_CHARS
        || raw.chars().any(char::is_control)
    {
        return Err(PlatformError::InvalidPath);
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        return normalize_long_unc(rest);
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return normalize_long_drive(rest);
    }
    normalize_drive_absolute(raw)
}

/// Finds the first case-only collision in `paths`, if any.
///
/// Comparison uses Unicode lowercase folding without collapsing the stored
/// identities: byte-identical duplicates are the same identity and are
/// skipped, while two distinct spellings that fold together return their
/// indices. A repository walk containing a collision must fail closed before
/// any write.
#[must_use]
pub fn find_case_collision(paths: &[String]) -> Option<(usize, usize)> {
    let mut seen: HashMap<String, (usize, &str)> = HashMap::new();
    for (index, path) in paths.iter().enumerate() {
        let folded: String = path.chars().flat_map(char::to_lowercase).collect();
        match seen.get(&folded) {
            Some((first, original)) if *original != path.as_str() => {
                return Some((*first, index));
            }
            Some(_) => {}
            None => {
                seen.insert(folded, (index, path.as_str()));
            }
        }
    }
    None
}

/// Native reparse-point classification supplied by future Windows probing.
///
/// The coordinator never follows a link implicitly: only [`ReparseKind::Absent`]
/// admits a path today, and every other variant fails closed in
/// [`refuse_reparse`] until native enumeration evidence exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReparseKind {
    /// No reparse point was observed.
    Absent,
    /// Symbolic link; following it could escape owned authority.
    SymbolicLink,
    /// NTFS junction; retargeting it could escape owned authority.
    Junction,
    /// Mounted volume; its contents carry a separate identity.
    MountPoint,
    /// Any other reparse tag, including future or unknown values.
    Other,
}

/// Admits only [`ReparseKind::Absent`], refusing every link or mount.
///
/// # Errors
///
/// Returns [`PlatformError::InvalidPath`] for any observed reparse point.
pub fn refuse_reparse(kind: ReparseKind) -> Result<(), PlatformError> {
    if kind == ReparseKind::Absent {
        Ok(())
    } else {
        Err(PlatformError::InvalidPath)
    }
}

/// Exact executable identity binding a canonical path to content.
///
/// Replacement is detected by comparing a freshly observed digest and length
/// against the bound values; any mismatch fails closed. Digests are lowercase
/// hexadecimal SHA-256 strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableIdentity {
    /// Canonical normalized Windows path of the executable.
    canonical_path: String,
    /// Lowercase hexadecimal SHA-256 of the executable bytes.
    sha256: String,
    /// Exact executable length in bytes; zero is refused at bind time.
    byte_len: u64,
}

impl ExecutableIdentity {
    /// Binds an executable identity, requiring an already-canonical path.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidPath`] for a noncanonical path, a
    /// malformed digest, or a zero length.
    pub fn bind(canonical_path: &str, sha256: &str, byte_len: u64) -> Result<Self, PlatformError> {
        if normalize_windows_path(canonical_path).as_deref() != Ok(canonical_path)
            || !valid_digest(sha256)
            || byte_len == 0
        {
            return Err(PlatformError::InvalidPath);
        }
        Ok(Self {
            canonical_path: canonical_path.to_owned(),
            sha256: sha256.to_owned(),
            byte_len,
        })
    }

    /// Checks a fresh observation against the bound identity.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidPath`] for a malformed digest and
    /// [`PlatformError::ReplacementDetected`] when the observed content or
    /// length differs from the bound values.
    pub fn check(&self, sha256: &str, byte_len: u64) -> Result<(), PlatformError> {
        if !valid_digest(sha256) {
            return Err(PlatformError::InvalidPath);
        }
        if self.sha256 != sha256 || self.byte_len != byte_len {
            return Err(PlatformError::ReplacementDetected);
        }
        Ok(())
    }

    /// Returns the bound canonical path.
    #[must_use]
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
}

/// Exact repository-root identity binding a volume fingerprint to a path.
///
/// The fingerprint is an opaque caller-supplied volume identity (for example a
/// serial and object identifier observed by future native probing). Binding
/// and checking compare exact strings; a linked, moved, or replaced root fails
/// closed because at least one field differs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryIdentity {
    /// Opaque volume fingerprint observed at bind time.
    volume_fingerprint: String,
    /// Canonical normalized Windows path of the repository root.
    normalized_root: String,
}

impl RepositoryIdentity {
    /// Binds a repository identity, requiring an already-canonical root.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidPath`] for an empty, overlong,
    /// control-bearing, or noncanonical fingerprint or root.
    pub fn bind(volume_fingerprint: &str, root: &str) -> Result<Self, PlatformError> {
        if !valid_fingerprint(volume_fingerprint)
            || normalize_windows_path(root).as_deref() != Ok(root)
        {
            return Err(PlatformError::InvalidPath);
        }
        Ok(Self {
            volume_fingerprint: volume_fingerprint.to_owned(),
            normalized_root: root.to_owned(),
        })
    }

    /// Checks a fresh observation against the bound identity.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidPath`] for malformed inputs and
    /// [`PlatformError::ReplacementDetected`] when the fingerprint or root
    /// differs from the bound values.
    pub fn check(&self, volume_fingerprint: &str, root: &str) -> Result<(), PlatformError> {
        if !valid_fingerprint(volume_fingerprint)
            || normalize_windows_path(root).as_deref() != Ok(root)
        {
            return Err(PlatformError::InvalidPath);
        }
        if self.volume_fingerprint != volume_fingerprint || self.normalized_root != root {
            return Err(PlatformError::ReplacementDetected);
        }
        Ok(())
    }
}

/// Returns the drive-letter prefix length when `value` starts with `X:` plus
/// the required separator class.
fn drive_prefix_len(value: &str, long_namespace: bool) -> Option<usize> {
    let mut bytes = value.bytes();
    match (bytes.next(), bytes.next(), bytes.next()) {
        (Some(letter), Some(b':'), Some(separator))
            if letter.is_ascii_alphabetic()
                && (separator == b'\\' || (!long_namespace && separator == b'/')) =>
        {
            Some(3)
        }
        _ => None,
    }
}

fn validate_segment(segment: &str) -> Result<(), PlatformError> {
    if segment.is_empty()
        || segment
            .bytes()
            .any(|byte| matches!(byte, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*'))
    {
        return Err(PlatformError::InvalidPath);
    }
    let trimmed = segment.trim_end_matches(['.', ' ']);
    if trimmed.len() != segment.len() || is_reserved_device_stem(trimmed) {
        return Err(PlatformError::InvalidPath);
    }
    Ok(())
}

fn normalize_drive_absolute(raw: &str) -> Result<String, PlatformError> {
    if drive_prefix_len(raw, false).is_none() {
        return Err(PlatformError::Unsupported);
    }
    let Some(tail) = raw.get(3..) else {
        return Err(PlatformError::InvalidPath);
    };
    let mut normalized = String::with_capacity(raw.len() + 2);
    let drive_letter = raw.bytes().next().unwrap_or(b'C').to_ascii_uppercase();
    normalized.push(char::from(drive_letter));
    normalized.push(':');
    normalized.push('\\');
    for segment in tail.split(['\\', '/']) {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." {
            return Err(PlatformError::InvalidPath);
        }
        validate_segment(segment)?;
        normalized.push_str(segment);
        normalized.push('\\');
    }
    if normalized.len() > 3 {
        let _ = normalized.pop();
    }
    Ok(normalized)
}

fn normalize_long_drive(rest: &str) -> Result<String, PlatformError> {
    if rest.contains('/') || drive_prefix_len(rest, true).is_none() {
        return Err(PlatformError::InvalidPath);
    }
    let Some(tail) = rest.get(3..) else {
        return Err(PlatformError::InvalidPath);
    };
    let body = tail.strip_suffix('\\').unwrap_or(tail);
    let mut normalized = String::with_capacity(rest.len() + 4);
    let drive_letter = rest.bytes().next().unwrap_or(b'C').to_ascii_uppercase();
    normalized.push(char::from(drive_letter));
    normalized.push(':');
    normalized.push('\\');
    if !body.is_empty() {
        for segment in body.split('\\') {
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(PlatformError::InvalidPath);
            }
            validate_segment(segment)?;
            normalized.push_str(segment);
            normalized.push('\\');
        }
        let _ = normalized.pop();
    }
    Ok(format!(r"\\?\{normalized}"))
}

fn normalize_long_unc(rest: &str) -> Result<String, PlatformError> {
    if rest.contains('/') {
        return Err(PlatformError::InvalidPath);
    }
    let body = rest.strip_suffix('\\').unwrap_or(rest);
    let mut segments = body.split('\\');
    let (Some(server), Some(share)) = (segments.next(), segments.next()) else {
        return Err(PlatformError::InvalidPath);
    };
    if server.is_empty() || share.is_empty() {
        return Err(PlatformError::InvalidPath);
    }
    validate_segment(server)?;
    validate_segment(share)?;
    let mut normalized = format!(r"\\?\UNC\{server}\{share}");
    for segment in segments {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(PlatformError::InvalidPath);
        }
        validate_segment(segment)?;
        normalized.push('\\');
        normalized.push_str(segment);
    }
    Ok(normalized)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_fingerprint(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_VOLUME_FINGERPRINT_CHARS
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST_A: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const DIGEST_B: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn normalized(path: &str) -> String {
        normalize_windows_path(path).expect("valid fixture must normalize")
    }

    #[test]
    fn drive_paths_normalize_letter_separators_and_dots() {
        assert_eq!(normalized("c:/Foo/Bar"), r"C:\Foo\Bar");
        assert_eq!(normalized("D:\\\\Tools\\\\bin\\\\"), r"D:\Tools\bin");
        assert_eq!(
            normalize_windows_path("e:"),
            Err(PlatformError::Unsupported)
        );
    }

    #[test]
    fn drive_root_is_preserved() {
        assert_eq!(normalized("c:/"), "C:\\");
        assert_eq!(normalized(r"\\?\C:\"), r"\\?\C:\");
        assert_eq!(normalized(r"\\?\UNC\Server\Share"), r"\\?\UNC\Server\Share");
    }

    #[test]
    fn namespaces_never_collapse_into_each_other() {
        let drive = normalized(r"C:\Foo");
        let long = normalized(r"\\?\C:\Foo");
        assert_eq!(drive, r"C:\Foo");
        assert_eq!(long, r"\\?\C:\Foo");
        assert_ne!(drive, long);
    }

    #[test]
    fn long_namespaces_keep_literal_segments_and_case() {
        assert_eq!(normalized(r"\\?\D:\Pkgs\Lib"), r"\\?\D:\Pkgs\Lib");
        assert_eq!(
            normalized(r"\\?\UNC\BuildSrv\Artifacts\a"),
            r"\\?\UNC\BuildSrv\Artifacts\a"
        );
    }

    #[test]
    fn parent_segments_are_refused_not_resolved() {
        for raw in [
            r"C:\Foo\..\Bar",
            r"C:\..",
            r"\\?\C:\Foo\..\Bar",
            r"\\?\UNC\s\sh\..\x",
            r"\\?\C:\.",
            r"C:\.\.\Evil",
        ] {
            assert_eq!(
                normalize_windows_path(raw),
                Err(PlatformError::InvalidPath),
                "{raw}"
            );
        }
    }

    #[test]
    fn unsupported_namespaces_fail_closed() {
        for raw in [
            "C:relative",
            r"\\Server\Share\Dir",
            r"\\.\C:\Device",
            "/unix/absolute",
            "relative\\path",
        ] {
            assert_eq!(
                normalize_windows_path(raw),
                Err(PlatformError::Unsupported),
                "{raw}"
            );
        }
    }

    #[test]
    fn malformed_content_fails_closed() {
        let padding = "a".repeat(MAX_WINDOWS_PATH_CHARS);
        let overlong = format!("C:\\{padding}");
        for raw in [
            String::new(),
            "C:\\a*b".to_owned(),
            "C:\\a?b".to_owned(),
            "C:\\a<b".to_owned(),
            "C:\\bad\0name".to_owned(),
            "C:\\trailing.".to_owned(),
            "C:\\trailing ".to_owned(),
            r"\\?\C:/slash".to_owned(),
            r"\\?\UNC\s\sh/a".to_owned(),
            r"\\?\C:\\double".to_owned(),
            r"\\?\UNC\\share".to_owned(),
            overlong,
        ] {
            assert_eq!(
                normalize_windows_path(&raw),
                Err(PlatformError::InvalidPath),
                "{raw}"
            );
        }
    }

    #[test]
    fn reserved_device_names_are_rejected_with_and_without_extensions() {
        assert!(is_reserved_device_stem("NUL"));
        assert!(is_reserved_device_stem("nul.txt"));
        assert!(is_reserved_device_stem("Com9"));
        assert!(is_reserved_device_stem("LPT1.sys"));
        assert!(!is_reserved_device_stem("console"));
        assert!(!is_reserved_device_stem("COM10"));
        assert!(!is_reserved_device_stem("null device"));
        assert!(!is_reserved_device_stem(""));
        for raw in [
            r"C:\NUL",
            r"C:\Dir\aux.log",
            r"C:\NUL.",
            r"\\?\C:\COM1",
            r"\\?\UNC\s\sh\PRN",
        ] {
            assert_eq!(
                normalize_windows_path(raw),
                Err(PlatformError::InvalidPath),
                "{raw}"
            );
        }
    }

    #[test]
    fn case_collisions_are_detected_without_collapsing_identities() {
        let colliding = vec!["C:\\Foo".to_owned(), "c:\\FOO".to_owned()];
        assert_eq!(find_case_collision(&colliding), Some((0, 1)));
        let distinct = vec!["C:\\Foo".to_owned(), "C:\\Bar".to_owned()];
        assert_eq!(find_case_collision(&distinct), None);
        let duplicates = vec!["C:\\Foo".to_owned(), "C:\\Foo".to_owned()];
        assert_eq!(find_case_collision(&duplicates), None);
        let empty: Vec<String> = Vec::new();
        assert_eq!(find_case_collision(&empty), None);
    }

    #[test]
    fn executable_identity_detects_replacement() {
        let bound = ExecutableIdentity::bind(r"C:\Tools\codingmage.exe", DIGEST_A, 1024)
            .expect("valid fixture must bind");
        assert_eq!(bound.canonical_path(), r"C:\Tools\codingmage.exe");
        assert!(bound.check(DIGEST_A, 1024).is_ok());
        assert_eq!(
            bound.check(DIGEST_B, 1024),
            Err(PlatformError::ReplacementDetected)
        );
        assert_eq!(
            bound.check(DIGEST_A, 2048),
            Err(PlatformError::ReplacementDetected)
        );
        assert_eq!(
            bound.check("not-a-digest", 1024),
            Err(PlatformError::InvalidPath)
        );
    }

    #[test]
    fn executable_bind_rejects_noncanonical_and_empty_inputs() {
        assert_eq!(
            ExecutableIdentity::bind("c:/Tools/app.exe", DIGEST_A, 64),
            Err(PlatformError::InvalidPath)
        );
        assert_eq!(
            ExecutableIdentity::bind(r"C:\Tools\app.exe", "ABC", 64),
            Err(PlatformError::InvalidPath)
        );
        assert_eq!(
            ExecutableIdentity::bind(r"C:\Tools\app.exe", DIGEST_A, 0),
            Err(PlatformError::InvalidPath)
        );
        assert_eq!(
            ExecutableIdentity::bind(r"C:\NUL", DIGEST_A, 64),
            Err(PlatformError::InvalidPath)
        );
    }

    #[test]
    fn repository_identity_detects_link_or_replace() {
        let bound = RepositoryIdentity::bind("volume-9f27", r"D:\Repos\Target")
            .expect("valid fixture must bind");
        assert!(bound.check("volume-9f27", r"D:\Repos\Target").is_ok());
        assert_eq!(
            bound.check("volume-0000", r"D:\Repos\Target"),
            Err(PlatformError::ReplacementDetected)
        );
        assert_eq!(
            bound.check("volume-9f27", r"D:\Repos\Other"),
            Err(PlatformError::ReplacementDetected)
        );
        assert_eq!(
            RepositoryIdentity::bind("", r"D:\Repos\Target"),
            Err(PlatformError::InvalidPath)
        );
        assert_eq!(
            RepositoryIdentity::bind("volume-9f27", "d:/Repos/Target"),
            Err(PlatformError::InvalidPath)
        );
    }

    #[test]
    fn every_reparse_kind_but_absence_is_refused() {
        assert!(refuse_reparse(ReparseKind::Absent).is_ok());
        for kind in [
            ReparseKind::SymbolicLink,
            ReparseKind::Junction,
            ReparseKind::MountPoint,
            ReparseKind::Other,
        ] {
            assert_eq!(refuse_reparse(kind), Err(PlatformError::InvalidPath));
        }
    }

    #[test]
    fn digest_and_fingerprint_boundaries_hold() {
        assert!(valid_digest(DIGEST_A));
        assert!(!valid_digest(&DIGEST_A.to_ascii_uppercase()));
        assert!(!valid_digest("abc"));
        assert!(valid_fingerprint("serial-1"));
        assert!(!valid_fingerprint(""));
        assert!(!valid_fingerprint(
            &"x".repeat(MAX_VOLUME_FINGERPRINT_CHARS + 1)
        ));
    }
}
