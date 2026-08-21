//! Generic integrity-bound atomic documents for typed coordinator state.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::journal::{private_directory, sha256_hex, sync_directory};

const DOCUMENT_VERSION: u16 = 1;
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

/// Typed payload plus its canonical SHA-256 integrity binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrityDocument<T> {
    /// Closed envelope schema version.
    pub version: u16,
    /// Typed coordinator-owned payload.
    pub payload: T,
    /// SHA-256 of canonical payload bytes.
    pub payload_sha256: String,
}

impl<T> IntegrityDocument<T>
where
    T: Clone + DeserializeOwned + Serialize,
{
    /// Creates an integrity envelope after the caller validates semantic invariants.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrityDocumentError`] for semantic or serialization failure.
    pub fn create(
        payload: T,
        validate: impl FnOnce(&T) -> bool,
    ) -> Result<Self, IntegrityDocumentError> {
        if !validate(&payload) {
            return Err(IntegrityDocumentError::Projection);
        }
        let canonical =
            serde_json::to_vec(&payload).map_err(|_| IntegrityDocumentError::Encoding)?;
        if canonical.len() > MAX_DOCUMENT_BYTES {
            return Err(IntegrityDocumentError::TooLarge);
        }
        Ok(Self {
            version: DOCUMENT_VERSION,
            payload,
            payload_sha256: sha256_hex(&canonical),
        })
    }

    /// Writes a validated document through a private, flushed, atomic replacement.
    ///
    /// # Errors
    ///
    /// Returns an identity, validation, encoding, size, or durable I/O error.
    pub fn write_atomic(
        root: &Path,
        name: &str,
        payload: T,
        validate: impl FnOnce(&T) -> bool,
    ) -> Result<Self, IntegrityDocumentError> {
        let current = document_path(root, name)?;
        private_directory(root).map_err(|_| IntegrityDocumentError::Io)?;
        let document = Self::create(payload, validate)?;
        let encoded =
            serde_json::to_vec(&document).map_err(|_| IntegrityDocumentError::Encoding)?;
        if encoded.len() > MAX_DOCUMENT_BYTES {
            return Err(IntegrityDocumentError::TooLarge);
        }
        let temporary = temporary_path(root, name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| IntegrityDocumentError::Io)?;
        let result = (|| {
            file.write_all(&encoded)
                .map_err(|_| IntegrityDocumentError::Io)?;
            file.sync_all().map_err(|_| IntegrityDocumentError::Io)?;
            fs::rename(&temporary, &current).map_err(|_| IntegrityDocumentError::Io)?;
            sync_directory(root).map_err(|_| IntegrityDocumentError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(document)
    }

    /// Loads one exact regular nonsymlink document and revalidates integrity and semantics.
    ///
    /// # Errors
    ///
    /// Returns an identity, validation, encoding, size, hash, or I/O error.
    pub fn load(
        root: &Path,
        name: &str,
        validate: impl FnOnce(&T) -> bool,
    ) -> Result<Self, IntegrityDocumentError> {
        let path = document_path(root, name)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| IntegrityDocumentError::Io)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(IntegrityDocumentError::Identity);
        }
        if metadata.len() > MAX_DOCUMENT_BYTES as u64 {
            return Err(IntegrityDocumentError::TooLarge);
        }
        let mut encoded = Vec::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut encoded))
            .map_err(|_| IntegrityDocumentError::Io)?;
        let document: Self =
            serde_json::from_slice(&encoded).map_err(|_| IntegrityDocumentError::Encoding)?;
        if document.version != DOCUMENT_VERSION {
            return Err(IntegrityDocumentError::Version);
        }
        let canonical =
            serde_json::to_vec(&document.payload).map_err(|_| IntegrityDocumentError::Encoding)?;
        if sha256_hex(&canonical) != document.payload_sha256 {
            return Err(IntegrityDocumentError::Hash);
        }
        if !validate(&document.payload) {
            return Err(IntegrityDocumentError::Projection);
        }
        Ok(document)
    }
}

/// Content-free integrity-document failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrityDocumentError {
    /// Document name or filesystem object identity is invalid.
    Identity,
    /// Document schema version is unsupported.
    Version,
    /// Payload or envelope encoding is invalid.
    Encoding,
    /// Document exceeds its fixed maximum size.
    TooLarge,
    /// Payload integrity hash does not match.
    Hash,
    /// Typed semantic projection is invalid.
    Projection,
    /// Durable filesystem operation failed.
    Io,
}

impl fmt::Display for IntegrityDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.state.document.identity",
            Self::Version => "codingmage.state.document.version",
            Self::Encoding => "codingmage.state.document.encoding",
            Self::TooLarge => "codingmage.state.document.too_large",
            Self::Hash => "codingmage.state.document.hash",
            Self::Projection => "codingmage.state.document.projection",
            Self::Io => "codingmage.state.document.io",
        })
    }
}

impl std::error::Error for IntegrityDocumentError {}

fn document_path(root: &Path, name: &str) -> Result<PathBuf, IntegrityDocumentError> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || name.starts_with('.')
        || name.contains("..")
    {
        return Err(IntegrityDocumentError::Identity);
    }
    Ok(root.join(name))
}

fn temporary_path(root: &Path, name: &str) -> PathBuf {
    let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
    root.join(format!(".{name}.{}.{sequence}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    struct FixtureState {
        sequence: u64,
        value: String,
    }

    fn root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-integrity-document-{}-{unique}",
            std::process::id()
        ))
    }

    fn valid(state: &FixtureState) -> bool {
        state.sequence > 0 && !state.value.is_empty() && state.value.len() <= 64
    }

    #[test]
    fn atomic_document_round_trips_and_abandoned_temporary_is_inert() {
        let root = root();
        let state = FixtureState {
            sequence: 1,
            value: "ready".to_owned(),
        };
        let written =
            IntegrityDocument::write_atomic(&root, "team-state.json", state.clone(), valid)
                .unwrap();
        fs::write(root.join(".team-state.json.abandoned.tmp"), b"partial").unwrap();
        let loaded =
            IntegrityDocument::<FixtureState>::load(&root, "team-state.json", valid).unwrap();
        assert_eq!(loaded, written);
        assert_eq!(loaded.payload, state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hash_schema_projection_symlink_and_name_mutations_fail_closed() {
        let root = root();
        let state = FixtureState {
            sequence: 1,
            value: "ready".to_owned(),
        };
        IntegrityDocument::write_atomic(&root, "team-state.json", state, valid).unwrap();
        let path = root.join("team-state.json");
        let original = fs::read(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["payload_sha256"] = serde_json::Value::String("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            IntegrityDocument::<FixtureState>::load(&root, "team-state.json", valid).unwrap_err(),
            IntegrityDocumentError::Hash
        );

        fs::write(&path, &original).unwrap();
        assert_eq!(
            IntegrityDocument::<FixtureState>::load(&root, "../escape", valid).unwrap_err(),
            IntegrityDocumentError::Identity
        );
        assert_eq!(
            IntegrityDocument::<FixtureState>::load(&root, "team-state.json", |_| false)
                .unwrap_err(),
            IntegrityDocumentError::Projection
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let real = root.join("real.json");
            fs::rename(&path, &real).unwrap();
            symlink(&real, &path).unwrap();
            assert_eq!(
                IntegrityDocument::<FixtureState>::load(&root, "team-state.json", valid)
                    .unwrap_err(),
                IntegrityDocumentError::Identity
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
