//! Offline help and source notices for the native workspace.

use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;

use super::{App, Connection, Screen, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response},
    command,
    messages::{self, Catalogue},
};

const SOURCE_LICENSE: &str = include_str!("../../../../LICENSE");
const SOURCE_NOTICES: &str = include_str!("../../../../THIRD-PARTY-NOTICES.md");

/// Presentation state for an explicitly requested coordinator support bundle.
#[derive(Default)]
pub struct SupportState {
    /// Fresh absolute directory chosen by the owner.
    pub output_path: String,
    /// Last outcome or refusal; a failed request may have left a partial directory.
    pub message: Option<Result<String, String>>,
    pending: Option<PendingSupport>,
    next_request: u64,
}

impl SupportState {
    /// Whether one bundle request is waiting for a bound coordinator response.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending.is_some()
    }
}

struct PendingSupport {
    request_id: String,
    destination: PathBuf,
    repository_path: PathBuf,
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SupportManifest {
    schema_version: u16,
    coordinator_version: String,
    campaign_id: String,
    repository_id: String,
    authority_sha256: String,
    files: Vec<SupportEntry>,
    absent: Vec<String>,
    redacted: bool,
    uploaded: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SupportEntry {
    name: String,
    sha256: String,
    bytes: u64,
}

impl App {
    pub(super) fn help_screen(&mut self, ui: &mut egui::Ui) {
        self.help_screen_with_catalogue(ui, messages::english(), false);
    }

    fn help_screen_with_catalogue(
        &mut self,
        ui: &mut egui::Ui,
        catalogue: &Catalogue,
        right_to_left: bool,
    ) {
        if right_to_left {
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                self.help_content(ui, catalogue);
            });
        } else {
            self.help_content(ui, catalogue);
        }
    }

    fn help_content(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("help_title"));
        ui.label(catalogue.text("help_offline"));
        ui.horizontal(|ui| {
            if ui.button(catalogue.text("help_return")).clicked() {
                self.screen = Screen::Overview;
            }
            if ui.button(catalogue.text("help_setup")).clicked() {
                self.screen = Screen::Setup;
            }
        });
        ui.separator();
        ui.heading(catalogue.text("help_get_started"));
        ui.label(catalogue.text("help_open_configuration"));
        ui.label(catalogue.text("help_select_campaign"));
        ui.separator();
        ui.heading(catalogue.text("help_attention"));
        ui.label(catalogue.text("help_missing_coordinator"));
        ui.label(catalogue.text("help_provider"));
        ui.label(catalogue.text("help_stale"));
        ui.label(catalogue.text("help_blocked"));
        ui.separator();
        ui.heading(catalogue.text("help_keyboard_title"));
        ui.label(catalogue.text("help_keyboard_navigation"));
        ui.label(catalogue.text("help_keyboard_commands"));
        ui.separator();
        glossary(ui, catalogue);
        ui.separator();
        self.support_bundle_controls(ui);
        ui.separator();
        let summary = diagnostic_summary(&self.connection);
        ui.heading(catalogue.text("help_diagnostics_title"));
        ui.label(catalogue.text("help_diagnostics_description"));
        ui.monospace(&summary);
        let copy = ui.button(catalogue.text("help_diagnostics_copy"));
        if copy.has_focus() {
            copy.scroll_to_me(None);
        }
        if copy.clicked() {
            ui.ctx().copy_text(summary);
        }
        ui.separator();
        about(ui, catalogue);
    }

    /// Manual support-bundle form state for an opened campaign.
    #[must_use]
    pub const fn support_state(&self) -> &SupportState {
        &self.support
    }

    /// Mutable manual support-bundle form state.
    pub const fn support_state_mut(&mut self) -> &mut SupportState {
        &mut self.support
    }

    /// Requests one redacted bundle through the sibling coordinator; never retries it.
    pub fn request_support_bundle(&mut self) {
        if self.support.pending() {
            self.support.message = Some(Err(
                "A support bundle request is already in progress.".to_owned()
            ));
            return;
        }
        let (arguments, pending) = match self.support_arguments() {
            Ok(prepared) => prepared,
            Err(error) => {
                self.support.message = Some(Err(error));
                return;
            }
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.support.message = Some(Err(
                "The exact support-bundle command cannot be displayed safely.".to_owned(),
            ));
            return;
        }
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SupportBundle {
                arguments,
                destination: pending.destination.clone(),
                repository: pending.repository_path.clone(),
                deadline: Duration::from_mins(2),
            },
            request_id: Some(pending.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.support.next_request += 1;
                self.support.pending = Some(pending);
                self.support.message = None;
            }
            Err(error) => {
                self.support.message = Some(Err(format!(
                    "The coordinator refused the support bundle request ({}). No automatic retry will occur.",
                    error.code()
                )));
            }
        }
    }

    fn support_arguments(&self) -> Result<(Vec<String>, PendingSupport), String> {
        let next_request = self
            .support
            .next_request
            .checked_add(1)
            .ok_or("Too many support bundle requests in this session.")?;
        let project = self.project.as_ref().ok_or("Open a repository first.")?;
        let campaign = self.campaign.as_ref().ok_or("Select a campaign first.")?;
        if self.binding().repository_id.as_deref() != Some(campaign.spec.repository_id.as_str()) {
            return Err(
                "Wait for a matching repository diagnosis before creating diagnostics.".to_owned(),
            );
        }
        let destination = PathBuf::from(self.support.output_path.trim());
        if !destination.is_absolute()
            || destination
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(
                "Choose an absolute new directory without parent-path components.".to_owned(),
            );
        }
        if destination.starts_with(&project.config.target_path) {
            return Err(
                "The support bundle destination must be outside the target repository.".to_owned(),
            );
        }
        let arguments = vec![
            "support-bundle".to_owned(),
            "--config".to_owned(),
            utf8(&project.config_path)?.to_owned(),
            "--campaign".to_owned(),
            utf8(&campaign.spec_path)?.to_owned(),
            "--output".to_owned(),
            utf8(&destination)?.to_owned(),
        ];
        Ok((
            arguments,
            PendingSupport {
                request_id: format!("support:{}:{next_request}", self.generation.0),
                destination,
                repository_path: project.config.target_path.clone(),
                campaign_id: campaign.spec.campaign_id.clone(),
                repository_id: campaign.spec.repository_id.clone(),
                authority_sha256: campaign.authority_sha256.clone(),
            },
        ))
    }

    pub(super) fn accept_support_bundle(&mut self, response: Response) -> bool {
        if self
            .support
            .pending
            .as_ref()
            .map(|pending| pending.request_id.as_str())
            != response.request_id.as_deref()
            || response.request_id.is_none()
        {
            self.discarded_stale += 1;
            return false;
        }
        let Some(pending) = self.support.pending.take() else {
            self.discarded_stale += 1;
            return false;
        };
        let result = response.result.and_then(|bytes| {
            serde_json::from_slice::<SupportManifest>(&bytes)
                .map_err(|_| BackendError::Refused("invalid support-bundle receipt".to_owned()))
        });
        self.support.message = Some(match result {
            Ok(manifest) if manifest.matches(&pending) => Ok(format!(
                "Redacted support bundle created at {} ({} files; nothing uploaded). Review it before sharing.",
                pending.destination.display(),
                manifest.files.len()
            )),
            Ok(_) => Err(format!(
                "The coordinator returned a mismatched support-bundle receipt. Inspect {} before attempting another request; no automatic retry will occur.",
                pending.destination.display()
            )),
            Err(error) => {
                let reason = match error {
                    BackendError::Refused(reason) => reason,
                    other => other.code(),
                };
                Err(format!(
                    "Support-bundle result is unavailable ({reason}). Inspect {} for a partial directory before attempting another request; no automatic retry will occur.",
                    pending.destination.display()
                ))
            }
        });
        true
    }

    fn support_bundle_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Manual diagnostics");
        ui.label("For a selected campaign, the coordinator can write a redacted support bundle into a new private directory. It excludes source, prompts, provider output and credentials, and uploads nothing. Review the files before sharing them yourself.");
        ui.horizontal(|ui| {
            let label = ui.label("New directory outside the repository");
            ui.add(
                egui::TextEdit::singleline(&mut self.support.output_path)
                    .hint_text("/absolute/path/to/new-support-directory")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
        });
        let prepared = self.support_arguments();
        let ready = prepared.as_ref().is_ok_and(|(arguments, _)| {
            command::can_preview(self.binary_path.as_deref(), Some(arguments))
        }) && !self.support.pending();
        let create = ui.add_enabled(ready, egui::Button::new("Create redacted support bundle"));
        if create.has_focus() {
            create.scroll_to_me(None);
        }
        if create.clicked() {
            self.request_support_bundle();
        }
        match prepared {
            Ok((arguments, _)) => command::show_for(
                ui,
                "Create support bundle",
                self.binary_path.as_deref(),
                &arguments,
            ),
            Err(reason) => {
                ui.small(reason);
                command::show_unavailable_for(ui, "Create support bundle");
            }
        }
        if self.support.pending() {
            ui.label("The coordinator is creating the bundle. This window does not upload it.");
        }
        if let Some(message) = &self.support.message {
            match message {
                Ok(text) => {
                    ui.colored_label(super::current_tokens(ui.ctx()).success, text);
                }
                Err(text) => failure_box(
                    ui,
                    "Support bundle not confirmed",
                    text,
                    "Choose a fresh directory outside the repository and retry only after inspecting any partial output.",
                ),
            }
        }
    }
}

impl SupportManifest {
    fn matches(&self, pending: &PendingSupport) -> bool {
        const RECORDS: [&str; 5] = [
            "campaign-explain-blocker.json",
            "campaign-report.json",
            "campaign-status.json",
            "configuration.json",
            "mission-status.json",
        ];
        let mut reported = self
            .files
            .iter()
            .map(|entry| entry.name.as_str())
            .chain(self.absent.iter().map(String::as_str))
            .collect::<Vec<_>>();
        reported.sort_unstable();
        self.schema_version == 1
            && !self.coordinator_version.is_empty()
            && self.campaign_id == pending.campaign_id
            && self.repository_id == pending.repository_id
            && self.authority_sha256 == pending.authority_sha256
            && self.redacted
            && !self.uploaded
            && self
                .files
                .iter()
                .any(|entry| entry.name == "configuration.json")
            && reported.as_slice() == RECORDS.as_slice()
            && self.files.iter().all(|entry| {
                !entry.name.is_empty()
                    && entry.sha256.len() == 64
                    && entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && entry.bytes > 0
            })
    }
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "A path cannot be displayed as an exact UTF-8 command.".to_owned())
}

fn diagnostic_summary(connection: &Connection) -> String {
    let coordinator = match connection {
        Connection::Ready(_) => "sibling executable available",
        Connection::Unavailable(_) => "unavailable",
    };
    format!(
        "CodingMage UI {}\nPlatform: {}/{}\nSibling coordinator: {coordinator}\nNo paths, repository content, prompts or credentials included.",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

fn glossary(ui: &mut egui::Ui, catalogue: &Catalogue) {
    ui.heading(catalogue.text("help_glossary_title"));
    for (term, meaning) in [
        ("help_term_campaign", "help_meaning_campaign"),
        ("help_term_admission", "help_meaning_admission"),
        ("help_term_preflight", "help_meaning_preflight"),
        ("help_term_source_checkbox", "help_meaning_source_checkbox"),
        ("help_term_gate", "help_meaning_gate"),
        ("help_term_stale", "help_meaning_stale"),
        ("help_term_blocker", "help_meaning_blocker"),
        ("help_term_support_bundle", "help_meaning_support_bundle"),
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{}:", catalogue.text(term)));
            ui.label(catalogue.text(meaning));
        });
    }
}

fn about(ui: &mut egui::Ui, catalogue: &Catalogue) {
    ui.heading(catalogue.text("help_about_title"));
    ui.label(format!(
        "CodingMage native interface {}",
        env!("CARGO_PKG_VERSION")
    ));
    ui.label(catalogue.text("help_about_source_notice"));
    ui.collapsing(catalogue.text("help_about_source_license"), |ui| {
        ui.label(SOURCE_LICENSE);
    });
    ui.collapsing(catalogue.text("help_about_third_party"), |ui| {
        ui.label(SOURCE_NOTICES);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    struct HelpPreviewApp {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for HelpPreviewApp {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.app
                        .help_screen_with_catalogue(ui, &self.catalogue, self.right_to_left);
                });
            });
        }
    }

    #[test]
    fn expanded_and_right_aligned_help_remain_reachable_at_minimum_window() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |creation| HelpPreviewApp {
                    app: App::with_state_dir(
                        &creation.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("/example/missing/codingmage"),
                        }),
                        Ok(std::env::temp_dir().join("codingmage-ui-message-preview")),
                    ),
                    catalogue,
                    right_to_left,
                });
            harness.run_steps(2);
            let title = harness.get_by_label_contains(if right_to_left {
                "אבג ⟦Help and About⟧"
            } else {
                "⟦Help and About⟧"
            });
            assert!(title.accesskit_node().has_bounds());
            let return_button = harness.get_by_label_contains("Return to Overview");
            assert!(return_button.accesskit_node().has_bounds());
            harness.get_by_label_contains("Open Setup");
            harness.get_by_label_contains("Get started");
        }
    }

    #[test]
    fn support_receipt_must_match_authority_and_redaction_contract() {
        let pending = PendingSupport {
            request_id: "support:1:1".to_owned(),
            destination: PathBuf::from("/tmp/new-support-bundle"),
            repository_path: PathBuf::from("/tmp/repository"),
            campaign_id: "campaign-a".to_owned(),
            repository_id: "repository-a".to_owned(),
            authority_sha256: "a".repeat(64),
        };
        let mut manifest = SupportManifest {
            schema_version: 1,
            coordinator_version: "0.1.0".to_owned(),
            campaign_id: pending.campaign_id.clone(),
            repository_id: pending.repository_id.clone(),
            authority_sha256: pending.authority_sha256.clone(),
            files: vec![SupportEntry {
                name: "configuration.json".to_owned(),
                sha256: "b".repeat(64),
                bytes: 12,
            }],
            absent: vec![
                "campaign-explain-blocker.json".to_owned(),
                "campaign-report.json".to_owned(),
                "campaign-status.json".to_owned(),
                "mission-status.json".to_owned(),
            ],
            redacted: true,
            uploaded: false,
        };
        assert!(manifest.matches(&pending));
        manifest.uploaded = true;
        assert!(!manifest.matches(&pending));
        manifest.uploaded = false;
        manifest.authority_sha256 = "c".repeat(64);
        assert!(!manifest.matches(&pending));
        manifest.authority_sha256 = pending.authority_sha256.clone();
        manifest.schema_version = 2;
        assert!(!manifest.matches(&pending));
        manifest.schema_version = 1;
        manifest.absent.push("source.rs".to_owned());
        assert!(!manifest.matches(&pending));
        manifest.absent.pop();
        manifest.absent.push("campaign-status.json".to_owned());
        assert!(!manifest.matches(&pending));
    }

    #[test]
    fn copyable_diagnostics_omit_the_unavailable_binary_path() {
        let private_path = PathBuf::from("/example/hidden/coordinator");
        let summary =
            diagnostic_summary(&Connection::Unavailable(BackendError::BinaryUnavailable {
                expected: private_path.clone(),
            }));
        assert!(summary.contains("Sibling coordinator: unavailable"));
        assert!(!summary.contains(private_path.to_str().unwrap()));
        assert!(summary.contains("No paths, repository content, prompts or credentials included."));
        let malformed = Connection::Unavailable(BackendError::Command {
            code: "/example/hidden/code".to_owned(),
            exit_code: Some(1),
        });
        assert!(!diagnostic_summary(&malformed).contains("/example/hidden/code"));
    }
}
