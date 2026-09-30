//! Reports: inspect and export outcome and blocker reports assembled from real records.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, SyncSender, TrySendError, channel, sync_channel},
    thread,
    time::{Duration, Instant},
};

use super::{App, Screen, failure_box};
use crate::{
    admission::Admission,
    backend::models::{BlockerExplanation, CampaignOutcome, CampaignReport, CampaignStatus},
    backend::{BackendError, Generation, Job, Request, Response, explain_code},
    command, content,
    launch::LaunchState,
    messages::{self, Catalogue},
    observed::{Freshness, Observed, age_label},
    records::{CommitSummary, FileChange, RunRecord},
    report::{ChangeCoverage, OutcomeReport, ReportInputs},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObservationKey {
    observed_at: Option<Instant>,
    error_at: Option<Instant>,
    loading: bool,
    freshness: Freshness,
}

impl ObservationKey {
    fn of<T>(observation: &Observed<T>, now: Instant) -> Self {
        Self {
            observed_at: observation.observed_at,
            error_at: observation.last_error.as_ref().map(|(at, _)| *at),
            loading: observation.loading,
            freshness: observation.freshness(now),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReportKey {
    generation: Generation,
    source_revision: u64,
    include_paths: bool,
    status: ObservationKey,
    blockers: ObservationKey,
    final_report: ObservationKey,
    changes: ObservationKey,
    runs: ObservationKey,
    admission_digest: Option<String>,
    invocation: Option<CampaignOutcome>,
    runs_truncated: bool,
}

struct ReportSource {
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
    initial_commit: String,
    publication: String,
    admission: Option<Admission>,
    status: Option<CampaignStatus>,
    blockers: Option<BlockerExplanation>,
    final_report: Option<CampaignReport>,
    last_invocation: Option<CampaignOutcome>,
    commits: Vec<CommitSummary>,
    files: Vec<FileChange>,
    change_coverage: ChangeCoverage,
    runs: Vec<RunRecord>,
    run_records_observed: bool,
    run_records_truncated: bool,
    freshness: String,
}

impl ReportSource {
    fn assemble(&self, include_paths: bool) -> OutcomeReport {
        let mut report = OutcomeReport::assemble(
            &ReportInputs {
                campaign_id: &self.campaign_id,
                repository_id: &self.repository_id,
                authority_sha256: &self.authority_sha256,
                initial_commit: &self.initial_commit,
                publication: self.publication.clone(),
                admission: self.admission.as_ref(),
                status: self.status.as_ref(),
                blockers: self.blockers.as_ref(),
                final_report: self.final_report.as_ref(),
                last_invocation: self.last_invocation.as_ref(),
                commits: &self.commits,
                files: &self.files,
                change_coverage: self.change_coverage,
                runs: &self.runs,
                run_records_observed: self.run_records_observed,
                run_records_truncated: self.run_records_truncated,
            },
            include_paths,
        );
        report.limits.push(self.freshness.clone());
        report
    }
}

struct ReportWork {
    key: ReportKey,
    source: ReportSource,
}

struct ReportResult {
    key: ReportKey,
    report: OutcomeReport,
    preview: Result<String, String>,
}

enum ReportQueueError {
    Full,
    Stopped,
}

/// CPU-only report assembly. The queue and thread are separate from coordinator controls.
pub(super) struct ReportWorker {
    sender: SyncSender<ReportWork>,
    receiver: Receiver<ReportResult>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ReportWorker {
    pub(super) fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (sender, requests) = sync_channel::<ReportWork>(1);
        let (responses, receiver) = channel::<ReportResult>();
        let handle = thread::Builder::new()
            .name("codingmage-ui-report-assembly".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let report = request.source.assemble(request.key.include_paths);
                    let preview = report
                        .to_preview_bytes()
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        .map_err(|error| error.to_string());
                    if responses
                        .send(ReportResult {
                            key: request.key,
                            report,
                            preview,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wake();
                }
            })
            .ok();
        Self {
            sender,
            receiver,
            handle,
        }
    }

    fn submit(&self, request: ReportWork) -> Result<(), ReportQueueError> {
        match self.sender.try_send(request) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(ReportQueueError::Full),
            Err(TrySendError::Disconnected(_)) => Err(ReportQueueError::Stopped),
        }
    }

    fn drain(&self) -> Vec<ReportResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.receiver.try_recv() {
            results.push(result);
        }
        results
    }
}

impl Drop for ReportWorker {
    fn drop(&mut self) {
        drop(std::mem::replace(&mut self.sender, sync_channel(1).0));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct AssembledReport {
    key: ReportKey,
    report: OutcomeReport,
    preview: Result<String, String>,
}

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
    assembly_pending: Option<ReportKey>,
    assembled: Option<AssembledReport>,
    assembly_error: Option<String>,
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
        self.reports.assembled = None;
        self.reports.assembly_error = None;
    }

    /// Assembles the outcome report synchronously from the current observations.
    /// Production rendering and export use the bounded report worker instead.
    #[must_use]
    pub fn assemble_report(&self, include_paths: bool) -> Option<OutcomeReport> {
        Some(self.report_source(Instant::now())?.assemble(include_paths))
    }

    fn report_source(&self, now: Instant) -> Option<ReportSource> {
        let campaign = self.campaign.as_ref()?;
        let last_invocation = match &self.execution.observed {
            Some((_, LaunchState::Exited(outcome))) => Some(outcome.clone()),
            _ => None,
        };
        let changes = self.changes.value.as_ref();
        Some(ReportSource {
            campaign_id: campaign.spec.campaign_id.clone(),
            repository_id: campaign.spec.repository_id.clone(),
            authority_sha256: campaign.authority_sha256.clone(),
            initial_commit: campaign.spec.initial_commit.clone(),
            publication: format!("{:?}", campaign.spec.publication),
            admission: self.execution.admission.clone(),
            status: self.status.value.as_ref().and_then(Option::as_ref).cloned(),
            blockers: self
                .explanation
                .value
                .as_ref()
                .and_then(Option::as_ref)
                .cloned(),
            final_report: self.report.value.as_ref().and_then(Option::as_ref).cloned(),
            last_invocation,
            commits: changes.map_or_else(Vec::new, |changes| changes.commits.clone()),
            files: changes.map_or_else(Vec::new, |changes| changes.files.clone()),
            change_coverage: ChangeCoverage {
                observed: changes.is_some(),
                commits_truncated: changes.is_some_and(|changes| changes.commits_truncated),
                files_truncated: changes.is_some_and(|changes| changes.files_truncated),
            },
            runs: self.records.value.clone().unwrap_or_default(),
            run_records_observed: self.records.value.is_some(),
            run_records_truncated: self.records_truncated,
            freshness: self.report_source_freshness(now),
        })
    }

    fn report_key(&self, now: Instant) -> Option<ReportKey> {
        self.campaign.as_ref()?;
        Some(ReportKey {
            generation: self.generation,
            source_revision: self.report_source_revision,
            include_paths: self.reports.include_paths,
            status: ObservationKey::of(&self.status, now),
            blockers: ObservationKey::of(&self.explanation, now),
            final_report: ObservationKey::of(&self.report, now),
            changes: ObservationKey::of(&self.changes, now),
            runs: ObservationKey::of(&self.records, now),
            admission_digest: self
                .execution
                .admission
                .as_ref()
                .map(|admission| admission.preflight_sha256.clone()),
            invocation: match &self.execution.observed {
                Some((_, LaunchState::Exited(outcome))) => Some(outcome.clone()),
                _ => None,
            },
            runs_truncated: self.records_truncated,
        })
    }

    /// Whether the current campaign observations have an assembled report ready.
    #[must_use]
    pub fn report_ready(&self) -> bool {
        self.report_key(Instant::now()).is_some_and(|key| {
            self.reports
                .assembled
                .as_ref()
                .is_some_and(|cached| cached.key == key)
        })
    }

    fn request_report_assembly(&mut self) {
        let now = Instant::now();
        let Some(key) = self.report_key(now) else {
            return;
        };
        if self
            .reports
            .assembled
            .as_ref()
            .is_some_and(|cached| cached.key == key)
            || self.reports.assembly_pending.is_some()
        {
            return;
        }
        let Some(source) = self.report_source(now) else {
            return;
        };
        match self.report_worker.submit(ReportWork {
            key: key.clone(),
            source,
        }) {
            Ok(()) => {
                self.reports.assembly_pending = Some(key);
                self.reports.assembly_error = None;
            }
            Err(ReportQueueError::Full) => {
                self.reports.assembly_error =
                    Some("report assembly is busy; wait for the current snapshot".to_owned());
            }
            Err(ReportQueueError::Stopped) => {
                self.reports.assembly_error = Some(
                    "report assembly worker stopped; reopen the workspace to retry".to_owned(),
                );
            }
        }
    }

    pub(super) fn poll_report_assembly(&mut self) {
        for result in self.report_worker.drain() {
            if self.reports.assembly_pending.as_ref() != Some(&result.key) {
                continue;
            }
            self.reports.assembly_pending = None;
            if self.report_key(Instant::now()).as_ref() == Some(&result.key) {
                self.reports.assembled = Some(AssembledReport {
                    key: result.key,
                    report: result.report,
                    preview: result.preview,
                });
                self.reports.assembly_error = None;
            }
        }
    }

    fn report_source_freshness(&self, now: Instant) -> String {
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
        if self.campaign.is_none() {
            self.reports.message = Some(Err("select a campaign first".to_owned()));
            return;
        }
        let Some(report) = self
            .reports
            .assembled
            .as_ref()
            .filter(|cached| self.report_key(Instant::now()).as_ref() == Some(&cached.key))
            .map(|cached| cached.report.clone())
        else {
            self.reports.message = Some(Err(
                "the report is updating; wait for the current observation before exporting"
                    .to_owned(),
            ));
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
        if self.project.is_none() {
            ui.label(catalogue.text("reports_no_project"));
            self.report_navigation(ui, catalogue, Screen::Setup, "reports_open_setup");
            return;
        }
        if self.campaign.is_none() {
            ui.label(catalogue.text("reports_open_campaign"));
            self.report_navigation(
                ui,
                catalogue,
                Screen::Campaign,
                "reports_open_campaign_action",
            );
            return;
        }
        ui.label(catalogue.text("reports_intro"));
        self.report_source_controls(ui, catalogue);
        let freshness = self.report_source_freshness(self.now);
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
        self.request_report_assembly();
        let current_key = self.report_key(Instant::now());
        let Some(cached) = self
            .reports
            .assembled
            .as_ref()
            .filter(|cached| current_key.as_ref() == Some(&cached.key))
        else {
            ui.label(catalogue.text("reports_assembling"));
            if let Some(error) = &self.reports.assembly_error {
                failure_box(
                    ui,
                    catalogue.text("reports_assembly_failed"),
                    error,
                    catalogue.text("reports_assembly_recovery"),
                );
            }
            return;
        };
        let report = &cached.report;
        if !report.run_records_observed {
            ui.colored_label(
                super::current_tokens(ui.ctx()).error,
                catalogue.text("reports_run_unknown"),
            );
        } else if report.run_records_truncated {
            ui.label(catalogue.text("reports_run_truncated"));
        }
        ui.separator();
        outcome_summary(ui, report, catalogue);
        ui.separator();
        blocker_report(ui, report, catalogue);
        ui.separator();
        self.report_export_section(ui, catalogue);
    }

    fn report_export_section(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
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
                    !self.report_export_pending() && self.report_ready(),
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
            && self.report_ready()
            && let Some(cached) = &self.reports.assembled
        {
            match &cached.preview {
                Ok(preview) => {
                    egui::ScrollArea::vertical()
                        .id_salt("report-json")
                        .max_height(super::current_tokens(ui.ctx()).layout.preview_reports)
                        .show(ui, |ui| {
                            crate::content::render(ui, preview);
                        });
                }
                Err(error) => failure_box(
                    ui,
                    catalogue.text("reports_preview_failed"),
                    error,
                    catalogue.text("reports_preview_recovery"),
                ),
            }
        }
    }

    fn report_navigation(
        &mut self,
        ui: &mut egui::Ui,
        catalogue: &Catalogue,
        target: Screen,
        key: &str,
    ) {
        if ui.button(catalogue.text(key)).clicked() {
            self.screen = target;
        }
        if ui.button(catalogue.text("reports_open_help")).clicked() {
            self.screen = Screen::Help;
        }
    }

    fn report_source_controls(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.strong(catalogue.text("reports_sources_title"));
        self.report_status_source(ui, catalogue);
        self.report_blocker_source(ui, catalogue);
        ui.small(catalogue.text("reports_other_sources"));
        if ui.button(catalogue.text("reports_open_changes")).clicked() {
            self.screen = Screen::Changes;
        }
        if ui
            .button(catalogue.text("reports_open_campaign_action"))
            .clicked()
        {
            self.screen = Screen::Campaign;
        }
    }

    fn report_status_source(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let arguments = self.status_arguments();
        let previewable = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                previewable && !self.status.loading,
                egui::Button::new(catalogue.text("reports_refresh_status")),
            )
            .clicked()
        {
            self.request_status();
        }
        if let Some(arguments) = arguments {
            command::show_for(
                ui,
                catalogue.text("reports_refresh_status"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("reports_refresh_status"));
        }
        if !previewable {
            ui.label(catalogue.text("reports_refresh_unavailable"));
        }
        let freshness = self.status.freshness(self.now);
        ui.label(format!(
            "{} {} ({})",
            catalogue.text("reports_status_observation"),
            freshness.label(),
            age_label(self.status.age(self.now))
        ));
        if self.status.loading {
            ui.label(catalogue.text("reports_status_loading"));
        }
        if freshness == Freshness::Stale {
            ui.label(catalogue.text("reports_status_stale"));
        }
        if let Some((_, error)) = &self.status.last_error {
            let code = error.code();
            let (cause, action) = explain_code(&code);
            failure_box(
                ui,
                catalogue.text("reports_status_failed"),
                &format!("{} ({})", cause, content::list_label(&code)),
                action,
            );
        }
        match (&self.status.value, freshness) {
            (None, Freshness::NotRequested) => {
                ui.label(catalogue.text("reports_status_unobserved"));
            }
            (None, Freshness::Failed) => {
                ui.label(catalogue.text("reports_status_unknown"));
            }
            (Some(None), _) => {
                ui.label(catalogue.text("reports_status_not_started"));
            }
            _ => {}
        }
    }

    fn report_blocker_source(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let arguments = self.explanation_arguments();
        let previewable = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                previewable && !self.explanation.loading,
                egui::Button::new(catalogue.text("reports_refresh_blockers")),
            )
            .clicked()
        {
            self.request_explanation();
        }
        if let Some(arguments) = arguments {
            command::show_for(
                ui,
                catalogue.text("reports_refresh_blockers"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("reports_refresh_blockers"));
        }
        if !previewable {
            ui.label(catalogue.text("reports_refresh_unavailable"));
        }
        let freshness = self.explanation.freshness(self.now);
        ui.label(format!(
            "{} {} ({})",
            catalogue.text("reports_blocker_observation"),
            freshness.label(),
            age_label(self.explanation.age(self.now))
        ));
        if self.explanation.loading {
            ui.label(catalogue.text("reports_blockers_loading"));
        }
        if freshness == Freshness::Stale {
            ui.label(catalogue.text("reports_blockers_stale"));
        }
        if let Some((_, error)) = &self.explanation.last_error {
            let code = error.code();
            let (cause, action) = explain_code(&code);
            failure_box(
                ui,
                catalogue.text("reports_blockers_failed"),
                &format!("{} ({})", cause, content::list_label(&code)),
                action,
            );
        }
        match (&self.explanation.value, freshness) {
            (None, Freshness::NotRequested) => {
                ui.label(catalogue.text("reports_blockers_unobserved"));
            }
            (None, Freshness::Failed) => {
                ui.label(catalogue.text("reports_blockers_unknown"));
            }
            (Some(None), _) => {
                ui.label(catalogue.text("reports_blockers_none"));
            }
            _ => {}
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
                "reports_no_project",
                "reports_open_setup",
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
