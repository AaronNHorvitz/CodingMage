//! Campaign selection and the per-task overlay of coordinator observations.
//!
//! Selection binds one campaign specification to the opened repository. The overlay combines
//! the active checkout's source checkbox, the campaign head's task source, the durable status
//! projection and the final report into distinct, never-merged states.

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_campaign::{CampaignError, CampaignSpec};
use codingmage_plan::CheckState;

use crate::backend::models::{
    ActiveTask, CampaignReport, CampaignStatus, Deferral, TaskCompletion, TaskReason,
};

/// One selected campaign bound to the opened repository.
#[derive(Clone, Debug)]
pub struct CampaignSelection {
    /// Absolute specification path.
    pub spec_path: PathBuf,
    /// Verified specification.
    pub spec: CampaignSpec,
    /// Digest of the complete campaign authority.
    pub authority_sha256: String,
}

/// Why a campaign specification was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectError {
    /// The path is not absolute.
    RelativePath,
    /// The existing loader rejected the file.
    Invalid(CampaignError),
    /// The specification names a different repository path than the opened configuration.
    DifferentRepositoryPath {
        /// Repository path named by the specification.
        specified: PathBuf,
        /// Repository path of the opened configuration.
        opened: PathBuf,
    },
    /// The specification names a different repository identity than the diagnosis observed.
    DifferentRepositoryId {
        /// Identity named by the specification.
        specified: String,
        /// Identity observed by `doctor`.
        observed: String,
    },
}

impl fmt::Display for SelectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativePath => formatter.write_str("the campaign path must be absolute"),
            Self::Invalid(error) => {
                write!(formatter, "the campaign specification is invalid: {error}")
            }
            Self::DifferentRepositoryPath { specified, opened } => write!(
                formatter,
                "the campaign names repository {} but the opened configuration targets {}",
                specified.display(),
                opened.display()
            ),
            Self::DifferentRepositoryId {
                specified,
                observed,
            } => write!(
                formatter,
                "the campaign names repository identity {specified} but the coordinator observed {observed}"
            ),
        }
    }
}

impl std::error::Error for SelectError {}

impl CampaignSelection {
    /// Loads and verifies a specification and binds it to the opened repository.
    ///
    /// # Errors
    ///
    /// Returns [`SelectError`] for invalid files or cross-repository specifications.
    pub fn load(
        spec_path: &Path,
        opened_target: &Path,
        observed_repository_id: Option<&str>,
    ) -> Result<Self, SelectError> {
        if !spec_path.is_absolute() {
            return Err(SelectError::RelativePath);
        }
        let spec = CampaignSpec::load(spec_path).map_err(SelectError::Invalid)?;
        let authority_sha256 = spec.authority_sha256().map_err(SelectError::Invalid)?;
        let opened =
            fs::canonicalize(opened_target).unwrap_or_else(|_| opened_target.to_path_buf());
        let specified = fs::canonicalize(&spec.repository_path)
            .unwrap_or_else(|_| spec.repository_path.clone());
        if specified != opened {
            return Err(SelectError::DifferentRepositoryPath { specified, opened });
        }
        if let Some(observed) = observed_repository_id
            && observed != spec.repository_id
        {
            return Err(SelectError::DifferentRepositoryId {
                specified: spec.repository_id.clone(),
                observed: observed.to_owned(),
            });
        }
        Ok(Self {
            spec_path: spec_path.to_path_buf(),
            spec,
            authority_sha256,
        })
    }
}

/// Distinct coordinator-derived states for one task identifier.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaskOverlay {
    /// Checkbox in the active checkout's task source.
    pub source: Option<CheckState>,
    /// Checkbox in the task source at the campaign head, when observed.
    pub campaign_head: Option<CheckState>,
    /// Active unit projection.
    pub active: Option<ActiveTask>,
    /// Blocked reason code.
    pub blocked: Option<String>,
    /// Deferral projection.
    pub deferred: Option<Deferral>,
    /// Human-decision reason code.
    pub human_decision: Option<String>,
    /// Final report completion identities (parallel campaigns).
    pub accepted: Option<TaskCompletion>,
}

/// Presented state labels for one task, in priority order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskStateLabels {
    /// Labels shown next to the row.
    pub labels: Vec<String>,
}

impl TaskOverlay {
    /// Distinct labels for the row. Source checkbox, verified completion, accepted outcome,
    /// active, blocked, deferred and human-decision states never collapse into one another.
    #[must_use]
    pub fn labels(&self, observation_known: bool) -> Vec<String> {
        let mut labels = Vec::new();
        match self.source {
            Some(CheckState::Checked) => labels.push("checked in source".to_owned()),
            Some(CheckState::Open) => labels.push("open in source".to_owned()),
            None => {}
        }
        match self.campaign_head {
            Some(CheckState::Checked) if self.source != Some(CheckState::Checked) => {
                labels.push(
                    "completed at campaign head (verified, not yet in active checkout)".to_owned(),
                );
            }
            Some(CheckState::Checked) => labels.push("completed at campaign head".to_owned()),
            _ => {}
        }
        if let Some(accepted) = &self.accepted {
            labels.push(format!(
                "accepted outcome (reviewed {})",
                short(&accepted.reviewed_commit)
            ));
        }
        if let Some(active) = &self.active {
            labels.push(format!("active: {} by {}", active.state, active.actor));
        }
        if let Some(reason) = &self.blocked {
            labels.push(format!("blocked: {reason}"));
        }
        if let Some(deferral) = &self.deferred {
            labels.push(format!(
                "deferred: {} until {} ({})",
                deferral.reason_code, deferral.trigger_code, deferral.trigger_state
            ));
        }
        if let Some(reason) = &self.human_decision {
            labels.push(format!("human decision required: {reason}"));
        }
        if !observation_known {
            labels.push("coordinator state unknown".to_owned());
        }
        labels
    }
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

/// Builds the overlay for every task identifier known to any source.
#[must_use]
pub fn build_overlay(
    source: &BTreeMap<String, CheckState>,
    campaign_head: Option<&BTreeMap<String, CheckState>>,
    status: Option<&CampaignStatus>,
    report: Option<&CampaignReport>,
) -> BTreeMap<String, TaskOverlay> {
    let mut overlay: BTreeMap<String, TaskOverlay> = BTreeMap::new();
    for (id, state) in source {
        overlay.entry(id.clone()).or_default().source = Some(*state);
    }
    if let Some(head) = campaign_head {
        for (id, state) in head {
            overlay.entry(id.clone()).or_default().campaign_head = Some(*state);
        }
    }
    if let Some(status) = status {
        for active in &status.active_tasks {
            overlay.entry(active.task_id.clone()).or_default().active = Some(active.clone());
        }
        for TaskReason {
            task_id,
            reason_code,
        } in &status.blockers
        {
            overlay.entry(task_id.clone()).or_default().blocked = Some(reason_code.clone());
        }
        for deferral in &status.deferrals {
            overlay
                .entry(deferral.task_id.clone())
                .or_default()
                .deferred = Some(deferral.clone());
        }
        for TaskReason {
            task_id,
            reason_code,
        } in &status.human_decisions
        {
            overlay.entry(task_id.clone()).or_default().human_decision = Some(reason_code.clone());
        }
    }
    if let Some(report) = report {
        for (id, completion) in &report.tasks {
            overlay.entry(id.clone()).or_default().accepted = Some(completion.clone());
        }
    }
    overlay
}

/// Roles the current backend can report, by stable actor code.
pub const SUPPORTED_ROLES: &[(&str, &str)] = &[
    (
        "coordinator",
        "Deterministic coordinator: validation, Git, checkpoints, cleanup",
    ),
    (
        "codex-lead",
        "Read-only campaign lead proposing dependency-ready units",
    ),
    (
        "pod",
        "Implementation pod editing leased paths in an isolated worktree",
    ),
    (
        "integration",
        "Serialized integration of reviewed candidates",
    ),
];

/// Owner involvement modes with a backend contract (`campaign-mission-admit`): code, label and
/// what the coordinator does with a choice the charter does not delegate.
pub const INVOLVEMENT_MODES: &[(&str, &str, &str)] = &[
    (
        "supervised",
        "Supervised",
        "undelegated choices become exact owner decision requests answered through campaign-mission-answer",
    ),
    (
        "exception_only",
        "Exception-only",
        "undelegated choices produce one deduplicated exception request each while other work continues",
    ),
    (
        "hands_off",
        "Hands-off",
        "no mid-campaign request is issued; undelegated choices are retained as blockers",
    ),
];

/// Human label for an involvement mode code reported by the backend.
#[must_use]
pub fn involvement_label(code: &str) -> &str {
    INVOLVEMENT_MODES
        .iter()
        .find(|(known, _, _)| *known == code)
        .map_or(code, |(_, label, _)| label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_keeps_source_head_and_coordinator_states_distinct() {
        let mut source = BTreeMap::new();
        source.insert("1.1.1.1".to_owned(), CheckState::Open);
        source.insert("1.1.1.2".to_owned(), CheckState::Checked);
        let mut head = BTreeMap::new();
        head.insert("1.1.1.1".to_owned(), CheckState::Checked);
        head.insert("1.1.1.2".to_owned(), CheckState::Checked);
        let status: CampaignStatus = serde_json::from_str(
            r#"{"schema_version":5,"campaign_id":"c","state":"paused","actor":"coordinator","model":null,"branch":"codingmage/c","head":"h","current_task_id":null,"current_round":null,"active_tasks":[],"last_task_id":"1.1.1.1","completed_units":1,"attempt_count":3,"planning_generation":2,"identical_planning_generations":0,"pending_planning_triggers":[],"watchdog_state":"idle","reconciliation_state":"reconciled","outcomes":{"completed":1,"blocked":1,"deferred":0,"pending_human_decision":0,"rejected_proposals":0,"accepted":2,"max_accepted":3},"utilization":{"provider_attempts":3,"malformed_report_repairs":0,"correction_rounds":0,"process_invocations":9,"output_bytes":10,"retained_state_bytes":20,"execution_elapsed_ms":30},"limits":{"provider_attempts":100,"malformed_report_repairs":10,"correction_rounds":10,"process_invocations":1000,"output_bytes":1000,"retained_state_bytes":1000,"execution_elapsed_ms":100000},"blocker_count":1,"blocker_code":null,"blockers":[{"task_id":"1.1.1.3","reason_code":"unavailable_external_dependency"}],"deferrals":[],"human_decisions":[],"elapsed_ms":5,"updated_at_ms":6}"#,
        )
        .unwrap();
        let overlay = build_overlay(&source, Some(&head), Some(&status), None);
        let first = overlay.get("1.1.1.1").unwrap().labels(true);
        assert_eq!(
            first,
            vec![
                "open in source",
                "completed at campaign head (verified, not yet in active checkout)"
            ]
        );
        let second = overlay.get("1.1.1.2").unwrap().labels(true);
        assert_eq!(
            second,
            vec!["checked in source", "completed at campaign head"]
        );
        let third = overlay.get("1.1.1.3").unwrap().labels(false);
        assert_eq!(
            third,
            vec![
                "blocked: unavailable_external_dependency",
                "coordinator state unknown"
            ]
        );
    }

    #[test]
    fn selection_rejects_relative_and_cross_repository_specifications() {
        assert_eq!(
            CampaignSelection::load(Path::new("relative.toml"), Path::new("/tmp"), None)
                .unwrap_err(),
            SelectError::RelativePath
        );
        assert!(matches!(
            CampaignSelection::load(
                Path::new("/nonexistent/campaign.toml"),
                Path::new("/tmp"),
                None
            )
            .unwrap_err(),
            SelectError::Invalid(_)
        ));
    }
}
