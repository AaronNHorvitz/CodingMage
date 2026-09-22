//! Reports: inspect and export outcome and blocker reports assembled from real records.

use std::path::PathBuf;

use super::{App, failure_box};
use crate::{
    launch::LaunchState,
    report::{OutcomeReport, ReportInputs},
};

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
    /// Show the full JSON document inline.
    pub show_json: bool,
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
            blockers: self.explanation.value.as_ref(),
            final_report: self.report.value.as_ref().and_then(Option::as_ref),
            last_invocation: last_invocation.as_ref(),
            commits: changes.map_or(&empty_commits, |changes| &changes.commits),
            files: changes.map_or(&empty_files, |changes| &changes.files),
            runs: self.records.value.as_deref().unwrap_or(&empty_runs),
        };
        Some(OutcomeReport::assemble(&inputs, include_paths))
    }

    /// Exports the report to the configured destination.
    pub fn export_report(&mut self) {
        let Some(report) = self.assemble_report(self.reports.include_paths) else {
            self.reports.message = Some(Err("select a campaign first".to_owned()));
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let destination = PathBuf::from(self.reports.export_path.trim());
        let repository = project.config.target_path.clone();
        self.reports.message = Some(
            match report.export(&destination, &repository, self.reports.overwrite) {
                Ok(path) => Ok(format!(
                    "report exported to {} ({} repository paths)",
                    path.display(),
                    if report.contains_repository_paths {
                        "with"
                    } else {
                        "without"
                    }
                )),
                Err(error) => Err(error.to_string()),
            },
        );
    }

    pub(super) fn reports_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reports");
        let Some(report) = self.assemble_report(self.reports.include_paths) else {
            ui.label("Open a repository and select a campaign to assemble its reports.");
            return;
        };
        ui.label("Reports restate coordinator records. Viewing or exporting them changes no task status and creates no review authority.");
        ui.separator();
        outcome_summary(ui, &report);
        ui.separator();
        blocker_report(ui, &report);
        ui.separator();
        ui.strong("Export");
        ui.horizontal(|ui| {
            ui.label("Destination");
            ui.add(
                egui::TextEdit::singleline(&mut self.reports.export_path)
                    .hint_text("/absolute/path/outside/the/repository/report.json")
                    .desired_width(420.0),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.reports.include_paths,
                "Include repository file paths",
            );
            ui.checkbox(&mut self.reports.overwrite, "Replace an existing file");
            if ui.button("Export report").clicked() {
                self.export_report();
            }
        });
        if let Some(message) = &self.reports.message {
            match message {
                Ok(text) => {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 120), text);
                }
                Err(text) => failure_box(
                    ui,
                    "Export refused",
                    text,
                    "Choose an absolute destination outside the repository, or confirm replacement.",
                ),
            }
        }
        ui.checkbox(&mut self.reports.show_json, "Show the full report document");
        if self.reports.show_json
            && let Ok(bytes) = report.to_bytes()
        {
            egui::ScrollArea::vertical()
                .id_salt("report-json")
                .max_height(300.0)
                .show(ui, |ui| {
                    ui.monospace(String::from_utf8_lossy(&bytes).to_string());
                });
        }
    }
}

fn outcome_summary(ui: &mut egui::Ui, report: &OutcomeReport) {
    ui.strong("Outcome report");
    let disposition = &report.disposition;
    egui::Grid::new("report-disposition")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Campaign state");
            ui.label(
                disposition
                    .campaign_state
                    .as_deref()
                    .unwrap_or("not observed"),
            );
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
            ui.label(report.commits.len().to_string());
            ui.end_row();
            ui.label("Changed files");
            ui.label(report.changed_file_count.to_string());
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
            ui.label("No blocker explanation has been observed.");
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
