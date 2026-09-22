//! Inspectable and exportable outcome and blocker reports assembled from real records only.
//!
//! A report is a plain document of what the coordinator and the repository objects reported.
//! It never contains configuration paths, environment values, provider executables or
//! credentials, and it includes repository file paths only when the owner opts in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    admission::{Admission, now_ms},
    backend::models::{BlockerExplanation, CampaignOutcome, CampaignReport, CampaignStatus},
    records::{CommitSummary, FileChange, RunRecord},
    setup::WriteError,
};

/// Report document version.
pub const REPORT_SCHEMA_VERSION: u16 = 1;

/// One run summarized for the report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunSummary {
    /// Run identity.
    pub run_id: String,
    /// Task identity when known.
    pub task_id: Option<String>,
    /// Reviewed candidate commit when checkpointed.
    pub candidate_commit: Option<String>,
    /// Review verdict when recorded.
    pub review_verdict: Option<String>,
    /// Correction rounds when checkpointed.
    pub correction_rounds: Option<u16>,
    /// Gate evidence identities.
    pub gate_evidence: Vec<String>,
    /// Problems observed while reading the records.
    pub problems: Vec<String>,
}

/// Delivery disposition, always stated separately from engineering completion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Disposition {
    /// Accepted outcomes against the ceiling.
    pub accepted_outcomes: Option<u32>,
    /// Accepted-outcome ceiling.
    pub max_accepted: Option<u32>,
    /// Completed and reconciled units.
    pub completed_units: Option<u32>,
    /// Blocked task count.
    pub blocked: Option<u32>,
    /// Deferred task count.
    pub deferred: Option<u32>,
    /// Human decisions pending.
    pub pending_human_decision: Option<u32>,
    /// Campaign phase code.
    pub campaign_state: Option<String>,
    /// Publication policy code.
    pub publication: String,
    /// Always withheld in this milestone: no push, merge or promotion exists.
    pub delivery: String,
}

/// The exportable report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeReport {
    /// Document version.
    pub schema_version: u16,
    /// Unix milliseconds when assembled.
    pub generated_at_ms: u64,
    /// Interface version that assembled the report.
    pub interface_version: String,
    /// Campaign identity.
    pub campaign_id: String,
    /// Repository identity.
    pub repository_id: String,
    /// Campaign authority digest.
    pub authority_sha256: String,
    /// Initial commit.
    pub initial_commit: String,
    /// Campaign head when observed.
    pub campaign_head: Option<String>,
    /// Preflight report digest the admission was recorded against, when admitted.
    pub admitted_preflight_sha256: Option<String>,
    /// Durable status as reported by the coordinator.
    pub status: Option<CampaignStatus>,
    /// Blocker explanation as reported by the coordinator.
    pub blockers: Option<BlockerExplanation>,
    /// Final parallel-campaign report when one exists.
    pub final_report: Option<CampaignReport>,
    /// Terminal outcome of the last coordinator invocation launched by the interface.
    pub last_invocation: Option<CampaignOutcome>,
    /// Coordinator commits on the campaign branch.
    pub commits: Vec<CommitSummary>,
    /// Number of changed files.
    pub changed_file_count: usize,
    /// Changed files, present only when repository paths were included.
    pub changed_files: Option<Vec<FileChange>>,
    /// Whether the document contains repository paths.
    pub contains_repository_paths: bool,
    /// Per-run summaries.
    pub runs: Vec<RunSummary>,
    /// Engineering and delivery disposition.
    pub disposition: Disposition,
    /// Fixed statement of what this document is not.
    pub limits: Vec<String>,
}

/// Inputs used to assemble a report.
#[derive(Clone, Debug)]
pub struct ReportInputs<'a> {
    /// Campaign identity.
    pub campaign_id: &'a str,
    /// Repository identity from the specification.
    pub repository_id: &'a str,
    /// Authority digest.
    pub authority_sha256: &'a str,
    /// Initial commit.
    pub initial_commit: &'a str,
    /// Publication code.
    pub publication: String,
    /// Admission when recorded.
    pub admission: Option<&'a Admission>,
    /// Status observation.
    pub status: Option<&'a CampaignStatus>,
    /// Blocker explanation.
    pub blockers: Option<&'a BlockerExplanation>,
    /// Final report.
    pub final_report: Option<&'a CampaignReport>,
    /// Last invocation outcome.
    pub last_invocation: Option<&'a CampaignOutcome>,
    /// Commits.
    pub commits: &'a [CommitSummary],
    /// Changed files.
    pub files: &'a [FileChange],
    /// Run records.
    pub runs: &'a [RunRecord],
}

impl OutcomeReport {
    /// Assembles the report; repository paths are included only when requested.
    #[must_use]
    pub fn assemble(inputs: &ReportInputs<'_>, include_repository_paths: bool) -> Self {
        let runs = inputs
            .runs
            .iter()
            .map(|record| RunSummary {
                run_id: record.run_id.clone(),
                task_id: record.task_id(),
                candidate_commit: record
                    .checkpoint
                    .as_ref()
                    .map(|checkpoint| checkpoint.candidate_commit.clone()),
                review_verdict: record
                    .checkpoint
                    .as_ref()
                    .and_then(|checkpoint| checkpoint.review_verdict.clone()),
                correction_rounds: record
                    .checkpoint
                    .as_ref()
                    .map(|checkpoint| checkpoint.correction_rounds),
                gate_evidence: record
                    .checkpoint
                    .as_ref()
                    .map(|checkpoint| checkpoint.gate_evidence.clone())
                    .unwrap_or_default(),
                problems: record
                    .checkpoint_problem
                    .iter()
                    .chain(record.journal_problem.iter())
                    .cloned()
                    .chain((record.malformed_journal_lines > 0).then(|| {
                        format!(
                            "{} malformed journal line(s)",
                            record.malformed_journal_lines
                        )
                    }))
                    .collect(),
            })
            .collect();
        let status = inputs.status;
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            generated_at_ms: now_ms(),
            interface_version: env!("CARGO_PKG_VERSION").to_owned(),
            campaign_id: inputs.campaign_id.to_owned(),
            repository_id: inputs.repository_id.to_owned(),
            authority_sha256: inputs.authority_sha256.to_owned(),
            initial_commit: inputs.initial_commit.to_owned(),
            campaign_head: status.map(|status| status.head.clone()),
            admitted_preflight_sha256: inputs
                .admission
                .map(|admission| admission.preflight_sha256.clone()),
            status: status.cloned(),
            blockers: inputs.blockers.cloned(),
            final_report: inputs.final_report.cloned(),
            last_invocation: inputs.last_invocation.cloned(),
            commits: inputs.commits.to_vec(),
            changed_file_count: inputs.files.len(),
            changed_files: include_repository_paths.then(|| inputs.files.to_vec()),
            contains_repository_paths: include_repository_paths,
            runs,
            disposition: Disposition {
                accepted_outcomes: status.map(|status| status.outcomes.accepted),
                max_accepted: status.map(|status| status.outcomes.max_accepted),
                completed_units: status.map(|status| status.completed_units),
                blocked: status.map(|status| status.outcomes.blocked),
                deferred: status.map(|status| status.outcomes.deferred),
                pending_human_decision: status.map(|status| status.outcomes.pending_human_decision),
                campaign_state: status.map(|status| status.state.clone()),
                publication: inputs.publication.clone(),
                delivery: "withheld: no push, merge or destination promotion exists in this milestone".to_owned(),
            },
            limits: [
                "This document restates coordinator records; it is not independent review.",
                "Reviewer finding text is not retained by the backend and is absent here.",
                "Accepted outcomes and completed units are engineering results on the campaign branch, not delivery.",
                "Blocked, deferred and human-decision counts are separate from completed counts.",
                "No credential, environment value, provider executable path or configuration path is included.",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }

    /// Pretty JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Encode`] when serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, WriteError> {
        serde_json::to_vec_pretty(self).map_err(|_| WriteError::Encode)
    }

    /// Writes the report with the export safeguards: absolute path outside the repository,
    /// never through a symbolic link, never replacing a file unless overwrite was requested.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError`] for refused destinations or I/O failure.
    pub fn export(
        &self,
        destination: &Path,
        repository: &Path,
        overwrite: bool,
    ) -> Result<PathBuf, WriteError> {
        if destination.starts_with(repository) {
            return Err(WriteError::InsideRepository(destination.to_path_buf()));
        }
        crate::setup::export_bytes(destination, &self.to_bytes()?, overwrite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_omits_paths_unless_requested_and_states_delivery_as_withheld() {
        let files = vec![FileChange {
            path: "src/lib.rs".to_owned(),
            added: Some(1),
            deleted: Some(1),
        }];
        let inputs = ReportInputs {
            campaign_id: "c",
            repository_id: "repo-1",
            authority_sha256: "a",
            initial_commit: "b",
            publication: "local_only".to_owned(),
            admission: None,
            status: None,
            blockers: None,
            final_report: None,
            last_invocation: None,
            commits: &[],
            files: &files,
            runs: &[],
        };
        let private = OutcomeReport::assemble(&inputs, false);
        let text = String::from_utf8(private.to_bytes().unwrap()).unwrap();
        assert!(!text.contains("src/lib.rs"));
        assert!(text.contains("\"changed_file_count\": 1"));
        assert!(text.contains("withheld"));
        let with_paths = OutcomeReport::assemble(&inputs, true);
        let text = String::from_utf8(with_paths.to_bytes().unwrap()).unwrap();
        assert!(text.contains("src/lib.rs"));
        assert!(with_paths.contains_repository_paths);
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("repo")).unwrap();
        assert!(matches!(
            private.export(&root.join("repo/report.json"), &root.join("repo"), false),
            Err(WriteError::InsideRepository(_))
        ));
        let written = private
            .export(&root.join("report.json"), &root.join("repo"), false)
            .unwrap();
        assert!(matches!(
            private.export(&written, &root.join("repo"), false),
            Err(WriteError::Exists(_))
        ));
        std::os::unix::fs::symlink(&written, root.join("link.json")).unwrap();
        assert!(matches!(
            private.export(&root.join("link.json"), &root.join("repo"), true),
            Err(WriteError::Exists(_))
        ));
        assert!(private.export(&written, &root.join("repo"), true).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
