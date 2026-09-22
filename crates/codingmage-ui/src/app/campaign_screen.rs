//! Campaign selection, durable status observation and the campaign screen.

use std::{collections::BTreeMap, path::Path};

use codingmage_campaign::CampaignExecutionMode;
use codingmage_plan::{CheckState, PlanItemKind};

use super::{App, GIT_DEADLINE, STATUS_DEADLINE, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response, explain_code, models::parse_campaign_status},
    browser::Browser,
    campaign::{CampaignSelection, SUPPORTED_ROLES, TaskOverlay, UNAVAILABLE_MODES, build_overlay},
    observed::{Freshness, age_label},
};

impl App {
    /// Selects a campaign specification for the opened repository and requests its status.
    pub fn select_campaign(&mut self, spec_path: &Path) {
        let Some(project) = &self.project else {
            self.campaign_error = None;
            self.set_status("open a repository before selecting a campaign");
            return;
        };
        let observed = self
            .diagnosis
            .value
            .as_ref()
            .map(|diagnosis| diagnosis.repository_id.clone());
        let target = project.config.target_path.clone();
        self.clear_campaign_observations();
        self.campaign = None;
        match CampaignSelection::load(spec_path, &target, observed.as_deref()) {
            Ok(selection) => {
                self.campaign_input = selection.spec_path.display().to_string();
                self.campaign = Some(selection);
                self.campaign_error = None;
                self.persist_campaign_memory();
                self.restore_execution();
                self.set_status("campaign selected; requesting durable status");
                self.refresh_campaign();
            }
            Err(error) => {
                self.set_status(format!("campaign refused: {error}"));
                self.campaign_error = Some(error);
            }
        }
    }

    /// Clears the campaign selection without affecting any coordinator process.
    pub fn clear_campaign(&mut self) {
        self.clear_campaign_observations();
        self.campaign = None;
        self.campaign_error = None;
        self.execution = super::ExecutionState::default();
        self.persist_campaign_memory();
        self.set_status("campaign selection cleared; no coordinator process was affected");
    }

    pub(super) fn clear_campaign_observations(&mut self) {
        self.preflight.clear();
        self.changes.clear();
        self.changes_range = None;
        self.pending_changes = None;
        self.records.clear();
        self.status.clear();
        self.explanation.clear();
        self.report.clear();
        self.head_plan.clear();
        self.head_plan_commit = None;
        self.last_status_request = None;
    }

    /// Requests the durable status, blocker explanation and final report for the campaign.
    pub fn refresh_campaign(&mut self) {
        let Some((config_path, spec_path, parallel)) = self.campaign_arguments() else {
            return;
        };
        self.last_status_request = Some(self.now);
        if !self.records.loading {
            self.request_records();
        }
        let base = [
            "--config".to_owned(),
            config_path.clone(),
            "--campaign".to_owned(),
            spec_path.clone(),
        ];
        let mut jobs = vec![
            ("campaign-status", "campaign-status"),
            ("campaign-explain-blocker", "campaign-explain-blocker"),
        ];
        if parallel {
            jobs.push(("campaign-report", "campaign-report"));
        }
        for (label, command) in jobs {
            let mut arguments = vec![command.to_owned()];
            arguments.extend(base.iter().cloned());
            let request = Request {
                generation: self.generation,
                binding: self.binding(),
                job: Job::Command {
                    label,
                    arguments,
                    deadline: STATUS_DEADLINE,
                },
                request_id: None,
            };
            match self.submit(request) {
                Ok(()) => match label {
                    "campaign-status" => self.status.loading = true,
                    "campaign-explain-blocker" => self.explanation.loading = true,
                    _ => self.report.loading = true,
                },
                Err(error) => match label {
                    "campaign-status" => self.status.fail(error, self.now),
                    "campaign-explain-blocker" => self.explanation.fail(error, self.now),
                    _ => self.report.fail(error, self.now),
                },
            }
        }
    }

    fn campaign_arguments(&self) -> Option<(String, String, bool)> {
        let project = self.project.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        let parallel = campaign
            .spec
            .multi_agent
            .as_ref()
            .is_some_and(|policy| policy.execution_mode == CampaignExecutionMode::Parallel);
        Some((
            project.config_path.display().to_string(),
            campaign.spec_path.display().to_string(),
            parallel,
        ))
    }

    pub(super) fn accept_status(&mut self, response: Response) {
        match response
            .result
            .and_then(|bytes| parse_campaign_status(&bytes).map_err(BackendError::from))
        {
            Ok(status) => {
                let head = status.as_ref().map(|status| status.head.clone());
                self.status.accept(status, response.generation, self.now);
                if let Some(head) = &head
                    && self.head_plan_commit.as_deref() != Some(head.as_str())
                {
                    self.request_head_plan(head);
                }
                let base = self
                    .campaign
                    .as_ref()
                    .map(|campaign| campaign.spec.initial_commit.clone());
                if let (Some(head), Some(base)) = (head, base)
                    && self.changes_range.as_ref() != Some(&(base.clone(), head.clone()))
                {
                    self.request_changes(&base, &head);
                }
            }
            Err(error) => {
                self.set_status(format!("campaign status failed: {}", error.code()));
                self.status.fail(error, self.now);
            }
        }
    }

    fn request_head_plan(&mut self, head: &str) {
        let Some(project) = &self.project else {
            return;
        };
        let repository = project.config.target_path.clone();
        let task_source = project.config.task_source.display().to_string();
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::GitRead {
                label: "git-head-plan",
                repository,
                arguments: vec!["show".to_owned(), format!("{head}:{task_source}")],
                deadline: GIT_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => {
                self.head_plan.loading = true;
                self.head_plan_commit = Some(head.to_owned());
            }
            Err(error) => self.head_plan.fail(error, self.now),
        }
    }

    /// Per-task overlay of every observation for the opened plan.
    #[must_use]
    pub fn task_overlay(&self) -> BTreeMap<String, TaskOverlay> {
        let source = self
            .plan_index
            .as_ref()
            .map(|index| {
                index
                    .rows()
                    .iter()
                    .filter(|row| row.kind == PlanItemKind::SubTask)
                    .map(|row| (row.id.clone(), row.state))
                    .collect::<BTreeMap<String, CheckState>>()
            })
            .unwrap_or_default();
        let head = self.head_plan.value.as_ref().and_then(|plan| {
            plan.as_ref().map(|plan| {
                plan.plan
                    .items
                    .iter()
                    .filter(|item| item.kind == PlanItemKind::SubTask)
                    .map(|item| (item.id.clone(), item.state))
                    .collect::<BTreeMap<String, CheckState>>()
            })
        });
        let status = self.status.value.as_ref().and_then(Option::as_ref);
        let report = self.report.value.as_ref().and_then(Option::as_ref);
        build_overlay(&source, head.as_ref(), status, report)
    }

    pub(super) fn campaign_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Campaign");
        if self.project.is_none() {
            ui.label("Open a repository before selecting a campaign.");
            return;
        }
        self.campaign_selection_controls(ui);
        let Some(campaign) = &self.campaign else {
            ui.label("No campaign selected. Select the campaign specification that binds this repository, or create one in Setup.");
            roles_and_modes(ui);
            return;
        };
        let spec = &campaign.spec;
        ui.separator();
        ui.heading(format!("Campaign {}", spec.campaign_id));
        egui::Grid::new("campaign-authority")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Authority sha256");
                ui.monospace(&campaign.authority_sha256);
                ui.end_row();
                ui.label("Repository id");
                ui.monospace(&spec.repository_id);
                ui.end_row();
                ui.label("Initial commit");
                ui.monospace(&spec.initial_commit);
                ui.end_row();
                ui.label("Task source sha256");
                ui.monospace(&spec.task_source_sha256);
                ui.end_row();
                ui.label("Execution");
                ui.label(match &spec.multi_agent {
                    Some(policy) if policy.execution_mode == CampaignExecutionMode::Parallel => {
                        format!("parallel, up to {} pods", spec.max_parallel_pods)
                    }
                    _ => "serial, one pod".to_owned(),
                });
                ui.end_row();
                ui.label("Accepted-outcome ceiling");
                ui.label(spec.max_units.to_string());
                ui.end_row();
                ui.label("Publication");
                ui.label(format!("{:?}", spec.publication));
                ui.end_row();
            });
        self.authority_drift(ui);
        ui.separator();
        self.readiness_section(ui);
        ui.separator();
        self.execution_section(ui);
        ui.separator();
        self.status_section(ui);
        ui.separator();
        roles_and_modes(ui);
    }

    fn campaign_selection_controls(&mut self, ui: &mut egui::Ui) {
        let mut select: Option<std::path::PathBuf> = None;
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.label("Campaign specification");
            ui.add(
                egui::TextEdit::singleline(&mut self.campaign_input)
                    .hint_text("/absolute/path/campaign.toml")
                    .desired_width(420.0),
            );
            if ui.button("Select campaign").clicked() {
                select = Some(std::path::PathBuf::from(self.campaign_input.trim()));
            }
            if self.campaign.is_some() && ui.button("Clear campaign").clicked() {
                clear = true;
            }
            if ui
                .button(if self.campaign_browser.is_some() {
                    "Hide campaign browser"
                } else {
                    "Browse campaigns"
                })
                .clicked()
            {
                self.campaign_browser = match self.campaign_browser.take() {
                    Some(_) => None,
                    None => Some(Browser::at_home(vec!["toml"])),
                };
            }
        });
        if let Some(browser) = &mut self.campaign_browser {
            let mut enter = None;
            let mut up = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Up").clicked() {
                        up = true;
                    }
                    ui.monospace(browser.current.display().to_string());
                });
                egui::ScrollArea::vertical()
                    .id_salt("campaign-browser")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for entry in &browser.entries {
                            let label = if entry.is_dir {
                                format!("{}/", entry.name)
                            } else {
                                entry.name.clone()
                            };
                            if ui
                                .add_enabled(!entry.is_symlink, egui::Button::new(label))
                                .clicked()
                            {
                                if entry.is_dir {
                                    enter = Some(entry.path.clone());
                                } else {
                                    select = Some(entry.path.clone());
                                }
                            }
                        }
                    });
            });
            if up {
                browser.up();
            }
            if let Some(path) = enter {
                browser.enter(&path);
            }
        }
        if let Some(error) = &self.campaign_error {
            failure_box(
                ui,
                "Campaign specification refused",
                &error.to_string(),
                "Select a specification whose repository path and identity match the opened repository.",
            );
        }
        if let Some(path) = select {
            self.select_campaign(&path);
        }
        if clear {
            self.clear_campaign();
        }
    }

    fn authority_drift(&self, ui: &mut egui::Ui) {
        let (Some(campaign), Some(diagnosis)) = (&self.campaign, &self.diagnosis.value) else {
            return;
        };
        let mut drift = Vec::new();
        if diagnosis.repository_id != campaign.spec.repository_id {
            drift.push("repository identity differs from the diagnosis");
        }
        if diagnosis.task_source_sha256 != campaign.spec.task_source_sha256 {
            drift.push("active checkout task source differs from the campaign's bound digest");
        }
        if diagnosis.head != campaign.spec.initial_commit {
            drift.push("active checkout head differs from the campaign's initial commit");
        }
        if drift.is_empty() {
            ui.label("Active checkout matches the campaign's bound repository identity, head and task-source digest.");
        } else {
            for line in drift {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 160, 60),
                    format!("Binding drift: {line}"),
                );
            }
            ui.small("Drift is reported, not corrected. A campaign already started keeps its own branch; a campaign not yet started must be re-authored against the current head.");
        }
    }

    fn status_section(&self, ui: &mut egui::Ui) {
        ui.heading("Durable campaign status");
        let freshness = self.status.freshness(self.now);
        ui.label(format!(
            "Observation: {} ({})",
            freshness.label(),
            age_label(self.status.age(self.now))
        ));
        if let Some((_, error)) = &self.status.last_error {
            let (what, action) = explain_code(&error.code());
            failure_box(ui, what, &error.to_string(), action);
        }
        match (&self.status.value, freshness) {
            (None, Freshness::Loading) => {
                ui.label("Reading durable campaign state from the coordinator...");
            }
            (None, _) => {}
            (Some(None), _) => {
                ui.label("No durable campaign state exists for this authority. The campaign has never been started; nothing is running and no outcome exists.");
            }
            (Some(Some(status)), _) => {
                if freshness == Freshness::Stale {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 120, 0),
                        "Stale observation: values below may no longer match the coordinator.",
                    );
                }
                status_grid(ui, status);
                ui.separator();
                outcomes_grid(ui, status);
                ui.separator();
                active_tasks(ui, status);
                holds_section(ui, status);
                ui.separator();
                utilization_grid(ui, status);
            }
        }
        if let Some(explanation) = &self.explanation.value
            && explanation.blocker_code.is_some()
        {
            ui.label(format!(
                "Campaign-level blocker: {}",
                explanation.blocker_code.as_deref().unwrap_or("")
            ));
        }
        if let Some(Some(report)) = &self.report.value {
            ui.separator();
            ui.strong("Final report exists");
            ui.monospace(format!(
                "final commit {} with {} accepted tasks",
                report.final_commit,
                report.tasks.len()
            ));
        }
    }
}

fn outcomes_grid(ui: &mut egui::Ui, status: &crate::backend::models::CampaignStatus) {
    ui.strong("Outcome counters (independent, never merged)");
    let outcomes = &status.outcomes;
    egui::Grid::new("campaign-outcomes")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Completed and reconciled");
            ui.label(outcomes.completed.to_string());
            ui.end_row();
            ui.label("Blocked");
            ui.label(outcomes.blocked.to_string());
            ui.end_row();
            ui.label("Deferred");
            ui.label(outcomes.deferred.to_string());
            ui.end_row();
            ui.label("Awaiting human decision");
            ui.label(outcomes.pending_human_decision.to_string());
            ui.end_row();
            ui.label("Rejected proposals (no ceiling use)");
            ui.label(outcomes.rejected_proposals.to_string());
            ui.end_row();
            ui.label("Accepted against ceiling");
            ui.label(format!(
                "{} of {}",
                outcomes.accepted, outcomes.max_accepted
            ));
            ui.end_row();
        });
}

fn active_tasks(ui: &mut egui::Ui, status: &crate::backend::models::CampaignStatus) {
    ui.strong("Active tasks");
    if status.active_tasks.is_empty() {
        ui.label("No unit is active.");
    }
    for active in &status.active_tasks {
        ui.monospace(format!(
            "{} - {} by {}{} (round {}, heartbeat {}{})",
            active.task_id,
            active.state,
            active.actor,
            active
                .model
                .as_ref()
                .map_or(String::new(), |model| format!(" using {model}")),
            active.correction_round,
            active.heartbeat_sequence,
            active
                .pod_id
                .as_ref()
                .map_or(String::new(), |pod| format!(", pod {pod}")),
        ));
    }
}

fn status_grid(ui: &mut egui::Ui, status: &crate::backend::models::CampaignStatus) {
    egui::Grid::new("campaign-status")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Phase");
            ui.label(&status.state);
            ui.end_row();
            ui.label("Actor");
            ui.label(&status.actor);
            ui.end_row();
            ui.label("Model");
            ui.label(status.model.as_deref().unwrap_or("not owned by a provider"));
            ui.end_row();
            ui.label("Campaign branch");
            ui.monospace(&status.branch);
            ui.end_row();
            ui.label("Campaign head");
            ui.monospace(&status.head);
            ui.end_row();
            ui.label("Current task");
            ui.label(status.current_task_id.as_deref().unwrap_or("none"));
            ui.end_row();
            ui.label("Last task");
            ui.label(status.last_task_id.as_deref().unwrap_or("none"));
            ui.end_row();
            ui.label("Watchdog");
            ui.label(&status.watchdog_state);
            ui.end_row();
            ui.label("Reconciliation");
            ui.label(&status.reconciliation_state);
            ui.end_row();
            ui.label("Blocker code");
            ui.label(status.blocker_code.as_deref().unwrap_or("none"));
            ui.end_row();
            ui.label("Elapsed");
            ui.label(format!("{} s since creation", status.elapsed_ms / 1000));
            ui.end_row();
            ui.label("Updated");
            ui.label(format!("unix {} ms", status.updated_at_ms));
            ui.end_row();
        });
}

fn holds_section(ui: &mut egui::Ui, status: &crate::backend::models::CampaignStatus) {
    ui.separator();
    ui.strong("Blocked tasks");
    if status.blockers.is_empty() {
        ui.label("none");
    }
    for blocker in &status.blockers {
        ui.monospace(format!("{} - {}", blocker.task_id, blocker.reason_code));
    }
    ui.strong("Deferred tasks");
    if status.deferrals.is_empty() {
        ui.label("none");
    }
    for deferral in &status.deferrals {
        ui.monospace(format!(
            "{} - {} until {} ({})",
            deferral.task_id, deferral.reason_code, deferral.trigger_code, deferral.trigger_state
        ));
    }
    ui.strong("Human decisions required");
    if status.human_decisions.is_empty() {
        ui.label("none");
    }
    for decision in &status.human_decisions {
        ui.monospace(format!("{} - {}", decision.task_id, decision.reason_code));
    }
}

fn utilization_grid(ui: &mut egui::Ui, status: &crate::backend::models::CampaignStatus) {
    ui.strong("Utilization against limits");
    let used = &status.utilization;
    let limits = &status.limits;
    egui::Grid::new("campaign-utilization")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Provider attempts");
            ui.label(format!(
                "{} of {}",
                used.provider_attempts, limits.provider_attempts
            ));
            ui.end_row();
            ui.label("Malformed-report repairs");
            ui.label(format!(
                "{} of {}",
                used.malformed_report_repairs, limits.malformed_report_repairs
            ));
            ui.end_row();
            ui.label("Correction rounds");
            ui.label(format!(
                "{} of {}",
                used.correction_rounds, limits.correction_rounds
            ));
            ui.end_row();
            ui.label("Process invocations");
            ui.label(format!(
                "{} of {}",
                used.process_invocations, limits.process_invocations
            ));
            ui.end_row();
            ui.label("Output bytes");
            ui.label(format!("{} of {}", used.output_bytes, limits.output_bytes));
            ui.end_row();
            ui.label("Retained state bytes");
            ui.label(format!(
                "{} of {}",
                used.retained_state_bytes, limits.retained_state_bytes
            ));
            ui.end_row();
            ui.label("Execution time");
            ui.label(format!(
                "{} ms of {} ms",
                used.execution_elapsed_ms, limits.execution_elapsed_ms
            ));
            ui.end_row();
        });
    ui.small("Token usage is not reported by this backend and is shown as unknown rather than estimated.");
}

fn roles_and_modes(ui: &mut egui::Ui) {
    ui.heading("Roles this backend reports");
    for (code, description) in SUPPORTED_ROLES {
        ui.label(format!("{code}: {description}"));
    }
    ui.heading("Owner involvement modes");
    ui.label("Unavailable in this backend revision. The campaign runs under its recorded authority; the interface neither asks routine questions nor answers them.");
    for (mode, reason) in UNAVAILABLE_MODES {
        ui.label(format!("{mode}: unavailable, {reason}"));
    }
}
