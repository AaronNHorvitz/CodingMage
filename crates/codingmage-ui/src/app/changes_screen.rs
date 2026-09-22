//! Changes and reviews: exact candidate changes, review and test records, bounded activity.

use super::{App, GIT_DEADLINE, failure_box};
use crate::{
    backend::{BackendError, Job, Request, Response, explain_code},
    observed::{Freshness, age_label},
    records::{CommitSummary, FileChange, RunRecord, parse_log, parse_numstat},
};

/// Exact changes between the campaign's initial commit and its head.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
    /// Base commit.
    pub base: String,
    /// Head commit.
    pub head: String,
    /// Coordinator commits, newest first.
    pub commits: Vec<CommitSummary>,
    /// Changed files.
    pub files: Vec<FileChange>,
}

impl App {
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
        let Some(project) = &self.project else {
            return;
        };
        if base == head {
            self.changes.accept(
                ChangeSet {
                    base: base.to_owned(),
                    head: head.to_owned(),
                    commits: Vec::new(),
                    files: Vec::new(),
                },
                self.generation,
                self.now,
            );
            self.changes_range = Some((base.to_owned(), head.to_owned()));
            return;
        }
        let repository = project.config.target_path.clone();
        let jobs = [
            (
                "git-log",
                vec![
                    "log".to_owned(),
                    "--format=%H%x1f%s%x1f%ct".to_owned(),
                    format!("{base}..{head}"),
                ],
            ),
            (
                "git-numstat",
                vec![
                    "diff".to_owned(),
                    "--numstat".to_owned(),
                    "-z".to_owned(),
                    base.to_owned(),
                    head.to_owned(),
                ],
            ),
        ];
        self.pending_changes = Some(ChangeSet {
            base: base.to_owned(),
            head: head.to_owned(),
            commits: Vec::new(),
            files: Vec::new(),
        });
        self.pending_changes_parts = 0;
        for (label, arguments) in jobs {
            let request = Request {
                generation: self.generation,
                binding: self.binding(),
                job: Job::GitRead {
                    label,
                    repository: repository.clone(),
                    arguments,
                    deadline: GIT_DEADLINE,
                },
                request_id: None,
            };
            match self.submit(request) {
                Ok(()) => self.changes.loading = true,
                Err(error) => self.changes.fail(error, self.now),
            }
        }
        self.changes_range = Some((base.to_owned(), head.to_owned()));
    }

    pub(super) fn request_records(&mut self) {
        let (Some(project), Some(campaign)) = (&self.project, &self.campaign) else {
            return;
        };
        let campaign_dir = project
            .config
            .state_root
            .join("campaigns")
            .join(&campaign.spec.campaign_id);
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::ScanRecords {
                label: "records",
                campaign_dir,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => self.records.loading = true,
            Err(error) => self.records.fail(error, self.now),
        }
    }

    pub(super) fn accept_changes_part(&mut self, response: Response) {
        let Some(pending) = &mut self.pending_changes else {
            return;
        };
        match response.result {
            Ok(bytes) => {
                if response.label == "git-log" {
                    pending.commits = parse_log(&bytes);
                } else {
                    pending.files = parse_numstat(&bytes);
                }
                self.pending_changes_parts += 1;
                if self.pending_changes_parts >= 2 {
                    let complete = self.pending_changes.take().unwrap_or_default();
                    self.changes.accept(complete, response.generation, self.now);
                }
            }
            Err(error) => {
                self.pending_changes = None;
                self.changes.fail(error, self.now);
            }
        }
    }

    pub(super) fn accept_records(&mut self, response: Response) {
        match response.result.and_then(|bytes| {
            serde_json::from_slice::<Vec<RunRecord>>(&bytes)
                .map_err(|_| BackendError::Contract(crate::backend::models::ModelError::Malformed))
        }) {
            Ok(records) => self.records.accept(records, response.generation, self.now),
            Err(error) => self.records.fail(error, self.now),
        }
    }

    pub(super) fn changes_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Changes and reviews");
        let (Some(_), Some(campaign)) = (&self.project, &self.campaign) else {
            ui.label("Open a repository and select a campaign to inspect its changes.");
            return;
        };
        ui.strong("Delivery boundary");
        ui.label(format!(
            "Publication {:?}: accepted work lives on the coordinator-owned campaign branch only. Nothing here was pushed, merged into a protected branch or promoted to a destination; engineering completion below is not delivery.",
            campaign.spec.publication
        ));
        ui.separator();
        self.changes_block(ui);
        ui.separator();
        self.records_block(ui);
        ui.separator();
        self.activity_block(ui);
    }

    fn changes_block(&self, ui: &mut egui::Ui) {
        ui.strong("Exact candidate changes (campaign branch)");
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
            let (what, action) = explain_code(&error.code());
            failure_box(ui, what, &error.to_string(), action);
        }
        let Some(changes) = &self.changes.value else {
            if freshness == Freshness::Loading {
                ui.label("Reading the campaign branch objects...");
            }
            return;
        };
        ui.monospace(format!(
            "{}..{}",
            &changes.base[..12.min(changes.base.len())],
            &changes.head[..12.min(changes.head.len())]
        ));
        if changes.commits.is_empty() {
            ui.label("The campaign head equals the initial commit: no reviewed candidate has been integrated.");
        }
        for commit in &changes.commits {
            ui.monospace(format!(
                "{} {} (unix {})",
                &commit.id[..12],
                commit.subject,
                commit.timestamp
            ));
        }
        if !changes.files.is_empty() {
            ui.label(format!("{} changed file(s)", changes.files.len()));
            egui::Grid::new("changed-files")
                .num_columns(3)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    for file in &changes.files {
                        ui.monospace(&file.path);
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
        let freshness = self.records.freshness(self.now);
        ui.label(format!(
            "Observation: {} ({})",
            freshness.label(),
            age_label(self.records.age(self.now))
        ));
        if let Some((_, error)) = &self.records.last_error {
            let (what, action) = explain_code(&error.code());
            failure_box(ui, what, &error.to_string(), action);
        }
        let Some(records) = &self.records.value else {
            return;
        };
        if records.is_empty() {
            ui.label("No run records exist for this campaign. Nothing has been implemented, reviewed or tested.");
            return;
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
                            egui::Color32::from_rgb(255, 150, 150),
                            record
                                .checkpoint_problem
                                .as_deref()
                                .unwrap_or("checkpoint unavailable"),
                        );
                    }
                }
                ui.label(record.review_label());
                ui.label(record.gate_label());
                if let Some(problem) = &record.journal_problem {
                    ui.colored_label(egui::Color32::from_rgb(255, 150, 150), problem);
                }
                if record.malformed_journal_lines > 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 150, 150),
                        format!(
                            "{} malformed journal line(s) ignored",
                            record.malformed_journal_lines
                        ),
                    );
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
                    ui.label(format!("Journaled phases: {}", observed.join(" > ")));
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
                    ui.monospace(line);
                }
                ui.small("Lines are the coordinator's own content-minimized stream: actor and stage only, never prompts, source or provider output.");
            }
            None => {
                ui.label("No coordinator launch is recorded for this campaign from this interface; activity lines are not available.");
            }
        }
    }
}
