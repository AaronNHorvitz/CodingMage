//! Guided setup: repository configuration, campaign authority and the authorization record.

use std::{
    fs,
    io::Read as _,
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
    time::Duration,
};

use codingmage_campaign::CampaignAuthentication;
use codingmage_core::{CapabilityGrant, PublicationMode};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{App, failure_box};
use crate::{
    backend::{Job, PrivateInput, Request, Response},
    browser::Browser,
    command,
    project::hex,
    setup::{
        CampaignBinding, CampaignForm, ConfigForm, EFFORTS, GateForm, ProfileForm, ProviderForm,
    },
    setup_export_process::{self, ExportIntent},
};

const AUTHORIZATION_DEADLINE: Duration = Duration::from_mins(1);

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
    destination: PathBuf,
    intent: ExportIntent,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CampaignReceipt {
    schema_version: u64,
    repository_id: String,
    campaign_id: String,
    head: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportReceipt {
    schema_version: u64,
    repository_id: String,
    written: bool,
    bytes: usize,
    sha256: String,
}

fn export_matches_receipt(path: &Path, receipt: &ExportReceipt) -> bool {
    if receipt.bytes == 0 || receipt.bytes > 1024 * 1024 {
        return false;
    }
    let Ok(descriptor) = open(
        path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) else {
        return false;
    };
    let file = fs::File::from(descriptor);
    let Ok(held) = file.metadata() else {
        return false;
    };
    if !held.is_file() || held.len() != receipt.bytes as u64 {
        return false;
    }
    let mut bytes = Vec::with_capacity(receipt.bytes);
    if file
        .take((receipt.bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() != receipt.bytes
        || hex(&Sha256::digest(&bytes)) != receipt.sha256
    {
        return false;
    }
    fs::symlink_metadata(path).is_ok_and(|named| {
        named.is_file()
            && named.dev() == held.dev()
            && named.ino() == held.ino()
            && named.len() == held.len()
    })
}

fn export_receipt_matches(bytes: &[u8], intent: &ExportIntent) -> bool {
    serde_json::from_slice::<ExportReceipt>(bytes).is_ok_and(|receipt| {
        receipt.schema_version == 1
            && receipt.written
            && receipt.repository_id == intent.repository_id
            && export_matches_receipt(&intent.destination, &receipt)
    })
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
    pub(super) recovery_export: Option<ExportIntent>,
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
        if self.pending_export.take().is_some() {
            self.message = Some(Err("the selected source changed during export; inspect the destination before retrying".to_owned()));
        }
    }

    pub(super) fn cancel_matching_export(&mut self, request_id: Option<&str>) {
        if self
            .pending_export
            .as_ref()
            .is_some_and(|pending| Some(pending.request_id.as_str()) == request_id)
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
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Ok(directory) = self.state_dir.as_ref() else {
            return;
        };
        match setup_export_process::load(directory, &project.config_path) {
            Ok(Some(intent)) => {
                self.setup.recovery_export = Some(intent);
                self.setup.message = Some(Err(
                    "a previous export may still be running or need inspection; check its outcome in Setup"
                        .to_owned(),
                ));
            }
            Ok(None) => {}
            Err(_) => {
                self.setup.message = Some(Err(
                    "private Setup export recovery state is invalid; inspect it before another export"
                        .to_owned(),
                ));
            }
        }
    }

    pub(super) fn reconcile_setup_export(&mut self) {
        let Some(intent) = self.setup.recovery_export.clone() else {
            return;
        };
        if self.setup.pending_export.is_some() {
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
        match setup_export_process::outcome(directory, &intent) {
            Ok(Some(Ok(bytes))) if export_receipt_matches(&bytes, &intent) => {
                if setup_export_process::clear(directory, &intent).is_ok() {
                    self.setup.recovery_export = None;
                    self.setup.message = Some(Ok(format!(
                        "previous export completed and verified at {}",
                        intent.destination.display()
                    )));
                } else {
                    self.setup.message = Some(Err(
                        "previous export bytes match, but its recovery record could not be cleared"
                            .to_owned(),
                    ));
                }
            }
            Ok(Some(Err(error))) => {
                self.setup.message = Some(Err(format!(
                    "previous export was not confirmed ({}); inspect its destination",
                    error.code()
                )));
            }
            Ok(Some(Ok(_))) | Err(_) => {
                self.setup.message = Some(Err(
                    "previous export receipt or destination is invalid; inspect its destination"
                        .to_owned(),
                ));
            }
            Ok(None) => {
                self.setup.message = Some(Err(
                    "previous export is running or its outcome is unknown; inspect its destination before retrying"
                        .to_owned(),
                ));
            }
        }
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
        self.next_evidence_request = self.next_evidence_request.wrapping_add(1);
        let request_id = format!("setup-campaign-{}", self.next_evidence_request);
        let pending = PendingCampaign {
            request_id: request_id.clone(),
            inspect_arguments: inspect_arguments.clone(),
            write_arguments: write_arguments.clone(),
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

    fn campaign_preview_arguments(&self) -> Option<(Vec<String>, Vec<String>)> {
        self.setup
            .pending_campaign
            .as_ref()
            .map(|pending| {
                (
                    pending.inspect_arguments.clone(),
                    pending.write_arguments.clone(),
                )
            })
            .or_else(|| {
                self.campaign_setup_arguments(
                    self.setup.campaign_form.as_ref()?,
                    &self.campaign_binding()?,
                )
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
        let result = response.result.and_then(|bytes| {
            let receipt: CampaignReceipt = serde_json::from_slice(&bytes).map_err(|_| {
                crate::backend::BackendError::Contract(
                    crate::backend::models::ModelError::Malformed,
                )
            })?;
            if receipt.schema_version != 1
                || !receipt.written
                || receipt.repository_id != pending.repository_id
                || receipt.campaign_id != pending.campaign_id
                || receipt.head != pending.head
                || receipt.bytes == 0
                || receipt.sha256.len() != 64
                || !receipt.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(crate::backend::BackendError::Contract(
                    crate::backend::models::ModelError::Malformed,
                ));
            }
            Ok(receipt)
        });
        match result {
            Ok(receipt) => {
                let selected = self.select_campaign_with_receipt(
                    &pending.specification_path,
                    Some((receipt.bytes, &receipt.sha256)),
                );
                if selected
                    && self.campaign.as_ref().is_some_and(|selected| {
                        selected.spec_path == pending.specification_path
                            && selected.spec.campaign_id == pending.campaign_id
                    })
                {
                    self.set_authorization_record(&pending.authorization_path);
                    self.setup.message = Some(Ok(format!(
                        "campaign specification written and verified at {}",
                        pending.specification_path.display()
                    )));
                } else {
                    if selected {
                        self.clear_campaign();
                    }
                    self.setup.message = Some(Err(format!(
                        "the coordinator confirmed a write at {}, but the resulting campaign could not be selected; inspect the destination before retrying",
                        pending.specification_path.display()
                    )));
                }
            }
            Err(error) => {
                self.setup.message = Some(Err(format!(
                    "campaign write not confirmed: {error}. Inspect the destination before retrying."
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
        if self.setup.pending_export.is_some() || self.setup.recovery_export.is_some() {
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
            destination,
            intent: intent.clone(),
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
                self.setup.message = Some(Ok(
                    "export pending under its own helper; closing this view does not cancel it"
                        .to_owned(),
                ));
            }
            Err(error) => {
                let cleared = setup_export_process::clear(&directory, &intent).is_ok();
                if !cleared {
                    self.setup.recovery_export = Some(intent);
                }
                self.setup.message = Some(Err(format!(
                    "export was not queued: {}; {}",
                    error.code(),
                    if cleared {
                        "no export started"
                    } else {
                        "inspect private recovery state before retrying"
                    }
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
            Ok(bytes) if export_receipt_matches(&bytes, &pending.intent) => {
                let cleared = self.state_dir.as_ref().is_ok_and(|directory| {
                    setup_export_process::clear(directory, &pending.intent).is_ok()
                });
                if cleared {
                    self.setup.recovery_export = None;
                    self.setup.message = Some(Ok(format!(
                        "exported and verified at {}",
                        pending.destination.display()
                    )));
                } else {
                    self.setup.message = Some(Err(
                        "export bytes match but recovery state could not be cleared; inspect before retrying"
                            .to_owned(),
                    ));
                }
            }
            Ok(_) => self.setup.message = Some(Err("export not confirmed: receipt or destination did not match; inspect the destination before retrying".to_owned())),
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
        ui.heading("Setup");
        ui.label("Everything here goes through the existing configuration and campaign contracts. Files are written only after the same loaders the coordinator uses accept them. No credential is ever requested or stored; providers use their own existing logins.");
        if let Some(message) = &self.setup.message {
            match message {
                Ok(text) => {
                    ui.colored_label(super::current_tokens(ui.ctx()).success, text);
                }
                Err(text) => failure_box(ui, "Refused", text, "Correct the field and try again."),
            }
        }
        ui.separator();
        ui.heading("1. Open or create a repository configuration");
        self.open_controls(ui);
        ui.horizontal(|ui| {
            let label = ui.label("Workspace directory for new authority files (optional)");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.workspace_root)
                    .hint_text("/absolute/path outside the repository")
                    .desired_width(super::current_tokens(ui.ctx()).layout.field_path),
            )
            .labelled_by(label.id);
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
                    .max_height(super::current_tokens(ui.ctx()).layout.preview_short)
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
                .is_some_and(|(inspect, write)| {
                    command::can_preview(self.binary_path.as_deref(), Some(&inspect))
                        && command::can_preview(self.binary_path.as_deref(), Some(&write))
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
        if let Some((inspect, write)) = self.campaign_preview_arguments() {
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
            ui.small("The candidate specification is sent through private stdin. It is absent from command arguments; the coordinator checks the record and source again before publication.");
        } else {
            command::show_unavailable_for(ui, "inspect authorization record");
            command::show_unavailable_for(ui, "write campaign specification");
        }
    }

    fn export_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Import means opening an existing configuration or selecting an existing campaign above. Export copies the validated file elsewhere; digests and paths inside it are unchanged and no credential exists to leak.");
        if let Some(intent) = self.setup.recovery_export.clone() {
            ui.label(format!(
                "Previous export destination: {}. Its outcome remains bound to the original repository and request.",
                intent.destination.display()
            ));
            if ui.button("Check previous export outcome").clicked() {
                self.reconcile_setup_export();
            }
            if self.setup.pending_export.is_none()
                && ui
                    .button("I inspected the destination; clear export notice")
                    .clicked()
            {
                let cleared = self
                    .state_dir
                    .as_ref()
                    .is_ok_and(|directory| setup_export_process::clear(directory, &intent).is_ok());
                if cleared {
                    self.setup.recovery_export = None;
                    self.setup.message = Some(Ok(
                        "local export notice cleared after destination inspection".to_owned(),
                    ));
                } else {
                    self.setup.message = Some(Err(
                        "export is still running or private recovery state changed; wait and check its outcome before clearing"
                            .to_owned(),
                    ));
                }
            }
            ui.small("These recovery controls only inspect or clear the private notice. They do not run a coordinator command or stop an export already in progress.");
        }
        let editable = self.setup.pending_export.is_none() && self.setup.recovery_export.is_none();
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

fn config_form_body(ui: &mut egui::Ui, form: &mut ConfigForm) -> (bool, bool) {
    let mut apply = false;
    let mut discard = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Repository configuration (version 1, deny-first)");
        egui::Grid::new("config-form")
            .num_columns(2)
            .spacing(super::current_tokens(ui.ctx()).layout.grid)
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
                    let caption = ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(super::current_tokens(ui.ctx()).layout.field_full))
                        .labelled_by(caption.id);
                    ui.end_row();
                }
            });
        ui.label("Agent profiles (identifier, provider, model passed to the provider unchanged)");
        let mut remove_profile = None;
        for (index, profile) in form.profiles.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut profile.id).desired_width(super::current_tokens(ui.ctx()).layout.field_short));
                ui.add(egui::TextEdit::singleline(&mut profile.provider).desired_width(super::current_tokens(ui.ctx()).layout.field_tiny));
                ui.add(egui::TextEdit::singleline(&mut profile.model).desired_width(super::current_tokens(ui.ctx()).layout.field_small));
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
                ui.add(egui::TextEdit::singleline(&mut gate.executable).desired_width(super::current_tokens(ui.ctx()).layout.field_label));
                ui.add(egui::TextEdit::singleline(&mut gate.arguments).desired_width(super::current_tokens(ui.ctx()).layout.field_label));
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
    use super::{PendingAuthorization, receipt_matches};
    use std::path::PathBuf;

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
