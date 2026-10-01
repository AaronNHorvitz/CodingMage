//! Readiness: local checks plus the coordinator's `campaign-preflight` result.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use super::{App, backend_failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response, models::parse_preflight},
    messages::{self, Catalogue},
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
        let Some(arguments) = self.preflight_arguments() else {
            self.set_status("select a campaign, authorization record and coordinator with paths that can be shown exactly before preflight");
            return;
        };
        if !crate::command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.set_status(
                "preflight cannot run because the exact coordinator command cannot be shown safely",
            );
            return;
        }
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

    fn preflight_arguments(&self) -> Option<Vec<String>> {
        let (project, campaign, record) = (
            self.project.as_ref()?,
            self.campaign.as_ref()?,
            self.authorization_record.as_ref()?,
        );
        let arguments = vec![
            "campaign-preflight".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            campaign.spec_path.to_str()?.to_owned(),
            "--authorization".to_owned(),
            record.to_str()?.to_owned(),
        ];
        Some(arguments)
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
        self.readiness_section_with_catalogue(ui, messages::english());
    }

    fn readiness_section_with_catalogue(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("readiness_title"));
        if self.project.is_none() || self.campaign.is_none() {
            ui.label(catalogue.text("readiness_no_campaign"));
            ui.add_enabled(
                false,
                egui::Button::new(catalogue.text("readiness_run_preflight")),
            );
            crate::command::show_unavailable_for(ui, catalogue.text("readiness_run_preflight"));
            return;
        }
        ui.horizontal(|ui| {
            let label = ui.label(catalogue.text("readiness_authorization"));
            ui.add(
                egui::TextEdit::singleline(&mut self.authorization_input)
                    .hint_text(catalogue.text("readiness_authorization_hint"))
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
            if ui.button(catalogue.text("readiness_use_record")).clicked() {
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
        ui.label(catalogue.format(
            "readiness_local_checks",
            &[
                ("passed", &(checks.len() - failing - unknown).to_string()),
                ("failed", &failing.to_string()),
                ("unknown", &unknown.to_string()),
            ],
        ));
        for check in &checks {
            let color = match check.status {
                CheckStatus::Pass => super::current_tokens(ui.ctx()).success,
                CheckStatus::Fail => super::current_tokens(ui.ctx()).error,
                CheckStatus::Unknown => super::current_tokens(ui.ctx()).warning,
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, format!("{}: {}", check.name, check.status.label()));
                ui.label(&check.detail);
                if check.status != CheckStatus::Pass && !check.action.is_empty() {
                    ui.small(check.action);
                }
            });
        }
        let preflight_command = self.preflight_arguments();
        let can_run =
            crate::command::can_preview(self.binary_path.as_deref(), preflight_command.as_deref())
                && !self.preflight.loading;
        if ui
            .add_enabled(
                can_run,
                egui::Button::new(catalogue.text("readiness_run_preflight")),
            )
            .clicked()
        {
            self.run_preflight();
        }
        if let Some(arguments) = preflight_command {
            crate::command::show_for(
                ui,
                catalogue.text("readiness_run_preflight"),
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            crate::command::show_unavailable_for(ui, catalogue.text("readiness_run_preflight"));
            if self.authorization_record.is_some() && self.campaign.is_some() {
                ui.small(catalogue.text("readiness_command_unavailable"));
            }
        }
        self.preflight_result(ui, catalogue);
    }

    fn preflight_result(&self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let freshness = self.preflight.freshness(self.now);
        ui.label(catalogue.format(
            "readiness_observation",
            &[
                ("freshness", freshness.label()),
                ("age", &age_label(self.preflight.age(self.now))),
            ],
        ));
        self.preflight_progress(ui, freshness, catalogue);
        let Some(observation) = &self.preflight.value else {
            return;
        };
        let report = &observation.report;
        ui.strong(catalogue.format(
            "readiness_report_state",
            &[
                ("state", &report.state),
                ("digest", &observation.report_sha256),
            ],
        ));
        if ui.button(catalogue.text("readiness_copy_digest")).clicked() {
            ui.ctx().copy_text(observation.report_sha256.clone());
        }
        render_preflight_report(ui, catalogue, report);
    }

    fn preflight_progress(&self, ui: &mut egui::Ui, freshness: Freshness, catalogue: &Catalogue) {
        if let Some((_, error)) = &self.preflight.last_error {
            let effect = if self.preflight.value.is_some() {
                "failure_preflight_retained"
            } else {
                "failure_preflight_no_observation"
            };
            backend_failure_box(ui, error, effect);
        }
        if self.preflight.loading {
            ui.label(catalogue.text(if self.preflight.value.is_some() {
                "readiness_refreshing"
            } else {
                "readiness_probing"
            }));
        }
        if freshness == Freshness::Stale && self.preflight.last_error.is_none() {
            ui.label(catalogue.text("readiness_stale"));
        }
    }
}

fn render_preflight_report(
    ui: &mut egui::Ui,
    catalogue: &Catalogue,
    report: &crate::backend::models::PreflightReport,
) {
    egui::Grid::new("preflight-grid")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
        .show(ui, |ui| {
            ui.label(catalogue.text("readiness_authority_digest"));
            ui.monospace(&report.authority_sha256);
            ui.end_row();
            ui.label(catalogue.text("readiness_authorization_digest"));
            ui.monospace(&report.operator_authorization_sha256);
            ui.end_row();
            ui.label(catalogue.text("readiness_repository"));
            ui.label(catalogue.format(
                "readiness_repository_detail",
                &[
                    ("id", &report.repository.repository_id),
                    (
                        "head",
                        &report.repository.initial_commit
                            [..12.min(report.repository.initial_commit.len())],
                    ),
                    ("clean", &report.repository.clean.to_string()),
                    ("branch", &report.repository.dedicated_branch.to_string()),
                    ("checkout", &report.repository.checkout_safe.to_string()),
                    ("items", &report.repository.plan_item_count.to_string()),
                    ("open", &report.repository.open_subtask_count.to_string()),
                ],
            ));
            ui.end_row();
            ui.label(catalogue.text("readiness_policy"));
            ui.label(catalogue.format(
                "readiness_policy_detail",
                &[
                    ("pods", &report.policy.max_parallel_pods.to_string()),
                    ("outcomes", &report.policy.max_accepted_outcomes.to_string()),
                    ("publication", &report.policy.publication),
                    (
                        "protected",
                        &report.policy.default_branch_protected.to_string(),
                    ),
                    (
                        "denied",
                        &report.policy.external_capabilities_denied.to_string(),
                    ),
                ],
            ));
            ui.end_row();
            ui.label(catalogue.text("readiness_providers"));
            ui.label(
                report
                    .providers
                    .iter()
                    .map(|provider| preflight_provider_summary(catalogue, provider))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            ui.end_row();
            ui.label(catalogue.text("readiness_gates"));
            ui.label(catalogue.format(
                "readiness_gates_detail",
                &[
                    ("commands", &report.gates.command_count.to_string()),
                    ("tiers", &report.gates.tier_count.to_string()),
                ],
            ));
            ui.end_row();
            ui.label(catalogue.text("readiness_controls"));
            ui.label(catalogue.format(
                "readiness_controls_detail",
                &[
                    ("count", &report.controls.operator_control_count.to_string()),
                    ("guard", &report.controls.process_guard_verified.to_string()),
                ],
            ));
            ui.end_row();
            ui.label(catalogue.text("readiness_storage"));
            ui.label(catalogue.format(
                "readiness_storage_detail",
                &[
                    ("sufficient", &report.storage.sufficient.to_string()),
                    ("scratch", &report.storage.scratch_sufficient.to_string()),
                    ("state", &report.storage.state_sufficient.to_string()),
                    (
                        "bytes",
                        &report.storage.required_available_bytes.to_string(),
                    ),
                ],
            ));
            ui.end_row();
            ui.label(catalogue.text("readiness_source_free"));
            ui.label(report.source_free.to_string());
            ui.end_row();
        });
}

fn preflight_provider_summary(
    catalogue: &Catalogue,
    provider: &crate::backend::models::PreflightProvider,
) -> String {
    catalogue.format(
        "readiness_provider_detail",
        &[
            ("role", &provider.role),
            ("verified", &provider.capability_verified.to_string()),
            ("probes", &provider.probe_process_count.to_string()),
            ("authentication", &provider.authentication),
        ],
    )
}

#[cfg(test)]
mod catalogue_tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use std::time::Instant;

    struct ReadinessPreview {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for ReadinessPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                if self.right_to_left {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        self.app
                            .readiness_section_with_catalogue(ui, &self.catalogue);
                    });
                } else {
                    self.app
                        .readiness_section_with_catalogue(ui, &self.catalogue);
                }
            });
        }
    }

    #[test]
    fn readiness_panel_selection_copy() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let empty = catalogue.text("readiness_no_campaign").to_owned();
            let action = catalogue.text("readiness_run_preflight").to_owned();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |ctx| ReadinessPreview {
                    app: App::with_state_dir(
                        &ctx.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("missing-coordinator"),
                        }),
                        Ok(std::env::temp_dir()),
                    ),
                    catalogue,
                    right_to_left,
                });
            harness.run_steps(2);
            assert!(harness.get_by_label(&empty).accesskit_node().has_bounds());
            assert!(harness.get_by_label(&action).accesskit_node().has_bounds());
            assert!(
                harness
                    .get_by_label_contains("Show command:")
                    .accesskit_node()
                    .has_bounds()
            );
            assert!(
                harness
                    .query_by_label_contains("Local checks: 0 pass")
                    .is_none()
            );
            assert_eq!(
                harness.state().app.preflight.freshness(Instant::now()),
                Freshness::NotRequested
            );
        }
    }
}
