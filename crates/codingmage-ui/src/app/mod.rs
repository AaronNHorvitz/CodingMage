//! Application shell: navigation, project selection and bounded backend observation.

mod blockers_screen;
mod campaign_screen;
mod changes_screen;
mod execution_screen;
mod help_screen;
mod readiness_screen;
mod reports_screen;
mod setup_screen;

pub use changes_screen::ChangeSet;
pub use execution_screen::{ExecutionState, LAUNCH_OBSERVE_INTERVAL};
pub use help_screen::SupportState;
use reports_screen::ReportWorker;
pub use reports_screen::ReportsState;
pub use setup_screen::SetupState;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use codingmage_plan::{CheckState, PlanItemKind};

use crate::{
    backend::models::{
        BlockerExplanation, CampaignReport, CampaignStatus, HeadPlanProjection, MissionStatus,
        ModelError, TaskDetailProjection,
    },
    backend::{
        BackendError, Binding, CoordinatorBinary, Generation, Job, QueueError, Request, Response,
        Worker, explain_code,
        models::{Diagnosis, parse_diagnosis},
    },
    browser::Browser,
    campaign::{CampaignSelection, SelectError, campaign_state_label, involvement_label},
    command, content,
    design::{Appearance, Palette, Tokens, current_tokens},
    messages::{self, Catalogue},
    observed::{Freshness, Observed, age_label},
    project::{OpenError, PlanLoadError, Project},
    state_dir::{ProjectMemory, RecentProjects, StateError, user_config_dir},
    workplan::{KindFilter, PlanFilter, PlanIndex, PlanRow, SourceReadiness, StateFilter},
};

/// Deadline for read-only diagnosis commands.
pub const DIAGNOSIS_DEADLINE: Duration = Duration::from_mins(1);

fn project_open_arguments(path: &Path) -> Option<Vec<String>> {
    let path = path.to_str().filter(|_| path.is_absolute())?;
    Some(vec![
        "project-open".to_owned(),
        "--config".to_owned(),
        path.to_owned(),
    ])
}

fn directory_list_arguments(path: &Path) -> Option<Vec<String>> {
    let path = path.to_str().filter(|_| path.is_absolute())?;
    Some(vec![
        "directory-list".to_owned(),
        "--directory".to_owned(),
        path.to_owned(),
    ])
}

fn campaign_select_arguments(path: &Path) -> Option<Vec<String>> {
    let path = path.to_str().filter(|_| path.is_absolute())?;
    Some(vec![
        "campaign-select".to_owned(),
        "--campaign".to_owned(),
        path.to_owned(),
    ])
}

struct PendingCampaignSelection {
    path: PathBuf,
    request_id: String,
}

#[derive(Clone, Copy)]
enum BrowserSlot {
    Configuration,
    Target,
    Campaign,
}

impl BrowserSlot {
    const fn label(self) -> &'static str {
        match self {
            Self::Configuration => "directory-list-config",
            Self::Target => "directory-list-target",
            Self::Campaign => "directory-list-campaign",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "directory-list-config" => Some(Self::Configuration),
            "directory-list-target" => Some(Self::Target),
            "directory-list-campaign" => Some(Self::Campaign),
            _ => None,
        }
    }
}

/// Deadline for read-only campaign projections.
pub const STATUS_DEADLINE: Duration = Duration::from_mins(1);
/// Deadline for read-only Git object reads.
pub const GIT_DEADLINE: Duration = Duration::from_secs(30);
/// Campaign status polling interval while a campaign is selected.
pub const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(15);
/// Minimum window size the layout supports.
pub const MIN_WINDOW: [f32; 2] = [1024.0, 640.0];

/// Navigation destinations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    /// Repository identity, readiness and counts.
    Overview,
    /// Searchable task plan.
    WorkPlan,
    /// Campaign and team activity.
    Campaign,
    /// Bound campaign holds and recovery guidance.
    Blockers,
    /// Exact changes, review and test results.
    Changes,
    /// Outcome and blocker reports.
    Reports,
    /// Guided configuration and readiness.
    Setup,
    /// Appearance and accessibility preferences.
    Settings,
    /// Offline help, glossary and source licence notices.
    Help,
}

impl Screen {
    /// All screens in navigation order.
    pub const ALL: [Self; 9] = [
        Self::Overview,
        Self::WorkPlan,
        Self::Campaign,
        Self::Blockers,
        Self::Changes,
        Self::Reports,
        Self::Setup,
        Self::Settings,
        Self::Help,
    ];

    /// Navigation label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::WorkPlan => "Work plan",
            Self::Campaign => "Campaign",
            Self::Blockers => "Blockers",
            Self::Changes => "Changes and reviews",
            Self::Reports => "Reports",
            Self::Setup => "Setup",
            Self::Settings => "Settings",
            Self::Help => "Help and About",
        }
    }

    /// Keyboard shortcut digit (Ctrl+digit).
    #[must_use]
    pub const fn shortcut(self) -> egui::Key {
        match self {
            Self::Overview => egui::Key::Num1,
            Self::WorkPlan => egui::Key::Num2,
            Self::Campaign => egui::Key::Num3,
            Self::Blockers => egui::Key::Num9,
            Self::Changes => egui::Key::Num4,
            Self::Reports => egui::Key::Num5,
            Self::Setup => egui::Key::Num6,
            Self::Settings => egui::Key::Num7,
            Self::Help => egui::Key::Num8,
        }
    }
}

/// Backend connection state.
pub enum Connection {
    /// The coordinator executable is available and the worker is running.
    Ready(Worker),
    /// The coordinator executable is unavailable; every backend request is refused.
    Unavailable(BackendError),
}

#[derive(Clone, Debug)]
struct EvidenceTicket {
    request_id: String,
    status_epoch: u64,
}

/// Interface state.
pub struct App {
    connection: Connection,
    binary_path: Option<PathBuf>,
    generation: Generation,
    project: Option<Project>,
    pending_project: Option<PathBuf>,
    open_error: Option<OpenError>,
    diagnosis: Observed<Diagnosis>,
    screen: Screen,
    config_input: String,
    recent: RecentProjects,
    state_dir: Result<PathBuf, StateError>,
    status_line: Option<(Instant, String)>,
    discarded_stale: u64,
    next_browser_request: u64,
    started_at: Instant,
    now: Instant,
    plan_index: Option<PlanIndex>,
    plan_filter: PlanFilter,
    selected_item: Option<String>,
    browser: Option<Browser>,
    campaign: Option<CampaignSelection>,
    remembered_campaign: Option<PathBuf>,
    pending_campaign: Option<PendingCampaignSelection>,
    next_campaign_request: u64,
    campaign_input: String,
    campaign_error: Option<SelectError>,
    campaign_browser: Option<Browser>,
    blocker_query: String,
    status: Observed<Option<CampaignStatus>>,
    explanation: Observed<Option<BlockerExplanation>>,
    report: Observed<Option<CampaignReport>>,
    mission: Observed<Option<MissionStatus>>,
    head_plan: Observed<Option<HeadPlanProjection>>,
    head_plan_commit: Option<String>,
    task_detail: Observed<TaskDetailProjection>,
    task_detail_key: Option<(String, String)>,
    last_status_request: Option<Instant>,
    status_epoch: u64,
    next_evidence_request: u64,
    setup: SetupState,
    authorization_record: Option<PathBuf>,
    authorization_input: String,
    preflight: Observed<crate::readiness::PreflightObservation>,
    execution: ExecutionState,
    changes: Observed<ChangeSet>,
    changes_range: Option<(String, String)>,
    changes_request: Option<EvidenceTicket>,
    records: Observed<Vec<crate::records::RunRecord>>,
    records_request: Option<EvidenceTicket>,
    records_status: Option<(String, u64)>,
    records_truncated: bool,
    report_source_revision: u64,
    reports: ReportsState,
    report_worker: ReportWorker,
    support: SupportState,
    appearance: Appearance,
    system_dark: bool,
    applied_palette: Option<Palette>,
}

impl App {
    /// Creates the shell with the sibling coordinator executable.
    #[must_use]
    pub fn new(ctx: &egui::Context) -> Self {
        let binary = CoordinatorBinary::sibling();
        Self::with_binary(ctx, binary)
    }

    /// Creates the shell with an explicit coordinator resolution result.
    #[must_use]
    pub fn with_binary(
        ctx: &egui::Context,
        binary: Result<CoordinatorBinary, BackendError>,
    ) -> Self {
        Self::with_state_dir(ctx, binary, user_config_dir())
    }

    /// Creates the shell with an explicit private state directory (used by tests).
    #[must_use]
    pub fn with_state_dir(
        ctx: &egui::Context,
        binary: Result<CoordinatorBinary, BackendError>,
        state_dir: Result<PathBuf, StateError>,
    ) -> Self {
        let wake_ctx = ctx.clone();
        let (connection, binary_path) = match binary {
            Ok(binary) => {
                let path = binary.path().to_path_buf();
                (
                    Connection::Ready(Worker::start(binary, move || wake_ctx.request_repaint())),
                    Some(path),
                )
            }
            Err(error) => (Connection::Unavailable(error), None),
        };
        let recent = state_dir
            .as_ref()
            .ok()
            .and_then(|directory| RecentProjects::load(directory).ok())
            .unwrap_or_default();
        let mut setup = SetupState::default();
        if let Ok(directory) = &state_dir {
            match crate::setup_config_process::load(directory) {
                Ok(Some(intent)) => {
                    setup.recovery_config = Some(intent);
                    setup.message = Some(Err(
                        "a previous configuration write may need inspection; check its outcome in Setup"
                            .to_owned(),
                    ));
                }
                Ok(None) => {}
                Err(_) => {
                    setup.message = Some(Err(
                        "private configuration recovery state is invalid; inspect it before another write"
                            .to_owned(),
                    ));
                }
            }
        }
        let now = Instant::now();
        Self {
            connection,
            binary_path,
            generation: Generation(1),
            project: None,
            pending_project: None,
            open_error: None,
            diagnosis: Observed::default(),
            screen: Screen::Overview,
            config_input: String::new(),
            recent,
            state_dir,
            status_line: None,
            discarded_stale: 0,
            next_browser_request: 0,
            started_at: now,
            now,
            plan_index: None,
            plan_filter: PlanFilter::default(),
            selected_item: None,
            browser: None,
            campaign: None,
            remembered_campaign: None,
            pending_campaign: None,
            next_campaign_request: 0,
            campaign_input: String::new(),
            campaign_error: None,
            campaign_browser: None,
            blocker_query: String::new(),
            status: Observed::default(),
            explanation: Observed::default(),
            report: Observed::default(),
            mission: Observed::default(),
            head_plan: Observed::default(),
            head_plan_commit: None,
            task_detail: Observed::default(),
            task_detail_key: None,
            last_status_request: None,
            status_epoch: 0,
            next_evidence_request: 0,
            setup,
            authorization_record: None,
            authorization_input: String::new(),
            preflight: Observed::default(),
            execution: ExecutionState::default(),
            changes: Observed::default(),
            changes_range: None,
            changes_request: None,
            records: Observed::default(),
            records_request: None,
            records_status: None,
            records_truncated: false,
            report_source_revision: 0,
            reports: ReportsState::default(),
            report_worker: ReportWorker::start({
                let wake_ctx = ctx.clone();
                move || wake_ctx.request_repaint()
            }),
            support: SupportState::default(),
            appearance: Appearance::System,
            system_dark: ctx.system_theme().unwrap_or(egui::Theme::Dark) == egui::Theme::Dark,
            applied_palette: None,
        }
    }

    /// Selected campaign, if any.
    #[must_use]
    pub const fn campaign(&self) -> Option<&CampaignSelection> {
        self.campaign.as_ref()
    }

    /// Latest campaign status observation (`Some(None)` means no durable state exists).
    #[must_use]
    pub const fn status(&self) -> &Observed<Option<CampaignStatus>> {
        &self.status
    }

    /// Latest blocker explanation for the selected campaign.
    #[must_use]
    pub const fn explanation(&self) -> &Observed<Option<BlockerExplanation>> {
        &self.explanation
    }

    /// Latest coordinator-validated sub-task states at the campaign head.
    #[must_use]
    pub const fn head_plan(&self) -> &Observed<Option<HeadPlanProjection>> {
        &self.head_plan
    }

    /// Latest requested source detail for one campaign-head task.
    #[must_use]
    pub const fn task_detail(&self) -> &Observed<TaskDetailProjection> {
        &self.task_detail
    }

    /// Latest final report observation.
    #[must_use]
    pub const fn report(&self) -> &Observed<Option<CampaignReport>> {
        &self.report
    }

    /// Latest mission authority observation (`Some(None)` means no charter is admitted).
    #[must_use]
    pub const fn mission(&self) -> &Observed<Option<MissionStatus>> {
        &self.mission
    }

    /// Last campaign selection failure.
    #[must_use]
    pub const fn campaign_error(&self) -> Option<&SelectError> {
        self.campaign_error.as_ref()
    }

    /// Index over the opened task plan, when it parsed.
    #[must_use]
    pub const fn plan_index(&self) -> Option<&PlanIndex> {
        self.plan_index.as_ref()
    }

    /// Current plan filter.
    #[must_use]
    pub const fn plan_filter(&self) -> &PlanFilter {
        &self.plan_filter
    }

    /// Replaces the plan filter.
    pub fn set_plan_filter(&mut self, filter: PlanFilter) {
        self.plan_filter = filter;
    }

    /// Selected plan item identifier.
    #[must_use]
    pub fn selected_item(&self) -> Option<&str> {
        self.selected_item.as_deref()
    }

    /// Current selection binding.
    #[must_use]
    pub fn binding(&self) -> Binding {
        Binding {
            config_path: self
                .project
                .as_ref()
                .map(|project| project.config_path.clone()),
            repository_id: self
                .diagnosis
                .value
                .as_ref()
                .map(|diagnosis| diagnosis.repository_id.clone()),
            campaign_id: self
                .campaign
                .as_ref()
                .map(|campaign| campaign.spec.campaign_id.clone()),
        }
    }

    /// Current generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Opened project, if any.
    #[must_use]
    pub const fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    /// Project-opening failure, if the selected configuration did not open.
    #[must_use]
    pub const fn open_error(&self) -> Option<&OpenError> {
        self.open_error.as_ref()
    }

    /// Latest diagnosis observation.
    #[must_use]
    pub const fn diagnosis(&self) -> &Observed<Diagnosis> {
        &self.diagnosis
    }

    /// Number of responses discarded because their binding or generation was stale.
    #[must_use]
    pub const fn discarded_stale(&self) -> u64 {
        self.discarded_stale
    }

    /// Selected screen.
    #[must_use]
    pub const fn screen(&self) -> Screen {
        self.screen
    }

    /// Selects a screen.
    pub const fn select_screen(&mut self, screen: Screen) {
        self.screen = screen;
    }

    /// Requests a project snapshot on the bounded worker; starts no agent.
    pub fn open_project(&mut self, config_path: &Path) {
        self.reset_project_selection();
        self.config_input = config_path.display().to_string();
        if !config_path.is_absolute() {
            self.open_error = Some(OpenError::RelativePath);
            return;
        }
        let Some(arguments) = project_open_arguments(config_path) else {
            self.open_error = Some(OpenError::Contract);
            return;
        };
        if self.binary_path.is_some()
            && !command::can_preview(self.binary_path.as_deref(), Some(&arguments))
        {
            self.open_error = Some(OpenError::Contract);
            return;
        }
        self.pending_project = Some(config_path.to_path_buf());
        self.set_status("opening configuration through the coordinator");
        let request = Request {
            generation: self.generation,
            binding: Binding {
                config_path: Some(config_path.to_path_buf()),
                repository_id: None,
                campaign_id: None,
            },
            job: Job::Command {
                label: "project-open",
                arguments,
                deadline: DIAGNOSIS_DEADLINE,
            },
            request_id: None,
        };
        if let Err(error) = self.submit(request) {
            self.pending_project = None;
            self.open_error = Some(OpenError::Backend(error));
        }
    }

    pub(super) fn open_loaded_project(&mut self, project: Project) {
        let config_path = project.config_path.clone();
        self.reset_project_selection();
        self.finish_project_open(&config_path, Ok(project));
    }

    fn reset_project_selection(&mut self) {
        self.setup.cancel_pending_config();
        self.setup.cancel_pending_authorization();
        self.setup.cancel_pending_export();
        self.setup.recovery_export = None;
        self.setup.authorization_text.clear();
        self.generation = Generation(self.generation.0 + 1);
        if let Connection::Ready(worker) = &self.connection {
            worker.advance(self.generation);
        }
        self.diagnosis.clear();
        self.project = None;
        self.pending_project = None;
        self.open_error = None;
        self.plan_index = None;
        self.selected_item = None;
        self.browser = None;
        self.clear_campaign_observations();
        self.campaign = None;
        self.remembered_campaign = None;
        self.pending_campaign = None;
        self.campaign_error = None;
        self.campaign_input.clear();
        self.authorization_record = None;
        self.authorization_input.clear();
    }

    fn finish_project_open(&mut self, config_path: &Path, loaded: Result<Project, OpenError>) {
        match loaded {
            Ok(project) => {
                if let Ok(directory) = &self.state_dir {
                    let _ = self.recent.remember(directory, &project.config_path);
                }
                self.config_input = project.config_path.display().to_string();
                self.plan_index = project
                    .plan
                    .as_ref()
                    .ok()
                    .map(|loaded| PlanIndex::new(&loaded.plan));
                self.project = Some(project);
                self.load_setup_export_recovery();
                self.set_status("opened configuration; requesting repository diagnosis");
                self.refresh_diagnosis();
                let remembered = self
                    .state_dir
                    .as_ref()
                    .ok()
                    .and_then(|directory| ProjectMemory::load(directory, config_path).ok());
                if let Some(memory) = remembered {
                    if let Some(record) = memory.authorization_record {
                        self.authorization_record = Some(record.clone());
                        self.authorization_input = record.display().to_string();
                    }
                    if let Some(spec_path) = memory.campaign_spec {
                        self.campaign_input = spec_path.display().to_string();
                        self.remembered_campaign = Some(spec_path);
                    }
                }
            }
            Err(error) => {
                self.set_status(format!("could not open configuration: {error}"));
                self.open_error = Some(error);
            }
        }
    }

    /// Closes the project without affecting any coordinator process.
    pub fn close_project(&mut self) {
        self.setup.cancel_pending_config();
        self.setup.cancel_pending_authorization();
        self.setup.cancel_pending_export();
        self.setup.recovery_export = None;
        self.setup.authorization_text.clear();
        self.generation = Generation(self.generation.0 + 1);
        if let Connection::Ready(worker) = &self.connection {
            worker.advance(self.generation);
        }
        self.project = None;
        self.pending_project = None;
        self.diagnosis.clear();
        self.open_error = None;
        self.plan_index = None;
        self.selected_item = None;
        self.clear_campaign_observations();
        self.campaign = None;
        self.remembered_campaign = None;
        self.pending_campaign = None;
        self.campaign_error = None;
        self.set_status("closed the repository view; no coordinator process was affected");
    }

    /// Requests a fresh `doctor` observation for the opened project.
    pub fn refresh_diagnosis(&mut self) {
        let Some(arguments) = self.doctor_arguments() else {
            if self.project.is_some() {
                self.set_status("diagnosis unavailable: the coordinator or configuration path cannot be shown exactly as a command");
            }
            return;
        };
        if self
            .binary_path
            .as_deref()
            .is_some_and(|path| !command::can_preview(Some(path), Some(&arguments)))
        {
            self.set_status(
                "diagnosis cannot run because the exact coordinator command cannot be shown safely",
            );
            return;
        }
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: "doctor",
                arguments,
                deadline: DIAGNOSIS_DEADLINE,
            },
            request_id: None,
        };
        match self.submit(request) {
            Ok(()) => self.diagnosis.loading = true,
            Err(error) => {
                self.diagnosis.fail(error, self.now);
            }
        }
    }

    fn doctor_arguments(&self) -> Option<Vec<String>> {
        let project = self.project.as_ref()?;
        let arguments = vec![
            "doctor".to_owned(),
            "--config".to_owned(),
            project.config_path.to_str()?.to_owned(),
        ];
        Some(arguments)
    }

    /// Queues one request through the worker.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the backend is unavailable or the queue is full.
    pub fn submit(&mut self, request: Request) -> Result<(), BackendError> {
        match &self.connection {
            Connection::Ready(worker) => match worker.submit(request) {
                Ok(()) => Ok(()),
                Err(QueueError::Full) => Err(BackendError::Refused(
                    "too many pending requests; wait for the current ones to finish".to_owned(),
                )),
                Err(QueueError::Stopped) => Err(BackendError::Spawn),
            },
            Connection::Unavailable(error) => Err(error.clone()),
        }
    }

    fn browser_mut(&mut self, slot: BrowserSlot) -> Option<&mut Browser> {
        match slot {
            BrowserSlot::Configuration => self.browser.as_mut(),
            BrowserSlot::Target => self.setup.target_browser.as_mut(),
            BrowserSlot::Campaign => self.campaign_browser.as_mut(),
        }
    }

    fn request_browser(&mut self, slot: BrowserSlot) {
        self.next_browser_request = self.next_browser_request.wrapping_add(1);
        let identity = self.next_browser_request;
        let Some(browser) = self.browser_mut(slot) else {
            return;
        };
        let path = browser.current.clone();
        let request_id = browser.begin_listing(identity);
        let Some(arguments) = directory_list_arguments(&path) else {
            browser.fail_listing("codingmage.cli.invalid_argument");
            return;
        };
        let request = Request {
            generation: self.generation,
            binding: self.binding(),
            job: Job::Command {
                label: slot.label(),
                arguments,
                deadline: DIAGNOSIS_DEADLINE,
            },
            request_id: Some(request_id),
        };
        if let Err(error) = self.submit(request)
            && let Some(browser) = self.browser_mut(slot)
        {
            browser.fail_listing(&error.code());
        }
    }

    fn accept_browser_listing(&mut self, slot: BrowserSlot, response: Response) -> bool {
        let Some(browser) = self.browser_mut(slot) else {
            self.discarded_stale += 1;
            return false;
        };
        if !browser.matches_request(response.request_id.as_deref()) {
            self.discarded_stale += 1;
            return false;
        }
        match response.result {
            Ok(bytes) => browser.apply_snapshot(&bytes),
            Err(error) => browser.fail_listing(&error.code()),
        }
        true
    }

    pub(super) fn advance_selection_generation(&mut self) {
        self.setup.cancel_pending_config();
        self.setup.cancel_pending_authorization();
        self.setup.cancel_pending_campaign();
        self.setup.cancel_pending_export();
        self.generation = Generation(self.generation.0 + 1);
        if let Connection::Ready(worker) = &self.connection {
            worker.advance(self.generation);
        }
    }

    /// Applies one worker response, discarding any whose binding or generation is stale.
    ///
    /// Returns true when the response was accepted.
    pub fn handle_response(&mut self, response: Response) -> bool {
        if response.generation != self.generation
            || !self.binding_matches(&response.binding, response.label)
        {
            self.discard_stale_response(&response);
            return false;
        }
        if matches!(
            response.label,
            "campaign-status"
                | "campaign-explain-blocker"
                | "campaign-report"
                | "campaign-changes"
                | "campaign-run-records"
        ) {
            self.report_source_revision = self.report_source_revision.wrapping_add(1);
        }
        match response.label {
            "directory-list-config" | "directory-list-target" | "directory-list-campaign" => {
                BrowserSlot::from_label(response.label)
                    .is_some_and(|slot| self.accept_browser_listing(slot, response))
            }
            "project-open" => {
                self.accept_project_open(response);
                true
            }
            "campaign-select" => self.accept_campaign_selection(response),
            "doctor" => {
                self.accept_diagnosis(response);
                true
            }
            "campaign-status" => {
                self.accept_status(response);
                true
            }
            "campaign-preflight" => {
                self.accept_preflight(response);
                true
            }
            "campaign-control" => {
                self.accept_control(response);
                true
            }
            "campaign-changes" => self.accept_changes(response),
            "campaign-run-records" => self.accept_records(response),
            "campaign-explain-blocker" => {
                self.accept_explanation(response);
                true
            }
            "campaign-mission-status" => {
                self.accept_mission(response);
                true
            }
            "campaign-report" => {
                self.accept_report(response);
                true
            }
            "campaign-head-plan" => {
                self.accept_head_plan(response);
                true
            }
            "campaign-task-detail" => {
                self.accept_task_detail(response);
                true
            }
            "support-bundle" => self.accept_support_bundle(response),
            "report-export" => self.accept_report_export(response),
            "campaign-outcome-report" => self.accept_source_report(response),
            "setup-write-authorization" => self.accept_authorization_write(response),
            "setup-write-config" => self.accept_config_write(&response),
            "setup-config-recover" => self.accept_config_recovery(response),
            "setup-config-clear" => self.accept_config_clear(&response),
            "setup-write-campaign" => self.accept_campaign_write(response),
            "setup-export-copy" => self.accept_setup_export(response),
            _ => false,
        }
    }

    fn discard_stale_response(&mut self, response: &Response) {
        if matches!(
            response.label,
            "setup-write-config" | "setup-config-recover" | "setup-config-clear"
        ) {
            self.setup
                .cancel_matching_config(response.request_id.as_deref());
        }
        if response.label == "setup-write-authorization" {
            self.setup
                .cancel_matching_authorization(response.request_id.as_deref());
        }
        if response.label == "setup-write-campaign" {
            self.setup
                .cancel_matching_campaign(response.request_id.as_deref());
        }
        if response.label == "setup-export-copy" {
            self.setup
                .cancel_matching_export(response.request_id.as_deref());
        }
        if response.label == "campaign-select"
            && self.pending_campaign.as_ref().is_some_and(|pending| {
                response.request_id.as_deref() == Some(pending.request_id.as_str())
            })
        {
            self.pending_campaign = None;
            self.campaign_error = Some(SelectError::DiagnosisUnavailable);
            self.set_status("campaign selection became stale; refresh repository diagnosis");
        }
        self.discarded_stale += 1;
    }

    fn binding_matches(&self, issued: &Binding, label: &str) -> bool {
        if label == "project-open" {
            return issued.config_path.as_ref() == self.pending_project.as_ref()
                && issued.repository_id.is_none()
                && issued.campaign_id.is_none();
        }
        let current = self.binding();
        if label == "campaign-select" {
            return issued.config_path == current.config_path
                && issued.repository_id.is_some()
                && issued.repository_id == current.repository_id
                && issued.campaign_id.is_none()
                && current.campaign_id.is_none()
                && self.diagnosis.freshness(self.now) == Freshness::Live;
        }
        if issued.config_path != current.config_path {
            return false;
        }
        if let (Some(issued_repository), Some(current_repository)) =
            (&issued.repository_id, &current.repository_id)
            && issued_repository != current_repository
        {
            return false;
        }
        // Repository-level observations stay valid when a campaign is selected afterwards;
        // campaign-level observations must belong to the currently selected campaign.
        label == "doctor" || issued.campaign_id == current.campaign_id
    }

    fn accept_project_open(&mut self, response: Response) {
        let Some(path) = self.pending_project.take() else {
            self.discarded_stale += 1;
            return;
        };
        let loaded = response
            .result
            .map_err(OpenError::Backend)
            .and_then(|bytes| Project::from_snapshot(&path, &bytes));
        self.finish_project_open(&path, loaded);
    }

    fn accept_diagnosis(&mut self, response: Response) {
        match response
            .result
            .and_then(|bytes| parse_diagnosis(&bytes).map_err(BackendError::from))
        {
            Ok(diagnosis) => {
                let target_changed = self.project.as_ref().is_some_and(|project| {
                    project.config.redacted_view().target_path_sha256
                        != diagnosis.configuration.target_path_sha256
                });
                if target_changed {
                    if let Some(project) = self.project.as_mut() {
                        project.plan = Err(PlanLoadError::Stale);
                    }
                    self.plan_index = None;
                    self.diagnosis.fail(
                        BackendError::Contract(ModelError::AuthorityMismatch),
                        self.now,
                    );
                    self.set_status("configuration target changed; reopen the configuration");
                    return;
                }
                if let Some(project) = self.project.as_mut()
                    && project
                        .plan
                        .as_ref()
                        .is_ok_and(|loaded| loaded.source_sha256 != diagnosis.task_source_sha256)
                {
                    project.plan = Err(PlanLoadError::Stale);
                    self.plan_index = None;
                }
                let plan_stale = self
                    .project
                    .as_ref()
                    .is_some_and(|project| matches!(&project.plan, Err(PlanLoadError::Stale)));
                self.diagnosis
                    .accept(diagnosis, response.generation, self.now);
                self.reconcile_setup_export();
                self.set_status(if plan_stale {
                    "task source changed after opening; reopen the configuration"
                } else {
                    "repository diagnosis observed"
                });
                if !plan_stale && let Some(path) = self.remembered_campaign.take() {
                    self.select_campaign(&path);
                }
            }
            Err(error) => {
                self.set_status(format!("repository diagnosis failed: {}", error.code()));
                self.diagnosis.fail(error, self.now);
            }
        }
    }

    /// Drains worker responses.
    pub fn poll(&mut self) {
        self.now = Instant::now();
        self.poll_report_assembly();
        let responses = match &self.connection {
            Connection::Ready(worker) => worker.drain(),
            Connection::Unavailable(_) => Vec::new(),
        };
        for response in responses {
            self.handle_response(response);
        }
        self.observe_launch(false);
        if self.campaign.is_some()
            && !self.status.loading
            && self
                .last_status_request
                .is_none_or(|last| self.now.duration_since(last) >= STATUS_POLL_INTERVAL)
        {
            self.refresh_campaign();
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_line = Some((self.now, message.into()));
    }

    /// Renders one frame into the root `Ui`.
    pub fn render(&mut self, root: &mut egui::Ui) {
        self.poll();
        let ctx = root.ctx().clone();
        let palette = self.appearance.resolve(&ctx, self.system_dark);
        if self.applied_palette != Some(palette) {
            Tokens::for_palette(palette).apply(&ctx, palette);
            self.applied_palette = Some(palette);
        }
        self.handle_shortcuts(&ctx);
        self.top_bar(root);
        self.status_bar(root);
        self.navigation(root);
        egui::CentralPanel::default().show(root, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.screen {
                    Screen::Overview => self.overview(ui),
                    Screen::WorkPlan => self.work_plan(ui),
                    Screen::Campaign => self.campaign_screen(ui),
                    Screen::Blockers => self.blockers_screen(ui),
                    Screen::Changes => self.changes_screen(ui),
                    Screen::Setup => self.setup(ui),
                    Screen::Reports => self.reports_screen(ui),
                    Screen::Settings => self.settings_screen(ui),
                    Screen::Help => self.help_screen(ui),
                });
        });
        content::confirmation(&ctx);
        if self.diagnosis.loading
            || self.status.loading
            || self.explanation.loading
            || self.head_plan.loading
            || self.task_detail.loading
            || self.preflight.loading
            || self.changes.loading
            || self.records.loading
            || self.support.pending()
            || self.report_export_pending()
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if self.launch_is_live() || self.execution.ledger.pending().is_some() {
            ctx.request_repaint_after(LAUNCH_OBSERVE_INTERVAL);
        } else if self.campaign.is_some() {
            ctx.request_repaint_after(STATUS_POLL_INTERVAL);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut refresh = false;
        let mut selected = None;
        ctx.input(|input| {
            if input.key_pressed(egui::Key::F5) {
                refresh = true;
            }
            if input.modifiers.command {
                for screen in Screen::ALL {
                    if input.key_pressed(screen.shortcut()) {
                        selected = Some(screen);
                    }
                }
            }
        });
        if let Some(screen) = selected {
            self.screen = screen;
        }
        if refresh {
            self.refresh_diagnosis();
            self.refresh_campaign();
        }
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top-bar").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("CodingMage");
                ui.separator();
                match &self.project {
                    Some(project) => {
                        ui.label(format!(
                            "Repository: {}",
                            project.config.target_path.display()
                        ));
                    }
                    None => {
                        ui.label("No repository opened");
                    }
                }
                ui.separator();
                let freshness = self.diagnosis.freshness(self.now);
                let age = age_label(self.diagnosis.age(self.now));
                ui.label(format!("Diagnosis: {} ({age})", freshness.label()));
                let doctor_command = self.doctor_arguments();
                let can_preview = command::can_preview(
                    self.binary_path.as_deref(),
                    doctor_command.as_deref(),
                );
                if ui
                    .add_enabled(
                        can_preview,
                        egui::Button::new("Refresh diagnosis"),
                    )
                    .clicked()
                {
                    self.refresh_diagnosis();
                }
                if let Some(arguments) = doctor_command {
                    command::show(ui, self.binary_path.as_deref(), &arguments);
                } else if self.project.is_some() {
                    ui.small("The coordinator or configuration path cannot be shown exactly as a command.");
                }
            });
        });
    }

    fn navigation(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .resizable(false)
            .default_size(current_tokens(root.ctx()).layout.navigation_width)
            .show(root, |ui| {
                ui.add_space(current_tokens(ui.ctx()).layout.navigation_gap);
                for screen in Screen::ALL {
                    if ui
                        .selectable_label(self.screen == screen, screen.label())
                        .clicked()
                    {
                        self.screen = screen;
                    }
                }
                ui.add_space(current_tokens(ui.ctx()).layout.section_gap);
                ui.separator();
                ui.small(messages::english().text("navigation_shortcuts"));
                ui.small(format!("Uptime {}s", self.started_at.elapsed().as_secs()));
            });
    }

    fn status_bar_fields(&self) -> (String, String, String, String) {
        let status_freshness = self.status.freshness(self.now);
        let status_stale = if status_freshness == Freshness::Stale {
            " (stale)"
        } else {
            ""
        };
        let (state, pods, backend_update) = if self.campaign.is_none() {
            (
                "State: no campaign selected".to_owned(),
                "Active pods: unknown".to_owned(),
                "Last backend update: none".to_owned(),
            )
        } else {
            match &self.status.value {
                Some(Some(status)) => {
                    let pods: BTreeSet<&str> = status
                        .active_tasks
                        .iter()
                        .filter_map(|task| task.pod_id.as_deref())
                        .filter(|id| !id.is_empty())
                        .collect();
                    (
                        format!(
                            "State: {}{status_stale}",
                            campaign_state_label(&status.state)
                        ),
                        format!("Active pods: {} identified{status_stale}", pods.len()),
                        format!(
                            "Last backend update: {} ms since 1970 UTC{status_stale}",
                            status.updated_at_ms
                        ),
                    )
                }
                Some(None) => (
                    format!("State: not started{status_stale}"),
                    format!("Active pods: 0 (not started){status_stale}"),
                    format!("Last backend update: none (not started){status_stale}"),
                ),
                None => (
                    match status_freshness {
                        Freshness::Loading => "State: loading from coordinator",
                        Freshness::Failed => "State: unavailable (refresh failed)",
                        _ => "State: not yet observed",
                    }
                    .to_owned(),
                    "Active pods: unknown".to_owned(),
                    "Last backend update: unknown".to_owned(),
                ),
            }
        };
        let mission_freshness = self.mission.freshness(self.now);
        let mission_stale = if mission_freshness == Freshness::Stale {
            " (stale)"
        } else {
            ""
        };
        let involvement = if self.campaign.is_none() {
            "Involvement: no campaign selected".to_owned()
        } else {
            match &self.mission.value {
                Some(Some(mission)) => {
                    let authority = if mission.revoked {
                        "; charter revoked"
                    } else if mission.expired {
                        "; charter expired"
                    } else {
                        ""
                    };
                    format!(
                        "Involvement: {} (last observed{authority}){mission_stale}",
                        involvement_label(&mission.involvement)
                    )
                }
                Some(None) => format!("Involvement: no charter admitted{mission_stale}"),
                None => match mission_freshness {
                    Freshness::Loading => "Involvement: loading from coordinator",
                    Freshness::Failed => "Involvement: unavailable (refresh failed)",
                    _ => "Involvement: not yet observed",
                }
                .to_owned(),
            }
        };
        (state, involvement, pods, backend_update)
    }

    fn status_bar(&self, root: &mut egui::Ui) {
        let (state, involvement, pods, backend_update) = self.status_bar_fields();
        egui::Panel::bottom("status-bar").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                match &self.connection {
                    Connection::Ready(_) => {
                        ui.label(messages::english().text("status_coordinator_configured"));
                    }
                    Connection::Unavailable(error) => {
                        ui.colored_label(
                            current_tokens(ui.ctx()).error,
                            format!("Coordinator unavailable: {error}"),
                        );
                    }
                }
                ui.separator();
                if let Some(project) = &self.project {
                    let name = project
                        .config
                        .target_path
                        .file_name()
                        .unwrap_or_else(|| project.config.target_path.as_os_str())
                        .to_string_lossy();
                    let repository_id = self
                        .campaign
                        .as_ref()
                        .map(|campaign| campaign.spec.repository_id.as_str())
                        .or_else(|| {
                            self.diagnosis
                                .value
                                .as_ref()
                                .map(|diagnosis| diagnosis.repository_id.as_str())
                        });
                    let identity = repository_id.map_or_else(
                        || "identity pending".to_owned(),
                        |id| {
                            format!(
                                "id {}…",
                                content::list_label(&id.chars().take(12).collect::<String>())
                            )
                        },
                    );
                    ui.label(format!(
                        "Repository: {} ({identity})",
                        content::list_label(&name)
                    ))
                    .on_hover_text(format!(
                        "Repository path: {}\nRepository ID: {}",
                        project.config.target_path.display(),
                        repository_id.unwrap_or("not yet observed")
                    ));
                } else {
                    ui.label("Repository: none opened");
                }
                ui.separator();
                ui.label(self.campaign.as_ref().map_or_else(
                    || "Campaign: none selected".to_owned(),
                    |campaign| {
                        format!(
                            "Campaign: {}",
                            content::list_label(&campaign.spec.campaign_id)
                        )
                    },
                ));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(state);
                ui.separator();
                ui.label(involvement);
                ui.separator();
                ui.label(pods);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Current gate: not reported by coordinator");
                ui.separator();
                ui.label(backend_update);
                if let Some((_, message)) = &self.status_line {
                    ui.separator();
                    ui.label(content::list_label(message));
                }
                if self.discarded_stale > 0 {
                    ui.separator();
                    ui.label(format!(
                        "{} stale responses discarded",
                        self.discarded_stale
                    ));
                }
            });
        });
    }

    fn settings_screen(&mut self, ui: &mut egui::Ui) {
        self.settings_screen_with_catalogue(ui, messages::english(), false);
    }

    fn settings_screen_with_catalogue(
        &mut self,
        ui: &mut egui::Ui,
        catalogue: &Catalogue,
        right_to_left: bool,
    ) {
        if right_to_left {
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                self.settings_content(ui, catalogue);
            });
        } else {
            self.settings_content(ui, catalogue);
        }
    }

    fn settings_content(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("settings_title"));
        ui.label(catalogue.text("settings_intro"));
        let before = self.appearance;
        ui.horizontal_wrapped(|ui| {
            for appearance in Appearance::ALL {
                let key = match appearance {
                    Appearance::System => "settings_system",
                    Appearance::Light => "settings_light",
                    Appearance::Dark => "settings_dark",
                    Appearance::HighContrast => "settings_high_contrast",
                };
                ui.radio_value(&mut self.appearance, appearance, catalogue.text(key));
            }
        });
        if self.appearance != before {
            self.applied_palette = None;
            ui.ctx().request_repaint();
        }
        let palette_key = match self.appearance.resolve(ui.ctx(), self.system_dark) {
            Palette::Light => "settings_light",
            Palette::Dark => "settings_dark",
            Palette::HighContrast => "settings_high_contrast",
        };
        ui.label(format!(
            "{} {}",
            catalogue.text("settings_palette_prefix"),
            catalogue.text(palette_key)
        ));
        ui.small(catalogue.text("settings_session_only"));
    }

    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        if let Connection::Unavailable(error) = &self.connection {
            failure_box(
                ui,
                "Coordinator executable unavailable",
                &error.to_string(),
                explain_code(&error.code()).1,
            );
        }
        if self.project.is_none() {
            ui.heading("Start with a repository");
            ui.label("What should the team do? Begin with the task source named by a repository configuration. Opening it shows readiness and the work plan; it starts no campaign or agent.");
            ui.strong(
                "Next: open an existing repository configuration below, or create one in Setup.",
            );
            self.open_controls(ui);
            ui.horizontal(|ui| {
                if ui.button("Create configuration in Setup").clicked() {
                    self.screen = Screen::Setup;
                }
                if ui.button("Open Help and About").clicked() {
                    self.screen = Screen::Help;
                }
            });
            ui.small("CodingMage keeps campaign records in the configured local state directory and working files in the configured scratch directory. Choose both in Setup before admitting a campaign.");
            ui.small("A provider uses its own existing sign-in. Check the provider and model in Setup, then run preflight; no provider or model is selected silently.");
            return;
        }
        self.overview_campaign(ui);
        ui.separator();
        let Some(project) = &self.project else {
            return;
        };
        egui::Grid::new("overview-grid")
            .num_columns(2)
            .spacing(current_tokens(ui.ctx()).layout.grid)
            .show(ui, |ui| {
                ui.label("Configuration");
                ui.monospace(project.config_path.display().to_string());
                ui.end_row();
                ui.label("Target path");
                ui.monospace(project.config.target_path.display().to_string());
                ui.end_row();
                ui.label("Task source");
                ui.monospace(project.config.task_source.display().to_string());
                ui.end_row();
                ui.label("Publication mode");
                ui.label(format!("{:?}", project.config.publication.mode));
                ui.end_row();
                ui.label("Agent profiles");
                ui.label(project.config.agent_profiles.len().to_string());
                ui.end_row();
                ui.label("Gate commands");
                ui.label(project.config.gate_commands.len().to_string());
                ui.end_row();
            });
        ui.separator();
        self.overview_diagnosis(ui);
        ui.separator();
        self.overview_plan(ui);
    }

    fn overview_campaign(&mut self, ui: &mut egui::Ui) {
        let catalogue = messages::english();
        ui.heading(catalogue.text("overview_campaign_title"));
        let Some(campaign) = &self.campaign else {
            ui.label(catalogue.text("overview_campaign_none"));
            if ui
                .button(catalogue.text("overview_campaign_open"))
                .clicked()
            {
                self.screen = Screen::Campaign;
            }
            return;
        };
        ui.label(format!(
            "{} {}",
            catalogue.text("overview_campaign_selected"),
            content::list_label(&campaign.spec.campaign_id)
        ));
        let freshness = self.status.freshness(self.now);
        ui.small(format!(
            "{} {} ({})",
            catalogue.text("overview_campaign_observation"),
            freshness.label(),
            age_label(self.status.age(self.now))
        ));
        match &self.status.value {
            Some(Some(status)) => {
                ui.label(format!(
                    "{} {}",
                    catalogue.text("overview_campaign_state"),
                    campaign_state_label(&status.state)
                ));
                ui.label(format!(
                    "{} {} {}, {} {}, {} {}, {} {}",
                    catalogue.text("overview_campaign_outcomes"),
                    status.outcomes.completed,
                    catalogue.text("overview_campaign_completed"),
                    status.outcomes.blocked,
                    catalogue.text("overview_campaign_blocked"),
                    status.outcomes.deferred,
                    catalogue.text("overview_campaign_deferred"),
                    status.outcomes.pending_human_decision,
                    catalogue.text("overview_campaign_decisions")
                ));
                if let Some(code) = &status.blocker_code {
                    ui.colored_label(
                        current_tokens(ui.ctx()).warning,
                        format!(
                            "{} {}",
                            catalogue.text("overview_campaign_attention"),
                            content::list_label(code)
                        ),
                    );
                } else if status.blocker_count > 0
                    || status.outcomes.blocked > 0
                    || status.outcomes.deferred > 0
                    || status.outcomes.pending_human_decision > 0
                {
                    ui.colored_label(
                        current_tokens(ui.ctx()).warning,
                        catalogue.text("overview_campaign_attention_counts"),
                    );
                } else {
                    ui.label(catalogue.text("overview_campaign_no_attention"));
                }
            }
            Some(None) => {
                ui.label(catalogue.text("overview_campaign_not_started"));
            }
            None => {
                let key = match freshness {
                    Freshness::Loading => "overview_campaign_loading",
                    Freshness::Failed => "overview_campaign_failed",
                    _ => "overview_campaign_unobserved",
                };
                ui.label(catalogue.text(key));
            }
        }
        if freshness == Freshness::Stale {
            ui.colored_label(
                current_tokens(ui.ctx()).warning,
                catalogue.text("overview_campaign_stale"),
            );
        }
        if ui
            .button(catalogue.text("overview_campaign_open"))
            .clicked()
        {
            self.screen = Screen::Campaign;
        }
    }

    fn overview_diagnosis(&self, ui: &mut egui::Ui) {
        ui.heading("Repository diagnosis");
        match self.diagnosis.freshness(self.now) {
            Freshness::NotRequested => {
                ui.label("Diagnosis has not been requested.");
            }
            Freshness::Loading => {
                ui.label("Requesting repository diagnosis from the coordinator...");
            }
            Freshness::Failed => {
                if let Some((_, error)) = &self.diagnosis.last_error {
                    backend_failure_box(ui, error, "failure_diagnosis_no_observation");
                }
            }
            Freshness::Live | Freshness::Stale => {
                if let Some((_, error)) = &self.diagnosis.last_error {
                    backend_failure_box(ui, error, "failure_diagnosis_retained");
                } else if self.diagnosis.freshness(self.now) == Freshness::Stale {
                    ui.colored_label(
                        current_tokens(ui.ctx()).warning,
                        format!(
                            "Stale: observed {}",
                            age_label(self.diagnosis.age(self.now))
                        ),
                    );
                }
                if let Some(diagnosis) = &self.diagnosis.value {
                    diagnosis_grid(ui, diagnosis);
                }
            }
        }
    }

    fn overview_plan(&self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        match &project.plan {
            Ok(plan) => {
                ui.label(format!(
                    "Task source parsed: {} sprints, {} stories, {} items ({} bytes, sha256 {}...)",
                    plan.plan.sprints.len(),
                    plan.plan.stories.len(),
                    plan.plan.items.len(),
                    plan.byte_length,
                    &plan.source_sha256[..12]
                ));
                if let Some(index) = &self.plan_index {
                    let counts = index.counts();
                    ui.label(format!(
                        "Sub-tasks: {} open ({} dependency-ready), {} checked in source; {} open acceptance criteria and gates",
                        counts.open_subtasks,
                        counts.ready_subtasks,
                        counts.checked_subtasks,
                        counts.open_acceptance
                    ));
                    ui.small("Source checkboxes are the repository's own claims; verified completion is shown separately on the Campaign screen.");
                }
            }
            Err(error) => {
                let (title, recovery) = if matches!(error, PlanLoadError::Stale) {
                    (
                        "Task source changed",
                        "Reopen the configuration to read the current task source.",
                    )
                } else {
                    (
                        "Task source unavailable",
                        "Fix the task source in the repository; the work plan stays empty until it parses.",
                    )
                };
                failure_box(ui, title, &error.to_string(), recovery);
            }
        }
    }

    fn browser_panel(&mut self, ui: &mut egui::Ui) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        let mut open: Option<PathBuf> = None;
        let mut enter: Option<PathBuf> = None;
        let mut up = false;
        let mut refresh = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Up").clicked() {
                    up = true;
                }
                if ui.button("Refresh").clicked() {
                    refresh = true;
                }
                ui.monospace(content::list_label(&browser.current.display().to_string()));
            });
            if browser.loading {
                ui.label("Loading directory through the coordinator…");
            }
            if let Some(arguments) = directory_list_arguments(&browser.current) {
                command::show_for(
                    ui,
                    "browse directory",
                    self.binary_path.as_deref(),
                    &arguments,
                );
            } else {
                command::show_unavailable_for(ui, "browse directory");
            }
            if let Some(error) = &browser.error {
                ui.colored_label(current_tokens(ui.ctx()).error, error);
            }
            if browser.truncated {
                ui.small("Listing truncated; navigate into a narrower directory.");
            }
            egui::ScrollArea::vertical()
                .max_height(current_tokens(ui.ctx()).layout.preview_tasks)
                .id_salt("browser-entries")
                .show(ui, |ui| {
                    for entry in &browser.entries {
                        let label = if entry.is_dir {
                            format!("{}/", content::list_label(&entry.name))
                        } else {
                            content::list_label(&entry.name)
                        };
                        let response = ui.add_enabled(!entry.is_symlink, egui::Button::new(label));
                        if response.clicked() {
                            if entry.is_dir {
                                enter = Some(entry.path.clone());
                            } else {
                                open = Some(entry.path.clone());
                            }
                        }
                    }
                });
        });
        let mut moved = up && browser.up();
        if let Some(path) = enter {
            moved |= browser.enter(&path);
        }
        if moved || refresh {
            self.request_browser(BrowserSlot::Configuration);
        }
        if let Some(path) = open {
            self.open_project(&path);
        }
    }

    fn work_plan(&mut self, ui: &mut egui::Ui) {
        self.work_plan_with_catalogue(ui, messages::english(), false);
    }

    fn work_plan_with_catalogue(
        &mut self,
        ui: &mut egui::Ui,
        catalogue: &Catalogue,
        right_to_left: bool,
    ) {
        if right_to_left {
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                self.work_plan_content(ui, catalogue);
            });
        } else {
            self.work_plan_content(ui, catalogue);
        }
    }

    fn work_plan_content(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        ui.heading(catalogue.text("work_plan_title"));
        if self.project.is_none() {
            ui.label(catalogue.text("work_plan_no_project"));
            if ui.button(catalogue.text("work_plan_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            if ui.button(catalogue.text("work_plan_open_help")).clicked() {
                self.screen = Screen::Help;
            }
            return;
        }
        if self.campaign.is_some() {
            self.work_plan_status_controls(ui, catalogue);
        }
        let Some(project) = &self.project else {
            return;
        };
        let Some(index) = &self.plan_index else {
            if let Err(error) = &project.plan {
                let (title, recovery) = if matches!(error, PlanLoadError::Stale) {
                    ("work_plan_source_stale", "work_plan_source_stale_recovery")
                } else {
                    ("work_plan_source_unavailable", "work_plan_source_recovery")
                };
                failure_box(
                    ui,
                    catalogue.text(title),
                    &error.to_string(),
                    catalogue.text(recovery),
                );
            }
            if ui.button(catalogue.text("work_plan_open_setup")).clicked() {
                self.screen = Screen::Setup;
            }
            if ui.button(catalogue.text("work_plan_open_help")).clicked() {
                self.screen = Screen::Help;
            }
            return;
        };
        let path = project.task_source_path().display().to_string();
        ui.small(catalogue.format("work_plan_source_claim", &[("path", &path)]));
        let mut filter = self.plan_filter.clone();
        plan_filter_controls(ui, &mut filter, catalogue);
        let rows = index.filtered(&filter);
        let shown = rows.len().to_string();
        let total = index.counts().items.to_string();
        ui.label(catalogue.format(
            "work_plan_items_shown",
            &[("shown", &shown), ("total", &total)],
        ));
        let observation_known = self.campaign.is_some()
            && !self.status.loading
            && self.status.freshness(self.now) == Freshness::Live
            && self.status.value.as_ref().is_some_and(Option::is_some);
        let overlay = if observation_known {
            self.task_overlay()
        } else {
            std::collections::BTreeMap::new()
        };
        let mut selected = self.selected_item.clone();
        render_plan_rows(
            ui,
            index,
            &rows,
            &overlay,
            observation_known,
            &mut selected,
            catalogue,
        );
        self.plan_filter = filter;
        if self.selected_item != selected {
            self.task_detail.clear();
            self.task_detail_key = None;
        }
        self.selected_item = selected;
        if let Some(row) = self
            .selected_item
            .as_ref()
            .and_then(|id| index.rows().iter().find(|row| &row.id == id))
            .cloned()
        {
            ui.separator();
            item_detail(ui, &row, index, catalogue);
            coordinator_detail(ui, overlay.get(&row.id), observation_known, catalogue);
            self.task_source_detail(ui, &row, catalogue);
        }
    }

    fn work_plan_status_controls(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let arguments = self.status_arguments();
        let previewable = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        let action = catalogue.text("work_plan_refresh_status");
        if ui
            .add_enabled(
                previewable && !self.status.loading,
                egui::Button::new(action),
            )
            .clicked()
        {
            self.request_status();
        }
        if let Some(arguments) = &arguments {
            command::show_for(ui, action, self.binary_path.as_deref(), arguments);
        } else {
            command::show_unavailable_for(ui, action);
        }
        let freshness = self.status.freshness(self.now);
        let age = age_label(self.status.age(self.now));
        ui.small(catalogue.format(
            "work_plan_status_observation",
            &[
                ("freshness", freshness.label()),
                ("age", &age),
                ("head", self.head_plan.freshness(self.now).label()),
            ],
        ));
        if self.status.loading {
            ui.label(catalogue.text("work_plan_status_loading"));
        } else if let Some((_, error)) = &self.status.last_error {
            let (cause, action) = explain_code(&error.code());
            failure_box(
                ui,
                catalogue.text("work_plan_status_unavailable"),
                &format!("{} ({})", cause, error.code()),
                &catalogue.format("work_plan_status_failure_effect", &[("action", action)]),
            );
        } else if freshness == Freshness::Stale {
            ui.label(catalogue.text("work_plan_status_stale"));
        } else if freshness == Freshness::NotRequested {
            ui.label(catalogue.text("work_plan_status_unrequested"));
        } else if self.status.value.as_ref().is_some_and(Option::is_none) {
            ui.label(catalogue.text("work_plan_status_absent"));
        }
        self.work_plan_report_controls(ui, catalogue);
    }

    fn work_plan_report_controls(&mut self, ui: &mut egui::Ui, catalogue: &Catalogue) {
        let Some(arguments) = self.report_arguments() else {
            return;
        };
        let can_refresh = command::can_preview(self.binary_path.as_deref(), Some(&arguments));
        let action = catalogue.text("work_plan_refresh_report");
        if ui
            .add_enabled(
                can_refresh && !self.report.loading,
                egui::Button::new(action),
            )
            .clicked()
        {
            self.request_report();
        }
        command::show_for(ui, action, self.binary_path.as_deref(), &arguments);
        let freshness = self.report.freshness(self.now);
        let age = age_label(self.report.age(self.now));
        ui.small(catalogue.format(
            "work_plan_report_observation",
            &[("freshness", freshness.label()), ("age", &age)],
        ));
        if self.report.loading {
            ui.label(catalogue.text("work_plan_report_loading"));
        } else if let Some((_, error)) = &self.report.last_error {
            let (cause, action) = explain_code(&error.code());
            failure_box(
                ui,
                catalogue.text("work_plan_report_unavailable"),
                &format!("{} ({})", cause, error.code()),
                &catalogue.format("work_plan_report_failure_effect", &[("action", action)]),
            );
        } else if freshness == Freshness::Stale {
            ui.label(catalogue.text("work_plan_report_stale"));
        } else if freshness == Freshness::NotRequested {
            ui.label(catalogue.text("work_plan_report_unrequested"));
        } else if self.report.value.as_ref().is_some_and(Option::is_none) {
            ui.label(catalogue.text("work_plan_report_absent"));
        }
    }

    fn task_source_detail(&mut self, ui: &mut egui::Ui, row: &PlanRow, catalogue: &Catalogue) {
        let arguments = self.task_detail_arguments(&row.id);
        let can_load = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        let action = catalogue.text("work_plan_source_load");
        if ui
            .add_enabled(
                can_load && !self.task_detail.loading,
                egui::Button::new(action),
            )
            .clicked()
        {
            self.request_task_detail(&row.id);
        }
        if let Some(arguments) = &arguments {
            command::show_for(ui, action, self.binary_path.as_deref(), arguments);
        } else {
            command::show_unavailable_for(ui, action);
            ui.small(catalogue.text("work_plan_source_select_campaign"));
        }
        if self.task_detail.loading {
            ui.label(catalogue.text("work_plan_source_loading"));
        }
        if let Some((_, error)) = &self.task_detail.last_error {
            ui.label(catalogue.format("work_plan_source_failure", &[("code", &error.code())]));
        }
        let Some(detail) = self.task_detail.value.as_ref().filter(|detail| {
            self.task_detail_key.as_ref() == Some(&(detail.head.clone(), detail.item_id.clone()))
                && detail.item_id == row.id
                && !self.status.loading
                && self.status.freshness(self.now) == Freshness::Live
                && self.task_detail.freshness(self.now) == Freshness::Live
        }) else {
            return;
        };
        render_source_detail(ui, detail, catalogue);
        self.task_run_evidence(ui, &row.id);
    }

    fn open_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let label = ui.label("Configuration file");
            let edit = egui::TextEdit::singleline(&mut self.config_input)
                .hint_text("/absolute/path/codingmage.toml")
                .desired_width(current_tokens(ui.ctx()).layout.field_long);
            let response = ui.add(edit).labelled_by(label.id);
            let submitted =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let candidate = PathBuf::from(self.config_input.trim());
            let candidate_arguments = project_open_arguments(&candidate);
            let can_open = self.pending_project.is_none()
                && candidate_arguments.as_ref().is_some_and(|arguments| {
                    command::can_preview(self.binary_path.as_deref(), Some(arguments))
                });
            if ui
                .add_enabled(can_open, egui::Button::new("Open"))
                .clicked()
                || (submitted && can_open)
            {
                self.open_project(&candidate);
            }
            if self.project.is_some() && ui.button("Close").clicked() {
                self.close_project();
            }
            if ui
                .button(if self.browser.is_some() {
                    "Hide browser"
                } else {
                    "Browse"
                })
                .clicked()
            {
                self.browser = match self.browser.take() {
                    Some(_) => None,
                    None => Some(Browser::at_home(vec!["toml"])),
                };
                if self.browser.is_some() {
                    self.request_browser(BrowserSlot::Configuration);
                }
            }
        });
        let selected_path = self
            .pending_project
            .clone()
            .unwrap_or_else(|| PathBuf::from(self.config_input.trim()));
        let open_arguments = project_open_arguments(&selected_path);
        if let Some(arguments) = &open_arguments {
            command::show_for(
                ui,
                "open configuration",
                self.binary_path.as_deref(),
                arguments,
            );
        } else {
            command::show_unavailable_for(ui, "open configuration");
        }
        if self.pending_project.is_some() {
            ui.label("Opening configuration through the coordinator…");
        }
        self.browser_panel(ui);
        if let Some(error) = &self.open_error {
            let recovery = match error {
                OpenError::Backend(BackendError::BinaryUnavailable { .. }) => {
                    "Install the sibling codingmage executable and reopen this configuration."
                }
                OpenError::Contract | OpenError::UnsupportedSchema => {
                    "Update the sibling codingmage executable to the matching version, then reopen."
                }
                _ => {
                    "Check the configuration path, authority roots, task source and policies, then reopen."
                }
            };
            failure_box(
                ui,
                "Configuration could not be opened",
                &error.to_string(),
                recovery,
            );
        }
        if !self.recent.configs.is_empty() {
            ui.label("Recent configurations");
            let recent = self.recent.configs.clone();
            for path in recent {
                if ui.button(path.display().to_string()).clicked() {
                    self.open_project(&path);
                }
            }
        }
    }
}

fn source_state_label(catalogue: &Catalogue, state: CheckState) -> &str {
    catalogue.text(match state {
        CheckState::Open => "work_plan_source_open",
        CheckState::Checked => "work_plan_source_checked",
    })
}

fn render_source_detail(ui: &mut egui::Ui, detail: &TaskDetailProjection, catalogue: &Catalogue) {
    ui.small(catalogue.format(
        "work_plan_source_head",
        &[
            ("head", &detail.head[..12]),
            ("state", source_state_label(catalogue, detail.source_state)),
        ],
    ));
    ui.collapsing(catalogue.text("work_plan_source_excerpt"), |ui| {
        let characters = detail.excerpt.chars().collect::<Vec<_>>();
        for chunk in characters.chunks(content::MAX_PREVIEW_CHARS) {
            content::render(ui, &chunk.iter().collect::<String>());
        }
        if detail.truncated {
            ui.small(catalogue.text("work_plan_source_excerpt_truncated"));
        }
    });
    ui.collapsing(catalogue.text("work_plan_source_criteria"), |ui| {
        if detail.story_criteria.is_empty() {
            ui.label(catalogue.text("work_plan_source_criteria_none"));
        }
        for criterion in &detail.story_criteria {
            ui.horizontal_wrapped(|ui| {
                content::render(ui, &criterion.id);
                ui.label(catalogue.format(
                    "work_plan_source_criterion_state",
                    &[(
                        "state",
                        source_state_label(catalogue, criterion.source_state),
                    )],
                ));
                content::render(ui, &criterion.title);
                if criterion.title_truncated {
                    ui.small(catalogue.text("work_plan_source_criterion_title_truncated"));
                }
            });
        }
        if detail.story_criteria_truncated {
            ui.small(catalogue.text("work_plan_source_criteria_truncated"));
        }
    });
}

fn diagnosis_grid(ui: &mut egui::Ui, diagnosis: &Diagnosis) {
    egui::Grid::new("diagnosis-grid")
        .num_columns(2)
        .spacing(current_tokens(ui.ctx()).layout.grid)
        .show(ui, |ui| {
            ui.label("State");
            ui.label(&diagnosis.state);
            ui.end_row();
            ui.label("Repository id");
            ui.monospace(&diagnosis.repository_id);
            ui.end_row();
            ui.label("Head");
            ui.monospace(&diagnosis.head);
            ui.end_row();
            ui.label("Branch");
            ui.monospace(diagnosis.branch.as_deref().unwrap_or("detached"));
            ui.end_row();
            ui.label("Clean checkout");
            ui.label(if diagnosis.clean { "yes" } else { "no" });
            ui.end_row();
            ui.label("Unsafe checkout features");
            ui.label(if diagnosis.unsafe_checkout_features {
                "present"
            } else {
                "absent"
            });
            ui.end_row();
            ui.label("Task source sha256");
            ui.monospace(&diagnosis.task_source_sha256);
            ui.end_row();
            ui.label("Items");
            ui.label(format!(
                "{} sprints, {} stories, {} items",
                diagnosis.sprints, diagnosis.stories, diagnosis.items
            ));
            ui.end_row();
            ui.label("External capabilities");
            ui.label(if diagnosis.configuration.capabilities.all_denied() {
                "all denied"
            } else {
                "some granted"
            });
            ui.end_row();
            ui.label("Publication");
            ui.label(&diagnosis.configuration.publication.mode);
            ui.end_row();
        });
}

fn plan_filter_controls(ui: &mut egui::Ui, filter: &mut PlanFilter, catalogue: &Catalogue) {
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(catalogue.text("work_plan_search"));
        ui.add(
            egui::TextEdit::singleline(&mut filter.query)
                .hint_text(catalogue.text("work_plan_search_hint"))
                .desired_width(current_tokens(ui.ctx()).layout.field_medium),
        )
        .labelled_by(label.id);
        for state in StateFilter::ALL {
            ui.selectable_value(
                &mut filter.state,
                state,
                state_filter_label(catalogue, state),
            );
        }
        ui.separator();
        for kind in KindFilter::ALL {
            ui.selectable_value(&mut filter.kind, kind, kind_filter_label(catalogue, kind));
        }
        ui.separator();
        ui.checkbox(
            &mut filter.ready_only,
            catalogue.text("work_plan_ready_only"),
        );
    });
}

fn state_filter_label(catalogue: &Catalogue, state: StateFilter) -> &str {
    catalogue.text(match state {
        StateFilter::All => "work_plan_state_all",
        StateFilter::Open => "work_plan_state_open",
        StateFilter::Checked => "work_plan_state_checked",
    })
}

fn kind_filter_label(catalogue: &Catalogue, kind: KindFilter) -> &str {
    catalogue.text(match kind {
        KindFilter::All => "work_plan_kind_all",
        KindFilter::SubTasks => "work_plan_kind_subtasks",
        KindFilter::Tasks => "work_plan_kind_tasks",
        KindFilter::Acceptance => "work_plan_kind_acceptance",
    })
}

fn render_plan_rows(
    ui: &mut egui::Ui,
    index: &PlanIndex,
    rows: &[&PlanRow],
    overlay: &std::collections::BTreeMap<String, crate::campaign::TaskOverlay>,
    observation_known: bool,
    selected: &mut Option<String>,
    catalogue: &Catalogue,
) {
    let display_rows = plan_display_rows(rows);
    let row_height = ui
        .spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body))
        + 4.0;
    egui::ScrollArea::vertical()
        .id_salt("plan-rows")
        .max_height(current_tokens(ui.ctx()).layout.preview_tall)
        .show_rows(ui, row_height, display_rows.len(), |ui, range| {
            for entry in &display_rows[range] {
                let row = rows[entry.index];
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        match entry.kind {
                            PlanDisplayKind::Sprint => {
                                let title = content::list_label(
                                    index.sprint_title(&row.sprint_id).unwrap_or(""),
                                );
                                ui.strong(catalogue.format(
                                    "work_plan_sprint_header",
                                    &[("id", &row.sprint_id), ("title", &title)],
                                ));
                            }
                            PlanDisplayKind::Story => {
                                if let Some(story) = &row.story_id {
                                    let title =
                                        content::list_label(index.story_title(story).unwrap_or(""));
                                    ui.label(catalogue.format(
                                        "work_plan_story_header",
                                        &[("id", story), ("title", &title)],
                                    ));
                                }
                            }
                            PlanDisplayKind::Item => {
                                let mut checked = row.state == CheckState::Checked;
                                ui.add_enabled(false, egui::Checkbox::without_text(&mut checked))
                                    .on_disabled_hover_text(
                                        catalogue.text("work_plan_checkbox_help"),
                                    );
                                let task = overlay.get(&row.id);
                                let badge_key = outcome_badge_key(task, observation_known);
                                let labels = coordinator_labels(task, observation_known, catalogue);
                                let hover = if labels.is_empty() {
                                    catalogue.text("work_plan_outcome_hover_empty").to_owned()
                                } else {
                                    labels.join("; ")
                                };
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                let outcome_label = if badge_key == "work_plan_outcome_unknown" {
                                    catalogue.format(
                                        "work_plan_outcome_for_item",
                                        &[("badge", catalogue.text(badge_key)), ("id", &row.id)],
                                    )
                                } else {
                                    catalogue.format(
                                        "work_plan_outcome_label",
                                        &[("badge", catalogue.text(badge_key))],
                                    )
                                };
                                ui.label(outcome_label)
                                    .on_hover_text(content::list_label(&hover));
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                let label = row_label(row, catalogue);
                                if ui
                                    .selectable_label(
                                        selected.as_deref() == Some(row.id.as_str()),
                                        label,
                                    )
                                    .clicked()
                                {
                                    *selected = Some(row.id.clone());
                                }
                            }
                        }
                    },
                );
            }
        });
}

#[derive(Clone, Copy)]
enum PlanDisplayKind {
    Sprint,
    Story,
    Item,
}

#[derive(Clone, Copy)]
struct PlanDisplayRow {
    kind: PlanDisplayKind,
    index: usize,
}

fn plan_display_rows(rows: &[&PlanRow]) -> Vec<PlanDisplayRow> {
    let mut display = Vec::with_capacity(rows.len());
    let mut last_sprint = None;
    let mut last_story = None;
    for (index, row) in rows.iter().enumerate() {
        if last_sprint != Some(row.sprint_id.as_str()) {
            display.push(PlanDisplayRow {
                kind: PlanDisplayKind::Sprint,
                index,
            });
            last_sprint = Some(row.sprint_id.as_str());
            last_story = None;
        }
        if last_story != row.story_id.as_deref() {
            if row.story_id.is_some() {
                display.push(PlanDisplayRow {
                    kind: PlanDisplayKind::Story,
                    index,
                });
            }
            last_story = row.story_id.as_deref();
        }
        display.push(PlanDisplayRow {
            kind: PlanDisplayKind::Item,
            index,
        });
    }
    display
}

fn coordinator_labels(
    overlay: Option<&crate::campaign::TaskOverlay>,
    observation_known: bool,
    catalogue: &Catalogue,
) -> Vec<String> {
    let Some(task) = overlay else {
        return if observation_known {
            Vec::new()
        } else {
            vec![catalogue.text("work_plan_coordinator_unknown").to_owned()]
        };
    };
    task.labels(observation_known)
        .into_iter()
        .filter(|state| !state.ends_with("in source"))
        .collect()
}

fn outcome_badge_key(
    overlay: Option<&crate::campaign::TaskOverlay>,
    observation_known: bool,
) -> &'static str {
    let Some(task) = overlay else {
        return if observation_known {
            "work_plan_outcome_none"
        } else {
            "work_plan_outcome_unknown"
        };
    };
    if task.accepted.is_some() {
        "work_plan_outcome_accepted"
    } else if task.campaign_head == Some(CheckState::Checked) {
        "work_plan_outcome_completed"
    } else if task.human_decision.is_some() {
        "work_plan_outcome_human"
    } else if task.blocked.is_some() {
        "work_plan_outcome_blocked"
    } else if task.deferred.is_some() {
        "work_plan_outcome_deferred"
    } else if task.active.is_some() {
        "work_plan_outcome_active"
    } else if observation_known {
        "work_plan_outcome_none"
    } else {
        "work_plan_outcome_unknown"
    }
}

fn coordinator_detail(
    ui: &mut egui::Ui,
    overlay: Option<&crate::campaign::TaskOverlay>,
    observation_known: bool,
    catalogue: &Catalogue,
) {
    ui.label(catalogue.text("work_plan_coordinator_heading"));
    let labels = coordinator_labels(overlay, observation_known, catalogue);
    if labels.is_empty() {
        ui.label(catalogue.text("work_plan_coordinator_none"));
    } else {
        for label in labels {
            content::render(ui, &label);
        }
    }
}

fn row_label(row: &PlanRow, catalogue: &Catalogue) -> String {
    let checkbox = match row.state {
        CheckState::Open => "[ ]",
        CheckState::Checked => "[x]",
    };
    let kind = catalogue.text(match row.kind {
        PlanItemKind::Task => "work_plan_kind_task",
        PlanItemKind::SubTask => "work_plan_kind_subtask",
        PlanItemKind::AcceptanceCriterion => "work_plan_kind_criterion",
        PlanItemKind::Gate => "work_plan_kind_gate",
    });
    let readiness = match row.readiness {
        SourceReadiness::NotApplicable => String::new(),
        other => catalogue.format(
            "work_plan_readiness_suffix",
            &[("state", source_readiness_label(catalogue, other))],
        ),
    };
    let title = content::list_label(&row.title);
    catalogue.format(
        "work_plan_row",
        &[
            ("checkbox", checkbox),
            ("kind", kind),
            ("id", &row.id),
            ("title", &title),
            ("readiness", &readiness),
        ],
    )
}

fn source_readiness_label(catalogue: &Catalogue, readiness: SourceReadiness) -> &str {
    catalogue.text(match readiness {
        SourceReadiness::Checked => "work_plan_readiness_checked",
        SourceReadiness::Ready => "work_plan_readiness_ready",
        SourceReadiness::Waiting => "work_plan_readiness_waiting",
        SourceReadiness::NotApplicable => "work_plan_detail_readiness_na",
    })
}

fn item_detail(ui: &mut egui::Ui, row: &PlanRow, index: &PlanIndex, catalogue: &Catalogue) {
    ui.heading(catalogue.format("work_plan_item_heading", &[("id", &row.id)]));
    item_detail_grid(ui, row, catalogue);
    item_detail_dependencies(ui, row, index, catalogue);
    if ui
        .button(catalogue.text("work_plan_detail_copy_identifier"))
        .clicked()
    {
        ui.ctx().copy_text(row.id.clone());
    }
}

fn item_detail_grid(ui: &mut egui::Ui, row: &PlanRow, catalogue: &Catalogue) {
    egui::Grid::new("item-detail")
        .num_columns(2)
        .spacing(current_tokens(ui.ctx()).layout.grid)
        .show(ui, |ui| {
            ui.label(catalogue.text("work_plan_detail_title"));
            content::render(ui, &row.title);
            ui.end_row();
            ui.label(catalogue.text("work_plan_detail_source_checkbox"));
            let source_key = match row.state {
                CheckState::Open => "work_plan_detail_source_open",
                CheckState::Checked => "work_plan_detail_source_checked",
            };
            ui.label(catalogue.text(source_key));
            ui.end_row();
            ui.label(catalogue.text("work_plan_detail_location"));
            let line = row.line.to_string();
            ui.monospace(catalogue.format(
                "work_plan_detail_location_value",
                &[("line", &line), ("digest", &row.line_sha256[..12])],
            ));
            ui.end_row();
            ui.label(catalogue.text("work_plan_detail_parent"));
            ui.monospace(&row.parent_id);
            ui.end_row();
            ui.label(catalogue.text("work_plan_detail_readiness"));
            ui.label(source_readiness_label(catalogue, row.readiness));
            ui.end_row();
        });
}

fn item_detail_dependencies(
    ui: &mut egui::Ui,
    row: &PlanRow,
    index: &PlanIndex,
    catalogue: &Catalogue,
) {
    if row.dependencies.is_empty() {
        ui.label(catalogue.text("work_plan_detail_dependencies_none"));
    } else {
        ui.label(catalogue.text("work_plan_detail_dependencies"));
        for dependency in &row.dependencies {
            let key = match dependency.state {
                Some(CheckState::Checked) => "work_plan_detail_dependency_checked",
                Some(CheckState::Open) => "work_plan_detail_dependency_open",
                None => "work_plan_detail_dependency_unknown",
            };
            ui.monospace(catalogue.format(
                "work_plan_detail_dependency_row",
                &[("id", &dependency.id), ("state", catalogue.text(key))],
            ));
        }
    }
    let dependents = index.dependents(&row.id);
    if !dependents.is_empty() {
        ui.label(catalogue.text("work_plan_detail_dependents"));
        for dependent in dependents {
            ui.monospace(
                catalogue.format("work_plan_detail_dependent_row", &[("id", &dependent.id)]),
            );
        }
    }
}

/// Renders one failure state with what happened and what to do.
pub fn failure_box(ui: &mut egui::Ui, title: &str, detail: &str, action: &str) {
    let tokens = current_tokens(ui.ctx());
    egui::Frame::group(ui.style())
        .fill(tokens.panel)
        .show(ui, |ui| {
            ui.colored_label(tokens.error, title);
            ui.monospace(detail);
            ui.label(action);
        });
}

fn backend_failure_box(ui: &mut egui::Ui, error: &BackendError, effect_key: &str) {
    let catalogue = messages::english();
    let (what, action) = match error {
        BackendError::Contract(crate::backend::models::ModelError::AuthorityMismatch) => (
            catalogue.text("failure_identity_mismatch_what"),
            catalogue.text("failure_identity_mismatch_action"),
        ),
        BackendError::Contract(crate::backend::models::ModelError::UnsupportedSchema {
            ..
        }) => (
            catalogue.text("failure_unsupported_schema_what"),
            catalogue.text("failure_unsupported_schema_action"),
        ),
        _ => explain_code(&error.code()),
    };
    failure_box(
        ui,
        catalogue.failure_title(error.failure_state()),
        what,
        action,
    );
    ui.small(format!("Cause code: {}. {error}", error.code()));
    ui.small(catalogue.text(effect_key));
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    struct SettingsPreviewApp {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for SettingsPreviewApp {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                self.app
                    .settings_screen_with_catalogue(ui, &self.catalogue, self.right_to_left);
            });
        }
    }

    #[test]
    fn settings_choices_remain_labelled_with_expanded_right_aligned_text() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let radio_labels = [
                "settings_system",
                "settings_light",
                "settings_dark",
                "settings_high_contrast",
            ]
            .map(|key| catalogue.text(key).to_owned());
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |creation| SettingsPreviewApp {
                    app: App::with_state_dir(
                        &creation.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("/example/missing/codingmage"),
                        }),
                        Ok(std::env::temp_dir().join("codingmage-ui-settings-preview")),
                    ),
                    catalogue,
                    right_to_left,
                });
            harness.run_steps(2);
            assert!(
                harness
                    .get_by_label_contains("Settings")
                    .accesskit_node()
                    .has_bounds()
            );
            for label in &radio_labels {
                assert!(
                    harness
                        .get_by_role_and_label(egui::accesskit::Role::RadioButton, label)
                        .accesskit_node()
                        .has_bounds(),
                    "missing bounds for {label}"
                );
            }
        }
    }
}

#[cfg(test)]
mod work_plan_tests {
    use super::*;
    use codingmage_plan::TaskPlan;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};

    struct WorkPlanPreview {
        app: App,
        catalogue: Catalogue,
        right_to_left: bool,
    }

    impl eframe::App for WorkPlanPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                self.app
                    .work_plan_with_catalogue(ui, &self.catalogue, self.right_to_left);
            });
        }
    }

    struct LoadedPlanPreview {
        catalogue: Catalogue,
        index: PlanIndex,
        filter: PlanFilter,
        selected: Option<String>,
        right_to_left: bool,
    }

    impl LoadedPlanPreview {
        fn render(&mut self, ui: &mut egui::Ui) {
            plan_filter_controls(ui, &mut self.filter, &self.catalogue);
            let rows = self.index.filtered(&self.filter);
            render_plan_rows(
                ui,
                &self.index,
                &rows,
                &std::collections::BTreeMap::new(),
                false,
                &mut self.selected,
                &self.catalogue,
            );
            if let Some(row) = self
                .selected
                .as_ref()
                .and_then(|id| self.index.rows().iter().find(|row| &row.id == id))
            {
                item_detail(ui, row, &self.index, &self.catalogue);
            }
        }
    }

    impl eframe::App for LoadedPlanPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                if self.right_to_left {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        self.render(ui);
                    });
                } else {
                    self.render(ui);
                }
            });
        }
    }

    struct SourceDetailPreview {
        catalogue: Catalogue,
        detail: TaskDetailProjection,
        right_to_left: bool,
    }

    impl eframe::App for SourceDetailPreview {
        fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(root, |ui| {
                if self.right_to_left {
                    ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                        render_source_detail(ui, &self.detail, &self.catalogue);
                    });
                } else {
                    render_source_detail(ui, &self.detail, &self.catalogue);
                }
            });
        }
    }

    #[test]
    fn expanded_right_aligned_work_plan_empty_state_keeps_recovery_reachable() {
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let title = catalogue.text("work_plan_title").to_owned();
            let guidance = catalogue.text("work_plan_no_project").to_owned();
            let setup = catalogue.text("work_plan_open_setup").to_owned();
            let help = catalogue.text("work_plan_open_help").to_owned();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |creation| WorkPlanPreview {
                    app: App::with_state_dir(
                        &creation.egui_ctx,
                        Err(BackendError::BinaryUnavailable {
                            expected: PathBuf::from("/example/missing/codingmage"),
                        }),
                        Ok(std::env::temp_dir().join("codingmage-ui-work-plan-preview")),
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
                .get_by_role_and_label(egui::accesskit::Role::Button, &help)
                .click();
            harness.run_steps(1);
            assert_eq!(harness.state().app.screen, Screen::Help);
        }
    }

    #[test]
    fn expanded_right_aligned_loaded_plan_preserves_row_and_detail_access() {
        let source = b"# Tasks\n\n## Sprint 1 - Local\n\n**Sprint goal:** Local.\n\n### Story 1.1 - Work\n\n- [ ] **Task 1.1.1 - Goal**\n  - [x] **Sub-task 1.1.1.1:** Prepare a fixture.\n  - [ ] **Sub-task 1.1.1.2:** Inspect the result.\n    <!-- depends-on: 1.1.1.1 -->\n";
        let index = PlanIndex::new(&TaskPlan::parse(source).unwrap());
        let row = index.rows().iter().find(|row| row.id == "1.1.1.2").unwrap();
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let label = row_label(row, &catalogue);
            let unknown = catalogue.format(
                "work_plan_outcome_for_item",
                &[
                    ("badge", catalogue.text("work_plan_outcome_unknown")),
                    ("id", &row.id),
                ],
            );
            let heading = catalogue.format("work_plan_item_heading", &[("id", &row.id)]);
            let source_label = catalogue
                .text("work_plan_detail_source_checkbox")
                .to_owned();
            let dependency = catalogue.format(
                "work_plan_detail_dependency_row",
                &[
                    ("id", "1.1.1.1"),
                    (
                        "state",
                        catalogue.text("work_plan_detail_dependency_checked"),
                    ),
                ],
            );
            let preview_index = index.clone();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |_| LoadedPlanPreview {
                    catalogue,
                    index: preview_index,
                    filter: PlanFilter::default(),
                    selected: None,
                    right_to_left,
                });
            harness.run_steps(2);
            assert!(
                harness
                    .get_by_label_contains(&unknown)
                    .accesskit_node()
                    .has_bounds()
            );
            let row_node = harness.get_by_label_contains(&label);
            assert!(row_node.accesskit_node().has_bounds());
            row_node.click();
            harness.run_steps(1);
            assert_eq!(harness.state().selected.as_deref(), Some("1.1.1.2"));
            for label in [&heading, &source_label, &dependency] {
                assert!(
                    harness
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds(),
                    "missing accessible bounds for {label}"
                );
            }
        }
    }

    #[test]
    fn expanded_right_aligned_source_detail_keeps_head_and_criteria_visible() {
        let detail = TaskDetailProjection {
            schema_version: 1,
            campaign_id: "campaign-1".to_owned(),
            repository_id: "repository-1".to_owned(),
            head: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            task_source_sha256: "a".repeat(64),
            item_id: "1.1.1.2".to_owned(),
            kind: PlanItemKind::SubTask,
            source_state: CheckState::Checked,
            source_line: 8,
            source_line_sha256: "b".repeat(64),
            excerpt: "Literal <script>source</script> text".to_owned(),
            truncated: true,
            story_criteria: vec![crate::backend::models::TaskCriterion {
                id: "AC 1.1".to_owned(),
                title: "A source claim, not an accepted outcome".to_owned(),
                title_truncated: false,
                source_state: CheckState::Open,
            }],
            story_criteria_truncated: false,
        };
        for right_to_left in [false, true] {
            let catalogue = messages::english().pseudo(right_to_left);
            let head = catalogue.format(
                "work_plan_source_head",
                &[
                    ("head", "0123456789ab"),
                    ("state", catalogue.text("work_plan_source_checked")),
                ],
            );
            let excerpt = catalogue.text("work_plan_source_excerpt").to_owned();
            let criteria = catalogue.text("work_plan_source_criteria").to_owned();
            let criterion_state = catalogue.format(
                "work_plan_source_criterion_state",
                &[("state", catalogue.text("work_plan_source_open"))],
            );
            let preview_detail = detail.clone();
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::Vec2::new(1024.0, 640.0))
                .with_pixels_per_point(2.0)
                .with_max_steps(4)
                .build_eframe(move |_| SourceDetailPreview {
                    catalogue,
                    detail: preview_detail,
                    right_to_left,
                });
            harness.run_steps(2);
            assert!(
                harness
                    .get_by_label_contains(&head)
                    .accesskit_node()
                    .has_bounds()
            );
            harness.get_by_label_contains(&excerpt).click();
            harness.get_by_label_contains(&criteria).click();
            harness.run_steps(1);
            for label in ["Literal <script>source</script> text", &criterion_state] {
                assert!(
                    harness
                        .get_by_label_contains(label)
                        .accesskit_node()
                        .has_bounds(),
                    "missing accessible bounds for {label}"
                );
            }
        }
    }
}

#[cfg(test)]
mod browser_response_tests {
    use super::*;

    fn response(app: &App, label: &'static str, request_id: &str, path: &str) -> Response {
        Response {
            generation: app.generation,
            binding: app.binding(),
            label,
            request_id: Some(request_id.to_owned()),
            result: Ok(format!(
                "{{\"schema_version\":1,\"directory\":\"{path}\",\"entries\":[{{\"name\":\"child\",\"kind\":\"directory\"}}],\"truncated\":false}}"
            )
            .into_bytes()),
        }
    }

    #[test]
    fn three_browser_slots_accept_only_their_current_bound_request() {
        let mut app = App::with_state_dir(
            &egui::Context::default(),
            Err(BackendError::BinaryUnavailable {
                expected: PathBuf::from("/missing/codingmage"),
            }),
            Ok(PathBuf::from("/missing/private-state")),
        );
        let path = Path::new("/example");
        assert_eq!(
            directory_list_arguments(path).unwrap(),
            ["directory-list", "--directory", "/example"]
        );
        app.browser = Some(Browser::new(path, vec!["toml"]));
        app.setup.target_browser = Some(Browser::new(path, vec![]));
        app.campaign_browser = Some(Browser::new(path, vec!["toml"]));
        app.browser.as_mut().unwrap().begin_listing(1);
        app.setup.target_browser.as_mut().unwrap().begin_listing(2);
        app.campaign_browser.as_mut().unwrap().begin_listing(3);

        assert!(!app.handle_response(response(&app, "directory-list-target", "1", "/example")));
        assert!(app.setup.target_browser.as_ref().unwrap().loading);
        assert_eq!(app.discarded_stale(), 1);
        assert!(app.handle_response(response(&app, "directory-list-config", "1", "/example")));
        assert!(app.handle_response(response(&app, "directory-list-target", "2", "/example")));
        assert!(app.handle_response(response(&app, "directory-list-campaign", "3", "/example")));
        assert_eq!(app.browser.as_ref().unwrap().entries.len(), 1);
        assert_eq!(app.setup.target_browser.as_ref().unwrap().entries.len(), 1);
        assert_eq!(app.campaign_browser.as_ref().unwrap().entries.len(), 1);

        app.browser.as_mut().unwrap().begin_listing(4);
        assert!(!app.handle_response(response(&app, "directory-list-config", "1", "/example")));
        assert!(app.browser.as_ref().unwrap().entries.is_empty());
        assert!(app.browser.as_ref().unwrap().loading);
        app.advance_selection_generation();
        let mut late = response(&app, "directory-list-config", "4", "/example");
        late.generation = Generation(app.generation.0 - 1);
        assert!(!app.handle_response(late));
        assert!(app.browser.as_ref().unwrap().entries.is_empty());
    }

    #[test]
    fn unavailable_coordinator_refuses_a_browser_request_without_cached_rows() {
        let mut app = App::with_state_dir(
            &egui::Context::default(),
            Err(BackendError::BinaryUnavailable {
                expected: PathBuf::from("/missing/codingmage"),
            }),
            Ok(PathBuf::from("/missing/private-state")),
        );
        app.browser = Some(Browser::new(Path::new("/example"), vec!["toml"]));
        app.request_browser(BrowserSlot::Configuration);
        let browser = app.browser.as_ref().unwrap();
        assert!(!browser.loading);
        assert!(!browser.ready);
        assert!(browser.entries.is_empty());
        assert!(browser.error.is_some());
    }
}
