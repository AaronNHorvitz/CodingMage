//! Presentation state for bounded public coordinator directory snapshots.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

/// Maximum entries accepted from one directory snapshot.
pub const MAX_ENTRIES: usize = 2_000;

/// One listed entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// Entry name.
    pub name: String,
    /// Absolute path derived from the selected directory and name.
    pub path: PathBuf,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symbolic link.
    pub is_symlink: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectorySnapshot {
    schema_version: u16,
    directory: PathBuf,
    entries: Vec<SnapshotEntry>,
    truncated: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotEntry {
    name: String,
    kind: EntryKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    Directory,
    File,
    Symlink,
}

/// Browser state for one selection surface; it performs no filesystem reads.
#[derive(Clone, Debug)]
pub struct Browser {
    /// Directory being listed.
    pub current: PathBuf,
    /// Entries of the current directory.
    pub entries: Vec<Entry>,
    /// Listing failure, if any.
    pub error: Option<String>,
    /// Whether a listing is in flight.
    pub loading: bool,
    /// Whether the coordinator truncated the listing.
    pub truncated: bool,
    /// Whether the selected directory has a valid current snapshot.
    pub ready: bool,
    /// Only show directories and files with these extensions (empty means all files).
    pub extensions: Vec<&'static str>,
    serial: u64,
}

impl Browser {
    /// Starts at the given directory without reading it.
    #[must_use]
    pub fn new(start: &Path, extensions: Vec<&'static str>) -> Self {
        Self {
            current: start.to_path_buf(),
            entries: Vec::new(),
            error: None,
            loading: false,
            truncated: false,
            ready: false,
            extensions,
            serial: 0,
        }
    }

    /// Starts at the configured home path or filesystem root without reading it.
    #[must_use]
    pub fn at_home(extensions: Vec<&'static str>) -> Self {
        let start = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(&start, extensions)
    }

    /// Clears old rows and returns a new per-browser request identity.
    pub fn begin_listing(&mut self, identity: u64) -> String {
        self.serial = identity;
        self.entries.clear();
        self.error = None;
        self.loading = true;
        self.truncated = false;
        self.ready = false;
        self.serial.to_string()
    }

    /// True only for the current in-flight listing.
    #[must_use]
    pub fn matches_request(&self, request_id: Option<&str>) -> bool {
        self.loading && request_id == Some(self.serial.to_string().as_str())
    }

    /// Accepts one exact-directory snapshot or presents a contract failure.
    pub fn apply_snapshot(&mut self, bytes: &[u8]) {
        self.loading = false;
        self.entries.clear();
        self.ready = false;
        let Ok(snapshot) = serde_json::from_slice::<DirectorySnapshot>(bytes) else {
            self.error = Some("directory response is malformed; retry browsing".to_owned());
            return;
        };
        if snapshot.schema_version != 1
            || snapshot.directory != self.current
            || snapshot.entries.len() > MAX_ENTRIES
        {
            self.error =
                Some("directory response did not match the selected path or contract".to_owned());
            return;
        }
        let mut names = BTreeSet::new();
        let mut entries = Vec::new();
        for entry in snapshot.entries {
            if entry.name.is_empty()
                || entry.name.starts_with('.')
                || entry.name.contains('\0')
                || !matches!(
                    Path::new(&entry.name)
                        .components()
                        .collect::<Vec<_>>()
                        .as_slice(),
                    [Component::Normal(_)]
                )
                || !names.insert(entry.name.clone())
            {
                self.error = Some("directory response contains an unsafe entry".to_owned());
                return;
            }
            let is_dir = matches!(entry.kind, EntryKind::Directory);
            let is_symlink = matches!(entry.kind, EntryKind::Symlink);
            let path = self.current.join(&entry.name);
            let keep = is_dir
                || self.extensions.is_empty()
                || self.extensions.iter().any(|extension| {
                    path.extension()
                        .is_some_and(|actual| actual.to_str() == Some(extension))
                });
            if keep {
                entries.push(Entry {
                    name: entry.name,
                    path,
                    is_dir,
                    is_symlink,
                });
            }
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.cmp(&right.name))
        });
        self.entries = entries;
        self.truncated = snapshot.truncated;
        self.error = None;
        self.ready = true;
    }

    /// Presents a failed public listing without retaining old rows.
    pub fn fail_listing(&mut self, code: &str) {
        self.loading = false;
        self.entries.clear();
        self.truncated = false;
        self.ready = false;
        self.error = Some(format!(
            "cannot list directory ({code}); retry or choose another path"
        ));
    }

    /// Enters one displayed directory without reading it.
    pub fn enter(&mut self, path: &Path) -> bool {
        if !self
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.is_dir && !entry.is_symlink)
        {
            return false;
        }
        self.current = path.to_path_buf();
        self.entries.clear();
        self.loading = false;
        self.error = None;
        self.truncated = false;
        self.ready = false;
        true
    }

    /// Moves to the parent directory when one exists, without reading it.
    pub fn up(&mut self) -> bool {
        let Some(parent) = self.current.parent() else {
            return false;
        };
        if parent == self.current {
            return false;
        }
        self.current = parent.to_path_buf();
        self.entries.clear();
        self.loading = false;
        self.error = None;
        self.truncated = false;
        self.ready = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_snapshot_filters_entries_and_rejects_forged_navigation() {
        let mut browser = Browser::new(Path::new("/example"), vec!["toml"]);
        assert_eq!(browser.begin_listing(1), "1");
        browser.apply_snapshot(br#"{"schema_version":1,"directory":"/example","entries":[{"name":"b.txt","kind":"file"},{"name":"a.toml","kind":"file"},{"name":"sub","kind":"directory"},{"name":"link.toml","kind":"symlink"}],"truncated":false}"#);
        let names = browser
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["sub", "a.toml", "link.toml"]);
        assert!(browser.ready);
        assert!(!browser.enter(Path::new("/example/link.toml")));
        assert!(!browser.enter(Path::new("/example/forged")));
        assert!(browser.enter(Path::new("/example/sub")));
        assert!(!browser.ready);
        assert!(browser.up());
    }

    #[test]
    fn malformed_cross_directory_and_unsafe_rows_leave_no_result() {
        let mut browser = Browser::new(Path::new("/example"), vec![]);
        browser.begin_listing(1);
        browser.apply_snapshot(
            br#"{"schema_version":1,"directory":"/other","entries":[],"truncated":false}"#,
        );
        assert!(browser.error.is_some());
        assert!(!browser.ready);
        browser.begin_listing(2);
        browser.apply_snapshot(br#"{"schema_version":1,"directory":"/example","entries":[{"name":"../escape","kind":"directory"}],"truncated":false}"#);
        assert!(browser.entries.is_empty());
        assert!(browser.error.is_some());
        browser.begin_listing(3);
        browser.apply_snapshot(
            br#"{"schema_version":9,"directory":"/example","entries":[],"truncated":false}"#,
        );
        assert!(browser.error.is_some());
    }

    #[test]
    fn unknown_fields_and_duplicate_names_refuse_a_listing() {
        let mut browser = Browser::new(Path::new("/example"), vec![]);
        browser.begin_listing(1);
        browser.apply_snapshot(br#"{"schema_version":1,"directory":"/example","entries":[],"truncated":false,"future":"unexpected"}"#);
        assert!(browser.error.is_some());
        assert!(!browser.ready);
        browser.begin_listing(2);
        browser.apply_snapshot(br#"{"schema_version":1,"directory":"/example","entries":[{"name":"same","kind":"directory"},{"name":"same","kind":"file"}],"truncated":false}"#);
        assert!(browser.error.is_some());
        assert!(browser.entries.is_empty());
        assert!(!browser.ready);
    }
}
