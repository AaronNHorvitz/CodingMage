//! Reports: inspect and export outcome and blocker reports assembled from real records.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{App, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response},
    launch::LaunchState,
    messages::{self, Catalogue},
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
        self.reports_screen_with_catalogue(ui, messages::english(), false);
    }

    fn reports_screen_with_catalogue(
        &mut self,
        ui: &mut egui::Ui,
        catalogue: &Catalogue,
        right_to_left: bool,
    ) {
        if right_to_left {
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                self.reports_content(ui, catalogue);
            });
        } else {
            self.reports_content(ui, catalogue);
        }
    }

    fn reports_content(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("reports_title"));
        let Some(report) = self.assemble_report(self.reports.include_paths) else {
            ui.label(catalogue.text("reports_open_campaign"));
            return;
        };
        ui.label(catalogue.text("reports_intro"));
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
            ui.small(catalogue.text("reports_source_warning"));
        } else {
            ui.label(freshness);
        }
        if !report.run_records_observed {
            ui.colored_label(
                super::current_tokens(ui.ctx()).error,
                catalogue.text("reports_run_unknown"),
            );
        } else if report.run_records_truncated {
            ui.label(catalogue.text("reports_run_truncated"));
        }
        ui.separator();
        outcome_summary(ui, &report, catalogue);
        ui.separator();
        blocker_report(ui, &report, catalogue);
        ui.separator();
        ui.strong(catalogue.text("reports_export_title"));
        ui.small(catalogue.text("reports_destination_guidance"));
        ui.small(catalogue.text("reports_command_unavailable"));
        ui.horizontal(|ui| {
            let label = ui.label(catalogue.text("reports_destination"));
            ui.add(
                egui::TextEdit::singleline(&mut self.reports.export_path)
                    .hint_text(catalogue.text("reports_destination_hint"))
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.reports.include_paths,
                catalogue.text("reports_include_paths"),
            );
            ui.checkbox(
                &mut self.reports.overwrite,
                catalogue.text("reports_overwrite"),
            );
            if ui
                .add_enabled(
                    !self.report_export_pending(),
                    egui::Button::new(catalogue.text("reports_export")),
                )
                .clicked()
            {
                self.export_report();
            }
        });
        if self.report_export_pending() {
            ui.label(catalogue.text("reports_pending"));
        }
        if let Some(message) = &self.reports.message {
            match message {
                Ok(text) => {
                    ui.colored_label(super::current_tokens(ui.ctx()).success, text);
                }
                Err(text) => failure_box(
                    ui,
                    catalogue.text("reports_refused"),
                    text,
                    catalogue.text("reports_recovery"),
                ),
            }
        }
        ui.checkbox(
            &mut self.reports.show_json,
            catalogue.text("reports_preview"),
        );
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

fn outcome_summary(ui: &mut egui::Ui, report: &OutcomeReport, catalogue: &Catalogue) {
    ui.strong(catalogue.text("reports_outcome_title"));
    let disposition = &report.disposition;
    egui::Grid::new("report-disposition")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label(catalogue.text("reports_campaign_state"));
            ui.label(disposition.campaign_state.as_deref().map_or_else(
                || catalogue.text("reports_not_observed").to_owned(),
                crate::campaign::campaign_state_label,
            ));
            ui.end_row();
            ui.label(catalogue.text("reports_accepted"));
            ui.label(
                match (disposition.accepted_outcomes, disposition.max_accepted) {
                    (Some(accepted), Some(max)) => {
                        format!("{accepted} {} {max}", catalogue.text("reports_of"))
                    }
                    _ => catalogue.text("reports_not_observed").to_owned(),
                },
            );
            ui.end_row();
            ui.label(catalogue.text("reports_completed"));
            ui.label(count(disposition.completed_units, catalogue));
            ui.end_row();
            ui.label(catalogue.text("reports_hold_counts"));
            ui.label(format!(
                "{} / {} / {}",
                count(disposition.blocked, catalogue),
                count(disposition.deferred, catalogue),
                count(disposition.pending_human_decision, catalogue)
            ));
            ui.end_row();
            ui.label(catalogue.text("reports_commits"));
            ui.label(observed_count(
                report
                    .change_coverage
                    .observed
                    .then_some(report.commits.len()),
                report.change_coverage.commits_truncated,
                catalogue,
            ));
            ui.end_row();
            ui.label(catalogue.text("reports_files"));
            ui.label(observed_count(
                report.changed_file_count,
                report.change_coverage.files_truncated,
                catalogue,
            ));
            ui.end_row();
            ui.label(catalogue.text("reports_runs"));
            ui.label(report.runs.len().to_string());
            ui.end_row();
            ui.label(catalogue.text("reports_delivery"));
            ui.label(&disposition.delivery);
            ui.end_row();
        });
    for limit in &report.limits {
        ui.small(limit);
    }
}

fn blocker_report(ui: &mut egui::Ui, report: &OutcomeReport, catalogue: &Catalogue) {
    ui.strong(catalogue.text("reports_blocker_title"));
    match &report.blockers {
        None => {
            ui.label(catalogue.text("reports_blocker_absent"));
        }
        Some(explanation) => {
            ui.label(format!(
                "{} {}",
                catalogue.text("reports_blocker_prefix"),
                explanation
                    .blocker_code
                    .as_deref()
                    .unwrap_or(catalogue.text("reports_none"))
            ));
            if explanation.blockers.is_empty()
                && explanation.deferrals.is_empty()
                && explanation.human_decisions.is_empty()
            {
                ui.label(catalogue.text("reports_no_holds"));
            }
            for blocker in &explanation.blockers {
                ui.monospace(format!(
                    "{} {} - {}",
                    catalogue.text("reports_blocked_prefix"),
                    blocker.task_id,
                    blocker.reason_code
                ));
            }
            for deferral in &explanation.deferrals {
                ui.monospace(format!(
                    "{} {} - {} {} {} ({})",
                    catalogue.text("reports_deferred_prefix"),
                    deferral.task_id,
                    deferral.reason_code,
                    catalogue.text("reports_until"),
                    deferral.trigger_code,
                    deferral.trigger_state
                ));
            }
            for decision in &explanation.human_decisions {
                ui.monospace(format!(
                    "{} {} - {}",
                    catalogue.text("reports_human_prefix"),
                    decision.task_id,
                    decision.reason_code
                ));
            }
            ui.small(catalogue.text("reports_clearance_boundary"));
        }
    }
}

fn count(value: Option<u32>, catalogue: &Catalogue) -> String {
    value.map_or_else(
        || catalogue.text("reports_not_observed").to_owned(),
        |value| value.to_string(),
    )
}

fn observed_count(value: Option<usize>, truncated: bool, catalogue: &Catalogue) -> String {
    match value {
        None => catalogue.text("reports_not_observed").to_owned(),
        Some(count) if truncated => format!(
            "{} {count}; {}",
            catalogue.text("reports_at_least"),
            catalogue.text("reports_more_omitted")
        ),
        Some(count) => count.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::BackendError, report::ReportInputs};
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    struct ReportsPreviewApp {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
        report: OutcomeReport,
    }

    impl eframe::App for ReportsPreviewApp {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.app
                        .reports_screen_with_catalogue(ui, &self.catalogue, self.right_to_left);
                    outcome_summary(ui, &self.report, &self.catalogue);
                    blocker_report(ui, &self.report, &self.catalogue);
                });
            });
        }
    }

    fn empty_report() -> OutcomeReport {
        OutcomeReport::assemble(
            &ReportInputs {
                campaign_id: "preview-campaign",
                repository_id: "preview-repository",
                authority_sha256: "preview-authority",
                initial_commit: "preview-commit",
                publication: "withheld".to_owned(),
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
        )
    }

    #[test]
    fn reports_empty_and_summary_stay_labelled_at_minimum_window_with_pseudo_text() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |creation| ReportsPreviewApp {
                    app: App::with_state_dir(
                        &creation.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("/example/missing/codingmage"),
                        }),
                        Ok(std::env::temp_dir().join("codingmage-ui-reports-preview")),
                    ),
                    catalogue,
                    right_to_left,
                    report: empty_report(),
                });
            harness.run_steps(2);
            for key in [
                "reports_title",
                "reports_open_campaign",
                "reports_outcome_title",
                "reports_accepted",
                "reports_blocker_title",
            ] {
                let label = harness.state().catalogue.text(key).to_owned();
                assert!(
                    harness
                        .get_by_label_contains(&label)
                        .accesskit_node()
                        .has_bounds()
                );
            }
        }
    }

    #[test]
    fn change_count_labels_do_not_turn_unknown_or_truncated_into_exact_zero() {
        let catalogue = messages::english();
        assert_eq!(observed_count(None, false, catalogue), "not observed");
        assert_eq!(observed_count(Some(0), false, catalogue), "0");
        assert_eq!(
            observed_count(Some(1), true, catalogue),
            "at least 1; more omitted"
        );
    }
}
