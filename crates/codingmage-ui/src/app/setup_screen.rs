//! Guided setup: repository configuration, campaign authority and the authorization record.

use std::path::{Path, PathBuf};

use codingmage_campaign::CampaignAuthentication;
use codingmage_core::{CapabilityGrant, PublicationMode};

use super::{App, failure_box};
use crate::{
    browser::Browser,
    setup::{
        CampaignBinding, CampaignForm, ConfigForm, EFFORTS, GateForm, ProfileForm, ProviderForm,
        WriteError, export_copy, write_authorization_record,
    },
};

/// Setup screen state kept on the application.
#[derive(Default)]
pub struct SetupState {
    /// Configuration form when editing or creating.
    pub config_form: Option<ConfigForm>,
    /// Campaign form when authoring.
    pub campaign_form: Option<CampaignForm>,
    /// Last outcome message: `Ok` for success, `Err` for a refusal.
    pub message: Option<Result<String, String>>,
    /// Text of the authorization record being created.
    pub authorization_text: String,
    /// Destination for the authorization record.
    pub authorization_path: String,
    /// Replace an existing authorization record.
    pub authorization_overwrite: bool,
    /// Directory browser for the target repository.
    pub target_browser: Option<Browser>,
    /// Export destination.
    pub export_path: String,
    /// Replace an existing export destination.
    pub export_overwrite: bool,
    /// Workspace directory proposed for new authority files.
    pub workspace_root: String,
}

impl App {
    /// Binding values for campaign authoring, taken from the diagnosis and configuration.
    #[must_use]
    pub fn campaign_binding(&self) -> Option<CampaignBinding> {
        let project = self.project.as_ref()?;
        let diagnosis = self.diagnosis.value.as_ref()?;
        Some(CampaignBinding {
            repository_id: diagnosis.repository_id.clone(),
            repository_path: project.config.target_path.clone(),
            initial_commit: diagnosis.head.clone(),
            task_source_sha256: diagnosis.task_source_sha256.clone(),
            default_branch: project.config.default_branch.clone(),
        })
    }

    /// Setup state.
    #[must_use]
    pub const fn setup_state(&self) -> &SetupState {
        &self.setup
    }

    /// Mutable setup state (used by tests to fill forms).
    pub const fn setup_state_mut(&mut self) -> &mut SetupState {
        &mut self.setup
    }

    /// Starts a fresh configuration form for a target directory.
    pub fn start_config_form(&mut self, target: &Path) {
        let workspace = self.workspace_root_for(target);
        self.setup.config_form = Some(ConfigForm::defaults(target, &workspace));
        self.setup.message = None;
    }

    /// Starts editing the opened configuration.
    pub fn edit_opened_config(&mut self) {
        if let Some(project) = &self.project {
            let mut form = ConfigForm::from_config(&project.config_path, &project.config);
            form.overwrite = true;
            self.setup.config_form = Some(form);
            self.setup.message = None;
        }
    }

    /// Writes the configuration form and opens the result.
    pub fn apply_config_form(&mut self) {
        let Some(form) = &self.setup.config_form else {
            return;
        };
        match form.write() {
            Ok(path) => {
                self.setup.message = Some(Ok(format!(
                    "configuration written and validated at {}",
                    path.display()
                )));
                self.open_project(&path);
            }
            Err(error) => self.setup.message = Some(Err(error.to_string())),
        }
    }

    /// Starts a campaign form for the opened repository.
    pub fn start_campaign_form(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let workspace = self.workspace_root_for(&project.config.target_path);
        let default_branch = project.config.default_branch.clone();
        let mut form = CampaignForm::defaults(&workspace, &default_branch);
        if let Some(campaign) = &self.campaign {
            form = CampaignForm::from_spec(&campaign.spec_path, &campaign.spec, None);
            form.overwrite = true;
        }
        self.setup
            .authorization_path
            .clone_from(&form.authorization_path);
        self.setup.campaign_form = Some(form);
        self.setup.message = None;
    }

    /// Writes the campaign form and selects the result.
    pub fn apply_campaign_form(&mut self) {
        let Some(binding) = self.campaign_binding() else {
            self.setup.message = Some(Err(
                "a live repository diagnosis is required before authoring a campaign".to_owned(),
            ));
            return;
        };
        let Some(form) = &self.setup.campaign_form else {
            return;
        };
        match form.write(&binding) {
            Ok(path) => {
                self.setup.message = Some(Ok(format!(
                    "campaign specification written and verified at {}",
                    path.display()
                )));
                let record = PathBuf::from(form.authorization_path.trim());
                self.select_campaign(&path);
                self.set_authorization_record(&record);
            }
            Err(error) => self.setup.message = Some(Err(error.to_string())),
        }
    }

    /// Writes the authorization record from the typed text.
    pub fn apply_authorization_record(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let repository = project.config.target_path.clone();
        let path = PathBuf::from(self.setup.authorization_path.trim());
        match write_authorization_record(
            &path,
            &self.setup.authorization_text,
            &repository,
            self.setup.authorization_overwrite,
        ) {
            Ok(digest) => {
                self.setup.message = Some(Ok(format!(
                    "authorization record written at {} (sha256 {digest})",
                    path.display()
                )));
                if let Some(form) = &mut self.setup.campaign_form {
                    form.authorization_path = path.display().to_string();
                }
                self.setup.authorization_text.clear();
                self.set_authorization_record(&path);
            }
            Err(error) => self.setup.message = Some(Err(error.to_string())),
        }
    }

    /// Exports the opened configuration or selected campaign to another path.
    pub fn export_document(&mut self, source: &Path) {
        let destination = PathBuf::from(self.setup.export_path.trim());
        let inside_repository = self
            .project
            .as_ref()
            .is_some_and(|project| destination.starts_with(&project.config.target_path));
        let result = if inside_repository {
            Err(WriteError::InsideRepository(destination))
        } else {
            export_copy(source, &destination, self.setup.export_overwrite)
        };
        self.setup.message = Some(match result {
            Ok(path) => Ok(format!("exported to {}", path.display())),
            Err(error) => Err(error.to_string()),
        });
    }

    fn workspace_root_for(&self, target: &Path) -> PathBuf {
        let typed = self.setup.workspace_root.trim();
        if Path::new(typed).is_absolute() {
            return PathBuf::from(typed);
        }
        let name = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository");
        target
            .parent()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
            .join(format!("{name}-codingmage"))
    }

    pub(super) fn setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Setup");
        ui.label("Everything here goes through the existing configuration and campaign contracts. Files are written only after the same loaders the coordinator uses accept them. No credential is ever requested or stored; providers use their own existing logins.");
        if let Some(message) = &self.setup.message {
            match message {
                Ok(text) => {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 120), text);
                }
                Err(text) => failure_box(ui, "Refused", text, "Correct the field and try again."),
            }
        }
        ui.separator();
        ui.heading("1. Open or create a repository configuration");
        self.open_controls(ui);
        ui.horizontal(|ui| {
            ui.label("Workspace directory for new authority files (optional)");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.workspace_root)
                    .hint_text("/absolute/path outside the repository")
                    .desired_width(360.0),
            );
        });
        self.target_selection(ui);
        if self.project.is_some() && ui.button("Edit the opened configuration").clicked() {
            self.edit_opened_config();
        }
        self.config_form_view(ui);
        ui.separator();
        ui.heading("2. Record the owner's authorization");
        self.authorization_view(ui);
        ui.separator();
        ui.heading("3. Author the campaign authority");
        self.campaign_form_view(ui);
        ui.separator();
        ui.heading("4. Import and export");
        self.export_view(ui);
    }

    fn target_selection(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.setup.target_browser.is_some() {
                    "Hide repository browser"
                } else {
                    "Choose a repository directory"
                })
                .clicked()
            {
                self.setup.target_browser = match self.setup.target_browser.take() {
                    Some(_) => None,
                    None => Some(Browser::at_home(vec!["never-match"])),
                };
            }
        });
        let mut chosen = None;
        let mut enter = None;
        let mut up = false;
        if let Some(browser) = &self.setup.target_browser {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Up").clicked() {
                        up = true;
                    }
                    ui.monospace(browser.current.display().to_string());
                    if ui.button("Use this directory as the target").clicked() {
                        chosen = Some(browser.current.clone());
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("target-browser")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for entry in browser.entries.iter().filter(|entry| entry.is_dir) {
                            if ui
                                .add_enabled(
                                    !entry.is_symlink,
                                    egui::Button::new(format!("{}/", entry.name)),
                                )
                                .clicked()
                            {
                                enter = Some(entry.path.clone());
                            }
                        }
                    });
            });
        }
        if let Some(browser) = &mut self.setup.target_browser {
            if up {
                browser.up();
            }
            if let Some(path) = enter {
                browser.enter(&path);
            }
        }
        if let Some(target) = chosen {
            self.start_config_form(&target);
        }
    }

    fn config_form_view(&mut self, ui: &mut egui::Ui) {
        let Some(form) = &mut self.setup.config_form else {
            return;
        };
        let (apply, discard) = config_form_body(ui, form);
        if apply {
            self.apply_config_form();
        }
        if discard {
            self.setup.config_form = None;
        }
    }

    fn authorization_view(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            ui.label("Open a repository first.");
            return;
        }
        ui.label("Type the owner's authorization in your own words. The exact bytes are written to a file outside the repository and their digest is bound into the campaign authority. The interface never invents this record.");
        ui.horizontal(|ui| {
            ui.label("Record file");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.authorization_path)
                    .hint_text("/absolute/path/operator-authorization.txt")
                    .desired_width(420.0),
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.setup.authorization_text)
                .hint_text(
                    "I authorize campaign ... on repository ... with local-only publication.",
                )
                .desired_rows(3)
                .desired_width(600.0),
        );
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.setup.authorization_overwrite,
                "Replace an existing record",
            );
            if ui.button("Write authorization record").clicked() {
                self.apply_authorization_record();
            }
        });
    }

    fn campaign_form_view(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            ui.label("Open a repository first.");
            return;
        }
        match self.campaign_binding() {
            Some(binding) => {
                ui.small(format!(
                    "Bound from the live diagnosis: repository {} at head {} with task-source digest {}...",
                    binding.repository_id,
                    &binding.initial_commit[..12.min(binding.initial_commit.len())],
                    &binding.task_source_sha256[..12.min(binding.task_source_sha256.len())]
                ));
            }
            None => {
                ui.label("A live repository diagnosis is required; refresh the overview first.");
            }
        }
        if self.setup.campaign_form.is_none() {
            if ui.button("Start a campaign specification").clicked() {
                self.start_campaign_form();
            }
            return;
        }
        let (apply, discard) = match &mut self.setup.campaign_form {
            Some(form) => campaign_form_body(ui, form),
            None => (false, false),
        };
        if apply {
            self.apply_campaign_form();
        }
        if discard {
            self.setup.campaign_form = None;
        }
    }

    fn export_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Import means opening an existing configuration or selecting an existing campaign above. Export copies the validated file elsewhere; digests and paths inside it are unchanged and no credential exists to leak.");
        ui.horizontal(|ui| {
            ui.label("Export destination");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.export_path)
                    .hint_text("/absolute/path/copy.toml")
                    .desired_width(420.0),
            );
            ui.checkbox(&mut self.setup.export_overwrite, "Replace");
        });
        let config_source = self
            .project
            .as_ref()
            .map(|project| project.config_path.clone());
        let campaign_source = self
            .campaign
            .as_ref()
            .map(|campaign| campaign.spec_path.clone());
        ui.horizontal(|ui| {
            if let Some(source) = config_source
                && ui.button("Export configuration").clicked()
            {
                self.export_document(&source);
            }
            if let Some(source) = campaign_source
                && ui.button("Export campaign").clicked()
            {
                self.export_document(&source);
            }
        });
    }
}

fn config_form_body(ui: &mut egui::Ui, form: &mut ConfigForm) -> (bool, bool) {
    let mut apply = false;
    let mut discard = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Repository configuration (version 1, deny-first)");
        egui::Grid::new("config-form")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("Configuration file", &mut form.config_path),
                    ("Target repository", &mut form.target_path),
                    ("Task source (relative)", &mut form.task_source),
                    ("Default branch", &mut form.default_branch),
                    ("Integration branch prefix", &mut form.integration_branch),
                    ("Scratch root", &mut form.scratch_root),
                    ("State root", &mut form.state_root),
                    ("Correction limit", &mut form.correction_limit),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(480.0));
                    ui.end_row();
                }
            });
        ui.label("Agent profiles (identifier, provider, model passed to the provider unchanged)");
        let mut remove_profile = None;
        for (index, profile) in form.profiles.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut profile.id).desired_width(160.0));
                ui.add(egui::TextEdit::singleline(&mut profile.provider).desired_width(100.0));
                ui.add(egui::TextEdit::singleline(&mut profile.model).desired_width(200.0));
                if ui.button("Remove profile").clicked() {
                    remove_profile = Some(index);
                }
            });
        }
        if let Some(index) = remove_profile {
            form.profiles.remove(index);
        }
        if ui.button("Add profile").clicked() {
            form.profiles.push(ProfileForm::default());
        }
        ui.label("Deterministic gate commands (absolute executable, literal arguments)");
        let mut remove_gate = None;
        for (index, gate) in form.gates.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut gate.executable).desired_width(260.0));
                ui.add(egui::TextEdit::singleline(&mut gate.arguments).desired_width(260.0));
                if ui.button("Remove gate").clicked() {
                    remove_gate = Some(index);
                }
            });
        }
        if let Some(index) = remove_gate {
            form.gates.remove(index);
        }
        if ui.button("Add gate").clicked() {
            form.gates.push(GateForm::default());
        }
        ui.horizontal(|ui| {
            ui.label("Publication");
            ui.selectable_value(&mut form.publication, PublicationMode::LocalOnly, "local only");
            ui.selectable_value(
                &mut form.publication,
                PublicationMode::PushFeatureBranch,
                "push feature branch",
            );
            ui.selectable_value(
                &mut form.publication,
                PublicationMode::DraftPullRequest,
                "draft pull request",
            );
        });
        ui.small("Publication authority is separate from execution. Anything beyond local only also needs the matching capability grants below and separately qualified GitHub authority.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Capabilities");
            capability_toggle(ui, "network", &mut form.capabilities.network);
            capability_toggle(ui, "push", &mut form.capabilities.push);
            capability_toggle(ui, "issues", &mut form.capabilities.issues);
            capability_toggle(ui, "pull requests", &mut form.capabilities.pull_requests);
            capability_toggle(ui, "task merge", &mut form.capabilities.task_merge);
            capability_toggle(
                ui,
                "destination merge",
                &mut form.capabilities.destination_merge,
            );
        });
        ui.checkbox(&mut form.allow_parent_discovery, "Allow parent discovery");
        ui.checkbox(&mut form.overwrite, "Replace the existing configuration file");
        ui.horizontal(|ui| {
            if ui.button("Validate and write configuration").clicked() {
                apply = true;
            }
            if ui.button("Discard form").clicked() {
                discard = true;
            }
        });

    });
    (apply, discard)
}

fn campaign_form_body(ui: &mut egui::Ui, form: &mut CampaignForm) -> (bool, bool) {
    let mut apply = false;
    let mut discard = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Campaign authority (version 3, serial, local only)");
        egui::Grid::new("campaign-form")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("Specification file", &mut form.spec_path),
                    ("Campaign identifier", &mut form.campaign_id),
                    ("Accepted-outcome ceiling", &mut form.max_units),
                    ("Authorization record", &mut form.authorization_path),
                    ("Allowed paths (space separated)", &mut form.allowed_paths),
                    ("Denied paths (space separated)", &mut form.denied_paths),
                    ("Protected branches", &mut form.protected_branches),
                    ("Gate tier name", &mut form.gate_tier),
                    ("Gate profiles", &mut form.gate_profiles),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(480.0));
                    ui.end_row();
                }
            });
        provider_rows(ui, "Team lead (read-only planner)", &mut form.team_lead);
        provider_rows(ui, "Implementer", &mut form.implementer);
        provider_rows(ui, "Reviewer (independent)", &mut form.reviewer);
        ui.small("Model selectors are passed to the provider executable unchanged and are checked by the provider probe during preflight; the interface does not maintain a model list.");
        ui.horizontal(|ui| {
            ui.label("Implementer login boundary");
            ui.selectable_value(
                &mut form.implementer_authentication,
                CampaignAuthentication::ExistingLogin,
                "existing login",
            );
            ui.selectable_value(
                &mut form.implementer_authentication,
                CampaignAuthentication::Bare,
                "bare",
            );
        });
        ui.label("Aggregate limits");
        egui::Grid::new("campaign-limits")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("Provider attempts", &mut form.limits.provider_attempts),
                    (
                        "Malformed-report repairs",
                        &mut form.limits.malformed_report_repairs,
                    ),
                    ("Correction rounds", &mut form.limits.correction_rounds),
                    ("Process invocations", &mut form.limits.process_invocations),
                    ("Output bytes", &mut form.limits.output_bytes),
                    ("Retained state bytes", &mut form.limits.retained_state_bytes),
                    ("Execution milliseconds", &mut form.limits.execution_elapsed_ms),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                    ui.end_row();
                }
            });
        ui.small("Parallel pods and draft pull requests are not offered here; import a specification carrying an explicit multi-agent policy and GitHub authority instead.");
        ui.checkbox(&mut form.overwrite, "Replace the existing specification file");
        ui.horizontal(|ui| {
            if ui.button("Verify and write campaign specification").clicked() {
                apply = true;
            }
            if ui.button("Discard campaign form").clicked() {
                discard = true;
            }
        });

    });
    (apply, discard)
}

fn capability_toggle(ui: &mut egui::Ui, label: &str, grant: &mut CapabilityGrant) {
    let mut allowed = *grant == CapabilityGrant::Allowed;
    if ui.checkbox(&mut allowed, label).changed() {
        *grant = if allowed {
            CapabilityGrant::Allowed
        } else {
            CapabilityGrant::Denied
        };
    }
}

fn provider_rows(ui: &mut egui::Ui, label: &str, provider: &mut ProviderForm) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut provider.executable)
                .hint_text("/absolute/path/to/provider")
                .desired_width(300.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut provider.model)
                .hint_text("model selector")
                .desired_width(160.0),
        );
        egui::ComboBox::from_id_salt(format!("effort-{label}"))
            .selected_text(provider.effort.clone())
            .show_ui(ui, |ui| {
                for effort in EFFORTS {
                    ui.selectable_value(&mut provider.effort, effort.to_owned(), effort);
                }
            });
    });
}
