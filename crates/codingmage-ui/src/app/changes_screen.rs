//! Changes and reviews: exact candidate changes, review and test records, bounded activity.

use super::{App, GIT_DEADLINE, Screen, backend_failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response},
    command, content,
    messages::{self, Catalogue},
    observed::{Freshness, age_label},
    records::{CommitSummary, FileChange, RunRecord, parse_run_records},
};
use serde::Deserialize;

/// Exact changes between the campaign's initial commit and its head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    /// Projection contract version.
    pub schema_version: u16,
    /// Campaign identity.
    pub campaign_id: String,
    /// Repository identity.
    pub repository_id: String,
    /// Base commit.
    pub base: String,
    /// Head commit.
    pub head: String,
    /// Coordinator commits, newest first.
    pub commits: Vec<CommitSummary>,
    /// Changed files.
    pub files: Vec<FileChange>,
    /// More commits exist than are shown.
    pub commits_truncated: bool,
    /// More changed paths exist than are shown.
    pub files_truncated: bool,
}

impl App {
    fn next_evidence_ticket(&mut self) -> Option<super::EvidenceTicket> {
        self.next_evidence_request = self.next_evidence_request.checked_add(1)?;
        Some(super::EvidenceTicket {
            request_id: format!(
                "evidence-{}-{}",
                self.generation.0, self.next_evidence_request
            ),
            status_epoch: self.status_epoch,
        })
    }

    pub(super) fn invalidate_evidence_requests(&mut self) {
        self.status_epoch = self.status_epoch.wrapping_add(1);
        if self.changes_request.is_some() {
            self.changes_range = None;
        }
        self.changes_request = None;
        self.records_request = None;
        self.changes.loading = false;
        self.records.loading = false;
    }

    fn status_live_at(&self, now: std::time::Instant) -> bool {
        !self.status.loading && self.status.freshness(now) == Freshness::Live
    }

    /// Opaque identity of the pending change read, for bounded response correlation.
    #[must_use]
    pub fn pending_changes_request_id(&self) -> Option<&str> {
        self.changes_request
            .as_ref()
            .map(|ticket| ticket.request_id.as_str())
    }

    /// Opaque identity of the pending run-record read, for bounded response correlation.
    #[must_use]
    pub fn pending_run_records_request_id(&self) -> Option<&str> {
        self.records_request
            .as_ref()
            .map(|ticket| ticket.request_id.as_str())
    }

    fn change_arguments(&self, head: &str) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        Some(vec![
            "campaign-changes".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            campaign.spec_path.to_str()?.to_owned(),
            "--head".to_owned(),
            head.to_owned(),
        ])
    }

    fn run_record_arguments(&self) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        Some(vec![
            "campaign-run-records".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            campaign.spec_path.to_str()?.to_owned(),
        ])
    }

    /// Requests a new exact-head change projection only from a live campaign status.
    pub fn refresh_change_evidence(&mut self) {
        if !self.status_live_at(std::time::Instant::now()) || self.changes.loading {
            return;
        }
        let Some(status) = self.status.value.as_ref().and_then(Option::as_ref) else {
            return;
        };
        let head = status.head.clone();
        let Some(base) = self
            .campaign
            .as_ref()
            .map(|campaign| campaign.spec.initial_commit.clone())
        else {
            return;
        };
        if !command::can_preview(
            self.binary_path.as_deref(),
            self.change_arguments(&head).as_deref(),
        ) {
            return;
        }
        self.request_changes(&base, &head);
    }

    /// Requests current run evidence only from a live campaign status.
    pub fn refresh_run_evidence(&mut self) {
        if !self.status_live_at(std::time::Instant::now()) || self.records.loading {
            return;
        }
        if !command::can_preview(
            self.binary_path.as_deref(),
            self.run_record_arguments().as_deref(),
        ) {
            return;
        }
        self.request_records();
    }

    /// Latest change-set observation.
    #[must_use]
    pub const fn changes(&self) -> &crate::observed::Observed<ChangeSet> {
        &self.changes
    }

    /// Latest run-record observation.
    #[must_use]
    pub const fn run_records(&self) -> &crate::observed::Observed<Vec<RunRecord>> {
        &self.records
    }

    pub(super) fn request_changes(&mut self, base: &str, head: &str) {
        let Some(arguments) = self.change_arguments(head) else {
            self.changes.fail(
                BackendError::Refused(
                    "campaign paths cannot be represented in the command boundary".to_owned(),
                ),
                self.now,
            );
            return;
        };
        let Some(ticket) = self.next_evidence_ticket() else {
            self.changes.fail(
                BackendError::Refused("evidence request identity exhausted".to_owned()),
                self.now,
            );
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-changes",
                arguments,
                deadline: GIT_DEADLINE,
            },
            request_id: Some(ticket.request_id.clone()),
        };
        self.changes.clear();
        match self.submit(request) {
            Ok(()) => {
                self.changes.loading = true;
                self.changes_range = Some((base.to_owned(), head.to_owned()));
                self.changes_request = Some(ticket);
            }
            Err(error) => {
                self.changes_range = None;
                self.changes.fail(error, self.now);
            }
        }
    }

    pub(super) fn request_records(&mut self) {
        let Some(arguments) = self.run_record_arguments() else {
            self.records.clear();
            self.records_status = None;
            self.records_truncated = false;
            self.records.fail(
                BackendError::Refused(
                    "campaign paths cannot be represented in the command boundary".to_owned(),
                ),
                self.now,
            );
            return;
        };
        let Some(ticket) = self.next_evidence_ticket() else {
            self.records.fail(
                BackendError::Refused("evidence request identity exhausted".to_owned()),
                self.now,
            );
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-run-records",
                arguments,
                deadline: GIT_DEADLINE,
            },
            request_id: Some(ticket.request_id.clone()),
        };
        self.records.clear();
        self.records_status = None;
        self.records_truncated = false;
        match self.submit(request) {
            Ok(()) => {
                self.records.loading = true;
                self.records_request = Some(ticket);
            }
            Err(error) => self.records.fail(error, self.now),
        }
    }

    pub(super) fn accept_changes(&mut self, response: Response) -> bool {
        let Some(ticket) = self.changes_request.as_ref() else {
            self.discarded_stale += 1;
            return false;
        };
        if response.request_id.as_deref() != Some(ticket.request_id.as_str())
            || ticket.status_epoch != self.status_epoch
        {
            self.discarded_stale += 1;
            return false;
        }
        if !self.status_live_at(self.now) {
            self.changes_request = None;
            self.changes_range = None;
            self.changes.clear();
            self.discarded_stale += 1;
            return false;
        }
        self.changes_request = None;
        let parsed = response.result.and_then(|bytes| {
            serde_json::from_slice::<ChangeSet>(&bytes)
                .map_err(|_| BackendError::Contract(crate::backend::models::ModelError::Malformed))
        });
        match parsed {
            Ok(changes) if self.changes_binding_matches(&changes) => {
                self.changes.accept(changes, response.generation, self.now);
            }
            Ok(_) => {
                self.changes_range = None;
                self.changes.clear();
                self.changes.fail(
                    BackendError::Contract(crate::backend::models::ModelError::Malformed),
                    self.now,
                );
            }
            Err(error) => {
                self.changes_range = None;
                self.changes.clear();
                self.changes.fail(error, self.now);
            }
        }
        true
    }

    fn changes_binding_matches(&self, changes: &ChangeSet) -> bool {
        let Some(status) = self.status.value.as_ref().and_then(Option::as_ref) else {
            return false;
        };
        let (Some(campaign), Some(diagnosis)) = (&self.campaign, &self.diagnosis.value) else {
            return false;
        };
        changes.schema_version == 1
            && changes.campaign_id == campaign.spec.campaign_id
            && changes.repository_id == campaign.spec.repository_id
            && changes.repository_id == diagnosis.repository_id
            && changes.base == campaign.spec.initial_commit
            && changes.head == status.head
            && matches!(changes.base.len(), 40 | 64)
            && matches!(changes.head.len(), 40 | 64)
            && changes.base.bytes().all(|byte| byte.is_ascii_hexdigit())
            && changes.head.bytes().all(|byte| byte.is_ascii_hexdigit())
            && changes.commits.len() <= 500
            && changes.files.len() <= 500
            && (changes.base != changes.head
                || (changes.commits.is_empty() && changes.files.is_empty()))
            && (changes.base == changes.head
                || changes
                    .commits
                    .first()
                    .is_some_and(|commit| commit.id == changes.head))
            && changes.commits.iter().all(|commit| {
                commit.id.len() == changes.head.len()
                    && commit.id.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && commit.subject.len() <= 4096
                    && !commit.subject.chars().any(char::is_control)
            })
            && changes.files.iter().all(|file| {
                !file.path.is_empty()
                    && file.path.len() <= 4096
                    && !file.path.chars().any(char::is_control)
                    && file.added.is_some() == file.deleted.is_some()
            })
    }

    pub(super) fn accept_records(&mut self, response: Response) -> bool {
        let Some(ticket) = self.records_request.as_ref() else {
            self.discarded_stale += 1;
            return false;
        };
        if response.request_id.as_deref() != Some(ticket.request_id.as_str())
            || ticket.status_epoch != self.status_epoch
        {
            self.discarded_stale += 1;
            return false;
        }
        if !self.status_live_at(self.now) {
            self.records_request = None;
            self.records.clear();
            self.records_status = None;
            self.records_truncated = false;
            self.discarded_stale += 1;
            return false;
        }
        self.records_request = None;
        let parsed = response
            .result
            .and_then(|bytes| parse_run_records(&bytes).map_err(BackendError::from));
        match parsed {
            Ok(projection) if self.records_binding_matches(&projection) => {
                self.records_status = projection.head.clone().zip(projection.updated_at_ms);
                self.records_truncated = projection.records_truncated;
                self.records
                    .accept(projection.records, response.generation, self.now);
            }
            Ok(_) => {
                self.records.clear();
                self.records_status = None;
                self.records_truncated = false;
                self.records.fail(
                    BackendError::Contract(crate::backend::models::ModelError::Malformed),
                    self.now,
                );
            }
            Err(error) => {
                self.records.clear();
                self.records_status = None;
                self.records_truncated = false;
                self.records.fail(error, self.now);
            }
        }
        true
    }

    fn records_binding_matches(&self, projection: &crate::records::RunRecordsProjection) -> bool {
        let (Some(campaign), Some(diagnosis), Some(status)) =
            (&self.campaign, &self.diagnosis.value, &self.status.value)
        else {
            return false;
        };
        projection.campaign_id == campaign.spec.campaign_id
            && projection.repository_id == campaign.spec.repository_id
            && projection.repository_id == diagnosis.repository_id
            && projection.head.as_deref() == status.as_ref().map(|status| status.head.as_str())
            && projection.updated_at_ms == status.as_ref().map(|status| status.updated_at_ms)
    }

    /// Shows only run evidence bound to the currently observed status and selected task.
    pub(super) fn task_run_evidence(&self, ui: &mut egui::Ui, task_id: &str) {
        let catalogue = messages::english();
        ui.collapsing(catalogue.text("work_plan_runs_title"), |ui| {
            if let Some((config, campaign, _)) = self.campaign_arguments() {
                let arguments = vec![
                    "campaign-run-records".to_owned(),
                    "--config".to_owned(),
                    config,
                    "--campaign".to_owned(),
                    campaign,
                ];
                command::show_for(
                    ui,
                    catalogue.text("work_plan_runs_command"),
                    self.binary_path.as_deref(),
                    &arguments,
                );
            }
            let current = self.status.value.as_ref().and_then(Option::as_ref);
            let bound = current.is_some_and(|status| {
                self.records_status.as_ref() == Some(&(status.head.clone(), status.updated_at_ms))
            });
            if !self.status_live_at(self.now)
                || self.records.freshness(self.now) != Freshness::Live
                || !bound
            {
                ui.label(catalogue.text("work_plan_runs_unavailable"));
                return;
            }
            let Some(records) = self.records.value.as_ref() else {
                return;
            };
            render_task_run_records(ui, catalogue, task_id, records, self.records_truncated);
        });
    }

    pub(super) fn changes_screen(&mut self, ui: &mut egui::Ui) {
        let catalogue = messages::english();
        ui.heading("Changes and reviews");
        if self.project.is_none() {
            ui.label(catalogue.text("changes_no_project"));
            if ui.button(catalogue.text("changes_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            return;
        }
        let Some(campaign) = &self.campaign else {
            ui.label(catalogue.text("changes_no_campaign"));
            if ui.button(catalogue.text("changes_open_campaign")).clicked() {
                self.screen = Screen::Campaign;
            }
            return;
        };
        ui.strong("Delivery boundary");
        ui.label(format!(
            "Publication {:?}: accepted work lives on the coordinator-owned campaign branch only. Nothing here was pushed, merged into a protected branch or promoted to a destination; engineering completion below is not delivery.",
            campaign.spec.publication
        ));
        ui.separator();
        self.evidence_refresh_controls(ui);
        ui.separator();
        self.changes_block(ui);
        ui.separator();
        self.records_block(ui);
        ui.separator();
        self.activity_block(ui);
    }

    fn evidence_refresh_controls(&mut self, ui: &mut egui::Ui) {
        let catalogue = messages::english();
        let status_live = self.status_live_at(self.now);
        let change_arguments = self
            .status
            .value
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(|status| self.change_arguments(&status.head));
        let record_arguments = self.run_record_arguments();
        let change_enabled = status_live
            && !self.changes.loading
            && command::can_preview(self.binary_path.as_deref(), change_arguments.as_deref());
        let records_enabled = status_live
            && !self.records.loading
            && command::can_preview(self.binary_path.as_deref(), record_arguments.as_deref());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    change_enabled,
                    egui::Button::new(catalogue.text("changes_refresh_changes")),
                )
                .clicked()
            {
                self.refresh_change_evidence();
            }
            if ui
                .add_enabled(
                    records_enabled,
                    egui::Button::new(catalogue.text("changes_refresh_runs")),
                )
                .clicked()
            {
                self.refresh_run_evidence();
            }
        });
        if let Some(arguments) = change_arguments {
            command::show_for(
                ui,
                catalogue.text("changes_refresh_changes"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("changes_refresh_changes"));
        }
        if let Some(arguments) = record_arguments {
            command::show_for(
                ui,
                catalogue.text("changes_refresh_runs"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("changes_refresh_runs"));
        }
        if !status_live {
            ui.label(catalogue.text("changes_status_needed"));
        } else if !change_enabled || !records_enabled {
            ui.label(catalogue.text("changes_command_unavailable"));
        }
        if ui.button(catalogue.text("changes_open_campaign")).clicked() {
            self.screen = Screen::Campaign;
        }
    }

    fn changes_block(&self, ui: &mut egui::Ui) {
        ui.strong("Exact candidate changes (campaign branch)");
        if !self.status_live_at(self.now) {
            ui.label("Current campaign status is unavailable or refreshing; candidate changes are withheld until it is observed again.");
            return;
        }
        let freshness = self.changes.freshness(self.now);
        match (&self.status.value, freshness) {
            (Some(None), _) => {
                ui.label("The campaign has never started; there are no candidate changes.");
                return;
            }
            (None, _) => {
                ui.label("Campaign status has not been observed yet.");
                return;
            }
            _ => {}
        }
        ui.label(format!(
            "Observation: {} ({})",
            freshness.label(),
            age_label(self.changes.age(self.now))
        ));
        if let Some((_, error)) = &self.changes.last_error {
            backend_failure_box(ui, error, "failure_changes_no_observation");
        }
        let Some(changes) = &self.changes.value else {
            if freshness == Freshness::Loading {
                ui.label("Reading the campaign branch objects...");
            } else if freshness == Freshness::NotRequested {
                ui.label(messages::english().text("changes_unobserved"));
            }
            return;
        };
        if freshness == Freshness::Stale {
            ui.label(messages::english().text("changes_stale"));
        }
        ui.monospace(format!(
            "{}..{}",
            &changes.base[..12.min(changes.base.len())],
            &changes.head[..12.min(changes.head.len())]
        ));
        if changes.commits.is_empty() {
            ui.label("The campaign head equals the initial commit: no reviewed candidate has been integrated.");
        }
        if changes.commits_truncated || changes.files_truncated {
            ui.label("This bounded change summary omits additional commits or paths; inspect the exact range with the coordinator for the complete set.");
        }
        for commit in &changes.commits {
            content::render(
                ui,
                &format!(
                    "{} {} (unix {})",
                    &commit.id[..12],
                    commit.subject,
                    commit.timestamp
                ),
            );
        }
        if !changes.files.is_empty() {
            ui.label(format!("{} changed file(s)", changes.files.len()));
            egui::Grid::new("changed-files")
                .num_columns(3)
                .spacing(super::current_tokens(ui.ctx()).layout.grid_dense)
                .show(ui, |ui| {
                    for file in &changes.files {
                        content::render(ui, &file.path);
                        ui.label(match file.added {
                            Some(added) => format!("+{added}"),
                            None => "binary".to_owned(),
                        });
                        ui.label(match file.deleted {
                            Some(deleted) => format!("-{deleted}"),
                            None => String::new(),
                        });
                        ui.end_row();
                    }
                });
            ui.small("File names come from the owner's own repository objects; exports that include them are marked as containing repository paths.");
        }
    }

    fn records_block(&self, ui: &mut egui::Ui) {
        ui.strong("Independent review and test records (per run, from durable checkpoints)");
        if !self.status_live_at(self.now) {
            ui.label("Current campaign status is unavailable or refreshing; review and test records are withheld until it is observed again.");
            return;
        }
        let freshness = self.records.freshness(self.now);
        ui.label(format!(
            "Observation: {} ({})",
            freshness.label(),
            age_label(self.records.age(self.now))
        ));
        if let Some((_, error)) = &self.records.last_error {
            backend_failure_box(ui, error, "failure_records_no_observation");
        }
        let Some(records) = &self.records.value else {
            if freshness == Freshness::Loading {
                ui.label(messages::english().text("records_loading"));
            } else if freshness == Freshness::NotRequested {
                ui.label(messages::english().text("records_unobserved"));
            }
            return;
        };
        if freshness == Freshness::Stale {
            ui.label(messages::english().text("records_stale"));
        }
        if records.is_empty() {
            ui.label("No run records exist for this campaign. Nothing has been implemented, reviewed or tested.");
            return;
        }
        if self.records_truncated {
            ui.label("Additional campaign run records are omitted from this bounded view.");
        }
        ui.small("Reviewer finding text is not retained by the backend; only the verdict, correction rounds and evidence identities are durable.");
        for record in records {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.monospace(format!(
                    "{} - task {}",
                    record.run_id,
                    record.task_id().unwrap_or_else(|| "unknown".to_owned())
                ));
                match &record.checkpoint {
                    Some(checkpoint) => {
                        ui.label(format!(
                            "candidate commit {} after {} correction round(s)",
                            &checkpoint.candidate_commit
                                [..12.min(checkpoint.candidate_commit.len())],
                            checkpoint.correction_rounds
                        ));
                    }
                    None => {
                        ui.colored_label(
                            super::current_tokens(ui.ctx()).error,
                            record
                                .checkpoint_problem
                                .as_deref()
                                .unwrap_or("checkpoint unavailable"),
                        );
                    }
                }
                content::render(ui, &record.review_label());
                content::render(ui, &record.gate_label());
                if let Some(problem) = &record.journal_problem {
                    ui.colored_label(super::current_tokens(ui.ctx()).error, problem);
                }
                if record.phases_truncated {
                    ui.label("Additional journal phases are omitted from this bounded view.");
                }
                let observed = record
                    .phases
                    .iter()
                    .filter(|phase| phase.kind == "effect_observed")
                    .map(|phase| format!("{} ({})", phase.phase, phase.outcome))
                    .collect::<Vec<_>>();
                if observed.is_empty() {
                    ui.label("No journaled phase observations.");
                } else {
                    content::render(ui, &format!("Journaled phases: {}", observed.join(" > ")));
                }
            });
        }
    }

    fn activity_block(&self, ui: &mut egui::Ui) {
        ui.strong("Bounded activity");
        match &self.execution.record {
            Some(record) => {
                let tail = record.progress_tail(super::execution_screen::PROGRESS_LINES);
                if tail.is_empty() {
                    ui.label("The coordinator has not written activity lines yet.");
                }
                for line in tail {
                    content::render(ui, &line);
                }
                ui.small("Lines are the coordinator's own content-minimized stream: actor and stage only, never prompts, source or provider output.");
            }
            None => {
                ui.label("No coordinator launch is recorded for this campaign from this interface; activity lines are not available.");
            }
        }
    }
}

fn render_task_run_records(
    ui: &mut egui::Ui,
    catalogue: &Catalogue,
    task_id: &str,
    records: &[RunRecord],
    records_truncated: bool,
) {
    if records_truncated {
        ui.label(catalogue.text("work_plan_runs_truncated"));
    }
    let matching = records
        .iter()
        .filter(|record| record.bound_task_id == task_id)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        ui.label(catalogue.text("work_plan_runs_none"));
        return;
    }
    ui.small(catalogue.text("work_plan_runs_content_limit"));
    if matching.len() > 20 {
        ui.label(catalogue.format(
            "work_plan_runs_showing",
            &[("shown", "20"), ("total", &matching.len().to_string())],
        ));
    }
    for record in matching.into_iter().take(20) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(catalogue.format(
                "work_plan_runs_run",
                &[("run_id", &content::list_label(&record.run_id))],
            ));
            if let Some(checkpoint) = &record.checkpoint {
                ui.label(catalogue.format(
                    "work_plan_runs_candidate",
                    &[
                        ("commit", &checkpoint.candidate_commit),
                        ("rounds", &checkpoint.correction_rounds.to_string()),
                    ],
                ));
            } else {
                let problem = record
                    .checkpoint_problem
                    .as_deref()
                    .unwrap_or(catalogue.text("work_plan_runs_unknown_cause"));
                ui.label(
                    catalogue.format("work_plan_runs_checkpoint_problem", &[("problem", problem)]),
                );
            }
            content::render(ui, &run_review_label(record, catalogue));
            content::render(ui, &run_gate_label(record, catalogue));
            if let Some(problem) = &record.journal_problem {
                ui.label(
                    catalogue.format("work_plan_runs_journal_problem", &[("problem", problem)]),
                );
            } else {
                ui.label(catalogue.format(
                    "work_plan_runs_phases",
                    &[("count", &record.phases.len().to_string())],
                ));
                if record.phases_truncated {
                    ui.label(catalogue.text("work_plan_runs_phases_truncated"));
                }
                if record.phases.len() > 16 {
                    ui.label(catalogue.text("work_plan_runs_phases_showing"));
                }
                for phase in record.phases.iter().take(16) {
                    ui.small(catalogue.format(
                        "work_plan_runs_phase",
                        &[
                            ("timestamp", &phase.timestamp_ms.to_string()),
                            ("phase", &phase.phase),
                            ("kind", &phase.kind),
                            ("outcome", &phase.outcome),
                        ],
                    ));
                }
            }
        });
    }
}

fn run_review_label(record: &RunRecord, catalogue: &Catalogue) -> String {
    if record.checkpoint.is_some() && record.journal_problem.is_some() {
        return catalogue
            .text("work_plan_runs_review_uncorroborated")
            .to_owned();
    }
    match &record.checkpoint {
        None => catalogue
            .text("work_plan_runs_review_no_checkpoint")
            .to_owned(),
        Some(checkpoint) => checkpoint.review_verdict.as_ref().map_or_else(
            || catalogue.text("work_plan_runs_review_absent").to_owned(),
            |verdict| catalogue.format("work_plan_runs_review_verdict", &[("verdict", verdict)]),
        ),
    }
}

fn run_gate_label(record: &RunRecord, catalogue: &Catalogue) -> String {
    if record.checkpoint.is_some() && record.journal_problem.is_some() {
        return catalogue
            .text("work_plan_runs_gate_uncorroborated")
            .to_owned();
    }
    match &record.checkpoint {
        None => catalogue
            .text("work_plan_runs_gate_no_checkpoint")
            .to_owned(),
        Some(checkpoint) if checkpoint.gate_evidence.is_empty() => {
            catalogue.text("work_plan_runs_gate_absent").to_owned()
        }
        Some(checkpoint) => catalogue.format(
            "work_plan_runs_gate_summary",
            &[
                ("count", &checkpoint.gate_evidence.len().to_string()),
                ("ids", &checkpoint.gate_evidence.join(", ")),
            ],
        ),
    }
}

#[cfg(test)]
mod task_run_tests {
    use super::*;
    use crate::{backend::models::RunCheckpoint, records::PhaseObservation};
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    struct TaskRunPreview {
        catalogue: Catalogue,
        records: Vec<RunRecord>,
        right_to_left: bool,
    }

    impl eframe::App for TaskRunPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                if self.right_to_left {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        render_task_run_records(
                            ui,
                            &self.catalogue,
                            "1.1.1.1",
                            &self.records,
                            false,
                        );
                    });
                } else {
                    render_task_run_records(ui, &self.catalogue, "1.1.1.1", &self.records, false);
                }
            });
        }
    }

    fn accepted_record() -> RunRecord {
        RunRecord {
            run_id: "run-1".to_owned(),
            bound_task_id: "1.1.1.1".to_owned(),
            checkpoint: Some(RunCheckpoint {
                schema_version: 1,
                run_id: "run-1".to_owned(),
                task_id: "1.1.1.1".to_owned(),
                candidate_commit: "a".repeat(40),
                review_verdict: Some("pass".to_owned()),
                correction_rounds: 1,
                gate_evidence: vec!["evidence-1".to_owned()],
            }),
            checkpoint_problem: None,
            phases: vec![PhaseObservation {
                sequence: 0,
                phase: "review".to_owned(),
                kind: "gate_observed".to_owned(),
                outcome: "succeeded".to_owned(),
                timestamp_ms: 100,
                evidence: vec![],
                commit: None,
                gate: None,
            }],
            phases_truncated: false,
            journal_problem: None,
        }
    }

    #[test]
    fn task_run_evidence_keeps_missing_and_uncorroborated_results_distinct() {
        let catalogue = messages::english();
        let mut record = accepted_record();
        assert_eq!(run_review_label(&record, catalogue), "review verdict: pass");
        assert_eq!(
            run_gate_label(&record, catalogue),
            "1 gate evidence record(s): evidence-1"
        );
        record.journal_problem = Some("events.jsonl is absent".to_owned());
        assert_eq!(
            run_review_label(&record, catalogue),
            catalogue.text("work_plan_runs_review_uncorroborated")
        );
        assert_eq!(
            run_gate_label(&record, catalogue),
            catalogue.text("work_plan_runs_gate_uncorroborated")
        );
        record.checkpoint = None;
        record.checkpoint_problem = Some("checkpoint.json is absent".to_owned());
        assert_eq!(
            run_review_label(&record, catalogue),
            catalogue.text("work_plan_runs_review_no_checkpoint")
        );
        assert_eq!(
            run_gate_label(&record, catalogue),
            catalogue.text("work_plan_runs_gate_no_checkpoint")
        );
    }

    #[test]
    fn expanded_right_aligned_task_run_evidence_preserves_identity_and_outcomes() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let run = catalogue.format("work_plan_runs_run", &[("run_id", "run-1")]);
            let candidate = catalogue.format(
                "work_plan_runs_candidate",
                &[("commit", &"a".repeat(40)), ("rounds", "1")],
            );
            let review = catalogue.format("work_plan_runs_review_verdict", &[("verdict", "pass")]);
            let gate = catalogue.format(
                "work_plan_runs_gate_summary",
                &[("count", "1"), ("ids", "evidence-1")],
            );
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |_| TaskRunPreview {
                    catalogue,
                    records: vec![accepted_record()],
                    right_to_left,
                });
            harness.run_steps(2);
            for label in [&run, &candidate, &review, &gate] {
                assert!(
                    harness
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds(),
                    "missing accessible bounds for {label}"
                );
            }
        }
    }
}
