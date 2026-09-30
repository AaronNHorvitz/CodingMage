//! Inspectable and exportable outcome and blocker reports assembled from real records only.
//!
//! A report is a plain document of what the coordinator and the repository objects reported.
//! It never contains configuration paths, environment values, provider executables or
//! credentials, and it includes repository file paths only when the owner opts in.

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    admission::{Admission, now_ms},
    backend::models::{BlockerExplanation, CampaignOutcome, CampaignReport, CampaignStatus},
    records::{CommitSummary, FileChange, RunRecord},
    setup::WriteError,
};

/// Report document version.
pub const REPORT_SCHEMA_VERSION: u16 = 3;
/// Maximum serialized bytes shown in the inline report preview.
pub const REPORT_PREVIEW_LIMIT: usize = 128 * 1024;

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
    /// Review verdict when checkpoint and journal are both available.
    pub review_verdict: Option<String>,
    /// Correction rounds when checkpointed.
    pub correction_rounds: Option<u16>,
    /// Gate evidence identities when checkpoint and journal are both available.
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

/// Completeness of the coordinator's change projection used by this report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeCoverage {
    /// Whether the projection was observed at all.
    pub observed: bool,
    /// Whether the coordinator omitted additional commits.
    pub commits_truncated: bool,
    /// Whether the coordinator omitted additional changed files.
    pub files_truncated: bool,
}

impl ChangeCoverage {
    fn normalized(self) -> Self {
        Self {
            observed: self.observed,
            commits_truncated: self.observed && self.commits_truncated,
            files_truncated: self.observed && self.files_truncated,
        }
    }

    fn limitation(self) -> Option<&'static str> {
        if !self.observed {
            Some("Changes were not observed; commit and changed-file counts are unknown.")
        } else if self.commits_truncated || self.files_truncated {
            Some(
                "The coordinator omitted additional changes; affected displayed counts are lower bounds.",
            )
        } else {
            None
        }
    }
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
    /// Observation and truncation state of the coordinator's change projection.
    pub change_coverage: ChangeCoverage,
    /// Number of observed changed files, or unknown when changes were not observed.
    pub changed_file_count: Option<usize>,
    /// Changed files, present only when repository paths were included.
    pub changed_files: Option<Vec<FileChange>>,
    /// Whether the document contains repository paths.
    pub contains_repository_paths: bool,
    /// Per-run summaries.
    pub runs: Vec<RunSummary>,
    /// Whether bound run evidence was actually observed for this report.
    pub run_records_observed: bool,
    /// Whether the coordinator omitted additional bound runs at its fixed limit.
    pub run_records_truncated: bool,
    /// Engineering and delivery disposition.
    pub disposition: Disposition,
    /// Limitations and source freshness at assembly time.
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
    /// Observation and truncation state of the coordinator's change projection.
    pub change_coverage: ChangeCoverage,
    /// Run records.
    pub runs: &'a [RunRecord],
    /// Whether run records were observed.
    pub run_records_observed: bool,
    /// Whether the run projection omitted additional records.
    pub run_records_truncated: bool,
}

impl RunSummary {
    fn from_record(record: &RunRecord) -> Self {
        Self {
            run_id: record.run_id.clone(),
            task_id: record.task_id(),
            candidate_commit: record
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.candidate_commit.clone()),
            review_verdict: record
                .checkpoint
                .as_ref()
                .filter(|_| record.journal_problem.is_none())
                .and_then(|checkpoint| checkpoint.review_verdict.clone()),
            correction_rounds: record
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.correction_rounds),
            gate_evidence: record
                .checkpoint
                .as_ref()
                .filter(|_| record.journal_problem.is_none())
                .map(|checkpoint| checkpoint.gate_evidence.clone())
                .unwrap_or_default(),
            problems: record
                .checkpoint_problem
                .iter()
                .chain(record.journal_problem.iter())
                .cloned()
                .chain(
                    record
                        .phases_truncated
                        .then(|| "additional journal phases omitted".to_owned()),
                )
                .collect(),
        }
    }
}

impl OutcomeReport {
    /// Assembles the report; repository paths are included only when requested.
    #[must_use]
    pub fn assemble(inputs: &ReportInputs<'_>, include_repository_paths: bool) -> Self {
        let runs = inputs.runs.iter().map(RunSummary::from_record).collect();
        let status = inputs.status;
        let coverage = inputs.change_coverage.normalized();
        let mut report = Self {
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
            commits: if coverage.observed {
                inputs.commits.to_vec()
            } else {
                Vec::new()
            },
            change_coverage: coverage,
            changed_file_count: coverage.observed.then_some(inputs.files.len()),
            changed_files: (include_repository_paths && coverage.observed)
                .then(|| inputs.files.to_vec()),
            contains_repository_paths: include_repository_paths
                && coverage.observed
                && !inputs.files.is_empty(),
            runs,
            run_records_observed: inputs.run_records_observed,
            run_records_truncated: inputs.run_records_truncated,
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
        };
        if let Some(limit) = coverage.limitation() {
            report.limits.push(limit.to_owned());
        }
        report
    }

    /// Pretty JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Encode`] when serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, WriteError> {
        serde_json::to_vec_pretty(self).map_err(|_| WriteError::Encode)
    }

    /// Serializes only a bounded preview. A large document remains available through export.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Encode`] for a serialization failure unrelated to the limit.
    pub fn to_preview_bytes(&self) -> Result<Vec<u8>, WriteError> {
        let mut writer = PreviewWriter::default();
        let result = serde_json::to_writer_pretty(&mut writer, self);
        if writer.truncated {
            writer
                .bytes
                .extend_from_slice(b"\n[Preview truncated; export the full report.]\n");
            Ok(writer.bytes)
        } else {
            result.map_err(|_| WriteError::Encode)?;
            Ok(writer.bytes)
        }
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
        crate::report_export::validate_report_destination(destination, repository)?;
        crate::report_export::write_report(
            destination,
            repository,
            &self.to_bytes()?,
            overwrite,
            || {},
            || {},
        )
    }
}

#[derive(Default)]
struct PreviewWriter {
    bytes: Vec<u8>,
    truncated: bool,
}

impl Write for PreviewWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let available = REPORT_PREVIEW_LIMIT.saturating_sub(self.bytes.len());
        if buf.len() > available {
            self.bytes.extend_from_slice(&buf[..available]);
            self.truncated = true;
            Err(io::Error::other("report preview limit reached"))
        } else {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::RunCheckpoint;
    use std::fs;

    #[test]
    fn inline_preview_is_bounded_and_full_export_is_preserved() {
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
            files: &[],
            change_coverage: ChangeCoverage {
                observed: false,
                commits_truncated: false,
                files_truncated: false,
            },
            runs: &[],
            run_records_observed: false,
            run_records_truncated: false,
        };
        let mut report = OutcomeReport::assemble(&inputs, false);
        assert_eq!(
            report.to_preview_bytes().unwrap(),
            report.to_bytes().unwrap()
        );
        report.limits.push("x".repeat(REPORT_PREVIEW_LIMIT * 2));
        let preview = report.to_preview_bytes().unwrap();
        assert!(preview.len() < REPORT_PREVIEW_LIMIT + 100);
        assert!(preview.ends_with(b"[Preview truncated; export the full report.]\n"));
        assert!(report.to_bytes().unwrap().len() > REPORT_PREVIEW_LIMIT);
    }

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
            change_coverage: ChangeCoverage {
                observed: true,
                commits_truncated: false,
                files_truncated: false,
            },
            runs: &[],
            run_records_observed: false,
            run_records_truncated: false,
        };
        let private = OutcomeReport::assemble(&inputs, false);
        assert_eq!(private.schema_version, 3);
        let text = String::from_utf8(private.to_bytes().unwrap()).unwrap();
        assert!(!text.contains("src/lib.rs"));
        assert!(text.contains("\"changed_file_count\": 1"));
        assert!(text.contains("withheld"));
        assert!(text.contains("\"run_records_observed\": false"));
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

    #[test]
    fn missing_and_truncated_change_observations_keep_distinct_counts() {
        let file = FileChange {
            path: "src/one.rs".to_owned(),
            added: Some(1),
            deleted: Some(0),
        };
        let mut inputs = ReportInputs {
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
            files: std::slice::from_ref(&file),
            change_coverage: ChangeCoverage {
                observed: false,
                commits_truncated: true,
                files_truncated: true,
            },
            runs: &[],
            run_records_observed: false,
            run_records_truncated: false,
        };
        let missing = OutcomeReport::assemble(&inputs, true);
        assert_eq!(missing.changed_file_count, None);
        assert!(!missing.change_coverage.observed);
        assert!(!missing.change_coverage.commits_truncated);
        assert!(!missing.change_coverage.files_truncated);
        assert!(missing.changed_files.is_none());
        assert!(!missing.contains_repository_paths);
        assert!(missing.limits.iter().any(|line| line.contains("unknown")));
        inputs.files = &[];
        inputs.change_coverage = ChangeCoverage {
            observed: true,
            commits_truncated: false,
            files_truncated: false,
        };
        let observed_zero = OutcomeReport::assemble(&inputs, false);
        assert_eq!(observed_zero.changed_file_count, Some(0));
        assert!(observed_zero.change_coverage.observed);
        assert!(!OutcomeReport::assemble(&inputs, true).contains_repository_paths);
        inputs.files = std::slice::from_ref(&file);
        inputs.change_coverage.commits_truncated = true;
        inputs.change_coverage.files_truncated = true;
        let truncated = OutcomeReport::assemble(&inputs, true);
        assert_eq!(truncated.changed_file_count, Some(1));
        assert!(truncated.change_coverage.commits_truncated);
        assert!(truncated.change_coverage.files_truncated);
        assert!(
            truncated
                .limits
                .iter()
                .any(|line| line.contains("lower bounds"))
        );
        let json: serde_json::Value =
            serde_json::from_slice(&truncated.to_bytes().unwrap()).unwrap();
        assert_eq!(json["changed_file_count"], 1);
        assert_eq!(json["change_coverage"]["files_truncated"], true);
    }

    #[test]
    fn report_export_refuses_traversal_and_linked_parent_into_repository() {
        let root = std::env::temp_dir().join(format!(
            "codingmage-ui-report-parent-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let repository = root.join("repo");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        std::os::unix::fs::symlink(&repository, root.join("linked-repo")).unwrap();
        let report = OutcomeReport::assemble(
            &ReportInputs {
                campaign_id: "campaign",
                repository_id: "repo",
                authority_sha256: "a",
                initial_commit: "b",
                publication: "local_only".to_owned(),
                admission: None,
                status: None,
                blockers: None,
                final_report: None,
                last_invocation: None,
                commits: &[],
                files: &[],
                change_coverage: ChangeCoverage {
                    observed: false,
                    commits_truncated: false,
                    files_truncated: false,
                },
                runs: &[],
                run_records_observed: false,
                run_records_truncated: false,
            },
            false,
        );
        let linked = root.join("linked-repo/report.json");
        assert!(matches!(
            report.export(&linked, &repository, false),
            Err(WriteError::InsideRepository(_))
        ));
        let traversed = root.join("outside/../repo/report.json");
        assert!(matches!(
            report.export(&traversed, &repository, false),
            Err(WriteError::Fields(_))
        ));
        let missing_parent = root.join("new-directory/report.json");
        assert!(matches!(
            report.export(&missing_parent, &repository, false),
            Err(WriteError::Fields(_))
        ));
        assert!(!missing_parent.parent().unwrap().exists());
        assert!(!repository.join("report.json").exists());
        assert!(
            report
                .export(&root.join("outside/report.json"), &repository, false)
                .is_ok()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_journal_withholds_checkpoint_review_and_gate_claims() {
        let mut run = RunRecord {
            run_id: "run-1".to_owned(),
            bound_task_id: "1.1.1.1".to_owned(),
            checkpoint: Some(RunCheckpoint {
                schema_version: 1,
                run_id: "run-1".to_owned(),
                task_id: "1.1.1.1".to_owned(),
                candidate_commit: "a".repeat(40),
                review_verdict: Some("pass".to_owned()),
                correction_rounds: 0,
                gate_evidence: vec!["ev-1".to_owned()],
            }),
            checkpoint_problem: None,
            phases: Vec::new(),
            phases_truncated: false,
            journal_problem: None,
        };
        let report_for = |run: &RunRecord| {
            OutcomeReport::assemble(
                &ReportInputs {
                    campaign_id: "campaign-a",
                    repository_id: "repo-a",
                    authority_sha256: "a",
                    initial_commit: "b",
                    publication: "local_only".to_owned(),
                    admission: None,
                    status: None,
                    blockers: None,
                    final_report: None,
                    last_invocation: None,
                    commits: &[],
                    files: &[],
                    change_coverage: ChangeCoverage {
                        observed: false,
                        commits_truncated: false,
                        files_truncated: false,
                    },
                    runs: std::slice::from_ref(run),
                    run_records_observed: true,
                    run_records_truncated: false,
                },
                false,
            )
        };
        let observed = report_for(&run);
        assert_eq!(observed.runs[0].review_verdict.as_deref(), Some("pass"));
        assert_eq!(observed.runs[0].gate_evidence, ["ev-1".to_owned()]);
        run.journal_problem = Some("events.jsonl has invalid identity".to_owned());
        let unavailable = report_for(&run);
        assert!(unavailable.runs[0].review_verdict.is_none());
        assert!(unavailable.runs[0].gate_evidence.is_empty());
        assert_eq!(
            unavailable.runs[0].problems,
            ["events.jsonl has invalid identity".to_owned()]
        );
    }
}
