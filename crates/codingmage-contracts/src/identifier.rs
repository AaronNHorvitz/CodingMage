//! Validated identifiers that are safe to serialize and use as references.

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const MAX_IDENTIFIER_LENGTH: usize = 128;

/// The reason an identifier was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifierError {
    /// The identifier contained no bytes.
    Empty,
    /// The identifier exceeded the stable length bound.
    TooLong,
    /// The identifier contained a path separator or control character.
    UnsafeCharacter,
    /// The identifier did not use the canonical grammar.
    NonCanonical,
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "identifier is empty",
            Self::TooLong => "identifier exceeds length limit",
            Self::UnsafeCharacter => "identifier contains an unsafe character",
            Self::NonCanonical => "identifier is not canonical",
        })
    }
}

impl std::error::Error for IdentifierError {}

fn validate(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if value.len() > MAX_IDENTIFIER_LENGTH {
        return Err(IdentifierError::TooLong);
    }
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | '\0'))
    {
        return Err(IdentifierError::UnsafeCharacter);
    }

    let bytes = value.as_bytes();
    let canonical_edge = |byte: u8| byte.is_ascii_alphanumeric();
    if !canonical_edge(bytes[0])
        || !canonical_edge(*bytes.last().expect("non-empty identifier"))
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || value.contains("..")
    {
        return Err(IdentifierError::NonCanonical);
    }
    Ok(())
}

macro_rules! identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates a validated identifier.
            ///
            /// # Errors
            ///
            /// Returns [`IdentifierError`] when the value is empty, oversized, unsafe, or
            /// noncanonical.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                validate(&value)?;
                Ok(Self(value))
            }

            /// Returns the canonical identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(de::Error::custom)
            }
        }
    };
}

identifier!(
    RepositoryId,
    "Identity assigned to one authorized repository."
);
identifier!(RunId, "Identity assigned to one orchestration run.");
identifier!(TaskId, "Identity assigned to one bounded unit of work.");
identifier!(
    WorktreeId,
    "Identity assigned to one coordinator-owned worktree."
);
identifier!(
    AgentId,
    "Identity assigned to one configured agent profile."
);
identifier!(
    AttemptId,
    "Identity assigned to one bounded implementation attempt."
);
identifier!(ReviewId, "Identity assigned to one senior review.");
identifier!(
    EvidenceId,
    "Identity assigned to one immutable evidence record."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_boundary_length_and_orders_canonically() {
        let boundary = format!("a{}z", "x".repeat(MAX_IDENTIFIER_LENGTH - 2));
        let mut identifiers = [
            TaskId::new("task-2").unwrap(),
            TaskId::new("task-1").unwrap(),
        ];
        identifiers.sort();
        assert_eq!(identifiers[0].as_str(), "task-1");
        assert_eq!(
            TaskId::new(boundary).unwrap().as_str().len(),
            MAX_IDENTIFIER_LENGTH
        );
    }

    #[test]
    fn rejects_empty_oversized_path_control_and_ambiguous_values() {
        let cases = [
            ("", IdentifierError::Empty),
            (
                &"a".repeat(MAX_IDENTIFIER_LENGTH + 1),
                IdentifierError::TooLong,
            ),
            ("parent/child", IdentifierError::UnsafeCharacter),
            ("parent\\child", IdentifierError::UnsafeCharacter),
            ("line\nbreak", IdentifierError::UnsafeCharacter),
            (".hidden", IdentifierError::NonCanonical),
            ("task..one", IdentifierError::NonCanonical),
            ("task-", IdentifierError::NonCanonical),
            ("has space", IdentifierError::NonCanonical),
        ];
        for (value, expected) in cases {
            assert_eq!(TaskId::new(value).unwrap_err(), expected, "value={value:?}");
        }
        assert_eq!(
            TaskId::new("caf\u{e9}").unwrap_err(),
            IdentifierError::NonCanonical
        );
    }

    #[test]
    fn serialization_is_canonical_and_revalidates_input() {
        let identifier = RunId::new("run-0001").unwrap();
        assert_eq!(serde_json::to_string(&identifier).unwrap(), "\"run-0001\"");
        assert_eq!(
            serde_json::from_str::<RunId>("\"run-0001\"").unwrap(),
            identifier
        );
        assert!(serde_json::from_str::<RunId>("\"../escape\"").is_err());
    }
}
