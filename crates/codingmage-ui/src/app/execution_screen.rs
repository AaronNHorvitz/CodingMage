//! Execution: explicit admission, detached coordinator launches and control requests.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{App, STATUS_DEADLINE, failure_box};
use crate::{
    admission::{Admission, CurrentBinding, Staleness},
    backend::{BackendError, Job, Request, Response, explain_code, models::parse_control_outcome},
    command,
    controls::{ControlAction, ControlLedger, ControlRefusal},
    launch::{LaunchRecord, LaunchState, OwnedLaunch, campaign_arguments, launch_campaign},
    observed::{Freshness, age_label},
    state_dir::project_private_dir,
};

/// Interval between launch liveness observations.
pub const LAUNCH_OBSERVE_INTERVAL: Duration = Duration::from_secs(1);
/// Progress lines shown from the coordinator's content-minimized stream.
pub const PROGRESS_LINES: usize = 12;

/// Execution state kept on the application.
#[derive(Default)]
pub struct ExecutionState {
    /// Admission for the selected authority, when recorded.
    pub admission: Option<Admission>,
    /// Digest prefix typed by the owner.
    pub confirmation: String,
    /// Last admission or launch refusal.
    pub error: Option<String>,
    /// Launch owned by this process.
    pub owned: Option<OwnedLaunch>,
    /// Launch record for the campaign (this process or an earlier one).
    pub record: Option<LaunchRecord>,
    /// Last liveness observation.
    pub observed: Option<(Instant, LaunchState)>,
    /// Control ledger for the campaign.
    pub ledger: ControlLedger,
    /// Cancel needs a second press.
    pub cancel_armed: bool,
    /// Display-only request identities, bound to the current campaign authority.
    preview_ids: Vec<(ControlAction, String, String)>,
}

impl App {
    /// Execution state.
    #[must_use]
    pub const fn execution(&self) -> &ExecutionState {
        &self.execution
    }

    /// Mutable control ledger (used by tests to simulate lost outcomes).
    pub const fn ledger_mut(&mut self) -> &mut ControlLedger {
        &mut self.execution.ledger
    }

    /// Digest confirmation field.
    pub fn set_confirmation(&mut self, text: &str) {
        text.clone_into(&mut self.execution.confirmation);
    }

    /// Private directory for the opened configuration's execution state.
    #[must_use]
    pub fn project_dir(&self) -> Option<PathBuf> {
        let project = self.project.as_ref()?;
        let directory = self.state_dir.as_ref().ok()?;
        Some(project_private_dir(directory, &project.config_path))
    }

    /// Binding the admission is checked against, when a campaign and diagnosis exist.
    #[must_use]
    pub fn current_binding(&self) -> Option<CurrentBinding> {
        let campaign = self.campaign.as_ref()?;
        let diagnosis = self.diagnosis.value.as_ref()?;
        Some(CurrentBinding {
            authority_sha256: campaign.authority_sha256.clone(),
            repository_id: diagnosis.repository_id.clone(),
            head: diagnosis.head.clone(),
            task_source_sha256: diagnosis.task_source_sha256.clone(),
            operator_authorization_sha256: campaign.spec.operator_authorization_sha256.clone(),
        })
    }

    /// Why the admission does not apply now, if it does not.
    #[must_use]
    pub fn admission_staleness(&self) -> Option<Staleness> {
        let admission = self.execution.admission.as_ref()?;
        let binding = self.current_binding()?;
        admission.staleness(&binding)
    }

    /// Records the admission after the owner confirms the reviewed report digest.
    pub fn admit_campaign(&mut self) {
        if !self.preflight_can_admit() && self.preflight.value.is_some() {
            self.execution.error = Some(
                "the latest preflight is pending, stale or failed; rerun it successfully before admission"
                    .to_owned(),
            );
            return;
        }
        let (Some(campaign), Some(binding), Some(record)) = (
            &self.campaign,
            self.current_binding(),
            &self.authorization_record,
        ) else {
            self.execution.error =
                Some("select a campaign, refresh the diagnosis and choose the authorization record first".to_owned());
            return;
        };
        match Admission::record(
            self.preflight.value.as_ref(),
            &binding,
            &campaign.spec.campaign_id,
            &campaign.spec_path,
            record,
            &self.execution.confirmation,
        ) {
            Ok(admission) => {
                if let Some(directory) = self.project_dir()
                    && let Err(error) = admission.save(&directory)
                {
                    self.execution.error = Some(format!("admission not recorded: {error}"));
                    return;
                }
                self.set_status(format!(
                    "campaign {} admitted against preflight {}",
                    admission.campaign_id,
                    &admission.preflight_sha256[..12]
                ));
                self.execution.admission = Some(admission);
                self.execution.error = None;
                self.execution.confirmation.clear();
            }
            Err(error) => self.execution.error = Some(error.to_string()),
        }
    }

    fn preflight_can_admit(&self) -> bool {
        !self.preflight.loading && self.preflight.freshness(self.now) == Freshness::Live
    }

    /// Restores admission, launch record and ledger for the selected campaign.
    pub(super) fn restore_execution(&mut self) {
        self.execution = ExecutionState::default();
        let (Some(campaign), Some(directory)) = (&self.campaign, self.project_dir()) else {
            return;
        };
        self.execution.admission = Admission::load(&directory, &campaign.authority_sha256)
            .ok()
            .flatten();
        self.execution.record =
            LaunchRecord::load(&directory.join("launches").join(&campaign.spec.campaign_id))
                .ok()
                .flatten()
                .filter(|record| record.authority_sha256 == campaign.authority_sha256);
        let mut ledger =
            ControlLedger::load(&directory, &campaign.spec.campaign_id).unwrap_or_default();
        ledger.mark_pending_unknown();
        self.execution.ledger = ledger;
        self.persist_ledger();
        self.observe_launch(true);
    }

    /// Whether a coordinator launched for this campaign is alive.
    #[must_use]
    pub fn launch_is_live(&self) -> bool {
        matches!(self.execution.observed, Some((_, LaunchState::Live)))
    }

    pub(super) fn observe_launch(&mut self, force: bool) {
        let due = force
            || self
                .execution
                .observed
                .as_ref()
                .is_none_or(|(at, _)| self.now.duration_since(*at) >= LAUNCH_OBSERVE_INTERVAL);
        if !due {
            return;
        }
        if let Some(owned) = &mut self.execution.owned {
            owned.reap();
        }
        let state = self.execution.record.as_ref().map(LaunchRecord::observe);
        let transitioned = match (&self.execution.observed, &state) {
            (Some((_, LaunchState::Live)), Some(next)) => *next != LaunchState::Live,
            _ => false,
        };
        if let Some(state) = state {
            self.execution.observed = Some((self.now, state));
        }
        if transitioned {
            self.execution.owned = None;
            self.set_status("the coordinator process exited; reading its terminal outcome");
            self.refresh_campaign();
        }
    }

    /// Reasons starting is refused now, empty when it may start.
    #[must_use]
    pub fn start_refusals(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if self.campaign.is_none() {
            reasons.push("no campaign selected".to_owned());
            return reasons;
        }
        match (&self.execution.admission, self.admission_staleness()) {
            (None, _) => reasons.push("the campaign is not admitted".to_owned()),
            (Some(_), Some(stale)) => {
                reasons.push(format!("admission is stale: {}", stale.explain()));
            }
            (Some(_), None) => {}
        }
        if self.launch_is_live() {
            reasons.push("a coordinator launched for this campaign is still running".to_owned());
        }
        if let Some(Some(status)) = &self.status.value
            && matches!(status.state.as_str(), "complete" | "cancelled")
        {
            reasons.push(format!("the campaign is {}", status.state));
        }
        if self.execution.ledger.pending().is_some() {
            reasons.push("a control request is pending".to_owned());
        }
        reasons
    }

    /// Starts the coordinator for the admitted campaign, detached from this window.
    pub fn start_campaign(&mut self) {
        let refusals = self.start_refusals();
        if !refusals.is_empty() {
            self.execution.error = Some(format!("start refused: {}", refusals.join("; ")));
            return;
        }
        let (Some(project), Some(campaign), Some(directory)) =
            (&self.project, &self.campaign, self.project_dir())
        else {
            return;
        };
        let Some(arguments) = self.start_arguments() else {
            self.execution.error =
                Some("the exact start command cannot be shown safely".to_owned());
            return;
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.execution.error =
                Some("the exact start command cannot be shown safely".to_owned());
            return;
        }
        let binary = match &self.connection {
            super::Connection::Ready(_) => self
                .binary_path
                .as_ref()
                .and_then(|path| crate::backend::CoordinatorBinary::at(path).ok()),
            super::Connection::Unavailable(_) => None,
        };
        let Some(binary) = binary else {
            self.execution.error = Some("the coordinator executable is unavailable".to_owned());
            return;
        };
        let launch_dir = directory.join("launches").join(&campaign.spec.campaign_id);
        match launch_campaign(
            &binary,
            &project.config_path,
            &campaign.spec_path,
            &campaign.spec.campaign_id,
            &campaign.authority_sha256,
            &launch_dir,
        ) {
            Ok(owned) => {
                self.set_status(format!(
                    "coordinator started as pid {} in its own process group; closing this window does not stop it",
                    owned.record.pid
                ));
                self.execution.record = Some(owned.record.clone());
                self.execution.owned = Some(owned);
                self.execution.error = None;
                self.observe_launch(true);
            }
            Err(error) => self.execution.error = Some(error.to_string()),
        }
    }

    /// Issues one control request through `campaign-control`.
    ///
    /// # Errors
    ///
    /// Returns [`ControlRefusal`] when the request is refused before reaching the coordinator.
    pub fn request_control(&mut self, action: ControlAction) -> Result<String, ControlRefusal> {
        let Some(campaign) = &self.campaign else {
            return Err(ControlRefusal::NoCampaign);
        };
        let request_id = self
            .execution
            .ledger
            .preview_id(action, &campaign.authority_sha256);
        self.request_control_prepared(action, &request_id)
    }

    fn start_arguments(&self) -> Option<Vec<String>> {
        campaign_arguments(
            &self.project.as_ref()?.config_path,
            &self.campaign.as_ref()?.spec_path,
        )
    }

    fn control_arguments(&self, action: ControlAction, request_id: &str) -> Option<Vec<String>> {
        Some(vec![
            "campaign-control".to_owned(),
            "--config".to_owned(),
            self.project.as_ref()?.config_path.to_str()?.to_owned(),
            "--campaign".to_owned(),
            self.campaign.as_ref()?.spec_path.to_str()?.to_owned(),
            "--action".to_owned(),
            action.code().to_owned(),
            "--request".to_owned(),
            request_id.to_owned(),
        ])
    }

    fn prepared_control_id(&mut self, action: ControlAction) -> Option<String> {
        let authority = self.campaign.as_ref()?.authority_sha256.clone();
        if let Some((_, _, id)) =
            self.execution
                .preview_ids
                .iter()
                .find(|(stored_action, stored_authority, _)| {
                    *stored_action == action && *stored_authority == authority
                })
        {
            return Some(id.clone());
        }
        let id = self.execution.ledger.preview_id(action, &authority);
        self.execution
            .preview_ids
            .retain(|(stored_action, _, _)| *stored_action != action);
        self.execution
            .preview_ids
            .push((action, authority, id.clone()));
        Some(id)
    }

    /// Exact display-only coordinator command prepared for a campaign control.
    ///
    /// Preparing the command records no intent and does not start a process.
    #[must_use]
    pub fn preview_control(&mut self, action: ControlAction) -> Option<String> {
        let id = self.prepared_control_id(action)?;
        let arguments = self.control_arguments(action, &id)?;
        command::format_command(self.binary_path.as_deref()?, &arguments)
    }

    fn request_control_prepared(
        &mut self,
        action: ControlAction,
        request_id: &str,
    ) -> Result<String, ControlRefusal> {
        let Some(campaign) = &self.campaign else {
            return Err(ControlRefusal::NoCampaign);
        };
        if action == ControlAction::Cancel && !self.execution.cancel_armed {
            self.execution.cancel_armed = true;
            return Err(ControlRefusal::ConfirmCancel);
        }
        self.execution.cancel_armed = false;
        let authority = campaign.authority_sha256.clone();
        let arguments = self
            .control_arguments(action, request_id)
            .ok_or(ControlRefusal::CommandUnavailable)?;
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            return Err(ControlRefusal::CommandUnavailable);
        }
        let request_id = self
            .execution
            .ledger
            .begin_prepared(action, &authority, request_id)?;
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "campaign-control",
                arguments,
                deadline: STATUS_DEADLINE,
            },
            request_id: Some(request_id.clone()),
        };
        if let Err(error) = self.submit(request) {
            self.execution
                .ledger
                .finish(&request_id, false, Some(error.code()));
        }
        self.persist_ledger();
        self.set_status(format!("{} requested as {request_id}", action.label()));
        Ok(request_id)
    }

    fn persist_ledger(&self) {
        if let (Some(campaign), Some(directory)) = (&self.campaign, self.project_dir()) {
            let _ = self
                .execution
                .ledger
                .save(&directory, &campaign.spec.campaign_id);
        }
    }

    pub(super) fn accept_control(&mut self, response: Response) {
        let Some(request_id) = response.request_id.clone() else {
            return;
        };
        match response
            .result
            .and_then(|bytes| parse_control_outcome(&bytes).map_err(BackendError::from))
        {
            Ok(outcome) => {
                self.execution
                    .ledger
                    .finish(&request_id, outcome.created, None);
                self.set_status(format!(
                    "{} {}: {}",
                    outcome.action,
                    outcome.request_id,
                    if outcome.created {
                        "intent created"
                    } else {
                        "already recorded (idempotent replay)"
                    }
                ));
            }
            Err(error) => {
                self.execution
                    .ledger
                    .finish(&request_id, false, Some(error.code()));
                self.set_status(format!("control failed: {}", error.code()));
            }
        }
        self.persist_ledger();
        self.refresh_campaign();
    }

    pub(super) fn execution_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Execution");
        self.admission_block(ui);
        ui.separator();
        self.launch_block(ui);
        ui.separator();
        self.controls_block(ui);
        if let Some(error) = &self.execution.error {
            failure_box(ui, "Refused", error, "Nothing was started or changed.");
        }
    }

    fn admission_block(&mut self, ui: &mut egui::Ui) {
        ui.strong("Admission");
        match (&self.execution.admission, self.admission_staleness()) {
            (None, _) => {
                ui.label("Not admitted. Run preflight, review its report, then confirm its digest here. Admission records your review; the coordinator revalidates everything when it starts.");
            }
            (Some(admission), None) => {
                ui.label(format!(
                    "Admitted against preflight report {} at unix {} ms for head {}.",
                    &admission.preflight_sha256[..12],
                    admission.admitted_at_ms,
                    &admission.initial_commit[..12.min(admission.initial_commit.len())]
                ));
            }
            (Some(_), Some(stale)) => {
                ui.colored_label(
                    super::current_tokens(ui.ctx()).warning,
                    format!(
                        "Admission stale: {}. Run preflight again and re-admit.",
                        stale.explain()
                    ),
                );
            }
        }
        ui.horizontal(|ui| {
            let label = ui.label("Confirm report digest prefix");
            ui.add(
                egui::TextEdit::singleline(&mut self.execution.confirmation)
                    .hint_text("first 12 or more characters")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_label),
            )
            .labelled_by(label.id);
            if ui
                .add_enabled(
                    self.preflight_can_admit(),
                    egui::Button::new("Admit campaign"),
                )
                .clicked()
            {
                self.admit_campaign();
            }
        });
        if self.preflight.value.is_some() && !self.preflight_can_admit() {
            ui.small("Admission is unavailable until a fresh preflight succeeds; any earlier report is retained only for inspection.");
        }
    }

    fn launch_block(&mut self, ui: &mut egui::Ui) {
        ui.strong("Coordinator process");
        match &self.execution.observed {
            None => {
                ui.label("No coordinator has been launched for this campaign from this interface.");
            }
            Some((at, LaunchState::Live)) => {
                if let Some(record) = &self.execution.record {
                    ui.label(format!(
                        "Live: pid {} launched at unix {} ms (observed {}). The coordinator owns execution; closing this window does not stop it.",
                        record.pid,
                        record.launched_at_ms,
                        age_label(Some(self.now.duration_since(*at)))
                    ));
                }
            }
            Some((_, LaunchState::Exited(outcome))) => {
                ui.label(format!(
                    "Exited: state {}, stop reason {}, {} unit(s) completed by that invocation, head {}{}",
                    outcome.state,
                    outcome.stop_reason,
                    outcome.completed_units,
                    &outcome.head[..12.min(outcome.head.len())],
                    outcome
                        .blocker_code
                        .as_ref()
                        .map_or(String::new(), |code| format!(", blocker {code}"))
                ));
            }
            Some((_, LaunchState::ExitedWithoutOutcome { code })) => {
                let code = code
                    .clone()
                    .unwrap_or_else(|| "no stable code captured".to_owned());
                let (what, action) = explain_code(&code);
                failure_box(
                    ui,
                    "The coordinator exited without a terminal outcome",
                    &format!("{code}: {what}"),
                    action,
                );
            }
        }
        if let Some(record) = &self.execution.record {
            let tail = record.progress_tail(PROGRESS_LINES);
            if !tail.is_empty() {
                ui.small("Recent coordinator activity (content-minimized stream):");
                for line in tail {
                    crate::content::render(ui, &line);
                }
            }
        }
        let refusals = self.start_refusals();
        let arguments = self.start_arguments();
        let can_start = refusals.is_empty()
            && command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(can_start, egui::Button::new("Start coordinator"))
            .clicked()
        {
            self.start_campaign();
        }
        if !can_start && self.campaign.is_some() {
            if refusals.is_empty() {
                ui.small("Start unavailable: the exact command cannot be shown safely.");
            } else {
                ui.small(format!("Start unavailable: {}", refusals.join("; ")));
            }
        }
        if let Some(arguments) = arguments {
            command::show_for(
                ui,
                "Start coordinator",
                self.binary_path.as_deref(),
                &arguments,
            );
        } else {
            command::show_unavailable_for(ui, "Start coordinator");
        }
    }

    fn controls_block(&mut self, ui: &mut egui::Ui) {
        ui.strong(
            "Controls (recorded through campaign-control with create-once request identities)",
        );
        let pending = self.execution.ledger.pending().cloned();
        ui.horizontal_wrapped(|ui| {
            for action in ControlAction::ALL {
                let preview_id = self.prepared_control_id(action);
                let arguments = preview_id
                    .as_deref()
                    .and_then(|id| self.control_arguments(action, id));
                let label = if action == ControlAction::Cancel && self.execution.cancel_armed {
                    "Confirm cancel"
                } else {
                    action.label()
                };
                let enabled = pending.is_none()
                    && command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    let id = preview_id
                        .as_deref()
                        .expect("enabled control has an identity");
                    match self.request_control_prepared(action, id) {
                        Ok(_) => {
                            self.execution
                                .preview_ids
                                .retain(|(stored_action, _, _)| *stored_action != action);
                            self.execution.error = None;
                        }
                        Err(ControlRefusal::ConfirmCancel) => {
                            self.execution.error = None;
                        }
                        Err(error) => {
                            self.execution
                                .preview_ids
                                .retain(|(stored_action, _, _)| *stored_action != action);
                            self.execution.error = Some(error.to_string());
                        }
                    }
                }
                if let Some(arguments) = arguments {
                    command::show_for(ui, action.label(), self.binary_path.as_deref(), &arguments);
                } else {
                    command::show_unavailable_for(ui, action.label());
                }
            }
        });
        for action in ControlAction::ALL {
            ui.small(format!("{}: {}", action.label(), action.effect()));
        }
        if let Some(pending) = pending {
            ui.label(format!(
                "Pending: {} ({}) attempt {}",
                pending.request_id,
                pending.action.code(),
                pending.attempts
            ));
        }
        let recent = self
            .execution
            .ledger
            .entries
            .iter()
            .rev()
            .take(6)
            .cloned()
            .collect::<Vec<_>>();
        if !recent.is_empty() {
            ui.small("Recent requests");
            for entry in recent {
                let outcome = match &entry.result {
                    None => "pending".to_owned(),
                    Some(result) => match (&result.code, result.created) {
                        (Some(code), _) => format!("failed: {code}"),
                        (None, true) => "intent created".to_owned(),
                        (None, false) => "replayed (already recorded)".to_owned(),
                    },
                };
                ui.monospace(format!(
                    "{} {} - {outcome}",
                    entry.request_id,
                    entry.action.code()
                ));
            }
        }
    }
}
