//! Minimal accessible directory browser for selecting configurations and repositories.
//!
//! Browsing only lists directory entries; it never follows symbolic links into other trees
//! and never modifies anything.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Maximum entries listed for one directory.
pub const MAX_ENTRIES: usize = 2000;

/// One listed entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    /// Entry name.
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Whether the entry is a directory (symbolic links are never treated as directories).
    pub is_dir: bool,
    /// Whether the entry is a symbolic link.
    pub is_symlink: bool,
}

/// Browser state.
#[derive(Clone, Debug)]
pub struct Browser {
    /// Directory being listed.
    pub current: PathBuf,
    /// Entries of the current directory.
    pub entries: Vec<Entry>,
    /// Listing failure, if any.
    pub error: Option<String>,
    /// Whether the listing was truncated at [`MAX_ENTRIES`].
    pub truncated: bool,
    /// Only show directories and files with these extensions (empty means all files).
    pub extensions: Vec<&'static str>,
}

impl Browser {
    /// Starts at the given directory.
    #[must_use]
    pub fn new(start: &Path, extensions: Vec<&'static str>) -> Self {
        let mut browser = Self {
            current: start.to_path_buf(),
            entries: Vec::new(),
            error: None,
            truncated: false,
            extensions,
        };
        browser.refresh();
        browser
    }

    /// Starts at the user's home directory or the filesystem root.
    #[must_use]
    pub fn at_home(extensions: Vec<&'static str>) -> Self {
        let start = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute() && home.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(&start, extensions)
    }

    /// Re-reads the current directory.
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;
        self.truncated = false;
        let read = match fs::read_dir(&self.current) {
            Ok(read) => read,
            Err(error) => {
                self.error = Some(format!("cannot list directory: {}", error.kind()));
                return;
            }
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let is_symlink = metadata.file_type().is_symlink();
            let is_dir = metadata.is_dir();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let keep = is_dir
                || self.extensions.is_empty()
                || self.extensions.iter().any(|extension| {
                    path.extension()
                        .is_some_and(|actual| actual.to_str() == Some(extension))
                });
            if !keep {
                continue;
            }
            self.entries.push(Entry {
                name,
                path,
                is_dir,
                is_symlink,
            });
            if self.entries.len() >= MAX_ENTRIES {
                self.truncated = true;
                break;
            }
        }
        self.entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.cmp(&right.name))
        });
    }

    /// Enters a directory entry.
    pub fn enter(&mut self, path: &Path) {
        if path.is_absolute() {
            self.current = path.to_path_buf();
            self.refresh();
        }
    }

    /// Moves to the parent directory when one exists.
    pub fn up(&mut self) {
        if let Some(parent) = self.current.parent() {
            self.current = parent.to_path_buf();
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_filters_hidden_links_and_extensions_and_never_writes() {
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-browser-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.toml"), b"x").unwrap();
        fs::write(root.join("b.txt"), b"x").unwrap();
        fs::write(root.join(".hidden.toml"), b"x").unwrap();
        std::os::unix::fs::symlink(root.join("sub"), root.join("link")).unwrap();
        let browser = Browser::new(&root, vec!["toml"]);
        let names = browser
            .entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.is_dir, entry.is_symlink))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                ("sub".to_owned(), true, false),
                ("a.toml".to_owned(), false, false),
            ]
        );
        let mut browser = browser;
        browser.enter(&root.join("sub"));
        assert!(browser.entries.is_empty());
        browser.up();
        assert_eq!(browser.current, root);
        browser.enter(&root.join("missing"));
        assert!(browser.error.is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
