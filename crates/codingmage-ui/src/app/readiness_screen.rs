//! Readiness: local checks plus the coordinator's `campaign-preflight` result.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use super::{App, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response, explain_code, models::parse_preflight},
    observed::{Freshness, age_label},
    readiness::{Check, CheckStatus, PreflightObservation, ReadinessInput, evaluate},
    state_dir::ProjectMemory,
};

/// Deadline for `campaign-preflight`, which probes provider version and help surfaces.
pub const PREFLIGHT_DEADLINE: Duration = Duration::from_mins(5);

impl App {
    /// Selected authorization record.
    #[must_use]
    pub fn authorization_record(&self) -> Option<&Path> {
        self.authorization_record.as_deref()
    }

    /// Latest preflight observation.
    #[must_use]
    pub const fn preflight(&self) -> &crate::observed::Observed<PreflightObservation> {
        &self.preflight
    }

    /// Selects the operator authorization record for preflight and remembers it.
    pub fn set_authorization_record(&mut self, record: &Path) {
        if !record.is_absolute() {
            self.set_status("the authorization record path must be absolute");
            return;
        }
        self.authorization_record = Some(record.to_path_buf());
        self.authorization_input = record.display().to_string();
        self.preflight.clear();
        self.persist_campaign_memory();
    }

    pub(super) fn persist_campaign_memory(&self) {
        if let (Ok(directory), Some(project)) = (&self.state_dir, &self.project) {
            let memory = ProjectMemory {
                version: 1,
                campaign_spec: self
                    .campaign
                    .as_ref()
                    .map(|campaign| campaign.spec_path.clone()),
                authorization_record: self.authorization_record.clone(),
            };
            let _ = memory.save(directory, &project.config_path);
        }
    }

    /// Local readiness checks for the selected campaign.
    #[must_use]
    pub fn readiness_checks(&self) -> Vec<Check> {
        let (Some(project), Some(campaign)) = (&self.project, &self.campaign) else {
            return Vec::new();
        };
        evaluate(&ReadinessInput {
            config: &project.config,
            spec: &campaign.spec,
            diagnosis: self.diagnosis.value.as_ref(),
            plan_counts: self
                .plan_index
                .as_ref()
                .map(crate::workplan::PlanIndex::counts),
            authorization_record: self.authorization_record.as_deref(),
        })
    }

    /// Requests `campaign-preflight` for the selected campaign and authorization record.
    pub fn run_preflight(&mut self) {
        let (Some(project), Some(campaign), Some(record)) =
            (&self.project, &self.campaign, &self.authorization_record)
        else {
            self.set_status("select a campaign and an authorization record before preflight");
            return;
        };
        let arguments = vec![
            "campaign-preflight".to_owned(),
            "--config".to_owned(),
            project.config_path.display().to_string(),
            "--campaign".to_owned(),
            campaign.spec_path.display().to_string(),
            "--authorization".to_owned(),
            record.display().to_string(),
        ];
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-preflight",
                arguments,
                deadline: PREFLIGHT_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => {
                self.preflight.loading = true;
                self.set_status("preflight requested; provider probes may take a moment");
            }
            Err(error) => self.preflight.fail(error, self.now),
        }
    }

    pub(super) fn accept_preflight(&mut self, response: Response) {
        match response.result {
            Ok(bytes) => match parse_preflight(&bytes) {
                Ok(report) => {
                    let observation = PreflightObservation::new(report, bytes);
                    self.set_status(format!(
                        "preflight {} (report sha256 {})",
                        observation.report.state,
                        &observation.report_sha256[..12]
                    ));
                    self.preflight
                        .accept(observation, response.generation, self.now);
                }
                Err(error) => {
                    self.set_status("preflight output did not match the contract");
                    self.preflight.fail(BackendError::from(error), self.now);
                }
            },
            Err(error) => {
                self.set_status(format!("preflight failed: {}", error.code()));
                self.preflight.fail(error, self.now);
            }
        }
    }

    pub(super) fn readiness_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Readiness");
        ui.horizontal(|ui| {
            let label = ui.label("Authorization record");
            ui.add(
                egui::TextEdit::singleline(&mut self.authorization_input)
                    .hint_text("/absolute/path/operator-authorization.txt")
                    .desired_width(420.0),
            )
            .labelled_by(label.id);
            if ui.button("Use record").clicked() {
                let path = PathBuf::from(self.authorization_input.trim());
                self.set_authorization_record(&path);
            }
        });
        let checks = self.readiness_checks();
        let failing = checks
            .iter()
            .filter(|check| check.status == CheckStatus::Fail)
            .count();
        let unknown = checks
            .iter()
            .filter(|check| check.status == CheckStatus::Unknown)
            .count();
        ui.label(format!(
            "Local checks: {} pass, {failing} fail, {unknown} unknown. Preflight is the authority; these checks explain its stable codes.",
            checks.len() - failing - unknown
        ));
        for check in &checks {
            let color = match check.status {
                CheckStatus::Pass => egui::Color32::from_rgb(120, 200, 120),
                CheckStatus::Fail => egui::Color32::from_rgb(255, 150, 150),
                CheckStatus::Unknown => egui::Color32::from_rgb(200, 200, 120),
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, format!("{}: {}", check.name, check.status.label()));
                ui.label(&check.detail);
                if check.status != CheckStatus::Pass && !check.action.is_empty() {
                    ui.small(check.action);
                }
            });
        }
        let can_run = self.authorization_record.is_some() && !self.preflight.loading;
        if ui
            .add_enabled(can_run, egui::Button::new("Run preflight"))
            .clicked()
        {
            self.run_preflight();
        }
        self.preflight_result(ui);
    }

    fn preflight_result(&self, ui: &mut egui::Ui) {
        let freshness = self.preflight.freshness(self.now);
        ui.label(format!(
            "Preflight: {} ({})",
            freshness.label(),
            age_label(self.preflight.age(self.now))
        ));
        if let Some((_, error)) = &self.preflight.last_error {
            let (what, action) = explain_code(&error.code());
            failure_box(ui, what, &error.to_string(), action);
            ui.small("Preflight probes version and help surfaces only; it never starts model inference, and a failure leaves no campaign state behind.");
        }
        if freshness == Freshness::Loading {
            ui.label("Probing providers, gates, guard and storage through the coordinator...");
        }
        let Some(observation) = &self.preflight.value else {
            return;
        };
        let report = &observation.report;
        ui.strong(format!(
            "Preflight state {} - report sha256 {}",
            report.state, observation.report_sha256
        ));
        if ui.button("Copy report digest").clicked() {
            ui.ctx().copy_text(observation.report_sha256.clone());
        }
        egui::Grid::new("preflight-grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("Authority sha256");
                ui.monospace(&report.authority_sha256);
                ui.end_row();
                ui.label("Authorization sha256");
                ui.monospace(&report.operator_authorization_sha256);
                ui.end_row();
                ui.label("Repository");
                ui.label(format!(
                    "{} at {} - clean {}, dedicated branch {}, checkout safe {}, {} plan items, {} open sub-tasks",
                    report.repository.repository_id,
                    &report.repository.initial_commit[..12.min(report.repository.initial_commit.len())],
                    report.repository.clean,
                    report.repository.dedicated_branch,
                    report.repository.checkout_safe,
                    report.repository.plan_item_count,
                    report.repository.open_subtask_count
                ));
                ui.end_row();
                ui.label("Policy");
                ui.label(format!(
                    "{} pod(s), {} accepted outcomes, publication {}, default branch protected {}, external capabilities denied {}",
                    report.policy.max_parallel_pods,
                    report.policy.max_accepted_outcomes,
                    report.policy.publication,
                    report.policy.default_branch_protected,
                    report.policy.external_capabilities_denied
                ));
                ui.end_row();
                ui.label("Providers");
                ui.label(
                    report
                        .providers
                        .iter()
                        .map(|provider| {
                            format!(
                                "{}: capability verified {} ({} probes, {})",
                                provider.role,
                                provider.capability_verified,
                                provider.probe_process_count,
                                provider.authentication
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; "),
                );
                ui.end_row();
                ui.label("Gates");
                ui.label(format!(
                    "{} commands, {} tiers",
                    report.gates.command_count, report.gates.tier_count
                ));
                ui.end_row();
                ui.label("Controls");
                ui.label(format!(
                    "{} operator controls, process guard verified {}",
                    report.controls.operator_control_count, report.controls.process_guard_verified
                ));
                ui.end_row();
                ui.label("Storage");
                ui.label(format!(
                    "sufficient {} (scratch {}, state {}, required {} bytes)",
                    report.storage.sufficient,
                    report.storage.scratch_sufficient,
                    report.storage.state_sufficient,
                    report.storage.required_available_bytes
                ));
                ui.end_row();
                ui.label("Source free");
                ui.label(report.source_free.to_string());
                ui.end_row();
            });
    }
}
