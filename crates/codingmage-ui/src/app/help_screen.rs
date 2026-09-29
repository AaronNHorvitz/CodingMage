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
}

impl SupportState {
    /// Whether one bundle request is waiting for a bound coordinator response.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending.is_some()
    }
}

struct PendingSupport {
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
        ui.heading("Help and About");
        ui.label("This help is available without a repository, coordinator connection or network access.");
        ui.horizontal(|ui| {
            if ui.button("Return to Overview").clicked() {
                self.screen = Screen::Overview;
            }
            if ui.button("Open Setup").clicked() {
                self.screen = Screen::Setup;
            }
        });
        ui.separator();
        ui.heading("Get started");
        ui.label("Open an existing configuration in Setup, or create one for a disposable repository. Opening it observes readiness and starts no agent.");
        ui.label("Select a campaign on the Campaign screen, review preflight, and explicitly admit and start it. Closing this window leaves coordinator work under coordinator control.");
        ui.separator();
        ui.heading("If something needs attention");
        ui.label("Coordinator missing: install the matching codingmage executable beside this app, then reopen the app.");
        ui.label("Provider unavailable or sign-in required: check the provider in Setup and rerun preflight. No other provider is selected silently.");
        ui.label("Stale or disconnected: reconnect or refresh before acting. A retained observation is labelled stale until the coordinator supplies a new one.");
        ui.label("Blocked campaign: read its explanation on Campaign and clear only the stated cause. A block is not a completed task.");
        ui.separator();
        ui.heading("Keyboard");
        ui.label("Ctrl+1 through Ctrl+8 select a destination. Tab and Shift+Tab move focus; Enter or Space activates the focused control.");
        ui.label("F5 requests a new diagnosis and campaign observation. A Show command section reveals the exact supported coordinator command without running it.");
        ui.separator();
        glossary(ui);
        ui.separator();
        self.support_bundle_controls(ui);
        ui.separator();
        let summary = diagnostic_summary(&self.connection);
        ui.heading("Copyable diagnostic summary");
        ui.label("This summary contains only the interface version, platform and sibling coordinator availability. It omits paths, repository content, prompts and credentials.");
        ui.monospace(&summary);
        let copy = ui.button("Copy diagnostic summary");
        if copy.has_focus() {
            copy.scroll_to_me(None);
        }
        if copy.clicked() {
            ui.ctx().copy_text(summary);
        }
        ui.separator();
        about(ui);
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
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => {
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
                destination,
                repository_path: project.config.target_path.clone(),
                campaign_id: campaign.spec.campaign_id.clone(),
                repository_id: campaign.spec.repository_id.clone(),
                authority_sha256: campaign.authority_sha256.clone(),
            },
        ))
    }

    pub(super) fn accept_support_bundle(&mut self, response: Response) {
        let Some(pending) = self.support.pending.take() else {
            return;
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

fn glossary(ui: &mut egui::Ui) {
    ui.heading("Glossary");
    for (term, meaning) in [
        (
            "Campaign",
            "A coordinator-owned run against one exact repository and campaign specification.",
        ),
        (
            "Admission",
            "A record that the owner reviewed one bound preflight result. It grants no extra permission.",
        ),
        (
            "Preflight",
            "Checks that the selected campaign meets the coordinator's current prerequisites before starting.",
        ),
        (
            "Source checkbox",
            "A mark in the task source document. It is separate from verified completion and accepted outcomes.",
        ),
        (
            "Gate",
            "A registered verification command and its recorded result. An unreported gate remains unknown.",
        ),
        (
            "Stale",
            "A previous observation retained after a failed or overdue refresh. Confirm it before acting.",
        ),
        (
            "Blocker",
            "A recorded reason work cannot advance until its stated cause is resolved.",
        ),
        (
            "Support bundle",
            "A manually requested redacted directory of selected coordinator records; sharing it is a separate owner action.",
        ),
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{term}:"));
            ui.label(meaning);
        });
    }
}

fn about(ui: &mut egui::Ui) {
    ui.heading("About");
    ui.label(format!(
        "CodingMage native interface {}",
        env!("CARGO_PKG_VERSION")
    ));
    ui.label("CodingMage source is licensed under Apache-2.0. The exact packaged third-party licence set is generated from the locked dependency graph for a release candidate; this source view is not a package qualification.");
    ui.collapsing("CodingMage source licence (Apache-2.0)", |ui| {
        ui.label(SOURCE_LICENSE);
    });
    ui.collapsing("Third-party source notices", |ui| {
        ui.label(SOURCE_NOTICES);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_receipt_must_match_authority_and_redaction_contract() {
        let pending = PendingSupport {
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
