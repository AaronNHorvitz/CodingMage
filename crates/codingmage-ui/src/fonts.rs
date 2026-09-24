//! System font discovery.
//!
//! The interface bundles no font because the toolkit's default fonts carry licences outside the
//! admitted list (Decision 0015). It loads one proportional and one monospace font from the
//! host's standard font directories, or from `CODINGMAGE_UI_FONT`, and fails visibly otherwise.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

/// Largest font file the interface will load.
pub const MAX_FONT_BYTES: u64 = 32 * 1024 * 1024;

const PROPORTIONAL_CANDIDATES: &[&str] = &[
    "LiberationSans-Regular.ttf",
    "DejaVuSans.ttf",
    "NotoSans-Regular.ttf",
    "OpenSans-Regular.ttf",
    "Cantarell-Regular.otf",
    "Ubuntu-R.ttf",
    "FreeSans.otf",
];

const MONOSPACE_CANDIDATES: &[&str] = &[
    "LiberationMono-Regular.ttf",
    "DejaVuSansMono.ttf",
    "NotoSansMono-Regular.ttf",
    "Hack-Regular.ttf",
    "UbuntuMono-R.ttf",
    "FreeMono.otf",
];

/// Font bytes selected for the interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedFonts {
    /// Proportional font file.
    pub proportional: PathBuf,
    /// Monospace font file; falls back to the proportional file when none is found.
    pub monospace: PathBuf,
}

/// Why no usable font could be selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FontError {
    /// No candidate file exists in any searched directory.
    NotFound {
        /// Directories that were searched.
        searched: Vec<PathBuf>,
    },
    /// The override or selected file is unreadable, linked or oversized.
    Unreadable(PathBuf),
}

impl fmt::Display for FontError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { searched } => {
                write!(
                    formatter,
                    "no sans-serif TrueType or OpenType font was found; install a font package or set CODINGMAGE_UI_FONT to a font file. Searched:"
                )?;
                for directory in searched {
                    write!(formatter, " {}", directory.display())?;
                }
                Ok(())
            }
            Self::Unreadable(path) => write!(
                formatter,
                "the font file {} could not be read as a regular file under the size limit",
                path.display()
            ),
        }
    }
}

impl std::error::Error for FontError {}

/// Standard directories searched for fonts, in order.
#[must_use]
pub fn search_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        directories.push(home.join(".local/share/fonts"));
        directories.push(home.join(".fonts"));
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        directories.push(PathBuf::from(data_home).join("fonts"));
    }
    directories.push(PathBuf::from("/usr/local/share/fonts"));
    directories.push(PathBuf::from("/usr/share/fonts"));
    directories
}

/// Selects fonts from the override variable or the searched directories.
///
/// # Errors
///
/// Returns [`FontError`] when no usable font exists.
pub fn select_fonts() -> Result<SelectedFonts, FontError> {
    if let Some(override_path) = std::env::var_os("CODINGMAGE_UI_FONT") {
        let path = PathBuf::from(override_path);
        readable(&path)?;
        return Ok(SelectedFonts {
            monospace: path.clone(),
            proportional: path,
        });
    }
    let directories = search_directories();
    let proportional = find_candidate(&directories, PROPORTIONAL_CANDIDATES).ok_or_else(|| {
        FontError::NotFound {
            searched: directories.clone(),
        }
    })?;
    let monospace =
        find_candidate(&directories, MONOSPACE_CANDIDATES).unwrap_or_else(|| proportional.clone());
    Ok(SelectedFonts {
        proportional,
        monospace,
    })
}

/// Reads one selected font file under the size bound.
///
/// # Errors
///
/// Returns [`FontError::Unreadable`] for linked, oversized or unreadable files.
pub fn read_font(path: &Path) -> Result<Vec<u8>, FontError> {
    readable(path)?;
    fs::read(path).map_err(|_| FontError::Unreadable(path.to_path_buf()))
}

fn readable(path: &Path) -> Result<(), FontError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| FontError::Unreadable(path.to_path_buf()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_FONT_BYTES {
        return Err(FontError::Unreadable(path.to_path_buf()));
    }
    Ok(())
}

fn find_candidate(directories: &[PathBuf], candidates: &[&str]) -> Option<PathBuf> {
    for candidate in candidates {
        for directory in directories {
            if let Some(found) = find_file(directory, candidate, 0) {
                return Some(found);
            }
        }
    }
    None
}

fn find_file(directory: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    let entries = fs::read_dir(directory).ok()?;
    let mut subdirectories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            subdirectories.push(path);
        } else if metadata.is_file() && path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }
    subdirectories.sort();
    subdirectories
        .into_iter()
        .find_map(|subdirectory| find_file(&subdirectory, name, depth + 1))
}

/// Builds the toolkit font definitions from the selected files.
///
/// # Errors
///
/// Returns [`FontError`] when a selected file cannot be read.
pub fn font_definitions(selected: &SelectedFonts) -> Result<egui::FontDefinitions, FontError> {
    let mut definitions = egui::FontDefinitions::empty();
    let proportional = read_font(&selected.proportional)?;
    definitions.font_data.insert(
        "system-proportional".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(proportional)),
    );
    let monospace = if selected.monospace == selected.proportional {
        None
    } else {
        Some(read_font(&selected.monospace)?)
    };
    if let Some(monospace) = monospace {
        definitions.font_data.insert(
            "system-monospace".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(monospace)),
        );
    }
    definitions
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("system-proportional".to_owned());
    let monospace_family = definitions
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    if definitions.font_data.contains_key("system-monospace") {
        monospace_family.push("system-monospace".to_owned());
    }
    monospace_family.push("system-proportional".to_owned());
    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_must_be_a_regular_bounded_file() {
        let root = std::env::temp_dir().join(format!("codingmage-ui-fonts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(
            readable(&root.join("missing.ttf")),
            Err(FontError::Unreadable(root.join("missing.ttf")))
        );
        assert!(readable(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_search_finds_files_by_exact_name() {
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-font-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/Wanted.ttf"), b"x").unwrap();
        assert_eq!(
            find_candidate(std::slice::from_ref(&root), &["Wanted.ttf"]),
            Some(root.join("a/b/Wanted.ttf"))
        );
        assert_eq!(
            find_candidate(std::slice::from_ref(&root), &["Other.ttf"]),
            None
        );
        fs::remove_dir_all(root).unwrap();
    }
}
