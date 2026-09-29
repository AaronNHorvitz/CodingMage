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
        TaskDetailProjection,
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
    project::{OpenError, Project},
    state_dir::{ProjectMemory, RecentProjects, StateError, user_config_dir},
    workplan::{KindFilter, PlanFilter, PlanIndex, PlanRow, SourceReadiness, StateFilter},
};

/// Deadline for read-only diagnosis commands.
pub const DIAGNOSIS_DEADLINE: Duration = Duration::from_mins(1);
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

/// Interface state.
pub struct App {
    connection: Connection,
    binary_path: Option<PathBuf>,
    generation: Generation,
    project: Option<Project>,
    open_error: Option<OpenError>,
    diagnosis: Observed<Diagnosis>,
    screen: Screen,
    config_input: String,
    recent: RecentProjects,
    state_dir: Result<PathBuf, StateError>,
    status_line: Option<(Instant, String)>,
    discarded_stale: u64,
    started_at: Instant,
    now: Instant,
    plan_index: Option<PlanIndex>,
    plan_filter: PlanFilter,
    selected_item: Option<String>,
    browser: Option<Browser>,
    campaign: Option<CampaignSelection>,
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
    setup: SetupState,
    authorization_record: Option<PathBuf>,
    authorization_input: String,
    preflight: Observed<crate::readiness::PreflightObservation>,
    execution: ExecutionState,
    changes: Observed<ChangeSet>,
    changes_range: Option<(String, String)>,
    records: Observed<Vec<crate::records::RunRecord>>,
    records_status: Option<(String, u64)>,
    records_truncated: bool,
    reports: ReportsState,
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
        let now = Instant::now();
        Self {
            connection,
            binary_path,
            generation: Generation(1),
            project: None,
            open_error: None,
            diagnosis: Observed::default(),
            screen: Screen::Overview,
            config_input: String::new(),
            recent,
            state_dir,
            status_line: None,
            discarded_stale: 0,
            started_at: now,
            now,
            plan_index: None,
            plan_filter: PlanFilter::default(),
            selected_item: None,
            browser: None,
            campaign: None,
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
            setup: SetupState::default(),
            authorization_record: None,
            authorization_input: String::new(),
            preflight: Observed::default(),
            execution: ExecutionState::default(),
            changes: Observed::default(),
            changes_range: None,
            records: Observed::default(),
            records_status: None,
            records_truncated: false,
            reports: ReportsState::default(),
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

    /// Opens a configuration path; starts no process and changes no file except the recent list.
    pub fn open_project(&mut self, config_path: &Path) {
        self.generation = Generation(self.generation.0 + 1);
        if let Connection::Ready(worker) = &self.connection {
            worker.advance(self.generation);
        }
        self.diagnosis.clear();
        self.project = None;
        self.open_error = None;
        self.plan_index = None;
        self.selected_item = None;
        self.browser = None;
        self.clear_campaign_observations();
        self.campaign = None;
        self.campaign_error = None;
        self.campaign_input.clear();
        self.authorization_record = None;
        self.authorization_input.clear();
        match Project::open(config_path) {
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
                        self.select_campaign(&spec_path);
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
        self.generation = Generation(self.generation.0 + 1);
        if let Connection::Ready(worker) = &self.connection {
            worker.advance(self.generation);
        }
        self.project = None;
        self.diagnosis.clear();
        self.open_error = None;
        self.plan_index = None;
        self.selected_item = None;
        self.clear_campaign_observations();
        self.campaign = None;
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

    pub(super) fn advance_selection_generation(&mut self) {
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
            self.discarded_stale += 1;
            return false;
        }
        match response.label {
            "doctor" => {
                match response
                    .result
                    .and_then(|bytes| parse_diagnosis(&bytes).map_err(BackendError::from))
                {
                    Ok(diagnosis) => {
                        self.diagnosis
                            .accept(diagnosis, response.generation, self.now);
                        self.set_status("repository diagnosis observed");
                    }
                    Err(error) => {
                        self.set_status(format!("repository diagnosis failed: {}", error.code()));
                        self.diagnosis.fail(error, self.now);
                    }
                }
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
            "campaign-changes" => {
                self.accept_changes(response);
                true
            }
            "campaign-run-records" => {
                self.accept_records(response);
                true
            }
            "campaign-explain-blocker" => {
                self.accept_explanation(response);
                true
            }
            "campaign-mission-status" => {
                self.accept_mission(response);
                true
            }
            "campaign-report" => {
                match response.result.and_then(|bytes| {
                    crate::backend::models::parse_campaign_report(&bytes)
                        .map_err(BackendError::from)
                }) {
                    Ok(report) => self.report.accept(report, response.generation, self.now),
                    Err(error) => self.report.fail(error, self.now),
                }
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
            _ => false,
        }
    }

    fn binding_matches(&self, issued: &Binding, label: &str) -> bool {
        let current = self.binding();
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

    /// Drains worker responses.
    pub fn poll(&mut self) {
        self.now = Instant::now();
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
                        ui.label("Coordinator: ready");
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
                    let (what, action) = explain_code(&error.code());
                    failure_box(ui, what, &error.to_string(), action);
                }
            }
            Freshness::Live | Freshness::Stale => {
                if let Some((_, error)) = &self.diagnosis.last_error {
                    let (what, action) = explain_code(&error.code());
                    failure_box(
                        ui,
                        &format!("Last refresh failed; showing the earlier observation. {what}"),
                        &error.to_string(),
                        action,
                    );
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
            Err(error) => failure_box(
                ui,
                "Task source unavailable",
                &error.to_string(),
                "Fix the task source in the repository; the work plan stays empty until it parses.",
            ),
        }
    }

    fn browser_panel(&mut self, ui: &mut egui::Ui) {
        let Some(browser) = &mut self.browser else {
            return;
        };
        let mut open: Option<PathBuf> = None;
        let mut enter: Option<PathBuf> = None;
        let mut up = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Up").clicked() {
                    up = true;
                }
                ui.monospace(browser.current.display().to_string());
            });
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
                            format!("{}/", entry.name)
                        } else {
                            entry.name.clone()
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
        if up {
            browser.up();
        }
        if let Some(path) = enter {
            browser.enter(&path);
        }
        if let Some(path) = open {
            self.open_project(&path);
        }
    }

    fn work_plan(&mut self, ui: &mut egui::Ui) {
        ui.heading("Work plan");
        let Some(project) = &self.project else {
            ui.label("Open a repository to see its task plan.");
            return;
        };
        let Some(index) = &self.plan_index else {
            if let Err(error) = &project.plan {
                failure_box(
                    ui,
                    "Task source unavailable",
                    &error.to_string(),
                    "Fix the task source in the repository; the plan is shown only when it parses.",
                );
            }
            return;
        };
        ui.small(format!(
            "Read from {} - source checkboxes only; nothing here edits the file.",
            project.task_source_path().display()
        ));
        let mut filter = self.plan_filter.clone();
        plan_filter_controls(ui, &mut filter);
        let rows = index.filtered(&filter);
        ui.label(format!(
            "{} of {} items shown",
            rows.len(),
            index.counts().items
        ));
        let overlay = self.task_overlay();
        let observation_known = self.status.value.is_some();
        if self.campaign.is_some() {
            ui.small(format!(
                "Coordinator overlay: status {} ({}), campaign-head source {}",
                self.status.freshness(self.now).label(),
                age_label(self.status.age(self.now)),
                self.head_plan.freshness(self.now).label()
            ));
        }
        let mut selected = self.selected_item.clone();
        let mut last_sprint: Option<&str> = None;
        let mut last_story: Option<&str> = None;
        egui::ScrollArea::vertical()
            .id_salt("plan-rows")
            .max_height(current_tokens(ui.ctx()).layout.preview_tall)
            .show(ui, |ui| {
                for row in &rows {
                    plan_group_headers(ui, index, row, &mut last_sprint, &mut last_story);
                    let is_selected = selected.as_deref() == Some(row.id.as_str());
                    let label = overlay_label(row, overlay.get(&row.id), observation_known);
                    ui.horizontal(|ui| {
                        let mut checked = row.state == CheckState::Checked;
                        ui.add_enabled(false, egui::Checkbox::without_text(&mut checked))
                            .on_disabled_hover_text(
                                "Source checkbox, read from the task source; the interface never edits it.",
                            );
                        let response = ui.selectable_label(is_selected, label);
                        if response.clicked() {
                            selected = Some(row.id.clone());
                        }
                    });
                }
            });
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
            item_detail(ui, &row, index);
            self.task_source_detail(ui, &row);
        }
    }

    fn task_source_detail(&mut self, ui: &mut egui::Ui, row: &PlanRow) {
        let arguments = self.task_detail_arguments(&row.id);
        let can_load = command::can_preview(self.binary_path.as_deref(), arguments.as_deref());
        if ui
            .add_enabled(
                can_load && !self.task_detail.loading,
                egui::Button::new("Load source detail"),
            )
            .clicked()
        {
            self.request_task_detail(&row.id);
        }
        if let Some(arguments) = &arguments {
            command::show_for(
                ui,
                "Load source detail",
                self.binary_path.as_deref(),
                arguments,
            );
        } else {
            command::show_unavailable_for(ui, "Load source detail");
            ui.small("Select a campaign and refresh its live status to inspect source at its reconciled head.");
        }
        if self.task_detail.loading {
            ui.label("Loading source detail from the coordinator…");
        }
        if let Some((_, error)) = &self.task_detail.last_error {
            ui.label(format!(
                "Source detail unavailable: {}. Refresh the campaign and try again.",
                error.code()
            ));
        }
        let Some(detail) = self.task_detail.value.as_ref().filter(|detail| {
            self.task_detail_key.as_ref() == Some(&(detail.head.clone(), detail.item_id.clone()))
                && detail.item_id == row.id
                && self.status.freshness(self.now) == Freshness::Live
                && self.task_detail.freshness(self.now) == Freshness::Live
        }) else {
            return;
        };
        ui.small(format!(
            "Source at campaign head {}…; checkbox is {:?}, not a verified outcome.",
            &detail.head[..12],
            detail.source_state
        ));
        ui.collapsing("Source excerpt", |ui| {
            let characters = detail.excerpt.chars().collect::<Vec<_>>();
            for chunk in characters.chunks(content::MAX_PREVIEW_CHARS) {
                content::render(ui, &chunk.iter().collect::<String>());
            }
            if detail.truncated {
                ui.small("Source excerpt ends at the 16 KiB display boundary.");
            }
        });
        ui.collapsing("Story acceptance criteria from source", |ui| {
            if detail.story_criteria.is_empty() {
                ui.label("No story criteria were parsed at this source head.");
            }
            for criterion in &detail.story_criteria {
                ui.horizontal_wrapped(|ui| {
                    content::render(ui, &criterion.id);
                    ui.label(format!("{:?} in source", criterion.source_state));
                    content::render(ui, &criterion.title);
                    if criterion.title_truncated {
                        ui.small("Criterion title shortened at 4 KiB.");
                    }
                });
            }
            if detail.story_criteria_truncated {
                ui.small("Additional story criteria were omitted after the first 100.");
            }
        });
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
            if ui.button("Open").clicked() || submitted {
                let path = PathBuf::from(self.config_input.trim());
                self.open_project(&path);
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
            }
        });
        self.browser_panel(ui);
        if let Some(error) = &self.open_error {
            failure_box(
                ui,
                "Configuration could not be opened",
                &error.to_string(),
                "Roots must be absolute existing directories, the task source relative, and policies must agree.",
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

fn plan_filter_controls(ui: &mut egui::Ui, filter: &mut PlanFilter) {
    ui.horizontal_wrapped(|ui| {
        let label = ui.label("Search");
        ui.add(
            egui::TextEdit::singleline(&mut filter.query)
                .hint_text("identifier or title")
                .desired_width(current_tokens(ui.ctx()).layout.field_medium),
        )
        .labelled_by(label.id);
        for state in StateFilter::ALL {
            ui.selectable_value(&mut filter.state, state, state.label());
        }
        ui.separator();
        for kind in KindFilter::ALL {
            ui.selectable_value(&mut filter.kind, kind, kind.label());
        }
        ui.separator();
        ui.checkbox(&mut filter.ready_only, "Dependency-ready only");
    });
}

fn plan_group_headers<'a>(
    ui: &mut egui::Ui,
    index: &PlanIndex,
    row: &'a PlanRow,
    last_sprint: &mut Option<&'a str>,
    last_story: &mut Option<&'a str>,
) {
    if *last_sprint != Some(row.sprint_id.as_str()) {
        *last_sprint = Some(row.sprint_id.as_str());
        *last_story = None;
        ui.strong(format!(
            "Sprint {} - {}",
            row.sprint_id,
            content::list_label(index.sprint_title(&row.sprint_id).unwrap_or(""))
        ));
    }
    if row.story_id.as_deref() != *last_story {
        *last_story = row.story_id.as_deref();
        if let Some(story) = *last_story {
            ui.label(format!(
                "Story {} - {}",
                story,
                content::list_label(index.story_title(story).unwrap_or(""))
            ));
        }
    }
}

fn overlay_label(
    row: &PlanRow,
    overlay: Option<&crate::campaign::TaskOverlay>,
    observation_known: bool,
) -> String {
    let mut label = row_label(row);
    if let Some(task) = overlay {
        let states = task
            .labels(observation_known)
            .into_iter()
            .filter(|state| !state.ends_with("in source"))
            .collect::<Vec<_>>();
        if !states.is_empty() {
            label.push_str(" [");
            label.push_str(&states.join("; "));
            label.push(']');
        }
    }
    label
}

fn row_label(row: &PlanRow) -> String {
    let checkbox = match row.state {
        CheckState::Open => "[ ]",
        CheckState::Checked => "[x]",
    };
    let kind = match row.kind {
        PlanItemKind::Task => "Task",
        PlanItemKind::SubTask => "Sub-task",
        PlanItemKind::AcceptanceCriterion => "AC",
        PlanItemKind::Gate => "Gate",
    };
    let readiness = match row.readiness {
        SourceReadiness::NotApplicable => String::new(),
        other => format!(" - {}", other.label()),
    };
    format!(
        "{checkbox} {kind} {} {}{readiness}",
        row.id,
        content::list_label(&row.title)
    )
}

fn item_detail(ui: &mut egui::Ui, row: &PlanRow, index: &PlanIndex) {
    ui.heading(format!("Item {}", row.id));
    egui::Grid::new("item-detail")
        .num_columns(2)
        .spacing(current_tokens(ui.ctx()).layout.grid)
        .show(ui, |ui| {
            ui.label("Title");
            content::render(ui, &row.title);
            ui.end_row();
            ui.label("Source checkbox");
            ui.label(match row.state {
                CheckState::Open => "open",
                CheckState::Checked => {
                    "checked (source claim; verified completion is shown on the Campaign screen)"
                }
            });
            ui.end_row();
            ui.label("Source location");
            ui.monospace(format!(
                "line {} (sha256 {}...)",
                row.line,
                &row.line_sha256[..12]
            ));
            ui.end_row();
            ui.label("Parent");
            ui.monospace(&row.parent_id);
            ui.end_row();
            ui.label("Readiness from source");
            ui.label(if row.readiness == SourceReadiness::NotApplicable {
                "not computed for this kind"
            } else {
                row.readiness.label()
            });
            ui.end_row();
        });
    if row.dependencies.is_empty() {
        ui.label("Dependencies: none declared");
    } else {
        ui.label("Dependencies");
        for dependency in &row.dependencies {
            let state = match dependency.state {
                Some(CheckState::Checked) => "checked",
                Some(CheckState::Open) => "open",
                None => "unknown identifier",
            };
            ui.monospace(format!("  {} - {state}", dependency.id));
        }
    }
    let dependents = index.dependents(&row.id);
    if !dependents.is_empty() {
        ui.label("Depended on by");
        for dependent in dependents {
            ui.monospace(format!("  {}", dependent.id));
        }
    }
    if ui.button("Copy identifier").clicked() {
        ui.ctx().copy_text(row.id.clone());
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
