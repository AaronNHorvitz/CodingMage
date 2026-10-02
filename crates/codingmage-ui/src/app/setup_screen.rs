//! Guided setup: repository configuration, campaign authority and the authorization record.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use codingmage_campaign::CampaignAuthentication;
use codingmage_core::{CapabilityGrant, PublicationMode};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{App, BrowserSlot, campaign_select_arguments, directory_list_arguments, failure_box};
use crate::{
    backend::{Job, PrivateInput, Request, Response},
    browser::Browser,
    command, content,
    messages::{self, Catalogue},
    project::{Project, hex},
    setup::{
        CampaignBinding, CampaignForm, ConfigForm, EFFORTS, GateForm, ProfileForm, ProviderForm,
    },
    setup_config_process::{self, ConfigIntent},
    setup_export_process::{self, ExportIntent},
};

const AUTHORIZATION_DEADLINE: Duration = Duration::from_mins(1);

#[derive(Clone, Debug)]
struct PendingConfig {
    request_id: String,
}

#[derive(Clone, Debug)]
struct PendingAuthorization {
    request_id: String,
    arguments: Vec<String>,
    path: PathBuf,
    bytes: usize,
    sha256: String,
    repository_id: String,
}

#[derive(Clone, Debug)]
struct PendingCampaign {
    request_id: String,
    inspect_arguments: Vec<String>,
    write_arguments: Vec<String>,
    select_arguments: Vec<String>,
    specification_path: PathBuf,
    authorization_path: PathBuf,
    repository_id: String,
    campaign_id: String,
    head: String,
    task_source_sha256: String,
}

#[derive(Clone, Debug)]
struct PendingExport {
    request_id: String,
    arguments: Vec<String>,
    source: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationReceipt {
    schema_version: u64,
    repository_id: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

fn receipt_matches(
    bytes: &[u8],
    pending: &PendingAuthorization,
) -> Result<bool, crate::backend::models::ModelError> {
    let receipt: AuthorizationReceipt =
        serde_json::from_slice(bytes).map_err(|_| crate::backend::models::ModelError::Malformed)?;
    Ok(receipt.schema_version == 1
        && receipt.written
        && receipt.repository_id == pending.repository_id
        && receipt.bytes == pending.bytes
        && receipt.sha256 == pending.sha256)
}

/// Setup screen state kept on the application.
#[derive(Default)]
pub struct SetupState {
    /// Configuration form when editing or creating.
    pub config_form: Option<ConfigForm>,
    pending_config: Option<PendingConfig>,
    pending_config_recovery: Option<String>,
    pending_config_clear: Option<(String, bool)>,
    pub(super) recovery_config: Option<ConfigIntent>,
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
    pending_authorization: Option<PendingAuthorization>,
    pending_campaign: Option<PendingCampaign>,
    pending_export: Option<PendingExport>,
    pending_export_load: Option<String>,
    pending_export_recovery: Option<String>,
    pending_export_clear: Option<(String, bool)>,
    pub(super) recovery_export: Option<ExportIntent>,
    pub(super) recovered_export_after_open: bool,
    /// Directory browser for the target repository.
    pub target_browser: Option<Browser>,
    /// Export destination.
    pub export_path: String,
    /// Replace an existing export destination.
    pub export_overwrite: bool,
    /// Workspace directory proposed for new authority files.
    pub workspace_root: String,
}

impl SetupState {
    pub(super) fn cancel_pending_config(&mut self) {
        let pending_write = self.pending_config.take().is_some();
        let pending_recovery = self.pending_config_recovery.take().is_some();
        let pending_clear = self.pending_config_clear.take().is_some();
        if pending_write || pending_recovery || pending_clear {
            self.message = Some(Err(
                "the selected repository changed during configuration setup; inspect the destination before retrying"
                    .to_owned(),
            ));
        }
    }

    pub(super) fn cancel_matching_config(&mut self, request_id: Option<&str>) {
        if self
            .pending_config
            .as_ref()
            .is_some_and(|pending| Some(pending.request_id.as_str()) == request_id)
            || self.pending_config_recovery.as_deref() == request_id
            || self
                .pending_config_clear
                .as_ref()
                .is_some_and(|(id, _)| Some(id.as_str()) == request_id)
        {
            self.cancel_pending_config();
        }
    }

    pub(super) fn cancel_pending_authorization(&mut self) {
        if self.pending_authorization.take().is_some() {
            self.message = Some(Err("the selected authority changed during the write; inspect the destination before retrying".to_owned()));
        }
    }

    pub(super) fn cancel_pending_campaign(&mut self) {
        if self.pending_campaign.take().is_some() {
            self.message = Some(Err("the selected authority changed during the campaign write; inspect the destination before retrying".to_owned()));
        }
    }

    pub(super) fn cancel_pending_export(&mut self) {
        let pending_write = self.pending_export.take().is_some();
        let pending_load = self.pending_export_load.take().is_some();
        let pending_recovery = self.pending_export_recovery.take().is_some();
        let pending_clear = self.pending_export_clear.take().is_some();
        if pending_write || pending_load || pending_recovery || pending_clear {
            self.message = Some(Err("the selected source changed during export; inspect the destination before retrying".to_owned()));
        }
    }

    pub(super) fn cancel_matching_export(&mut self, request_id: Option<&str>) {
        if self
            .pending_export
            .as_ref()
            .is_some_and(|pending| Some(pending.request_id.as_str()) == request_id)
            || self.pending_export_load.as_deref() == request_id
            || self.pending_export_recovery.as_deref() == request_id
            || self
                .pending_export_clear
                .as_ref()
                .is_some_and(|(id, _)| Some(id.as_str()) == request_id)
        {
            self.cancel_pending_export();
        }
    }

    pub(super) fn cancel_matching_campaign(&mut self, request_id: Option<&str>) {
        if self
            .pending_campaign
            .as_ref()
            .is_some_and(|pending| Some(pending.request_id.as_str()) == request_id)
        {
            self.cancel_pending_campaign();
        }
    }

    pub(super) fn cancel_matching_authorization(&mut self, request_id: Option<&str>) {
        if self
            .pending_authorization
            .as_ref()
            .is_some_and(|pending| Some(pending.request_id.as_str()) == request_id)
        {
            self.cancel_pending_authorization();
        }
    }
}

impl App {
    pub(super) fn load_setup_export_recovery(&mut self) {
        self.setup.recovery_export = None;
        self.setup.recovered_export_after_open = false;
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        let request_id = format!("setup-export-load-{}", self.generation.0);
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupExportLoad {
                directory: directory.clone(),
                config_path: project.config_path.clone(),
            },
            request_id: Some(request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => self.setup.pending_export_load = Some(request_id),
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "private export recovery inspection could not be queued ({}); try again",
                    error.code()
                )));
            }
        }
    }

    pub(super) fn accept_setup_export_load(&mut self, response: Response) -> bool {
        if self.setup.pending_export_load.as_deref() != response.request_id.as_deref() {
            return false;
        }
        self.setup.pending_export_load = None;
        match response.result {
            Ok(bytes) => {
                let Ok(intent) = serde_json::from_slice::<Option<ExportIntent>>(&bytes) else {
                    self.setup.message = Some(Err(
                        "private Setup export recovery state is invalid; inspect it before another export".to_owned(),
                    ));
                    return true;
                };
                if let Some(intent) = intent {
                    if self
                        .project
                        .as_ref()
                        .is_none_or(|project| project.config_path != intent.config_path)
                        || intent.schema_version != 1
                    {
                        self.setup.message = Some(Err(
                            "previous export belongs to another repository observation; inspect its destination".to_owned(),
                        ));
                        return true;
                    }
                    self.setup.recovery_export = Some(intent);
                    self.setup.recovered_export_after_open = true;
                    self.setup.message = Some(Err(
                        "a previous export may still be running or need inspection; check its outcome in Setup".to_owned(),
                    ));
                    if self.diagnosis.value.is_some() {
                        self.reconcile_setup_export();
                    }
                }
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "private Setup export recovery state is invalid ({}); inspect it before another export",
                    error.code()
                )));
            }
        }
        true
    }

    pub(super) fn reconcile_setup_export(&mut self) {
        let Some(intent) = self.setup.recovery_export.clone() else {
            return;
        };
        if self.setup.pending_export.is_some()
            || self.setup.pending_export_load.is_some()
            || self.setup.pending_export_recovery.is_some()
            || self.setup.pending_export_clear.is_some()
        {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(diagnosis) = self.diagnosis.value.as_ref() else {
            return;
        };
        if project.config_path != intent.config_path
            || diagnosis.repository_id != intent.repository_id
        {
            self.setup.message = Some(Err(
                "previous export belongs to another repository observation; inspect its destination"
                    .to_owned(),
            ));
            return;
        }
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.message = Some(Err("private export intent is invalid".to_owned()));
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupExportRecover {
                intent_path: setup_export_process::intent_path(directory, &intent.config_path),
                intent_sha256,
                request_id: intent.request_id.clone(),
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_export_recovery = Some(intent.request_id);
                self.setup.message = None;
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "export outcome inspection could not be queued ({}); try again",
                    error.code()
                )));
            }
        }
    }

    pub(super) fn accept_setup_export_recovery(&mut self, response: Response) -> bool {
        if self.setup.pending_export_recovery.as_deref() != response.request_id.as_deref() {
            return false;
        }
        self.setup.pending_export_recovery = None;
        match response.result {
            Ok(bytes) if bytes.is_empty() => self.clear_setup_export_notice(true),
            Ok(_) => {
                self.setup.message = Some(Err(
                    "export outcome was malformed; inspect its destination before retrying"
                        .to_owned(),
                ));
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "previous export was not confirmed ({}); inspect its destination before retrying",
                    error.code()
                )));
            }
        }
        true
    }

    pub(super) fn clear_setup_export_notice(&mut self, verified: bool) {
        let Some(intent) = self.setup.recovery_export.clone() else {
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(diagnosis) = self.diagnosis.value.as_ref() else {
            self.setup.message = Some(Err(
                "repository identity is unavailable; inspect the export destination before clearing its notice".to_owned(),
            ));
            return;
        };
        if project.config_path != intent.config_path
            || diagnosis.repository_id != intent.repository_id
        {
            self.setup.message = Some(Err(
                "previous export belongs to another repository observation; inspect its destination"
                    .to_owned(),
            ));
            return;
        }
        if self.setup.pending_export.is_some()
            || self.setup.pending_export_load.is_some()
            || self.setup.pending_export_recovery.is_some()
            || self.setup.pending_export_clear.is_some()
        {
            return;
        }
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.message = Some(Err("private export intent is invalid".to_owned()));
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupExportClear {
                intent_path: setup_export_process::intent_path(directory, &intent.config_path),
                config_path: project.config_path.clone(),
                observed_repository_id: diagnosis.repository_id.clone(),
                intent_sha256,
                request_id: intent.request_id.clone(),
                verified,
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_export_clear = Some((intent.request_id, verified));
                self.setup.message = None;
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "export notice cleanup could not be queued ({}); try again",
                    error.code()
                )));
            }
        }
    }

    pub(super) fn accept_setup_export_clear(&mut self, response: &Response) -> bool {
        let Some((request_id, verified)) = self.setup.pending_export_clear.as_ref() else {
            return false;
        };
        if response.request_id.as_deref() != Some(request_id.as_str()) {
            return false;
        }
        let verified = *verified;
        self.setup.pending_export_clear = None;
        if response.result.as_ref().is_ok_and(Vec::is_empty) {
            let destination = self
                .setup
                .recovery_export
                .as_ref()
                .map(|intent| intent.destination.display().to_string())
                .unwrap_or_default();
            let recovered_after_open = self.setup.recovered_export_after_open;
            self.setup.recovery_export = None;
            self.setup.recovered_export_after_open = false;
            self.setup.message = Some(Ok(if verified {
                if recovered_after_open {
                    format!("previous export completed and verified at {destination}")
                } else {
                    format!("exported and verified at {destination}")
                }
            } else {
                "local export notice cleared after destination inspection".to_owned()
            }));
        } else {
            self.setup.message = Some(Err(if verified {
                "export bytes matched, but its recovery record could not be cleared; inspect before retrying".to_owned()
            } else {
                "export is still running or private recovery state changed; check its outcome before clearing".to_owned()
            }));
        }
        true
    }

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
        if self.setup.recovery_config.is_some() {
            self.setup.message = Some(Err(
                "inspect the earlier configuration request before starting another one".to_owned(),
            ));
            return;
        }
        let workspace = self.workspace_root_for(target);
        self.setup.config_form = Some(ConfigForm::defaults(target, &workspace));
        self.setup.message = None;
    }

    /// Starts editing the opened configuration.
    pub fn edit_opened_config(&mut self) {
        if self.setup.recovery_config.is_some() {
            self.setup.message = Some(Err(
                "inspect the earlier configuration request before editing this one".to_owned(),
            ));
            return;
        }
        if let Some(project) = &self.project {
            let mut form = ConfigForm::from_config(&project.config_path, &project.config);
            form.overwrite = true;
            self.setup.config_form = Some(form);
            self.setup.message = None;
        }
    }

    /// Queues an exact, durable public configuration write.
    pub fn apply_config_form(&mut self) {
        if self.setup.pending_config.is_some() || self.setup.recovery_config.is_some() {
            self.setup.message = Some(Err(
                "a prior configuration write needs inspection before another request".to_owned(),
            ));
            return;
        }
        let Some(form) = self.setup.config_form.clone() else {
            return;
        };
        let (repository, destination, arguments, candidate) = match config_submission(&form) {
            Ok(submission) => submission,
            Err(message) => {
                self.setup.message = Some(Err(message));
                return;
            }
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.setup.message = Some(Err(
                "the exact configuration command cannot be shown safely".to_owned(),
            ));
            return;
        }
        let Ok(directory) = self.state_dir.as_ref().cloned() else {
            self.setup.message = Some(Err(
                "private recovery state is unavailable; no configuration write was started"
                    .to_owned(),
            ));
            return;
        };
        let Ok(intent) = setup_config_process::prepare(
            &directory,
            &repository,
            &destination,
            arguments,
            candidate,
        ) else {
            self.setup.message = Some(Err(
                "a prior write may be unresolved or private recovery state is unavailable; inspect it before retrying"
                    .to_owned(),
            ));
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.recovery_config = Some(intent);
            self.setup.message = Some(Err(
                "configuration intent could not be bound; inspect private recovery state"
                    .to_owned(),
            ));
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupConfig {
                intent_path: setup_config_process::intent_path(&directory),
                intent_sha256,
                deadline: AUTHORIZATION_DEADLINE,
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_config = Some(PendingConfig {
                    request_id: intent.request_id.clone(),
                });
                self.setup.recovery_config = Some(intent);
                self.setup.message = None;
            }
            Err(error) => {
                self.setup.recovery_config = Some(intent);
                self.setup.message = Some(Err(format!(
                    "configuration request was not queued ({}); inspect private recovery state before retrying",
                    error.code(),
                )));
            }
        }
    }

    /// Inspects the private terminal result and exact named destination after reopen.
    pub fn reconcile_config_write(&mut self) {
        let Some(intent) = self.setup.recovery_config.clone() else {
            return;
        };
        if self.setup.pending_config.is_some()
            || self.setup.pending_config_recovery.is_some()
            || self.setup.pending_config_clear.is_some()
        {
            return;
        }
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.message = Some(Err("configuration recovery intent is invalid".to_owned()));
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupConfigRecover {
                intent_path: setup_config_process::intent_path(directory),
                intent_sha256,
                request_id: intent.request_id.clone(),
                deadline: AUTHORIZATION_DEADLINE,
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_config_recovery = Some(intent.request_id);
                self.setup.message = None;
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "configuration outcome inspection could not be queued ({}); try again",
                    error.code()
                )));
            }
        }
    }

    pub(super) fn accept_config_write(&mut self, response: &Response) -> bool {
        let Some(pending) = self.setup.pending_config.take() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            self.setup.pending_config = Some(pending);
            return false;
        }
        self.reconcile_config_write();
        true
    }

    pub(super) fn accept_config_recovery(&mut self, response: Response) -> bool {
        let Some(request_id) = self.setup.pending_config_recovery.as_ref() else {
            return false;
        };
        if response.request_id.as_deref() != Some(request_id.as_str()) {
            return false;
        }
        self.setup.pending_config_recovery = None;
        let Some(intent) = self.setup.recovery_config.clone() else {
            return false;
        };
        match response.result {
            Ok(bytes) => {
                let project =
                    Project::from_snapshot_for_write(&intent.destination, &intent.sha256, &bytes);
                if let Ok(project) = project
                    && intent.matches_config(&project.config)
                {
                    self.open_loaded_project(project);
                    self.clear_config_notice(true);
                } else {
                    self.setup.message = Some(Err(
                        "configuration snapshot changed before selection; inspect the destination"
                            .to_owned(),
                    ));
                }
            }
            Err(error) => {
                let code = error.code();
                self.setup.message = Some(Err(format!(
                    "configuration write was not confirmed ({code}); inspect its destination before retrying"
                )));
            }
        }
        true
    }

    fn clear_config_notice(&mut self, verified: bool) {
        let Some(intent) = self.setup.recovery_config.clone() else {
            return;
        };
        if self.setup.pending_config.is_some()
            || self.setup.pending_config_recovery.is_some()
            || self.setup.pending_config_clear.is_some()
        {
            return;
        }
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.message = Some(Err("configuration recovery intent is invalid".to_owned()));
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupConfigClear {
                intent_path: setup_config_process::intent_path(directory),
                intent_sha256,
                request_id: intent.request_id.clone(),
                verified,
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_config_clear = Some((intent.request_id, verified));
                self.setup.message = None;
            }
            Err(error) => {
                if verified {
                    self.reset_project_selection();
                }
                self.setup.message = Some(Err(format!(
                    "configuration notice could not be cleared ({}); try again",
                    error.code()
                )));
            }
        }
    }

    pub(super) fn accept_config_clear(&mut self, response: &Response) -> bool {
        let Some((request_id, verified)) = self.setup.pending_config_clear.as_ref() else {
            return false;
        };
        if response.request_id.as_deref() != Some(request_id.as_str()) {
            return false;
        }
        let verified = *verified;
        self.setup.pending_config_clear = None;
        if response.result.is_ok() {
            let destination = self
                .setup
                .recovery_config
                .as_ref()
                .map(|intent| intent.destination.display().to_string());
            self.setup.recovery_config = None;
            self.setup.message = Some(Ok(if verified {
                format!(
                    "configuration written and verified at {}",
                    destination.unwrap_or_default()
                )
            } else {
                "local configuration notice cleared after destination inspection".to_owned()
            }));
        } else {
            if verified {
                self.reset_project_selection();
            }
            self.setup.message = Some(Err(if verified {
                "configuration was verified but its recovery record could not be cleared; inspect the destination and retry"
                    .to_owned()
            } else {
                "configuration helper is running or private recovery state changed; check again before clearing"
                    .to_owned()
            }));
        }
        true
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

    /// Queues an inspected campaign write and selects only a matching receipt.
    pub fn apply_campaign_form(&mut self) {
        if self.setup.pending_campaign.is_some() {
            self.setup.message = Some(Err(
                "a campaign write is already pending; inspect its result before retrying"
                    .to_owned(),
            ));
            return;
        }
        let Some(binding) = self.campaign_binding() else {
            self.setup.message = Some(Err(
                "a live repository diagnosis is required before authoring a campaign".to_owned(),
            ));
            return;
        };
        let Some(form) = self.setup.campaign_form.clone() else {
            return;
        };
        let Some((inspect_arguments, write_arguments)) =
            self.campaign_setup_arguments(&form, &binding)
        else {
            self.setup.message = Some(Err("open and diagnose the repository, then choose absolute specification and authorization paths".to_owned()));
            return;
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&inspect_arguments))
            || !command::can_preview(self.binary_path.as_deref(), Some(&write_arguments))
        {
            self.setup.message = Some(Err(
                "the exact commands cannot be shown safely; choose representable paths".to_owned(),
            ));
            return;
        }
        let Some(select_arguments) = campaign_select_arguments(Path::new(form.spec_path.trim()))
        else {
            self.setup.message = Some(Err(
                "the campaign destination cannot be selected through the coordinator".to_owned(),
            ));
            return;
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&select_arguments)) {
            self.setup.message = Some(Err(
                "the exact campaign selection command cannot be shown safely".to_owned(),
            ));
            return;
        }
        self.next_evidence_request = self.next_evidence_request.wrapping_add(1);
        let request_id = format!("setup-campaign-{}", self.next_evidence_request);
        let pending = PendingCampaign {
            request_id: request_id.clone(),
            inspect_arguments: inspect_arguments.clone(),
            write_arguments: write_arguments.clone(),
            select_arguments: select_arguments.clone(),
            specification_path: PathBuf::from(form.spec_path.trim()),
            authorization_path: PathBuf::from(form.authorization_path.trim()),
            repository_id: binding.repository_id.clone(),
            campaign_id: form.campaign_id.trim().to_owned(),
            head: binding.initial_commit.clone(),
            task_source_sha256: binding.task_source_sha256.clone(),
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::CampaignSetup {
                form: Box::new(form),
                source: binding,
                inspect_arguments,
                write_arguments,
                select_arguments,
                deadline: AUTHORIZATION_DEADLINE,
            },
            request_id: Some(request_id),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_campaign = Some(pending);
                self.setup.message = Some(Ok(
                    "campaign write pending; inspect the result before retrying".to_owned(),
                ));
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "campaign write was not queued: {}",
                    error.code()
                )));
            }
        }
    }

    fn campaign_setup_arguments(
        &self,
        form: &CampaignForm,
        binding: &CampaignBinding,
    ) -> Option<(Vec<String>, Vec<String>)> {
        let config = self.project.as_ref()?.config_path.to_str()?.to_owned();
        let authorization = Path::new(form.authorization_path.trim());
        let output = Path::new(form.spec_path.trim());
        if !authorization.is_absolute() || !output.is_absolute() {
            return None;
        }
        let common = vec![
            "--config".to_owned(),
            config,
            "--repository-id".to_owned(),
            binding.repository_id.clone(),
            "--head".to_owned(),
            binding.initial_commit.clone(),
            "--task-source-sha256".to_owned(),
            binding.task_source_sha256.clone(),
            "--authorization".to_owned(),
            authorization.to_str()?.to_owned(),
        ];
        let mut inspect = vec!["setup-inspect-authorization".to_owned()];
        inspect.extend(common.clone());
        let mut write = vec!["setup-write-campaign".to_owned()];
        write.extend(common);
        write.extend([
            "--output".to_owned(),
            output.to_str()?.to_owned(),
            "--overwrite".to_owned(),
            form.overwrite.to_string(),
        ]);
        Some((inspect, write))
    }

    fn campaign_preview_arguments(&self) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
        self.setup
            .pending_campaign
            .as_ref()
            .map(|pending| {
                (
                    pending.inspect_arguments.clone(),
                    pending.write_arguments.clone(),
                    pending.select_arguments.clone(),
                )
            })
            .or_else(|| {
                let (inspect, write) = self.campaign_setup_arguments(
                    self.setup.campaign_form.as_ref()?,
                    &self.campaign_binding()?,
                )?;
                let select = campaign_select_arguments(Path::new(
                    self.setup.campaign_form.as_ref()?.spec_path.trim(),
                ))?;
                Some((inspect, write, select))
            })
    }

    pub(super) fn accept_campaign_write(&mut self, response: Response) -> bool {
        let Some(pending) = self.setup.pending_campaign.take() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            self.setup.pending_campaign = Some(pending);
            return false;
        }
        if !self.campaign_binding().is_some_and(|current| {
            current.repository_id == pending.repository_id
                && current.initial_commit == pending.head
                && current.task_source_sha256 == pending.task_source_sha256
        }) {
            self.setup.message = Some(Err("the repository diagnosis changed during the campaign write; inspect the destination before retrying".to_owned()));
            return true;
        }
        let selection = response
            .result
            .map_err(crate::campaign::SelectError::Backend)
            .and_then(|bytes| {
                let project = self
                    .project
                    .as_ref()
                    .ok_or(crate::campaign::SelectError::Contract)?;
                let selected = crate::campaign::CampaignSelection::from_snapshot(
                    &pending.specification_path,
                    &project.config.target_path,
                    Some(&pending.repository_id),
                    &bytes,
                )?;
                if selected.spec.campaign_id != pending.campaign_id
                    || selected.spec.initial_commit != pending.head
                    || selected.spec.task_source_sha256 != pending.task_source_sha256
                {
                    return Err(crate::campaign::SelectError::Contract);
                }
                Ok(selected)
            });
        match selection {
            Ok(selection) => {
                self.adopt_written_campaign(selection);
                self.set_authorization_record(&pending.authorization_path);
                self.setup.message = Some(Ok(format!(
                    "campaign specification written and verified at {}",
                    pending.specification_path.display()
                )));
            }
            Err(error) => {
                if self
                    .campaign
                    .as_ref()
                    .is_some_and(|selected| selected.spec_path == pending.specification_path)
                {
                    self.clear_campaign();
                }
                self.campaign_error = Some(error.clone());
                self.set_status(format!("campaign selection refused: {error}"));
                self.setup.message = Some(Err(format!(
                    "campaign write or selection not confirmed: {error}. Inspect the destination before retrying."
                )));
            }
        }
        true
    }

    /// Writes the authorization record from the typed text.
    pub fn apply_authorization_record(&mut self) {
        if self.setup.pending_authorization.is_some() {
            self.setup.message = Some(Err(
                "an authorization write is already pending; inspect its result before retrying"
                    .to_owned(),
            ));
            return;
        }
        let Some(arguments) = self.authorization_arguments() else {
            self.setup.message = Some(Err(
                "open and diagnose the repository, then choose an absolute record path".to_owned(),
            ));
            return;
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.setup.message = Some(Err(
                "the exact command cannot be shown safely; choose a representable path".to_owned(),
            ));
            return;
        }
        let input = match PrivateInput::new(self.setup.authorization_text.as_bytes().to_vec()) {
            Ok(input)
                if !self.setup.authorization_text.trim().is_empty()
                    && !self.setup.authorization_text.contains('\0') =>
            {
                input
            }
            _ => {
                self.setup.message = Some(Err(
                    "enter nonempty authorization text of at most 1 MiB without NUL bytes"
                        .to_owned(),
                ));
                return;
            }
        };
        let repository_id = self
            .diagnosis
            .value
            .as_ref()
            .map(|value| value.repository_id.clone())
            .unwrap_or_default();
        let path = PathBuf::from(self.setup.authorization_path.trim());
        self.next_evidence_request = self.next_evidence_request.wrapping_add(1);
        let request_id = format!("setup-authorization-{}", self.next_evidence_request);
        let pending = PendingAuthorization {
            request_id: request_id.clone(),
            arguments: arguments.clone(),
            path,
            bytes: self.setup.authorization_text.len(),
            sha256: hex(&Sha256::digest(self.setup.authorization_text.as_bytes())),
            repository_id,
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::PrivateCommand {
                label: "setup-write-authorization",
                arguments,
                input,
                deadline: AUTHORIZATION_DEADLINE,
            },
            request_id: Some(request_id),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_authorization = Some(pending);
                self.setup.message = Some(Ok(
                    "authorization write pending; inspect the result before retrying".to_owned(),
                ));
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "authorization write was not queued: {}",
                    error.code()
                )));
            }
        }
    }

    fn authorization_arguments(&self) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let diagnosis = self.diagnosis.value.as_ref()?;
        let path = Path::new(self.setup.authorization_path.trim());
        if !path.is_absolute() {
            return None;
        }
        Some(vec![
            "setup-write-authorization".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--repository-id".to_owned(),
            diagnosis.repository_id.clone(),
            "--output".to_owned(),
            path.to_str()?.to_owned(),
            "--overwrite".to_owned(),
            self.setup.authorization_overwrite.to_string(),
        ])
    }

    fn authorization_preview_arguments(&self) -> Option<Vec<String>> {
        self.setup
            .pending_authorization
            .as_ref()
            .map(|pending| pending.arguments.clone())
            .or_else(|| self.authorization_arguments())
    }

    pub(super) fn accept_authorization_write(&mut self, response: Response) -> bool {
        let Some(pending) = self.setup.pending_authorization.take() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            self.setup.pending_authorization = Some(pending);
            return false;
        }
        let matches = response.result.and_then(|bytes| {
            receipt_matches(&bytes, &pending).map_err(crate::backend::BackendError::Contract)
        });
        match matches {
            Ok(true) => {
                self.setup.message = Some(Ok(format!("authorization record written at {} (sha256 {})", pending.path.display(), pending.sha256)));
                if let Some(form) = &mut self.setup.campaign_form {
                    form.authorization_path = pending.path.display().to_string();
                }
                if hex(&Sha256::digest(self.setup.authorization_text.as_bytes())) == pending.sha256 {
                    self.setup.authorization_text.clear();
                }
                self.set_authorization_record(&pending.path);
            }
            Ok(false) => self.setup.message = Some(Err("Setup receipt did not match the requested repository or exact input; inspect the destination before retrying".to_owned())),
            Err(error) => {
                let code = error.code();
                let (cause, action) = crate::backend::explain_code(&code);
                self.setup.message = Some(Err(format!(
                    "authorization write not confirmed: {cause} ({code}) {action} Inspect the destination before retrying."
                )));
            }
        }
        true
    }

    /// Exports the opened configuration or selected campaign to another path.
    pub fn export_document(&mut self, source: &Path) {
        if self.setup.pending_export.is_some()
            || self.setup.pending_export_load.is_some()
            || self.setup.pending_export_recovery.is_some()
            || self.setup.pending_export_clear.is_some()
            || self.setup.recovery_export.is_some()
        {
            self.setup.message = Some(Err(
                "an export is already pending or needs destination inspection before retrying"
                    .to_owned(),
            ));
            return;
        }
        let selected = self
            .project
            .as_ref()
            .is_some_and(|project| project.config_path == source)
            || self
                .campaign
                .as_ref()
                .is_some_and(|campaign| campaign.spec_path == source);
        if !selected {
            self.setup.message = Some(Err(
                "select the configuration or campaign before exporting it".to_owned(),
            ));
            return;
        }
        let Some(arguments) = self.export_arguments(source) else {
            self.setup.message = Some(Err(
                "open and diagnose the repository, then choose an absolute source and destination"
                    .to_owned(),
            ));
            return;
        };
        if !command::can_preview(self.binary_path.as_deref(), Some(&arguments)) {
            self.setup.message = Some(Err(
                "the exact export command cannot be shown safely; choose representable paths"
                    .to_owned(),
            ));
            return;
        }
        self.submit_export_intent(source, &arguments);
    }

    fn submit_export_intent(&mut self, source: &Path, arguments: &[String]) {
        let Some(repository_id) = self
            .diagnosis
            .value
            .as_ref()
            .map(|value| value.repository_id.clone())
        else {
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Ok(directory) = self.state_dir.as_ref().cloned() else {
            self.setup.message = Some(Err(
                "private recovery state is unavailable; export was not started".to_owned(),
            ));
            return;
        };
        let destination = PathBuf::from(self.setup.export_path.trim());
        let Ok(intent) = setup_export_process::prepare(
            &directory,
            &project.config_path,
            &repository_id,
            source,
            &destination,
            arguments.to_vec(),
        ) else {
            self.setup.message = Some(Err(
                "a prior export may be unresolved or private recovery state is unavailable; inspect the destination before retrying".to_owned(),
            ));
            return;
        };
        let Ok(intent_sha256) = intent.fingerprint() else {
            self.setup.recovery_export = Some(intent);
            self.setup.message = Some(Err(
                "private export intent could not be bound; inspect recovery state before retrying"
                    .to_owned(),
            ));
            return;
        };
        let pending = PendingExport {
            request_id: intent.request_id.clone(),
            arguments: arguments.to_vec(),
            source: source.to_path_buf(),
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::SetupExport {
                intent_path: setup_export_process::intent_path(&directory, &project.config_path),
                intent_sha256,
                deadline: AUTHORIZATION_DEADLINE,
            },
            request_id: Some(intent.request_id.clone()),
        };
        match self.submit(request) {
            Ok(()) => {
                self.setup.pending_export = Some(pending);
                self.setup.recovery_export = Some(intent);
                self.setup.recovered_export_after_open = false;
                self.setup.message = Some(Ok(
                    "export pending under its own helper; closing this view does not cancel it"
                        .to_owned(),
                ));
            }
            Err(error) => {
                self.setup.recovery_export = Some(intent);
                self.setup.message = Some(Err(format!(
                    "export was not queued: {}; {}",
                    error.code(),
                    "inspect the destination and clear the private notice before retrying"
                )));
            }
        }
    }

    fn export_arguments(&self, source: &Path) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let diagnosis = self.diagnosis.value.as_ref()?;
        let output = Path::new(self.setup.export_path.trim());
        if !source.is_absolute() || !output.is_absolute() {
            return None;
        }
        let mut arguments = vec![
            "setup-export-copy".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
            "--repository-id".to_owned(),
            diagnosis.repository_id.clone(),
            "--source".to_owned(),
            source.to_str()?.to_owned(),
            "--output".to_owned(),
            output.to_str()?.to_owned(),
        ];
        if let Some(campaign) = self
            .campaign
            .as_ref()
            .filter(|campaign| campaign.spec_path == source)
        {
            arguments.extend([
                "--campaign-authority-sha256".to_owned(),
                campaign.authority_sha256.clone(),
            ]);
        }
        arguments.extend([
            "--overwrite".to_owned(),
            self.setup.export_overwrite.to_string(),
        ]);
        Some(arguments)
    }

    fn export_preview_arguments(&self, source: &Path) -> Option<Vec<String>> {
        self.setup
            .pending_export
            .as_ref()
            .filter(|pending| pending.source == source)
            .map(|pending| pending.arguments.clone())
            .or_else(|| {
                self.setup
                    .recovery_export
                    .as_ref()
                    .filter(|intent| intent.source == source)
                    .map(|intent| intent.arguments.clone())
            })
            .or_else(|| self.export_arguments(source))
    }

    pub(super) fn accept_setup_export(&mut self, response: Response) -> bool {
        let Some(pending) = self.setup.pending_export.take() else {
            return false;
        };
        if response.request_id.as_deref() != Some(pending.request_id.as_str()) {
            self.setup.pending_export = Some(pending);
            return false;
        }
        match response.result {
            Ok(_) => self.reconcile_setup_export(),
            Err(error) => {
                let code = error.code();
                let (cause, action) = crate::backend::explain_code(&code);
                self.setup.message = Some(Err(format!(
                    "export not confirmed: {cause} ({code}) {action} Inspect the destination before retrying."
                )));
            }
        }
        true
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
        let catalogue = messages::english();
        ui.heading(catalogue.text("setup_title"));
        ui.label(catalogue.text("setup_intro"));
        if let Some(message) = &self.setup.message {
            match message {
                Ok(text) => {
                    ui.colored_label(super::current_tokens(ui.ctx()).success, text);
                }
                Err(text) => failure_box(
                    ui,
                    catalogue.text("setup_refused"),
                    text,
                    catalogue.text("setup_correct_field"),
                ),
            }
        }
        ui.separator();
        ui.heading(catalogue.text("setup_open_heading"));
        self.open_controls(ui);
        ui.horizontal(|ui| {
            let label = ui.label(catalogue.text("setup_workspace_directory"));
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.workspace_root)
                    .hint_text(catalogue.text("setup_workspace_directory_hint"))
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_path),
            )
            .labelled_by(label.id);
        });
        self.target_selection(ui);
        if self.project.is_some()
            && ui
                .button(catalogue.text("setup_edit_configuration"))
                .clicked()
        {
            self.edit_opened_config();
        }
        self.config_form_view(ui);
        ui.separator();
        ui.heading(catalogue.text("setup_authorization_heading"));
        self.authorization_view(ui);
        ui.separator();
        ui.heading(catalogue.text("setup_campaign_heading"));
        self.campaign_form_view(ui);
        ui.separator();
        ui.heading(catalogue.text("setup_export_heading"));
        self.export_view(ui);
    }

    fn target_selection(&mut self, ui: &mut egui::Ui) {
        let catalogue = messages::english();
        ui.horizontal(|ui| {
            if ui
                .button(if self.setup.target_browser.is_some() {
                    catalogue.text("setup_hide_browser")
                } else {
                    catalogue.text("setup_choose_repository")
                })
                .clicked()
            {
                self.setup.target_browser = match self.setup.target_browser.take() {
                    Some(_) => None,
                    None => Some(Browser::at_home(vec!["never-match"])),
                };
                if self.setup.target_browser.is_some() {
                    self.request_browser(BrowserSlot::Target);
                }
            }
        });
        let mut chosen = None;
        let mut enter = None;
        let mut up = false;
        let mut refresh = false;
        if let Some(browser) = &self.setup.target_browser {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(catalogue.text("setup_browser_up")).clicked() {
                        up = true;
                    }
                    if ui.button("Refresh").clicked() {
                        refresh = true;
                    }
                    ui.monospace(content::list_label(&browser.current.display().to_string()));
                    if ui
                        .add_enabled(
                            browser.ready,
                            egui::Button::new(catalogue.text("setup_use_target")),
                        )
                        .clicked()
                    {
                        chosen = Some(browser.current.clone());
                    }
                });
                if browser.loading {
                    ui.label("Loading directory through the coordinator…");
                }
                if let Some(arguments) = directory_list_arguments(&browser.current) {
                    command::show_for(
                        ui,
                        "browse repository directories",
                        self.binary_path.as_deref(),
                        &arguments,
                    );
                } else {
                    command::show_unavailable_for(ui, "browse repository directories");
                }
                if let Some(error) = &browser.error {
                    ui.label(error);
                }
                if browser.truncated {
                    ui.small("Listing truncated; navigate into a narrower directory.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("target-browser")
                    .max_height(super::current_tokens(ui.ctx()).layout.preview_short)
                    .show(ui, |ui| {
                        for entry in browser.entries.iter().filter(|entry| entry.is_dir) {
                            if ui
                                .add_enabled(
                                    !entry.is_symlink,
                                    egui::Button::new(format!(
                                        "{}/",
                                        content::list_label(&entry.name)
                                    )),
                                )
                                .clicked()
                            {
                                enter = Some(entry.path.clone());
                            }
                        }
                    });
            });
        }
        let mut moved = false;
        if let Some(browser) = &mut self.setup.target_browser {
            if up {
                moved |= browser.up();
            }
            if let Some(path) = enter {
                moved |= browser.enter(&path);
            }
        }
        if moved || refresh {
            self.request_browser(BrowserSlot::Target);
        }
        if let Some(target) = chosen {
            self.start_config_form(&target);
        }
    }

    fn config_form_view(&mut self, ui: &mut egui::Ui) {
        let catalogue = messages::english();
        if let Some(intent) = self.setup.recovery_config.clone() {
            ui.label(catalogue.format(
                "setup_config_previous_destination",
                &[("destination", &intent.destination.display().to_string())],
            ));
            if self.setup.pending_config.is_some()
                || self.setup.pending_config_recovery.is_some()
                || self.setup.pending_config_clear.is_some()
            {
                ui.small(catalogue.text("setup_config_pending"));
            } else {
                if ui
                    .button(catalogue.text("setup_config_check_outcome"))
                    .clicked()
                {
                    self.reconcile_config_write();
                }
                if ui
                    .button(catalogue.text("setup_config_clear_notice"))
                    .clicked()
                {
                    self.clear_config_notice(false);
                }
            }
            command::show_for(
                ui,
                catalogue.text("setup_config_command_label"),
                self.binary_path.as_deref(),
                &intent.arguments,
            );
            ui.small(catalogue.text("setup_config_recovery_stdin"));
        }
        let Some(form) = &mut self.setup.config_form else {
            return;
        };
        let editable = self.setup.pending_config.is_none() && self.setup.recovery_config.is_none();
        let preview = config_arguments(
            Path::new(form.target_path.trim()),
            Path::new(form.config_path.trim()),
            form.overwrite,
        );
        if self.setup.recovery_config.is_none() {
            if let Some(arguments) = &preview {
                command::show_for(
                    ui,
                    catalogue.text("setup_config_command_label"),
                    self.binary_path.as_deref(),
                    arguments,
                );
                ui.small(catalogue.text("setup_config_submission_stdin"));
            } else {
                command::show_unavailable_for(ui, catalogue.text("setup_config_command_label"));
            }
        }
        let (apply, discard) = ui
            .add_enabled_ui(editable, |ui| config_form_body(ui, form, catalogue))
            .inner;
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
        let pending = self.setup.pending_authorization.is_some();
        if pending {
            ui.small("A write is pending. The fields are locked until its result is known; Show command displays the submitted command.");
        }
        ui.horizontal(|ui| {
            let label = ui.label("Record file");
            ui.add_enabled(
                !pending,
                egui::TextEdit::singleline(&mut self.setup.authorization_path)
                    .hint_text("/absolute/path/operator-authorization.txt")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
        });
        ui.add_enabled(
            !pending,
            egui::TextEdit::multiline(&mut self.setup.authorization_text)
                .hint_text(
                    "I authorize campaign ... on repository ... with local-only publication.",
                )
                .desired_rows(3)
                .desired_width(super::current_tokens(ui.ctx()).layout.field_wide),
        );
        ui.horizontal(|ui| {
            ui.add_enabled(
                !pending,
                egui::Checkbox::new(
                    &mut self.setup.authorization_overwrite,
                    "Replace an existing record",
                ),
            );
            let arguments = self.authorization_arguments();
            let can_run = self.setup.pending_authorization.is_none()
                && command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
            if ui
                .add_enabled(can_run, egui::Button::new("Write authorization record"))
                .clicked()
            {
                self.apply_authorization_record();
            }
        });
        if let Some(arguments) = self.authorization_preview_arguments() {
            command::show_for(
                ui,
                "write authorization record",
                self.binary_path.as_deref(),
                &arguments,
            );
            ui.small("The typed record is sent through private stdin. It is intentionally absent from the command preview and process arguments.");
        } else {
            command::show_unavailable_for(ui, "write authorization record");
        }
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
        let can_write = self.setup.pending_campaign.is_none()
            && self
                .campaign_preview_arguments()
                .is_some_and(|(inspect, write, select)| {
                    command::can_preview(self.binary_path.as_deref(), Some(&inspect))
                        && command::can_preview(self.binary_path.as_deref(), Some(&write))
                        && command::can_preview(self.binary_path.as_deref(), Some(&select))
                });
        let (apply, discard) = ui
            .add_enabled_ui(self.setup.pending_campaign.is_none(), |ui| match &mut self
                .setup
                .campaign_form
            {
                Some(form) => campaign_form_body(ui, form, can_write),
                None => (false, false),
            })
            .inner;
        if apply {
            self.apply_campaign_form();
        }
        if discard {
            self.setup.campaign_form = None;
        }
        if let Some((inspect, write, select)) = self.campaign_preview_arguments() {
            command::show_for(
                ui,
                "inspect authorization record",
                self.binary_path.as_deref(),
                &inspect,
            );
            command::show_for(
                ui,
                "write campaign specification",
                self.binary_path.as_deref(),
                &write,
            );
            command::show_for(
                ui,
                "select written campaign",
                self.binary_path.as_deref(),
                &select,
            );
            ui.small("The candidate specification is sent through private stdin. It is absent from command arguments; the coordinator checks the record and source again before publication.");
        } else {
            command::show_unavailable_for(ui, "inspect authorization record");
            command::show_unavailable_for(ui, "write campaign specification");
            command::show_unavailable_for(ui, "select written campaign");
        }
    }

    fn export_recovery_controls(&mut self, ui: &mut egui::Ui) {
        if self.setup.pending_export_load.is_some() {
            ui.label("Checking private export recovery state…");
        }
        if let Some(intent) = self.setup.recovery_export.clone() {
            let repository_matches =
                self.project.as_ref().is_some_and(|project| {
                    project.config_path == intent.config_path
                        && self.diagnosis.value.as_ref().is_some_and(|diagnosis| {
                            diagnosis.repository_id == intent.repository_id
                        })
                });
            ui.label(format!(
                "Previous export destination: {}. Its outcome remains bound to the original repository and request.",
                intent.destination.display()
            ));
            if self.setup.pending_export_recovery.is_some() {
                ui.label("Checking the previous export and its destination…");
            }
            if self.setup.pending_export_clear.is_some() {
                ui.label("Clearing the local export notice…");
            }
            if ui
                .add_enabled(
                    self.setup.pending_export.is_none()
                        && self.setup.pending_export_recovery.is_none()
                        && self.setup.pending_export_clear.is_none(),
                    egui::Button::new("Check previous export outcome"),
                )
                .clicked()
            {
                self.reconcile_setup_export();
            }
            if ui
                .add_enabled(
                    repository_matches
                        && self.setup.pending_export.is_none()
                        && self.setup.pending_export_load.is_none()
                        && self.setup.pending_export_recovery.is_none()
                        && self.setup.pending_export_clear.is_none(),
                    egui::Button::new("I inspected the destination; clear export notice"),
                )
                .clicked()
            {
                self.clear_setup_export_notice(false);
            }
            if !repository_matches {
                ui.small("This export belongs to a different repository observation. Keep its recovery record and inspect its destination from the original repository.");
            }
            ui.small("These recovery controls only inspect or clear the private notice. They do not run a coordinator command or stop an export already in progress.");
        }
    }

    fn export_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Import means opening an existing configuration or selecting an existing campaign above. Export copies the validated file elsewhere; digests and paths inside it are unchanged and no credential exists to leak.");
        self.export_recovery_controls(ui);
        let editable = self.setup.pending_export.is_none()
            && self.setup.pending_export_load.is_none()
            && self.setup.pending_export_recovery.is_none()
            && self.setup.pending_export_clear.is_none()
            && self.setup.recovery_export.is_none();
        ui.horizontal(|ui| {
            let label = ui.label("Export destination");
            ui.add_enabled(
                editable,
                egui::TextEdit::singleline(&mut self.setup.export_path)
                    .hint_text("/absolute/path/copy.toml")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_long),
            )
            .labelled_by(label.id);
            ui.add_enabled(
                editable,
                egui::Checkbox::new(&mut self.setup.export_overwrite, "Replace"),
            );
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
            if let Some(source) = config_source.as_ref()
                && ui
                    .add_enabled(
                        editable
                            && self.export_arguments(source).is_some_and(|arguments| {
                                command::can_preview(self.binary_path.as_deref(), Some(&arguments))
                            }),
                        egui::Button::new("Export configuration"),
                    )
                    .clicked()
            {
                self.export_document(source);
            }
            if let Some(source) = campaign_source.as_ref()
                && ui
                    .add_enabled(
                        editable
                            && self.export_arguments(source).is_some_and(|arguments| {
                                command::can_preview(self.binary_path.as_deref(), Some(&arguments))
                            }),
                        egui::Button::new("Export campaign"),
                    )
                    .clicked()
            {
                self.export_document(source);
            }
        });
        for (label, source) in [
            ("export configuration", config_source.as_deref()),
            ("export campaign", campaign_source.as_deref()),
        ] {
            if let Some(source) = source {
                if let Some(arguments) = self.export_preview_arguments(source) {
                    command::show_for(ui, label, self.binary_path.as_deref(), &arguments);
                } else {
                    command::show_unavailable_for(ui, label);
                }
            }
        }
    }
}

fn config_submission(form: &ConfigForm) -> Result<(PathBuf, PathBuf, Vec<String>, String), String> {
    let config = form.build().map_err(|errors| {
        errors
            .iter()
            .map(|error| format!("{}: {}", error.field, error.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let candidate =
        toml::to_string_pretty(&config).map_err(|_| "configuration encoding failed".to_owned())?;
    let repository = config.target_path;
    let destination = PathBuf::from(form.config_path.trim());
    let arguments =
        config_arguments(&repository, &destination, form.overwrite).ok_or_else(|| {
            "choose absolute, representable repository and configuration paths".to_owned()
        })?;
    Ok((repository, destination, arguments, candidate))
}

fn config_arguments(repository: &Path, destination: &Path, overwrite: bool) -> Option<Vec<String>> {
    if !repository.is_absolute() || !destination.is_absolute() {
        return None;
    }
    Some(vec![
        "setup-write-config".to_owned(),
        "--repo".to_owned(),
        repository.to_str()?.to_owned(),
        "--output".to_owned(),
        destination.to_str()?.to_owned(),
        "--overwrite".to_owned(),
        overwrite.to_string(),
    ])
}

fn config_form_body(
    ui: &mut egui::Ui,
    form: &mut ConfigForm,
    catalogue: &Catalogue,
) -> (bool, bool) {
    let mut apply = false;
    let mut discard = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong(catalogue.text("setup_config_title"));
        config_fields(ui, form, catalogue);
        config_profiles(ui, form, catalogue);
        config_gates(ui, form, catalogue);
        config_publication(ui, form, catalogue);
        ui.checkbox(
            &mut form.allow_parent_discovery,
            catalogue.text("setup_config_parent_discovery"),
        );
        ui.checkbox(
            &mut form.overwrite,
            catalogue.text("setup_config_overwrite"),
        );
        ui.horizontal(|ui| {
            if ui.button(catalogue.text("setup_config_apply")).clicked() {
                apply = true;
            }
            if ui.button(catalogue.text("setup_config_discard")).clicked() {
                discard = true;
            }
        });
    });
    (apply, discard)
}

fn config_fields(ui: &mut egui::Ui, form: &mut ConfigForm, catalogue: &Catalogue) {
    egui::Grid::new("config-form")
        .num_columns(2)
        .spacing(super::current_tokens(ui.ctx()).layout.grid)
        .show(ui, |ui| {
            for (label, value) in [
                (catalogue.text("setup_config_file"), &mut form.config_path),
                (catalogue.text("setup_config_target"), &mut form.target_path),
                (
                    catalogue.text("setup_config_task_source"),
                    &mut form.task_source,
                ),
                (
                    catalogue.text("setup_config_default_branch"),
                    &mut form.default_branch,
                ),
                (
                    catalogue.text("setup_config_integration_branch"),
                    &mut form.integration_branch,
                ),
                (
                    catalogue.text("setup_config_scratch_root"),
                    &mut form.scratch_root,
                ),
                (
                    catalogue.text("setup_config_state_root"),
                    &mut form.state_root,
                ),
                (
                    catalogue.text("setup_config_correction_limit"),
                    &mut form.correction_limit,
                ),
            ] {
                let caption = ui.label(label);
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(super::current_tokens(ui.ctx()).layout.field_full),
                )
                .labelled_by(caption.id);
                ui.end_row();
            }
        });
}

fn config_profiles(ui: &mut egui::Ui, form: &mut ConfigForm, catalogue: &Catalogue) {
    ui.label(catalogue.text("setup_config_profiles"));
    let mut remove_profile = None;
    for (index, profile) in form.profiles.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut profile.id)
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_short),
            );
            ui.add(
                egui::TextEdit::singleline(&mut profile.provider)
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_tiny),
            );
            ui.add(
                egui::TextEdit::singleline(&mut profile.model)
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_small),
            );
            if ui
                .button(catalogue.text("setup_config_remove_profile"))
                .clicked()
            {
                remove_profile = Some(index);
            }
        });
    }
    if let Some(index) = remove_profile {
        form.profiles.remove(index);
    }
    if ui
        .button(catalogue.text("setup_config_add_profile"))
        .clicked()
    {
        form.profiles.push(ProfileForm::default());
    }
}

fn config_gates(ui: &mut egui::Ui, form: &mut ConfigForm, catalogue: &Catalogue) {
    ui.label(catalogue.text("setup_config_gates"));
    let mut remove_gate = None;
    for (index, gate) in form.gates.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut gate.executable)
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_label),
            );
            ui.add(
                egui::TextEdit::singleline(&mut gate.arguments)
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_label),
            );
            if ui
                .button(catalogue.text("setup_config_remove_gate"))
                .clicked()
            {
                remove_gate = Some(index);
            }
        });
    }
    if let Some(index) = remove_gate {
        form.gates.remove(index);
    }
    if ui.button(catalogue.text("setup_config_add_gate")).clicked() {
        form.gates.push(GateForm::default());
    }
}

fn config_publication(ui: &mut egui::Ui, form: &mut ConfigForm, catalogue: &Catalogue) {
    ui.horizontal(|ui| {
        ui.label(catalogue.text("setup_config_publication"));
        ui.selectable_value(
            &mut form.publication,
            PublicationMode::LocalOnly,
            catalogue.text("setup_config_publication_local"),
        );
        ui.selectable_value(
            &mut form.publication,
            PublicationMode::PushFeatureBranch,
            catalogue.text("setup_config_publication_push"),
        );
        ui.selectable_value(
            &mut form.publication,
            PublicationMode::DraftPullRequest,
            catalogue.text("setup_config_publication_draft"),
        );
    });
    ui.small(catalogue.text("setup_config_publication_hint"));
    ui.horizontal_wrapped(|ui| {
        ui.label(catalogue.text("setup_config_capabilities"));
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_network"),
            &mut form.capabilities.network,
        );
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_push"),
            &mut form.capabilities.push,
        );
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_issues"),
            &mut form.capabilities.issues,
        );
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_pull_requests"),
            &mut form.capabilities.pull_requests,
        );
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_task_merge"),
            &mut form.capabilities.task_merge,
        );
        capability_toggle(
            ui,
            catalogue.text("setup_config_capability_destination_merge"),
            &mut form.capabilities.destination_merge,
        );
    });
}

fn campaign_form_body(ui: &mut egui::Ui, form: &mut CampaignForm, can_write: bool) -> (bool, bool) {
    let mut apply = false;
    let mut discard = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Campaign authority (version 3, serial, local only)");
        egui::Grid::new("campaign-form")
            .num_columns(2)
            .spacing(super::current_tokens(ui.ctx()).layout.grid)
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
                    let caption = ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(super::current_tokens(ui.ctx()).layout.field_full))
                        .labelled_by(caption.id);
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
            .spacing(super::current_tokens(ui.ctx()).layout.grid_compact)
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
                    let caption = ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(super::current_tokens(ui.ctx()).layout.field_small))
                        .labelled_by(caption.id);
                    ui.end_row();
                }
            });
        ui.small("Parallel pods and draft pull requests are not offered here; import a specification carrying an explicit multi-agent policy and GitHub authority instead.");
        ui.checkbox(&mut form.overwrite, "Replace the existing specification file");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_write, egui::Button::new("Verify and write campaign specification"))
                .clicked()
            {
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
                .desired_width(super::current_tokens(ui.ctx()).layout.field_standard),
        );
        ui.add(
            egui::TextEdit::singleline(&mut provider.model)
                .hint_text("model selector")
                .desired_width(super::current_tokens(ui.ctx()).layout.field_short),
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

#[cfg(test)]
mod tests {
    use super::{PendingAuthorization, config_form_body, receipt_matches};
    use crate::{
        messages::{self, Catalogue},
        setup::ConfigForm,
    };
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use std::path::{Path, PathBuf};

    struct ConfigPreview {
        form: ConfigForm,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for ConfigPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.right_to_left {
                        ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                            config_form_body(ui, &mut self.form, &self.catalogue);
                        });
                    } else {
                        config_form_body(ui, &mut self.form, &self.catalogue);
                    }
                });
            });
        }
    }

    #[test]
    fn guided_configuration_labels_keep_accessible_bounds_with_expansion_and_right_alignment() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let title = catalogue.text("setup_config_title").to_owned();
            let file = catalogue.text("setup_config_file").to_owned();
            let target = catalogue.text("setup_config_target").to_owned();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |_| ConfigPreview {
                    form: ConfigForm::defaults(
                        Path::new("/synthetic/repository"),
                        Path::new("/synthetic/workspace"),
                    ),
                    catalogue,
                    right_to_left,
                });
            harness.run_steps(2);
            for label in [&title, &file, &target] {
                assert!(
                    harness
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds()
                );
            }
            let catalogue = messages::english().pseudo(right_to_left);
            let publication = catalogue.text("setup_config_publication_draft").to_owned();
            let capability = catalogue
                .text("setup_config_capability_destination_merge")
                .to_owned();
            let apply = catalogue.text("setup_config_apply").to_owned();
            let mut full_form = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 2200.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |_| ConfigPreview {
                    form: ConfigForm::defaults(
                        Path::new("/synthetic/repository"),
                        Path::new("/synthetic/workspace"),
                    ),
                    catalogue,
                    right_to_left,
                });
            full_form.run_steps(2);
            for label in [&publication, &capability, &apply] {
                assert!(
                    full_form
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds()
                );
            }
        }
    }

    #[test]
    fn authorization_receipt_requires_exact_identity_bytes_digest_and_schema() {
        let pending = PendingAuthorization {
            request_id: "request-1".to_owned(),
            arguments: vec!["setup-write-authorization".to_owned()],
            path: PathBuf::from("/synthetic/authorization.txt"),
            bytes: 4,
            sha256: "synthetic-digest".to_owned(),
            repository_id: "repository-1".to_owned(),
        };
        let good = serde_json::json!({
            "schema_version": 1,
            "repository_id": "repository-1",
            "written": true,
            "bytes": 4,
            "sha256": "synthetic-digest",
        });
        assert_eq!(
            receipt_matches(good.to_string().as_bytes(), &pending),
            Ok(true)
        );
        for (field, changed) in [
            ("schema_version", serde_json::json!(2)),
            ("repository_id", serde_json::json!("repository-2")),
            ("written", serde_json::json!(false)),
            ("bytes", serde_json::json!(3)),
            ("sha256", serde_json::json!("another-digest")),
        ] {
            let mut mismatch = good.clone();
            mismatch[field] = changed;
            assert_eq!(
                receipt_matches(mismatch.to_string().as_bytes(), &pending),
                Ok(false)
            );
        }
        let mut extra = good;
        extra["unexpected"] = serde_json::json!(true);
        assert!(receipt_matches(extra.to_string().as_bytes(), &pending).is_err());
        assert!(receipt_matches(b"not-json", &pending).is_err());
    }
}
