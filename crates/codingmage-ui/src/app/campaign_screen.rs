//! Campaign selection, durable status observation and the campaign screen.

use std::{collections::BTreeMap, path::Path};

use codingmage_campaign::CampaignExecutionMode;
use codingmage_plan::{CheckState, PlanItemKind};

use super::{App, GIT_DEADLINE, STATUS_DEADLINE, Screen, failure_box};
use crate::{
    backend::{
        BackendError, Job, Request, Response, explain_code,
        models::{
            MissionStatus, ModelError, parse_blocker_explanation, parse_campaign_status,
            parse_mission_status,
        },
    },
    browser::Browser,
    campaign::{
        CampaignSelection, INVOLVEMENT_MODES, SUPPORTED_ROLES, TaskOverlay, build_overlay,
        involvement_label,
    },
    command, content,
    messages::{self, Catalogue},
    observed::Observed,
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
        let diagnosis_loading = self.diagnosis.loading;
        self.advance_selection_generation();
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
        if diagnosis_loading {
            self.refresh_diagnosis();
        }
    }

    /// Clears the campaign selection without affecting any coordinator process.
    pub fn clear_campaign(&mut self) {
        let diagnosis_loading = self.diagnosis.loading;
        self.advance_selection_generation();
        self.clear_campaign_observations();
        self.campaign = None;
        self.campaign_error = None;
        self.execution = super::ExecutionState::default();
        self.persist_campaign_memory();
        self.set_status("campaign selection cleared; no coordinator process was affected");
        if diagnosis_loading {
            self.refresh_diagnosis();
        }
    }

    pub(super) fn clear_campaign_observations(&mut self) {
        self.invalidate_evidence_requests();
        self.blocker_query.clear();
        self.support = super::SupportState::default();
        self.clear_report_export();
        self.preflight.clear();
        self.changes.clear();
        self.changes_range = None;
        self.records.clear();
        self.records_status = None;
        self.records_truncated = false;
        self.status.clear();
        self.explanation.clear();
        self.report.clear();
        self.mission.clear();
        self.head_plan.clear();
        self.head_plan_commit = None;
        self.task_detail.clear();
        self.task_detail_key = None;
        self.last_status_request = None;
    }

    /// Requests the durable status, blocker explanation and final report for the campaign.
    pub fn refresh_campaign(&mut self) {
        let Some((config_path, spec_path, parallel)) = self.campaign_arguments() else {
            return;
        };
        self.request_status();
        self.request_explanation();
        let base = [
            "--config".to_owned(),
            config_path.clone(),
            "--campaign".to_owned(),
            spec_path.clone(),
        ];
        let mut jobs = vec![("campaign-mission-status", "campaign-mission-status")];
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
                    "campaign-mission-status" => self.mission.loading = true,
                    _ => self.report.loading = true,
                },
                Err(error) => match label {
                    "campaign-mission-status" => self.mission.fail(error, self.now),
                    _ => self.report.fail(error, self.now),
                },
            }
        }
    }

    pub(super) fn explanation_arguments(&self) -> Option<Vec<String>> {
        let (config, campaign, _) = self.campaign_arguments()?;
        Some(vec![
            "campaign-explain-blocker".to_owned(),
            "--config".to_owned(),
            config,
            "--campaign".to_owned(),
            campaign,
        ])
    }

    pub(super) fn request_explanation(&mut self) {
        let Some(arguments) = self.explanation_arguments() else {
            return;
        };
        if self.explanation.loading {
            return;
        }
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-explain-blocker",
                arguments,
                deadline: STATUS_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => self.explanation.loading = true,
            Err(error) => self.explanation.fail(error, self.now),
        }
    }

    pub(super) fn status_arguments(&self) -> Option<Vec<String>> {
        let (config, campaign, _) = self.campaign_arguments()?;
        Some(vec![
            "campaign-status".to_owned(),
            "--config".to_owned(),
            config,
            "--campaign".to_owned(),
            campaign,
        ])
    }

    pub(super) fn request_status(&mut self) {
        let Some(arguments) = self.status_arguments() else {
            return;
        };
        self.invalidate_evidence_requests();
        self.last_status_request = Some(self.now);
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-status",
                arguments,
                deadline: STATUS_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => self.status.loading = true,
            Err(error) => self.status.fail(error, self.now),
        }
    }

    pub(super) fn campaign_arguments(&self) -> Option<(String, String, bool)> {
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

    /// Accepts a mission status; a `codingmage.runtime.state` refusal means no charter exists.
    pub(super) fn accept_mission(&mut self, response: Response) {
        let expected = self.campaign.as_ref().map(|campaign| {
            (
                campaign.spec.campaign_id.as_str(),
                campaign.authority_sha256.as_str(),
            )
        });
        let parsed = response
            .result
            .and_then(|bytes| parse_mission_status(&bytes).map_err(BackendError::from))
            .and_then(|mission| {
                if expected
                    == Some((
                        mission.campaign_id.as_str(),
                        mission.authority_sha256.as_str(),
                    ))
                {
                    Ok(mission)
                } else {
                    Err(BackendError::Contract(ModelError::AuthorityMismatch))
                }
            });
        match parsed {
            Ok(mission) => self
                .mission
                .accept(Some(mission), response.generation, self.now),
            Err(BackendError::Command { code, .. }) if code == "codingmage.runtime.state" => {
                self.mission.accept(None, response.generation, self.now);
            }
            Err(error) => self.mission.fail(error, self.now),
        }
    }

    pub(super) fn accept_explanation(&mut self, response: Response) {
        let selected = self
            .campaign
            .as_ref()
            .map(|selection| selection.spec.campaign_id.as_str());
        let parsed = response
            .result
            .and_then(|bytes| parse_blocker_explanation(&bytes).map_err(BackendError::from))
            .and_then(|explanation| {
                if selected.is_some()
                    && explanation
                        .as_ref()
                        .is_none_or(|value| selected == Some(value.campaign_id.as_str()))
                {
                    Ok(explanation)
                } else {
                    Err(BackendError::Contract(ModelError::AuthorityMismatch))
                }
            });
        match parsed {
            Ok(explanation) => self
                .explanation
                .accept(explanation, response.generation, self.now),
            Err(error) => self.explanation.fail(error, self.now),
        }
    }

    pub(super) fn accept_status(&mut self, response: Response) {
        self.invalidate_evidence_requests();
        let expected = self
            .campaign
            .as_ref()
            .map(|campaign| campaign.spec.campaign_id.as_str());
        let parsed = response
            .result
            .and_then(|bytes| parse_campaign_status(&bytes).map_err(BackendError::from))
            .and_then(|status| {
                if status
                    .as_ref()
                    .is_some_and(|status| expected != Some(status.campaign_id.as_str()))
                {
                    Err(BackendError::Contract(ModelError::AuthorityMismatch))
                } else {
                    Ok(status)
                }
            });
        match parsed {
            Ok(status) => {
                let head = status.as_ref().map(|status| status.head.clone());
                self.status.accept(status, response.generation, self.now);
                if !self.records.loading {
                    self.request_records();
                }
                if head.is_none() {
                    self.head_plan.clear();
                    self.head_plan_commit = None;
                    self.task_detail.clear();
                    self.task_detail_key = None;
                    self.changes.clear();
                    self.changes_range = None;
                }
                if let Some(head) = &head
                    && (self.head_plan_commit.as_deref() != Some(head.as_str())
                        || self.head_plan.freshness(self.now) == Freshness::Stale)
                {
                    if self
                        .task_detail_key
                        .as_ref()
                        .is_some_and(|(known, _)| known != head)
                    {
                        self.task_detail.clear();
                        self.task_detail_key = None;
                    }
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
                self.records.clear();
                self.records_status = None;
                self.records_truncated = false;
                self.head_plan.clear();
                self.head_plan_commit = None;
                self.task_detail.clear();
                self.task_detail_key = None;
                self.changes.clear();
                self.changes_range = None;
            }
        }
    }

    pub(super) fn accept_report(&mut self, response: Response) {
        let expected = self.campaign.as_ref().map(|selection| &selection.spec);
        let parsed = response
            .result
            .and_then(|bytes| {
                crate::backend::models::parse_campaign_report(&bytes).map_err(BackendError::from)
            })
            .and_then(|report| {
                if expected.is_some_and(|spec| {
                    report.as_ref().is_none_or(|report| {
                        report.campaign_id == spec.campaign_id
                            && report.repository_id == spec.repository_id
                            && report.initial_commit == spec.initial_commit
                            && report.branch.starts_with(&spec.campaign_branch)
                    })
                }) {
                    Ok(report)
                } else {
                    Err(BackendError::Contract(ModelError::AuthorityMismatch))
                }
            });
        match parsed {
            Ok(report) => self.report.accept(report, response.generation, self.now),
            Err(error) => self.report.fail(error, self.now),
        }
    }

    fn request_head_plan(&mut self, head: &str) {
        let Some((config_path, spec_path, _)) = self.campaign_arguments() else {
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-head-plan",
                arguments: vec![
                    "campaign-head-plan".to_owned(),
                    "--config".to_owned(),
                    config_path,
                    "--campaign".to_owned(),
                    spec_path,
                    "--head".to_owned(),
                    head.to_owned(),
                ],
                deadline: GIT_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => {
                self.head_plan.clear();
                self.head_plan.loading = true;
                self.head_plan_commit = Some(head.to_owned());
            }
            Err(error) => {
                self.head_plan_commit = None;
                self.head_plan.fail(error, self.now);
            }
        }
    }

    pub(super) fn accept_head_plan(&mut self, response: Response) {
        match response.result.and_then(|bytes| {
            crate::backend::models::parse_head_plan(&bytes).map_err(BackendError::from)
        }) {
            Ok(plan) => {
                let status_head = self
                    .status
                    .value
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|status| status.head.as_str());
                let campaign_id = self
                    .campaign
                    .as_ref()
                    .map(|selection| selection.spec.campaign_id.as_str());
                let repository_id = self
                    .diagnosis
                    .value
                    .as_ref()
                    .map(|diagnosis| diagnosis.repository_id.as_str());
                if self.status.freshness(self.now) == Freshness::Live
                    && status_head == Some(plan.head.as_str())
                    && campaign_id == Some(plan.campaign_id.as_str())
                    && repository_id == Some(plan.repository_id.as_str())
                {
                    self.head_plan
                        .accept(Some(plan), response.generation, self.now);
                } else {
                    self.head_plan_commit = None;
                    self.head_plan.fail(
                        BackendError::Refused(
                            "campaign head or identity changed while task states were observed; refresh the campaign".to_owned(),
                        ),
                        self.now,
                    );
                }
            }
            Err(error) => {
                self.head_plan_commit = None;
                self.head_plan.fail(error, self.now);
            }
        }
    }

    pub(super) fn task_detail_arguments(&self, item_id: &str) -> Option<Vec<String>> {
        let (config, campaign, _) = self.campaign_arguments()?;
        if self.status.loading || self.status.freshness(self.now) != Freshness::Live {
            return None;
        }
        let head = &self.status.value.as_ref()?.as_ref()?.head;
        Some(vec![
            "campaign-task-detail".to_owned(),
            "--config".to_owned(),
            config,
            "--campaign".to_owned(),
            campaign,
            "--head".to_owned(),
            head.clone(),
            "--item".to_owned(),
            item_id.to_owned(),
        ])
    }

    pub(super) fn request_task_detail(&mut self, item_id: &str) {
        let Some(arguments) = self.task_detail_arguments(item_id) else {
            return;
        };
        let head = arguments[6].clone();
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-task-detail",
                arguments,
                deadline: GIT_DEADLINE,
            },
            request_id: Some(item_id.to_owned()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.task_detail.clear();
                self.task_detail.loading = true;
                self.task_detail_key = Some((head, item_id.to_owned()));
            }
            Err(error) => self.task_detail.fail(error, self.now),
        }
    }

    pub(super) fn accept_task_detail(&mut self, response: Response) {
        if response.request_id.as_deref() != self.selected_item.as_deref() {
            return;
        }
        let result = response.result.and_then(|bytes| {
            crate::backend::models::parse_task_detail(&bytes).map_err(BackendError::from)
        });
        match result {
            Ok(detail) => {
                let status_head = self
                    .status
                    .value
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(|status| status.head.as_str());
                let campaign_id = self
                    .campaign
                    .as_ref()
                    .map(|value| value.spec.campaign_id.as_str());
                let repository_id = self
                    .diagnosis
                    .value
                    .as_ref()
                    .map(|value| value.repository_id.as_str());
                let expected_key = Some((detail.head.clone(), detail.item_id.clone()));
                if self.status.freshness(self.now) == Freshness::Live
                    && status_head == Some(detail.head.as_str())
                    && campaign_id == Some(detail.campaign_id.as_str())
                    && repository_id == Some(detail.repository_id.as_str())
                    && self.task_detail_key == expected_key
                    && response.request_id.as_deref() == Some(detail.item_id.as_str())
                {
                    self.task_detail
                        .accept(detail, response.generation, self.now);
                } else {
                    self.task_detail.fail(
                        BackendError::Refused(
                            "task source detail belongs to a changed campaign, head or selection; refresh the campaign".to_owned(),
                        ),
                        self.now,
                    );
                }
            }
            Err(error) => self.task_detail.fail(error, self.now),
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
        let status = (!self.status.loading && self.status.freshness(self.now) == Freshness::Live)
            .then(|| self.status.value.as_ref().and_then(Option::as_ref))
            .flatten();
        let head = self.head_plan.value.as_ref().and_then(|plan| {
            plan.as_ref().and_then(|plan| {
                (self.status.freshness(self.now) == Freshness::Live
                    && self.head_plan.freshness(self.now) == Freshness::Live
                    && status.is_some_and(|status| status.head == plan.head))
                .then(|| {
                    plan.items
                        .iter()
                        .map(|item| (item.id.clone(), item.state))
                        .collect::<BTreeMap<String, CheckState>>()
                })
            })
        });
        let report = (!self.report.loading && self.report.last_error.is_none())
            .then(|| self.report.value.as_ref().and_then(Option::as_ref))
            .flatten()
            .filter(|report| {
                status.is_some_and(|status| {
                    status.state == "complete"
                        && status.campaign_id == report.campaign_id
                        && status.branch == report.branch
                        && status.head == report.final_commit
                })
            });
        build_overlay(&source, head.as_ref(), status, report)
    }

    pub(super) fn campaign_screen(&mut self, ui: &mut egui::Ui) {
        self.campaign_screen_with_catalogue(ui, messages::english());
    }

    fn campaign_screen_with_catalogue(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("campaign_title"));
        if self.project.is_none() {
            ui.label(catalogue.text("campaign_no_project"));
            if ui.button(catalogue.text("campaign_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            if ui.button(catalogue.text("campaign_open_help")).clicked() {
                self.screen = Screen::Help;
            }
            return;
        }
        self.campaign_selection_controls(ui, catalogue);
        let Some(campaign) = &self.campaign else {
            ui.label(catalogue.text("campaign_no_selection"));
            if ui.button(catalogue.text("campaign_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            roles_and_modes(ui, &self.mission, self.now, catalogue);
            return;
        };
        let spec = &campaign.spec;
        ui.separator();
        ui.heading(catalogue.format("campaign_named", &[("campaign_id", &spec.campaign_id)]));
        egui::Grid::new("campaign-authority")
            .num_columns(2)
            .spacing(super::current_tokens(ui.ctx()).layout.grid)
            .show(ui, |ui| {
                ui.label(catalogue.text("campaign_authority_sha256"));
                ui.monospace(&campaign.authority_sha256);
                ui.end_row();
                ui.label(catalogue.text("campaign_repository_id"));
                ui.monospace(&spec.repository_id);
                ui.end_row();
                ui.label(catalogue.text("campaign_initial_commit"));
                ui.monospace(&spec.initial_commit);
                ui.end_row();
                ui.label(catalogue.text("campaign_task_source_sha256"));
                ui.monospace(&spec.task_source_sha256);
                ui.end_row();
                ui.label(catalogue.text("campaign_execution"));
                ui.label(match &spec.multi_agent {
                    Some(policy) if policy.execution_mode == CampaignExecutionMode::Parallel => {
                        catalogue.format(
                            "campaign_execution_parallel",
                            &[("pods", &spec.max_parallel_pods.to_string())],
                        )
                    }
                    _ => catalogue.text("campaign_execution_serial").to_owned(),
                });
                ui.end_row();
                ui.label(catalogue.text("campaign_accepted_outcome_ceiling"));
                ui.label(spec.max_units.to_string());
                ui.end_row();
                ui.label(catalogue.text("campaign_publication"));
                ui.label(format!("{:?}", spec.publication));
                ui.end_row();
            });
        self.authority_drift(ui, catalogue);
        ui.separator();
        let admitted_or_launched =
            self.execution.admission.is_some() || self.execution.record.is_some();
        if admitted_or_launched {
            self.execution_section(ui);
            ui.separator();
            self.status_section(ui, catalogue);
            ui.separator();
            self.readiness_section(ui);
        } else {
            self.readiness_section(ui);
            ui.separator();
            self.execution_section(ui);
            ui.separator();
            self.status_section(ui, catalogue);
        }
        ui.separator();
        roles_and_modes(ui, &self.mission, self.now, catalogue);
    }

    fn campaign_selection_controls(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let mut select: Option<std::path::PathBuf> = None;
        let mut clear = false;
        ui.horizontal(|ui| {
            let label = ui.label(catalogue.text("campaign_specification"));
            ui.add(
                egui::TextEdit::singleline(&mut self.campaign_input)
                    .hint_text(catalogue.text("campaign_path_hint"))
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
            if ui
                .button(catalogue.text("campaign_select_campaign"))
                .clicked()
            {
                select = Some(std::path::PathBuf::from(self.campaign_input.trim()));
            }
            if self.campaign.is_some()
                && ui
                    .button(catalogue.text("campaign_clear_campaign"))
                    .clicked()
            {
                clear = true;
            }
            if ui
                .button(if self.campaign_browser.is_some() {
                    catalogue.text("campaign_hide_browser")
                } else {
                    catalogue.text("campaign_browse")
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
                    if ui.button(catalogue.text("campaign_up")).clicked() {
                        up = true;
                    }
                    ui.monospace(browser.current.display().to_string());
                });
                egui::ScrollArea::vertical()
                    .id_salt("campaign-browser")
                    .max_height(super::current_tokens(ui.ctx()).layout.preview_records)
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
                catalogue.text("campaign_spec_refused"),
                &error.to_string(),
                catalogue.text("campaign_spec_refused_action"),
            );
        }
        if let Some(path) = select {
            self.select_campaign(&path);
        }
        if clear {
            self.clear_campaign();
        }
    }

    fn authority_drift(&self, ui: &mut egui::Ui, catalogue: &Catalogue) {
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
            ui.label(catalogue.text("campaign_binding_matches"));
        } else {
            for line in drift {
                ui.colored_label(
                    super::current_tokens(ui.ctx()).warning,
                    catalogue.format("campaign_binding_drift", &[("reason", line)]),
                );
            }
            ui.small(catalogue.text("campaign_binding_drift_action"));
        }
    }

    fn status_section(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("campaign_durable_campaign_status"));
        let arguments = self.status_arguments();
        let can_refresh = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                can_refresh && !self.status.loading,
                egui::Button::new(catalogue.text("campaign_refresh_campaign_status")),
            )
            .clicked()
        {
            self.request_status();
        }
        if let Some(arguments) = &arguments {
            command::show_for(
                ui,
                catalogue.text("campaign_refresh_campaign_status"),
                self.binary_path.as_deref(),
                arguments,
            );
        } else {
            command::show_unavailable_for(ui, catalogue.text("campaign_refresh_campaign_status"));
        }
        if self.status.loading && self.status.value.is_some() {
            ui.small(catalogue.text("campaign_refreshing_retained"));
        }
        let freshness = self.status.freshness(self.now);
        ui.label(catalogue.format(
            "campaign_observation",
            &[
                ("freshness", freshness.label()),
                ("age", &age_label(self.status.age(self.now))),
            ],
        ));
        if let Some((_, error)) = &self.status.last_error {
            let (what, action) = explain_code(&error.code());
            failure_box(ui, what, &error.to_string(), action);
            ui.small(catalogue.text("campaign_status_unconfirmed"));
        }
        match (&self.status.value, freshness) {
            (None, Freshness::Loading) => {
                ui.label(catalogue.text("campaign_status_loading"));
            }
            (None, _) => {}
            (Some(None), _) => {
                ui.label(catalogue.text("campaign_status_not_started"));
            }
            (Some(Some(status)), _) => {
                if freshness == Freshness::Stale {
                    ui.colored_label(
                        super::current_tokens(ui.ctx()).warning,
                        catalogue.text("campaign_status_stale"),
                    );
                }
                status_grid(ui, status, catalogue);
                ui.separator();
                outcomes_grid(ui, status, catalogue);
                ui.separator();
                active_tasks(ui, status, catalogue);
                holds_section(ui, status, catalogue);
                ui.separator();
                utilization_grid(ui, status, catalogue);
            }
        }
        if let Some(Some(explanation)) = &self.explanation.value
            && explanation.blocker_code.is_some()
        {
            ui.label(catalogue.format(
                "campaign_level_blocker",
                &[("code", explanation.blocker_code.as_deref().unwrap_or(""))],
            ));
        }
        if let Some(Some(report)) = &self.report.value {
            ui.separator();
            ui.strong(catalogue.text("campaign_final_report_exists"));
            ui.monospace(catalogue.format(
                "campaign_final_summary",
                &[
                    ("commit", &report.final_commit),
                    ("accepted", &report.tasks.len().to_string()),
                ],
            ));
        }
    }
}

fn outcomes_grid(
    ui: &mut egui::Ui,
    status: &crate::backend::models::CampaignStatus,
    catalogue: &Catalogue,
) {
    ui.strong(catalogue.text("campaign_outcomes_title"));
    let outcomes = &status.outcomes;
    egui::Grid::new("campaign-outcomes")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label(catalogue.text("campaign_completed_and_reconciled"));
            ui.label(outcomes.completed.to_string());
            ui.end_row();
            ui.label(catalogue.text("campaign_blocked"));
            ui.label(outcomes.blocked.to_string());
            ui.end_row();
            ui.label(catalogue.text("campaign_deferred"));
            ui.label(outcomes.deferred.to_string());
            ui.end_row();
            ui.label(catalogue.text("campaign_awaiting_human_decision"));
            ui.label(outcomes.pending_human_decision.to_string());
            ui.end_row();
            ui.label(catalogue.text("campaign_rejected_proposals_no_ceiling_use"));
            ui.label(outcomes.rejected_proposals.to_string());
            ui.end_row();
            ui.label(catalogue.text("campaign_accepted_against_ceiling"));
            ui.label(usage_label(
                catalogue,
                &outcomes.accepted,
                &outcomes.max_accepted,
                false,
            ));
            ui.end_row();
        });
}

fn active_tasks(
    ui: &mut egui::Ui,
    status: &crate::backend::models::CampaignStatus,
    catalogue: &Catalogue,
) {
    ui.strong(catalogue.text("campaign_active_tasks"));
    if status.active_tasks.is_empty() {
        ui.label(catalogue.text("campaign_no_unit_is_active"));
    }
    for active in &status.active_tasks {
        let model = active.model.as_ref().map_or_else(String::new, |model| {
            catalogue.format("campaign_active_model", &[("model", model)])
        });
        let pod = active.pod_id.as_ref().map_or_else(String::new, |pod| {
            catalogue.format("campaign_active_pod", &[("pod", pod)])
        });
        content::render(
            ui,
            &catalogue.format(
                "campaign_active_line",
                &[
                    ("task", &active.task_id),
                    ("state", &active.state),
                    ("actor", &active.actor),
                    ("model", &model),
                    ("round", &active.correction_round.to_string()),
                    ("heartbeat", &active.heartbeat_sequence.to_string()),
                    ("pod", &pod),
                ],
            ),
        );
    }
}

fn status_grid(
    ui: &mut egui::Ui,
    status: &crate::backend::models::CampaignStatus,
    catalogue: &Catalogue,
) {
    egui::Grid::new("campaign-status")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label(catalogue.text("campaign_phase"));
            ui.label(crate::campaign::campaign_state_label(&status.state));
            ui.end_row();
            ui.label(catalogue.text("campaign_actor"));
            ui.label(&status.actor);
            ui.end_row();
            ui.label(catalogue.text("campaign_model"));
            content::render(
                ui,
                status
                    .model
                    .as_deref()
                    .unwrap_or(catalogue.text("campaign_model_absent")),
            );
            ui.end_row();
            ui.label(catalogue.text("campaign_branch"));
            ui.monospace(&status.branch);
            ui.end_row();
            ui.label(catalogue.text("campaign_head"));
            ui.monospace(&status.head);
            ui.end_row();
            ui.label(catalogue.text("campaign_current_task"));
            ui.label(
                status
                    .current_task_id
                    .as_deref()
                    .unwrap_or(catalogue.text("campaign_code_absent")),
            );
            ui.end_row();
            ui.label(catalogue.text("campaign_last_task"));
            ui.label(
                status
                    .last_task_id
                    .as_deref()
                    .unwrap_or(catalogue.text("campaign_code_absent")),
            );
            ui.end_row();
            ui.label(catalogue.text("campaign_watchdog"));
            ui.label(&status.watchdog_state);
            ui.end_row();
            ui.label(catalogue.text("campaign_reconciliation"));
            ui.label(&status.reconciliation_state);
            ui.end_row();
            ui.label(catalogue.text("campaign_blocker_code"));
            ui.label(
                status
                    .blocker_code
                    .as_deref()
                    .unwrap_or(catalogue.text("campaign_code_absent")),
            );
            ui.end_row();
            ui.label(catalogue.text("campaign_elapsed"));
            ui.label(catalogue.format(
                "campaign_elapsed_value",
                &[("seconds", &(status.elapsed_ms / 1000).to_string())],
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_updated"));
            ui.label(catalogue.format(
                "campaign_updated_value",
                &[("timestamp", &status.updated_at_ms.to_string())],
            ));
            ui.end_row();
        });
}

fn holds_section(
    ui: &mut egui::Ui,
    status: &crate::backend::models::CampaignStatus,
    catalogue: &Catalogue,
) {
    ui.separator();
    ui.strong(catalogue.text("campaign_blocked_tasks"));
    if status.blockers.is_empty() {
        ui.label(catalogue.text("campaign_none"));
    }
    for blocker in &status.blockers {
        ui.monospace(catalogue.format(
            "campaign_reason_line",
            &[("task", &blocker.task_id), ("reason", &blocker.reason_code)],
        ));
    }
    ui.strong(catalogue.text("campaign_deferred_tasks"));
    if status.deferrals.is_empty() {
        ui.label(catalogue.text("campaign_none"));
    }
    for deferral in &status.deferrals {
        ui.monospace(catalogue.format(
            "campaign_deferral_line",
            &[
                ("task", &deferral.task_id),
                ("reason", &deferral.reason_code),
                ("trigger", &deferral.trigger_code),
                ("state", &deferral.trigger_state),
            ],
        ));
    }
    ui.strong(catalogue.text("campaign_human_decisions_required"));
    if status.human_decisions.is_empty() {
        ui.label(catalogue.text("campaign_none"));
    }
    for decision in &status.human_decisions {
        ui.monospace(catalogue.format(
            "campaign_reason_line",
            &[
                ("task", &decision.task_id),
                ("reason", &decision.reason_code),
            ],
        ));
    }
}

fn utilization_grid(
    ui: &mut egui::Ui,
    status: &crate::backend::models::CampaignStatus,
    catalogue: &Catalogue,
) {
    ui.strong(catalogue.text("campaign_utilization_against_limits"));
    let used = &status.utilization;
    let limits = &status.limits;
    egui::Grid::new("campaign-utilization")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label(catalogue.text("campaign_provider_attempts"));
            ui.label(usage_label(
                catalogue,
                &used.provider_attempts,
                &limits.provider_attempts,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_malformed_report_repairs"));
            ui.label(usage_label(
                catalogue,
                &used.malformed_report_repairs,
                &limits.malformed_report_repairs,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_correction_rounds"));
            ui.label(usage_label(
                catalogue,
                &used.correction_rounds,
                &limits.correction_rounds,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_process_invocations"));
            ui.label(usage_label(
                catalogue,
                &used.process_invocations,
                &limits.process_invocations,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_output_bytes"));
            ui.label(usage_label(
                catalogue,
                &used.output_bytes,
                &limits.output_bytes,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_retained_state_bytes"));
            ui.label(usage_label(
                catalogue,
                &used.retained_state_bytes,
                &limits.retained_state_bytes,
                false,
            ));
            ui.end_row();
            ui.label(catalogue.text("campaign_execution_time"));
            ui.label(usage_label(
                catalogue,
                &used.execution_elapsed_ms,
                &limits.execution_elapsed_ms,
                true,
            ));
            ui.end_row();
        });
    ui.small(catalogue.text("campaign_tokens_unknown"));
}

fn usage_label(
    catalogue: &Catalogue,
    used: &impl ToString,
    limit: &impl ToString,
    milliseconds: bool,
) -> String {
    catalogue.format(
        if milliseconds {
            "campaign_used_time"
        } else {
            "campaign_used_of_limit"
        },
        &[("used", &used.to_string()), ("limit", &limit.to_string())],
    )
}

fn roles_and_modes(
    ui: &mut egui::Ui,
    mission: &Observed<Option<MissionStatus>>,
    now: std::time::Instant,
    catalogue: &Catalogue,
) {
    ui.heading(catalogue.text("campaign_roles_this_backend_reports"));
    for (code, description) in SUPPORTED_ROLES {
        ui.label(catalogue.format(
            "campaign_roles_line",
            &[("code", code), ("description", description)],
        ));
    }
    ui.heading(catalogue.text("campaign_owner_involvement"));
    let freshness = mission.freshness(now);
    ui.small(catalogue.format(
        "campaign_mission_observation",
        &[
            ("freshness", freshness.label()),
            ("age", &age_label(mission.age(now))),
        ],
    ));
    if let Some((_, error)) = &mission.last_error {
        let (what, action) = explain_code(&error.code());
        failure_box(ui, what, &error.to_string(), action);
    }
    if freshness == Freshness::Stale {
        ui.colored_label(
            super::current_tokens(ui.ctx()).warning,
            catalogue.text("campaign_mission_stale"),
        );
    }
    match &mission.value {
        Some(Some(status)) => {
            let mode = involvement_label(&status.involvement);
            let authority = mission_authority(catalogue, status, freshness);
            ui.label(catalogue.format(
                "campaign_mission_summary",
                &[
                    ("id", &status.mission_id),
                    ("generation", &status.generation.to_string()),
                    ("mode", &mode),
                    ("authority", authority),
                ],
            ));
            egui::Grid::new("mission-status")
                .num_columns(2)
                .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
                .show(ui, |ui| {
                    ui.label(catalogue.text("campaign_involvement"));
                    ui.label(mode);
                    ui.end_row();
                    ui.label(catalogue.text("campaign_revocation_epoch"));
                    ui.label(status.revocation_epoch.to_string());
                    ui.end_row();
                    ui.label(catalogue.text("campaign_expires_unix_ms"));
                    ui.label(status.expires_at_ms.to_string());
                    ui.end_row();
                    ui.label(catalogue.text("campaign_decisions_recorded"));
                    ui.label(catalogue.format(
                        "campaign_mission_decisions",
                        &[
                            ("recorded", &status.decisions_recorded.to_string()),
                            ("permitted", &status.permitted_choices.to_string()),
                            ("held", &status.held_decisions.to_string()),
                        ],
                    ));
                    ui.end_row();
                    ui.label(catalogue.text("campaign_pending_owner_decisions"));
                    ui.label(status.pending_owner_decisions.to_string());
                    ui.end_row();
                    ui.label(catalogue.text("campaign_owner_answers"));
                    ui.label(status.owner_answers.to_string());
                    ui.end_row();
                    ui.label(catalogue.text("campaign_charter_digest"));
                    ui.label(&status.mission_sha256);
                    ui.end_row();
                });
            ui.small(catalogue.format(
                "campaign_mission_age",
                &[("age", &age_label(mission.age(now)))],
            ));
        }
        Some(None) => {
            ui.label(catalogue.text("campaign_no_mission"));
            for (_, label, description) in INVOLVEMENT_MODES {
                ui.label(catalogue.format(
                    "campaign_mode_unavailable",
                    &[("label", label), ("description", description)],
                ));
            }
        }
        None => {
            if mission.loading {
                ui.label(catalogue.text("campaign_mission_loading"));
            } else if mission.last_error.is_none() {
                ui.label(catalogue.text("campaign_mission_authority_not_yet_observed"));
            }
        }
    }
}

fn mission_authority<'a>(
    catalogue: &'a Catalogue,
    status: &MissionStatus,
    freshness: Freshness,
) -> &'a str {
    if status.revoked {
        catalogue.text("campaign_mission_revoked")
    } else if status.expired {
        catalogue.text("campaign_mission_expired")
    } else if freshness == Freshness::Live {
        catalogue.text("campaign_mission_current")
    } else {
        catalogue.text("campaign_mission_unverified")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use std::path::PathBuf;

    struct CampaignPreview {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for CampaignPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                if self.right_to_left {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        self.app.campaign_screen_with_catalogue(ui, &self.catalogue);
                    });
                } else {
                    self.app.campaign_screen_with_catalogue(ui, &self.catalogue);
                }
            });
        }
    }

    #[test]
    fn campaign_empty_state_has_labelled_recovery_at_minimum_window_with_pseudo_text() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let title = catalogue.text("campaign_title").to_owned();
            let guidance = catalogue.text("campaign_no_project").to_owned();
            let setup = catalogue.text("campaign_open_setup").to_owned();
            let help = catalogue.text("campaign_open_help").to_owned();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |creation| CampaignPreview {
                    app: App::with_state_dir(
                        &creation.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("/example/missing/codingmage"),
                        }),
                        Ok(std::env::temp_dir().join("codingmage-ui-campaign-preview")),
                    ),
                    catalogue,
                    right_to_left,
                });
            harness.run_steps(2);
            for label in [&title, &guidance, &setup, &help] {
                assert!(
                    harness
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds()
                );
            }
            harness
                .get_by_role_and_label(egui::accesskit::Role::Button, &setup)
                .click();
            harness.run_steps(1);
            assert_eq!(harness.state().app.screen, Screen::Setup);
        }
    }
}
