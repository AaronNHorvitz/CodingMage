//! Reports: inspect and export outcome and blocker reports assembled from real records.

use serde::Deserialize;
use serde_json::Value;
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

const MAX_SOURCE_INSPECTION_BYTES: usize = 256 * 1024;

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
    campaign_id: String,
    repository_id: String,
    include_paths: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportReceipt {
    schema_version: u16,
    campaign_id: String,
    repository_id: String,
    written: bool,
    bytes: usize,
    repository_paths_requested: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceBoundReport {
    schema_version: u16,
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
    initial_commit: String,
    observed_head: Value,
    #[serde(rename = "status")]
    status: Value,
    #[serde(rename = "blockers")]
    blockers: Value,
    #[serde(rename = "final_report")]
    final_report: Value,
    changes: Value,
    #[serde(rename = "run_records")]
    run_records: Value,
    repository_paths_included: bool,
    #[serde(rename = "limits")]
    limits: Vec<String>,
}

struct PendingInspection {
    request_id: String,
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
    initial_commit: String,
}

struct SourceReportSnapshot {
    preview: String,
    observed_head: Option<String>,
    shortened: bool,
}

fn parse_source_report(
    bytes: Vec<u8>,
    pending: &PendingInspection,
) -> Result<SourceReportSnapshot, String> {
    if bytes.len() > MAX_SOURCE_INSPECTION_BYTES {
        return Err(
            "the source report is too large for inline inspection; export it outside the repository to inspect the full document"
                .to_owned(),
        );
    }
    let document: SourceBoundReport = serde_json::from_slice(&bytes)
        .map_err(|_| "the coordinator returned a malformed source report".to_owned())?;
    if document.schema_version != 1
        || document.campaign_id != pending.campaign_id
        || document.repository_id != pending.repository_id
        || document.authority_sha256 != pending.authority_sha256
        || document.initial_commit != pending.initial_commit
        || !(document.observed_head.is_null() || document.observed_head.is_string())
        || !(document.status.is_null() || document.status.is_object())
        || !(document.blockers.is_null() || document.blockers.is_object())
        || !(document.final_report.is_null() || document.final_report.is_object())
        || !(document.run_records.is_null() || document.run_records.is_object())
        || !document.changes.is_object()
        || document.changes.get("head") != Some(&document.observed_head)
        || document.repository_paths_included
        || document.changes["repository_paths_included"] != false
        || document.changes.get("changed_files") != Some(&Value::Null)
        || document.limits.is_empty()
        || (!document.status.is_null()
            && (document.status["campaign_id"].as_str() != Some(pending.campaign_id.as_str())
                || document.status["head"] != document.observed_head))
        || (!document.blockers.is_null()
            && document.blockers["campaign_id"].as_str() != Some(pending.campaign_id.as_str()))
        || (!document.final_report.is_null()
            && (document.final_report["campaign_id"].as_str()
                != Some(pending.campaign_id.as_str())
                || document.final_report["repository_id"].as_str()
                    != Some(pending.repository_id.as_str())
                || document.final_report["initial_commit"].as_str()
                    != Some(pending.initial_commit.as_str())))
        || (!document.run_records.is_null()
            && (document.run_records["campaign_id"].as_str() != Some(pending.campaign_id.as_str())
                || document.run_records["repository_id"].as_str()
                    != Some(pending.repository_id.as_str())
                || document.run_records["head"] != document.observed_head))
    {
        return Err(
            "the source report identity or path privacy did not match the selected campaign"
                .to_owned(),
        );
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| "the coordinator returned a non-UTF-8 source report".to_owned())?;
    let mut characters = text.chars();
    let preview: String = characters
        .by_ref()
        .take(content::MAX_PREVIEW_CHARS)
        .collect();
    Ok(SourceReportSnapshot {
        preview,
        observed_head: document.observed_head.as_str().map(str::to_owned),
        shortened: characters.next().is_some(),
    })
}

impl ExportReceipt {
    fn matches(&self, pending: &PendingExport) -> bool {
        self.schema_version == 1
            && self.campaign_id == pending.campaign_id
            && self.repository_id == pending.repository_id
            && self.written
            && self.bytes > 0
            && self.repository_paths_requested == pending.include_paths
    }
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
    /// Show a bounded local observation preview, separate from source-bound export.
    pub show_json: bool,
    pending: Option<PendingExport>,
    inspection_pending: Option<PendingInspection>,
    source_snapshot: Option<SourceReportSnapshot>,
    source_error: Option<String>,
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

    /// Whether a read-only source report request is pending.
    #[must_use]
    pub fn source_report_pending(&self) -> bool {
        self.reports.inspection_pending.is_some()
    }

    pub(super) fn clear_report_export(&mut self) {
        self.reports.pending = None;
        self.reports.message = None;
        self.reports.inspection_pending = None;
        self.reports.source_snapshot = None;
        self.reports.source_error = None;
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
        let Some(campaign) = self.campaign.as_ref() else {
            return;
        };
        let (campaign_id, repository_id) = (
            campaign.spec.campaign_id.clone(),
            campaign.spec.repository_id.clone(),
        );
        let Some(arguments) = self
            .report_export_arguments()
            .filter(|arguments| command::can_preview(self.binary_path.as_deref(), Some(arguments)))
        else {
            self.reports.message = Some(Err(
                "the exact report export command cannot be shown; check the coordinator and destination path"
                    .to_owned(),
            ));
            return;
        };
        let destination = PathBuf::from(self.reports.export_path.trim());
        self.next_evidence_request += 1;
        let request_id = format!("report-export-{}", self.next_evidence_request);
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SourceReportExport {
                arguments,
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
                    campaign_id,
                    repository_id,
                    include_paths: self.reports.include_paths,
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
            Ok(bytes) => match serde_json::from_slice::<ExportReceipt>(&bytes) {
                Ok(receipt) if receipt.matches(&pending) => Ok(format!(
                    "source-bound report exported to {} (repository paths {}requested)",
                    pending.destination.display(),
                    if pending.include_paths { "" } else { "not " }
                )),
                _ => Err("the coordinator returned an invalid export receipt; the destination is uncertain, so inspect it before retrying".to_owned()),
            },
            Err(BackendError::Refused(reason)) => Err(reason),
            Err(BackendError::Timeout) => Err(
                "report export timed out. The destination is uncertain; inspect it and any .codingmage-report-*.candidate file in that directory before retrying."
                    .to_owned(),
            ),
            Err(BackendError::Spawn) => Err(
                "the report writer could not start or returned an unreadable result; inspect the destination and retry after checking the desktop installation"
                    .to_owned(),
            ),
            Err(error) => Err(format!("{error}; inspect the destination before retrying")),
        });
        true
    }

    fn report_export_arguments(&self) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        let destination = self.reports.export_path.trim();
        if destination.is_empty() || !PathBuf::from(destination).is_absolute() {
            return None;
        }
        Some(vec![
            "report-export".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            campaign.spec_path.to_str()?.to_owned(),
            "--output".to_owned(),
            destination.to_owned(),
            "--include-paths".to_owned(),
            self.reports.include_paths.to_string(),
            "--overwrite".to_owned(),
            self.reports.overwrite.to_string(),
        ])
    }

    fn source_report_arguments(&self) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        Some(vec![
            "campaign-outcome-report".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            campaign.spec_path.to_str()?.to_owned(),
            "--include-paths".to_owned(),
            "false".to_owned(),
        ])
    }

    /// Requests a fresh, path-free report from the public coordinator command.
    pub fn inspect_source_report(&mut self) {
        if self.source_report_pending() {
            self.reports.source_error =
                Some("a source report inspection is already pending".to_owned());
            return;
        }
        let Some(campaign) = self.campaign.as_ref() else {
            self.reports.source_error = Some("select a campaign first".to_owned());
            return;
        };
        let pending = PendingInspection {
            request_id: format!(
                "source-report-{}",
                self.next_evidence_request.wrapping_add(1)
            ),
            campaign_id: campaign.spec.campaign_id.clone(),
            repository_id: campaign.spec.repository_id.clone(),
            authority_sha256: campaign.authority_sha256.clone(),
            initial_commit: campaign.spec.initial_commit.clone(),
        };
        let Some(arguments) = self
            .source_report_arguments()
            .filter(|arguments| command::can_preview(self.binary_path.as_deref(), Some(arguments)))
        else {
            self.reports.source_error = Some("the exact source report command cannot be shown; check the coordinator and selected campaign".to_owned());
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SourceReportInspect {
                arguments,
                deadline: Duration::from_secs(30),
            },
            request_id: Some(pending.request_id.clone()),
        };
        self.reports.source_error = None;
        match self.submit(request) {
            Ok(()) => {
                self.next_evidence_request = self.next_evidence_request.wrapping_add(1);
                self.reports.inspection_pending = Some(pending);
            }
            Err(error) => self.reports.source_error = Some(error.to_string()),
        }
    }

    pub(super) fn accept_source_report(&mut self, response: Response) -> bool {
        let Some(pending) = self.reports.inspection_pending.as_ref() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            return false;
        }
        let Some(pending) = self.reports.inspection_pending.take() else {
            return false;
        };
        match response.result {
            Ok(bytes) => match parse_source_report(bytes, &pending) {
                Ok(snapshot) => {
                    self.reports.source_snapshot = Some(snapshot);
                    self.reports.source_error = None;
                }
                Err(error) => {
                    self.reports.source_snapshot = None;
                    self.reports.source_error = Some(error);
                }
            },
            Err(error) => {
                self.reports.source_snapshot = None;
                self.reports.source_error = Some(error.to_string());
            }
        }
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
        ui.separator();
        self.report_source_inspection_section(ui, catalogue);
        ui.separator();
        self.report_export_section(ui, catalogue);
        ui.separator();
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
        self.local_report_preview(ui, catalogue);
    }

    fn report_source_inspection_section(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.strong(catalogue.text("reports_source_inspect_title"));
        ui.small(catalogue.text("reports_source_inspect_description"));
        let arguments = self.source_report_arguments();
        let previewable = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                previewable && !self.source_report_pending(),
                egui::Button::new(catalogue.text("reports_source_inspect")),
            )
            .clicked()
        {
            self.inspect_source_report();
        }
        if let Some(arguments) = arguments {
            command::show_for(
                ui,
                catalogue.text("reports_source_inspect"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("reports_source_inspect"));
        }
        if self.source_report_pending() {
            ui.label(catalogue.text("reports_source_inspect_pending"));
        }
        if let Some(error) = &self.reports.source_error {
            failure_box(
                ui,
                catalogue.text("reports_source_inspect_failed"),
                error,
                catalogue.text("reports_source_inspect_recovery"),
            );
        }
        if let Some(snapshot) = &self.reports.source_snapshot {
            ui.label(catalogue.text("reports_source_inspect_snapshot"));
            ui.label(snapshot.observed_head.as_deref().map_or_else(
                || catalogue.text("reports_source_inspect_no_head").to_owned(),
                |head| {
                    format!(
                        "{} {}",
                        catalogue.text("reports_source_inspect_head"),
                        content::list_label(head)
                    )
                },
            ));
            egui::ScrollArea::vertical()
                .id_salt("source-report-json")
                .max_height(super::current_tokens(ui.ctx()).layout.preview_reports)
                .show(ui, |ui| {
                    content::render(ui, &snapshot.preview);
                    if snapshot.shortened {
                        ui.label(catalogue.text("reports_source_inspect_shortened"));
                    }
                });
        } else if !self.source_report_pending() && self.reports.source_error.is_none() {
            ui.label(catalogue.text("reports_source_inspect_unobserved"));
        }
    }

    fn report_export_section(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.strong(catalogue.text("reports_export_title"));
        ui.small(catalogue.text("reports_destination_guidance"));
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
                    !self.report_export_pending()
                        && command::can_preview(
                            self.binary_path.as_deref(),
                            self.report_export_arguments().as_deref(),
                        ),
                    egui::Button::new(catalogue.text("reports_export")),
                )
                .clicked()
            {
                self.export_report();
            }
        });
        if let Some(arguments) = self
            .report_export_arguments()
            .filter(|arguments| command::can_preview(self.binary_path.as_deref(), Some(arguments)))
        {
            command::show_for(
                ui,
                catalogue.text("reports_export"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("reports_export"));
        }
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
    }

    fn local_report_preview(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.strong(catalogue.text("reports_local_observation_title"));
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

    fn source_fixture() -> (PendingInspection, Value) {
        let pending = PendingInspection {
            request_id: "source-report-1".to_owned(),
            campaign_id: "campaign-1".to_owned(),
            repository_id: "repository-1".to_owned(),
            authority_sha256: "authority-1".to_owned(),
            initial_commit: "initial-1".to_owned(),
        };
        let valid = serde_json::json!({
            "schema_version": 1,
            "campaign_id": "campaign-1",
            "repository_id": "repository-1",
            "authority_sha256": "authority-1",
            "initial_commit": "initial-1",
            "observed_head": null,
            "status": null,
            "blockers": null,
            "final_report": null,
            "changes": {
                "head": null,
                "repository_paths_included": false,
                "changed_files": null
            },
            "run_records": null,
            "repository_paths_included": false,
            "limits": ["source observation"]
        });
        (pending, valid)
    }

    #[test]
    fn source_inspection_requires_exact_identity_and_path_free_shape() {
        let (pending, valid) = source_fixture();
        let encode = |document: &Value| serde_json::to_vec(document).unwrap();
        let snapshot = parse_source_report(encode(&valid), &pending).unwrap();
        assert!(snapshot.preview.contains("campaign-1"));
        assert_eq!(snapshot.observed_head, None);
        assert!(!snapshot.shortened);
        for (pointer, replacement) in [
            ("/schema_version", serde_json::json!(2)),
            ("/campaign_id", serde_json::json!("other")),
            ("/repository_id", serde_json::json!("other")),
            ("/authority_sha256", serde_json::json!("other")),
            ("/initial_commit", serde_json::json!("other")),
            ("/repository_paths_included", serde_json::json!(true)),
            (
                "/changes/repository_paths_included",
                serde_json::json!(true),
            ),
            ("/changes/changed_files", serde_json::json!(["src/lib.rs"])),
            ("/changes/head", serde_json::json!("other")),
        ] {
            let mut document = valid.clone();
            *document.pointer_mut(pointer).unwrap() = replacement;
            assert!(parse_source_report(encode(&document), &pending).is_err());
        }
        let mut unknown = valid.clone();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(parse_source_report(encode(&unknown), &pending).is_err());
        let mut missing_files = valid.clone();
        missing_files["changes"]
            .as_object_mut()
            .unwrap()
            .remove("changed_files");
        assert!(parse_source_report(encode(&missing_files), &pending).is_err());
        assert!(parse_source_report(b"{".to_vec(), &pending).is_err());
        assert!(parse_source_report(vec![0xff], &pending).is_err());
        assert!(
            parse_source_report(vec![b'x'; MAX_SOURCE_INSPECTION_BYTES + 1], &pending)
                .err()
                .unwrap()
                .contains("too large")
        );
        let mut long = valid;
        long["limits"] = serde_json::json!(["x".repeat(content::MAX_PREVIEW_CHARS)]);
        let snapshot = parse_source_report(encode(&long), &pending).unwrap();
        assert!(snapshot.shortened);
        assert_eq!(snapshot.preview.chars().count(), content::MAX_PREVIEW_CHARS);
    }

    #[test]
    fn source_inspection_rejects_foreign_nested_evidence() {
        let (pending, valid) = source_fixture();
        let encode = |document: &Value| serde_json::to_vec(document).unwrap();
        let mut active = valid.clone();
        active["observed_head"] = serde_json::json!("head-1");
        active["status"] = serde_json::json!({
            "campaign_id": "campaign-1",
            "head": "head-1"
        });
        active["blockers"] = serde_json::json!({"campaign_id": "campaign-1"});
        active["final_report"] = serde_json::json!({
            "campaign_id": "campaign-1",
            "repository_id": "repository-1",
            "initial_commit": "initial-1"
        });
        active["changes"]["head"] = serde_json::json!("head-1");
        active["run_records"] = serde_json::json!({
            "campaign_id": "campaign-1",
            "repository_id": "repository-1",
            "head": "head-1"
        });
        assert_eq!(
            parse_source_report(encode(&active), &pending)
                .unwrap()
                .observed_head
                .as_deref(),
            Some("head-1")
        );
        for pointer in [
            "/status/campaign_id",
            "/blockers/campaign_id",
            "/final_report/campaign_id",
            "/final_report/repository_id",
            "/final_report/initial_commit",
            "/run_records/campaign_id",
            "/run_records/repository_id",
            "/run_records/head",
        ] {
            let mut document = active.clone();
            *document.pointer_mut(pointer).unwrap() = serde_json::json!("other");
            assert!(parse_source_report(encode(&document), &pending).is_err());
        }
    }

    #[test]
    fn source_export_receipt_requires_exact_bound_identity_and_privacy() {
        let pending = PendingExport {
            request_id: "report-export-1".to_owned(),
            destination: PathBuf::from("/example/report.json"),
            campaign_id: "campaign-1".to_owned(),
            repository_id: "repository-1".to_owned(),
            include_paths: false,
        };
        let valid = br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":false}"#;
        assert!(
            serde_json::from_slice::<ExportReceipt>(valid)
                .unwrap()
                .matches(&pending)
        );
        for invalid in [
            br#"{"schema_version":2,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":false}"#.as_slice(),
            br#"{"schema_version":1,"campaign_id":"other","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":false}"#,
            br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"other","written":true,"bytes":12,"repository_paths_requested":false}"#,
            br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":false,"bytes":12,"repository_paths_requested":false}"#,
            br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":0,"repository_paths_requested":false}"#,
            br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":true}"#,
        ] {
            assert!(!serde_json::from_slice::<ExportReceipt>(invalid).unwrap().matches(&pending));
        }
        for malformed in [
            b"{".as_slice(),
            br#"{"schema_version":1,"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":false}"#,
            br#"{"schema_version":1,"campaign_id":"campaign-1","repository_id":"repository-1","written":true,"bytes":12,"repository_paths_requested":false,"extra":true}"#,
        ] {
            assert!(serde_json::from_slice::<ExportReceipt>(malformed).is_err());
        }
    }
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
