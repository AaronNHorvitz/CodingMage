//! Explicit campaign admission bound to the reviewed preflight report.
//!
//! An admission records that the owner inspected one exact preflight report for one exact
//! campaign authority at one exact repository state. It grants nothing by itself: the
//! coordinator revalidates everything when `campaign` starts. The interface uses it only to
//! refuse starting a campaign whose inputs changed since the report was reviewed.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    readiness::PreflightObservation,
    state_dir::{StateError, read_private, write_private},
};

/// Minimum digest prefix the owner must confirm to admit a campaign.
pub const CONFIRMATION_PREFIX: usize = 12;

/// One recorded admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    /// Document version.
    pub version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Campaign authority digest the report was produced for.
    pub authority_sha256: String,
    /// Repository identity at review time.
    pub repository_id: String,
    /// Initial commit at review time.
    pub initial_commit: String,
    /// Task-source digest at review time.
    pub task_source_sha256: String,
    /// Authorization record digest at review time.
    pub operator_authorization_sha256: String,
    /// SHA-256 of the exact preflight report bytes reviewed.
    pub preflight_sha256: String,
    /// Absolute specification path.
    pub spec_path: PathBuf,
    /// Absolute authorization record path.
    pub authorization_record: PathBuf,
    /// Unix milliseconds when admitted.
    pub admitted_at_ms: u64,
}

/// Why an admission could not be recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmitError {
    /// No preflight report has been observed.
    NoPreflight,
    /// The report did not end in `ready`.
    NotReady(String),
    /// The report belongs to a different campaign authority.
    AuthorityMismatch,
    /// The confirmed digest prefix does not match the report digest.
    ConfirmationMismatch,
    /// The admission could not be persisted.
    State(StateError),
}

impl std::fmt::Display for AdmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPreflight => formatter.write_str("run preflight and review its report first"),
            Self::NotReady(state) => write!(formatter, "the preflight report state is {state}, not ready"),
            Self::AuthorityMismatch => formatter.write_str(
                "the preflight report belongs to a different campaign authority; run preflight again",
            ),
            Self::ConfirmationMismatch => write!(
                formatter,
                "type at least the first {CONFIRMATION_PREFIX} characters of the report digest exactly as shown"
            ),
            Self::State(error) => write!(formatter, "the admission could not be recorded: {error}"),
        }
    }
}

impl std::error::Error for AdmitError {}

/// Why a recorded admission no longer applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Staleness {
    /// The campaign authority changed.
    Authority,
    /// The repository identity changed.
    Repository,
    /// The checkout head differs from the admitted initial commit.
    Head,
    /// The task source differs from the admitted digest.
    TaskSource,
    /// The authorization record no longer matches.
    Authorization,
}

impl Staleness {
    /// Explanation for the owner.
    #[must_use]
    pub const fn explain(&self) -> &'static str {
        match self {
            Self::Authority => "the campaign authority changed since the reviewed preflight",
            Self::Repository => "the repository identity changed since the reviewed preflight",
            Self::Head => "the checkout head changed since the reviewed preflight",
            Self::TaskSource => "the task source changed since the reviewed preflight",
            Self::Authorization => "the authorization record changed since the reviewed preflight",
        }
    }
}

/// Inputs an admission is checked against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentBinding {
    /// Campaign authority digest of the selected specification.
    pub authority_sha256: String,
    /// Repository identity observed now.
    pub repository_id: String,
    /// Checkout head observed now.
    pub head: String,
    /// Task-source digest observed now.
    pub task_source_sha256: String,
    /// Authorization digest bound in the selected specification.
    pub operator_authorization_sha256: String,
}

impl Admission {
    /// Records an admission for the reviewed report after the owner confirms its digest.
    ///
    /// # Errors
    ///
    /// Returns [`AdmitError`] when the report is missing, not ready, for another authority, or
    /// the confirmation does not match.
    pub fn record(
        preflight: Option<&PreflightObservation>,
        binding: &CurrentBinding,
        campaign_id: &str,
        spec_path: &Path,
        authorization_record: &Path,
        confirmation: &str,
    ) -> Result<Self, AdmitError> {
        let observation = preflight.ok_or(AdmitError::NoPreflight)?;
        if observation.report.state != "ready" {
            return Err(AdmitError::NotReady(observation.report.state.clone()));
        }
        if observation.report.authority_sha256 != binding.authority_sha256
            || observation.report.operator_authorization_sha256
                != binding.operator_authorization_sha256
        {
            return Err(AdmitError::AuthorityMismatch);
        }
        let confirmation = confirmation.trim();
        if confirmation.len() < CONFIRMATION_PREFIX
            || !observation.report_sha256.starts_with(confirmation)
        {
            return Err(AdmitError::ConfirmationMismatch);
        }
        Ok(Self {
            version: 1,
            campaign_id: campaign_id.to_owned(),
            authority_sha256: binding.authority_sha256.clone(),
            repository_id: binding.repository_id.clone(),
            initial_commit: binding.head.clone(),
            task_source_sha256: binding.task_source_sha256.clone(),
            operator_authorization_sha256: binding.operator_authorization_sha256.clone(),
            preflight_sha256: observation.report_sha256.clone(),
            spec_path: spec_path.to_path_buf(),
            authorization_record: authorization_record.to_path_buf(),
            admitted_at_ms: now_ms(),
        })
    }

    /// Returns the first reason the admission no longer applies, if any.
    #[must_use]
    pub fn staleness(&self, binding: &CurrentBinding) -> Option<Staleness> {
        if self.authority_sha256 != binding.authority_sha256 {
            Some(Staleness::Authority)
        } else if self.repository_id != binding.repository_id {
            Some(Staleness::Repository)
        } else if self.initial_commit != binding.head {
            Some(Staleness::Head)
        } else if self.task_source_sha256 != binding.task_source_sha256 {
            Some(Staleness::TaskSource)
        } else if self.operator_authorization_sha256 != binding.operator_authorization_sha256 {
            Some(Staleness::Authorization)
        } else {
            None
        }
    }

    fn path(directory: &Path, authority_sha256: &str) -> PathBuf {
        directory
            .join("admissions")
            .join(format!("{authority_sha256}.json"))
    }

    /// Persists the admission under the project's private directory.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when persistence fails.
    pub fn save(&self, project_dir: &Path) -> Result<(), StateError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| StateError::Invalid)?;
        write_private(&Self::path(project_dir, &self.authority_sha256), &bytes)
    }

    /// Loads the admission for one authority, if recorded.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the document exists but is invalid.
    pub fn load(project_dir: &Path, authority_sha256: &str) -> Result<Option<Self>, StateError> {
        let path = Self::path(project_dir, authority_sha256);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_private(&path)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
        if value.version != 1 || value.authority_sha256 != authority_sha256 {
            return Err(StateError::Invalid);
        }
        Ok(Some(value))
    }
}

/// Current Unix time in milliseconds.
#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::parse_preflight;

    fn observation(state: &str) -> PreflightObservation {
        let json = format!(
            r#"{{"schema_version":2,"state":"{state}","authority_sha256":"{}","operator_authorization_sha256":"{}","repository":{{"repository_id":"repo-1","initial_commit":"{}","branch_sha256":"{}","task_source_sha256":"{}","status_sha256":"{}","references_sha256":"{}","worktrees_sha256":"{}","plan_item_count":12,"open_subtask_count":10,"clean":true,"dedicated_branch":true,"checkout_safe":true}},"policy":{{"max_parallel_pods":1,"max_accepted_outcomes":10,"publication":"local_only","default_branch_protected":true,"allowed_path_count":1,"allowed_paths_sha256":"{}","task_path_authority_count":0,"task_path_authority_sha256":"{}","denied_path_count":0,"denied_paths_sha256":"{}","external_capabilities_denied":true}},"providers":[],"gates":{{"command_count":1,"registry_sha256":"{}","executable_sha256":[],"tier_count":1,"tiers_sha256":"{}"}},"controls":{{"process_guard_sha256":"{}","operator_control_count":4,"operator_controls_sha256":"{}","process_guard_verified":true}},"storage":{{"scratch_sufficient":true,"state_sufficient":true,"required_available_bytes":1,"sufficient":true}},"source_free":true}}"#,
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(40),
            "d".repeat(64),
            "e".repeat(64),
            "f".repeat(64),
            "0".repeat(64),
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            "4".repeat(64),
            "5".repeat(64),
            "6".repeat(64),
            "7".repeat(64),
            "8".repeat(64),
        );
        let bytes = json.into_bytes();
        PreflightObservation::new(parse_preflight(&bytes).unwrap(), bytes)
    }

    fn binding() -> CurrentBinding {
        CurrentBinding {
            authority_sha256: "a".repeat(64),
            repository_id: "repo-1".to_owned(),
            head: "c".repeat(40),
            task_source_sha256: "e".repeat(64),
            operator_authorization_sha256: "b".repeat(64),
        }
    }

    #[test]
    fn admission_requires_a_ready_matching_report_and_a_confirmed_digest() {
        let ready = observation("ready");
        let binding = binding();
        assert_eq!(
            Admission::record(None, &binding, "c", Path::new("/s"), Path::new("/r"), "x")
                .unwrap_err(),
            AdmitError::NoPreflight
        );
        assert!(matches!(
            Admission::record(
                Some(&observation("blocked")),
                &binding,
                "c",
                Path::new("/s"),
                Path::new("/r"),
                "x"
            ),
            Err(AdmitError::NotReady(_))
        ));
        assert_eq!(
            Admission::record(
                Some(&ready),
                &binding,
                "c",
                Path::new("/s"),
                Path::new("/r"),
                "abc"
            )
            .unwrap_err(),
            AdmitError::ConfirmationMismatch
        );
        let mut other = binding.clone();
        other.authority_sha256 = "9".repeat(64);
        assert_eq!(
            Admission::record(
                Some(&ready),
                &other,
                "c",
                Path::new("/s"),
                Path::new("/r"),
                &ready.report_sha256
            )
            .unwrap_err(),
            AdmitError::AuthorityMismatch
        );
        let admission = Admission::record(
            Some(&ready),
            &binding,
            "c",
            Path::new("/s"),
            Path::new("/r"),
            &ready.report_sha256[..CONFIRMATION_PREFIX],
        )
        .unwrap();
        assert_eq!(admission.staleness(&binding), None);
        let mut moved = binding.clone();
        moved.head = "0".repeat(40);
        assert_eq!(admission.staleness(&moved), Some(Staleness::Head));
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-admission-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        admission.save(&root).unwrap();
        assert_eq!(
            Admission::load(&root, &binding.authority_sha256).unwrap(),
            Some(admission)
        );
        assert_eq!(Admission::load(&root, &"9".repeat(64)).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
