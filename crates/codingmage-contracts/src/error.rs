//! Public errors that contain stable codes and bounded, non-content metadata.

use core::fmt;
use serde::{Deserialize, Serialize};

use crate::{AttemptId, EvidenceId, RepositoryId, RunId, TaskId};

/// Stable top-level failure categories.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Configuration parsing or policy failure.
    Configuration,
    /// Repository authorization or identity failure.
    Repository,
    /// Git operation failure.
    Git,
    /// Local process execution failure.
    Process,
    /// Agent provider failure.
    Provider,
    /// Deterministic gate failure.
    Gate,
    /// State transition or recovery failure.
    State,
    /// Quota or usage-limit failure.
    Quota,
    /// Evidence validation or persistence failure.
    Evidence,
}

impl ErrorCategory {
    const fn segment(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Repository => "repository",
            Self::Git => "git",
            Self::Process => "process",
            Self::Provider => "provider",
            Self::Gate => "gate",
            Self::State => "state",
            Self::Quota => "quota",
            Self::Evidence => "evidence",
        }
    }
}

/// A syntactically validated stable error code.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ErrorCode(String);

impl ErrorCode {
    /// Creates `codingmage.<category>.<reason>` after validating the reason segment.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCodeError`] when `reason` is empty, oversized, or noncanonical.
    pub fn new(category: ErrorCategory, reason: &str) -> Result<Self, ErrorCodeError> {
        if reason.is_empty()
            || reason.len() > 64
            || !reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            || !reason.as_bytes()[0].is_ascii_lowercase()
        {
            return Err(ErrorCodeError);
        }
        Ok(Self(format!("codingmage.{}.{reason}", category.segment())))
    }

    /// Returns the stable wire representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_code(&value)
            .ok_or_else(|| serde::de::Error::custom("invalid CodingMage error code"))?;
        Ok(Self(value))
    }
}

fn parse_code(value: &str) -> Option<(&str, &str)> {
    let mut segments = value.split('.');
    if segments.next()? != "codingmage" {
        return None;
    }
    let category = segments.next()?;
    let reason = segments.next()?;
    if segments.next().is_some()
        || !matches!(
            category,
            "configuration"
                | "repository"
                | "git"
                | "process"
                | "provider"
                | "gate"
                | "state"
                | "quota"
                | "evidence"
        )
        || reason.is_empty()
        || reason.len() > 64
        || !reason
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !reason.as_bytes()[0].is_ascii_lowercase()
    {
        return None;
    }
    Some((category, reason))
}

/// Returned when an error-code reason does not use the stable grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorCodeError;

impl fmt::Display for ErrorCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid error code reason")
    }
}

impl std::error::Error for ErrorCodeError {}

/// Typed references and numeric bounds safe to expose in a public error.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorMetadata {
    /// Authorized repository involved in the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<RepositoryId>,
    /// Run involved in the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Task involved in the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Attempt involved in the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<AttemptId>,
    /// Evidence record involved in the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<EvidenceId>,
    /// Declared numeric limit, when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// Observed numeric value, when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<u64>,
}

/// Content-free error safe to cross a process or storage boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicError {
    /// Stable machine-readable error code.
    pub code: ErrorCode,
    /// Optional typed metadata without command, source, environment, or credential text.
    #[serde(default, skip_serializing_if = "ErrorMetadata::is_empty")]
    pub metadata: ErrorMetadata,
}

impl ErrorMetadata {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl PublicError {
    /// Creates a public error from a validated code and typed metadata.
    #[must_use]
    pub const fn new(code: ErrorCode, metadata: ErrorMetadata) -> Self {
        Self { code, metadata }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_error_round_trips_and_rejects_unknown_fields() {
        let error = PublicError::new(
            ErrorCode::new(ErrorCategory::Repository, "identity_changed").unwrap(),
            ErrorMetadata {
                repository_id: Some(RepositoryId::new("repo-1").unwrap()),
                observed: Some(2),
                ..ErrorMetadata::default()
            },
        );
        let encoded = serde_json::to_string(&error).unwrap();
        assert_eq!(
            serde_json::from_str::<PublicError>(&encoded).unwrap(),
            error
        );
        assert!(
            serde_json::from_str::<PublicError>(r#"{"code":"codingmage.git.failed","extra":1}"#)
                .is_err()
        );
    }

    #[test]
    fn syntactically_valid_future_reason_remains_compatible() {
        let encoded = r#"{"code":"codingmage.provider.future_reason"}"#;
        let decoded: PublicError = serde_json::from_str(encoded).unwrap();
        assert_eq!(decoded.code.as_str(), "codingmage.provider.future_reason");
        assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    }

    #[test]
    fn invalid_codes_fail_closed() {
        for value in [
            "other.git.failed",
            "codingmage.unknown.failed",
            "codingmage.git.UPPER",
            "codingmage.git.path/escape",
            "codingmage.git.",
        ] {
            let encoded = format!(r#"{{"code":"{value}"}}"#);
            assert!(
                serde_json::from_str::<PublicError>(&encoded).is_err(),
                "value={value}"
            );
        }
    }

    #[test]
    fn lower_level_sensitive_text_cannot_enter_public_error() {
        let synthetic_sensitive_value = ["secret", "-value"].concat();
        let error = PublicError::new(
            ErrorCode::new(ErrorCategory::Process, "failed").unwrap(),
            ErrorMetadata::default(),
        );
        let encoded = serde_json::to_string(&error).unwrap();
        assert!(!encoded.contains(&synthetic_sensitive_value));
        assert_eq!(encoded, r#"{"code":"codingmage.process.failed"}"#);
    }
}
