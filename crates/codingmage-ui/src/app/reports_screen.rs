//! Reports: inspect and export outcome and blocker reports assembled from real records.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{App, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response},
    launch::LaunchState,
    observed::Freshness,
    report::{ChangeCoverage, OutcomeReport, ReportInputs},
};

#[derive(Debug)]
struct PendingExport {
    request_id: String,
    destination: PathBuf,
    contains_repository_paths: bool,
}

/// Reports screen state.
#[derive(Default)]
pub struct ReportsState {
    /// Export destination.
    pub export_path: String,
    /// Include repository file paths in the export.
    pub include_paths: bool,
    /// Replace an existing destination.
    pub overwrite: bool,
    /// Last export outcome.
    pub message: Option<Result<String, String>>,
    /// Show a bounded JSON preview inline; export retains the full document.
    pub show_json: bool,
    pending: Option<PendingExport>,
}

impl App {
    /// Reports screen state.
    #[must_use]
    pub const fn reports_state(&self) -> &ReportsState {
        &self.reports
    }

    /// Mutable reports screen state (used by tests).
    pub const fn reports_state_mut(&mut self) -> &mut ReportsState {
        &mut self.reports
    }

    /// Whether an explicitly requested export is queued or running.
    #[must_use]
    pub fn report_export_pending(&self) -> bool {
        self.reports.pending.is_some()
    }

    pub(super) fn clear_report_export(&mut self) {
        self.reports.pending = None;
        self.reports.message = None;
    }

    /// Assembles the outcome report from the current observations.
    #[must_use]
    pub fn assemble_report(&self, include_paths: bool) -> Option<OutcomeReport> {
        let campaign = self.campaign.as_ref()?;
        let last_invocation = match &self.execution.observed {
            Some((_, LaunchState::Exited(outcome))) => Some(outcome.clone()),
            _ => None,
        };
        let empty_commits = Vec::new();
        let empty_files = Vec::new();
        let empty_runs = Vec::new();
        let changes = self.changes.value.as_ref();
        let inputs = ReportInputs {
            campaign_id: &campaign.spec.campaign_id,
            repository_id: &campaign.spec.repository_id,
            authority_sha256: &campaign.authority_sha256,
            initial_commit: &campaign.spec.initial_commit,
            publication: format!("{:?}", campaign.spec.publication),
            admission: self.execution.admission.as_ref(),
            status: self.status.value.as_ref().and_then(Option::as_ref),
            blockers: self.explanation.value.as_ref().and_then(Option::as_ref),
            final_report: self.report.value.as_ref().and_then(Option::as_ref),
            last_invocation: last_invocation.as_ref(),
            commits: changes.map_or(&empty_commits, |changes| &changes.commits),
            files: changes.map_or(&empty_files, |changes| &changes.files),
            change_coverage: ChangeCoverage {
                observed: changes.is_some(),
                commits_truncated: changes.is_some_and(|changes| changes.commits_truncated),
                files_truncated: changes.is_some_and(|changes| changes.files_truncated),
            },
            runs: self.records.value.as_deref().unwrap_or(&empty_runs),
            run_records_observed: self.records.value.is_some(),
            run_records_truncated: self.records_truncated,
        };
        let mut report = OutcomeReport::assemble(&inputs, include_paths);
        report.limits.push(self.report_source_freshness());
        Some(report)
    }

    fn report_source_freshness(&self) -> String {
        let now = Instant::now();
        format!(
            "Source observation freshness at assembly: campaign status {}, blockers {}, final report {}, changes {}, run records {}.",
            self.status.freshness(now).label(),
            self.explanation.freshness(now).label(),
            self.report.freshness(now).label(),
            self.changes.freshness(now).label(),
            self.records.freshness(now).label(),
        )
    }

    /// Exports the report to the configured destination.
    pub fn export_report(&mut self) {
        if self.reports.pending.is_some() {
            self.reports.message = Some(Err("a report export is already pending".to_owned()));
            return;
        }
        let Some(report) = self.assemble_report(self.reports.include_paths) else {
            self.reports.message = Some(Err("select a campaign first".to_owned()));
            return;
        };
        let Some(repository) = self
            .project
            .as_ref()
            .map(|project| project.config.target_path.clone())
        else {
            return;
        };
        let destination = PathBuf::from(self.reports.export_path.trim());
        let contains_repository_paths = report.contains_repository_paths;
        self.next_evidence_request += 1;
        let request_id = format!("report-export-{}", self.next_evidence_request);
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::ReportExport {
                report: Box::new(report),
                destination: destination.clone(),
                repository,
                overwrite: self.reports.overwrite,
                deadline: Duration::from_secs(30),
            },
            request_id: Some(request_id.clone()),
        };
        self.reports.message = None;
        match self.submit(request) {
            Ok(()) => {
                self.reports.pending = Some(PendingExport {
                    request_id,
                    destination,
                    contains_repository_paths,
                });
            }
            Err(error) => self.reports.message = Some(Err(error.to_string())),
        }
    }

    pub(super) fn accept_report_export(&mut self, response: Response) -> bool {
        let Some(pending) = self.reports.pending.as_ref() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            return false;
        }
        let Some(pending) = self.reports.pending.take() else {
            return false;
        };
        self.reports.message = Some(match response.result {
            Ok(_) => Ok(format!(
                "report exported to {} ({} repository paths)",
                pending.destination.display(),
                if pending.contains_repository_paths {
                    "with"
                } else {
                    "without"
                }
            )),
            Err(BackendError::Refused(reason)) => Err(reason),
            Err(BackendError::Timeout) => Err(
                "report export timed out. The destination is uncertain; inspect it and any .codingmage-report-*.candidate file in that directory before retrying."
                    .to_owned(),
            ),
            Err(BackendError::Spawn) => Err(
                "the report writer could not start or returned an unreadable result; inspect the destination and retry after checking the desktop installation"
                    .to_owned(),
            ),
            Err(error) => Err(error.to_string()),
        });
        true
    }

    pub(super) fn reports_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reports");
        let Some(report) = self.assemble_report(self.reports.include_paths) else {
            ui.label("Open a repository and select a campaign to assemble its reports.");
            return;
        };
        ui.label("Reports restate coordinator records. Viewing or exporting them changes no task status and creates no review authority.");
        let freshness = self.report_source_freshness();
        let source_states = [
            self.status.freshness(self.now),
            self.explanation.freshness(self.now),
            self.report.freshness(self.now),
            self.changes.freshness(self.now),
            self.records.freshness(self.now),
        ];
        if source_states.iter().any(|state| *state != Freshness::Live) {
            ui.colored_label(super::current_tokens(ui.ctx()).warning, &freshness);
            ui.small("Some source observations are loading, missing, stale or failed. Refresh the campaign before relying on retained data; an export retains this freshness statement.");
        } else {
            ui.label(freshness);
        }
        if !report.run_records_observed {
            ui.colored_label(
                super::current_tokens(ui.ctx()).error,
                "Run evidence has not been observed; the run list is unknown.",
            );
        } else if report.run_records_truncated {
            ui.label("Additional bound run records are omitted from this report.");
        }
        ui.separator();
        outcome_summary(ui, &report);
        ui.separator();
        blocker_report(ui, &report);
        ui.separator();
        ui.strong("Export");
        ui.small("Choose a file in an existing directory outside the target repository. Linked directories that lead into the repository are refused.");
        ui.small("This local report export has no matching codingmage command yet.");
        ui.horizontal(|ui| {
            let label = ui.label("Destination");
            ui.add(
                egui::TextEdit::singleline(&mut self.reports.export_path)
                    .hint_text("/absolute/path/outside/the/repository/report.json")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.reports.include_paths,
                "Include repository file paths",
            );
            ui.checkbox(&mut self.reports.overwrite, "Replace an existing file");
            if ui
                .add_enabled(
                    !self.report_export_pending(),
                    egui::Button::new("Export report"),
                )
                .clicked()
            {
                self.export_report();
            }
        });
        if self.report_export_pending() {
            ui.label("Export pending; the report snapshot was captured when you requested it.");
        }
        if let Some(message) = &self.reports.message {
            match message {
                Ok(text) => {
                    ui.colored_label(super::current_tokens(ui.ctx()).success, text);
                }
                Err(text) => failure_box(
                    ui,
                    "Export refused",
                    text,
                    "Check the stated cause and inspect any existing destination before retrying.",
                ),
            }
        }
        ui.checkbox(&mut self.reports.show_json, "Preview report document");
        if self.reports.show_json
            && let Ok(bytes) = report.to_bytes()
        {
            egui::ScrollArea::vertical()
                .id_salt("report-json")
                .max_height(super::current_tokens(ui.ctx()).layout.preview_reports)
                .show(ui, |ui| {
                    crate::content::render(ui, &String::from_utf8_lossy(&bytes));
                });
        }
    }
}

fn outcome_summary(ui: &mut egui::Ui, report: &OutcomeReport) {
    ui.strong("Outcome report");
    let disposition = &report.disposition;
    egui::Grid::new("report-disposition")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label("Campaign state");
            ui.label(disposition.campaign_state.as_deref().map_or_else(
                || "not observed".to_owned(),
                crate::campaign::campaign_state_label,
            ));
            ui.end_row();
            ui.label("Accepted outcomes");
            ui.label(
                match (disposition.accepted_outcomes, disposition.max_accepted) {
                    (Some(accepted), Some(max)) => format!("{accepted} of {max}"),
                    _ => "not observed".to_owned(),
                },
            );
            ui.end_row();
            ui.label("Completed units");
            ui.label(count(disposition.completed_units));
            ui.end_row();
            ui.label("Blocked / deferred / human decision");
            ui.label(format!(
                "{} / {} / {}",
                count(disposition.blocked),
                count(disposition.deferred),
                count(disposition.pending_human_decision)
            ));
            ui.end_row();
            ui.label("Coordinator commits");
            ui.label(observed_count(
                report
                    .change_coverage
                    .observed
                    .then_some(report.commits.len()),
                report.change_coverage.commits_truncated,
            ));
            ui.end_row();
            ui.label("Changed files");
            ui.label(observed_count(
                report.changed_file_count,
                report.change_coverage.files_truncated,
            ));
            ui.end_row();
            ui.label("Runs with records");
            ui.label(report.runs.len().to_string());
            ui.end_row();
            ui.label("Delivery");
            ui.label(&disposition.delivery);
            ui.end_row();
        });
    for limit in &report.limits {
        ui.small(limit);
    }
}

fn blocker_report(ui: &mut egui::Ui, report: &OutcomeReport) {
    ui.strong("Blocker report");
    match &report.blockers {
        None => {
            ui.label("No blocker explanation is available in these observations.");
        }
        Some(explanation) => {
            ui.label(format!(
                "Campaign-level blocker: {}",
                explanation.blocker_code.as_deref().unwrap_or("none")
            ));
            if explanation.blockers.is_empty()
                && explanation.deferrals.is_empty()
                && explanation.human_decisions.is_empty()
            {
                ui.label("No blocked, deferred or human-decision tasks.");
            }
            for blocker in &explanation.blockers {
                ui.monospace(format!(
                    "blocked {} - {}",
                    blocker.task_id, blocker.reason_code
                ));
            }
            for deferral in &explanation.deferrals {
                ui.monospace(format!(
                    "deferred {} - {} until {} ({})",
                    deferral.task_id,
                    deferral.reason_code,
                    deferral.trigger_code,
                    deferral.trigger_state
                ));
            }
            for decision in &explanation.human_decisions {
                ui.monospace(format!(
                    "human decision {} - {}",
                    decision.task_id, decision.reason_code
                ));
            }
            ui.small("Clearing a blocker or observing a trigger needs operator-controlled evidence digests through the campaign-clear-blocker and campaign-observe-trigger commands; this interface does not run them and never clears a blocker on its own.");
        }
    }
}

fn count(value: Option<u32>) -> String {
    value.map_or_else(|| "not observed".to_owned(), |value| value.to_string())
}

fn observed_count(value: Option<usize>, truncated: bool) -> String {
    match value {
        None => "not observed".to_owned(),
        Some(count) if truncated => format!("at least {count}; more omitted"),
        Some(count) => count.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::observed_count;

    #[test]
    fn change_count_labels_do_not_turn_unknown_or_truncated_into_exact_zero() {
        assert_eq!(observed_count(None, false), "not observed");
        assert_eq!(observed_count(Some(0), false), "0");
        assert_eq!(observed_count(Some(1), true), "at least 1; more omitted");
    }
}
