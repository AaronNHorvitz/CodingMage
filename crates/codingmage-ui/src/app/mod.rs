//! Application shell: navigation, project selection and bounded backend observation.

mod campaign_screen;
mod changes_screen;
mod execution_screen;
mod readiness_screen;
mod setup_screen;

pub use changes_screen::ChangeSet;
pub use execution_screen::{ExecutionState, LAUNCH_OBSERVE_INTERVAL};
pub use setup_screen::SetupState;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use codingmage_plan::{CheckState, PlanItemKind};

use crate::{
    backend::models::{BlockerExplanation, CampaignReport, CampaignStatus},
    backend::{
        BackendError, Binding, CoordinatorBinary, Generation, Job, QueueError, Request, Response,
        Worker, explain_code,
        models::{Diagnosis, parse_diagnosis},
    },
    browser::Browser,
    campaign::{CampaignSelection, SelectError},
    observed::{Freshness, Observed, age_label},
    project::{LoadedPlan, OpenError, Project},
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
pub const MIN_WINDOW: [f32; 2] = [720.0, 480.0];

/// Navigation destinations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    /// Repository identity, readiness and counts.
    Overview,
    /// Searchable task plan.
    WorkPlan,
    /// Campaign and team activity.
    Campaign,
    /// Exact changes, review and test results.
    Changes,
    /// Outcome and blocker reports.
    Reports,
    /// Guided configuration and readiness.
    Setup,
}

impl Screen {
    /// All screens in navigation order.
    pub const ALL: [Self; 6] = [
        Self::Overview,
        Self::WorkPlan,
        Self::Campaign,
        Self::Changes,
        Self::Reports,
        Self::Setup,
    ];

    /// Navigation label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::WorkPlan => "Work plan",
            Self::Campaign => "Campaign",
            Self::Changes => "Changes and reviews",
            Self::Reports => "Reports",
            Self::Setup => "Setup",
        }
    }

    /// Keyboard shortcut digit (Ctrl+digit).
    #[must_use]
    pub const fn shortcut(self) -> egui::Key {
        match self {
            Self::Overview => egui::Key::Num1,
            Self::WorkPlan => egui::Key::Num2,
            Self::Campaign => egui::Key::Num3,
            Self::Changes => egui::Key::Num4,
            Self::Reports => egui::Key::Num5,
            Self::Setup => egui::Key::Num6,
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
    status: Observed<Option<CampaignStatus>>,
    explanation: Observed<BlockerExplanation>,
    report: Observed<Option<CampaignReport>>,
    head_plan: Observed<Option<LoadedPlan>>,
    head_plan_commit: Option<String>,
    last_status_request: Option<Instant>,
    setup: SetupState,
    authorization_record: Option<PathBuf>,
    authorization_input: String,
    preflight: Observed<crate::readiness::PreflightObservation>,
    execution: ExecutionState,
    changes: Observed<ChangeSet>,
    changes_range: Option<(String, String)>,
    pending_changes: Option<ChangeSet>,
    pending_changes_parts: u8,
    records: Observed<Vec<crate::records::RunRecord>>,
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
            status: Observed::default(),
            explanation: Observed::default(),
            report: Observed::default(),
            head_plan: Observed::default(),
            head_plan_commit: None,
            last_status_request: None,
            setup: SetupState::default(),
            authorization_record: None,
            authorization_input: String::new(),
            preflight: Observed::default(),
            execution: ExecutionState::default(),
            changes: Observed::default(),
            changes_range: None,
            pending_changes: None,
            pending_changes_parts: 0,
            records: Observed::default(),
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

    /// Latest task source observed at the campaign head.
    #[must_use]
    pub const fn head_plan(&self) -> &Observed<Option<LoadedPlan>> {
        &self.head_plan
    }

    /// Latest final report observation.
    #[must_use]
    pub const fn report(&self) -> &Observed<Option<CampaignReport>> {
        &self.report
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
        let Some(project) = &self.project else {
            return;
        };
        let arguments = vec![
            "doctor".to_owned(),
            "--config".to_owned(),
            project.config_path.display().to_string(),
        ];
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
            "git-log" | "git-numstat" => {
                self.accept_changes_part(response);
                true
            }
            "records" => {
                self.accept_records(response);
                true
            }
            "campaign-explain-blocker" => {
                match response.result.and_then(|bytes| {
                    crate::backend::models::parse_blocker_explanation(&bytes)
                        .map_err(BackendError::from)
                }) {
                    Ok(explanation) => {
                        self.explanation
                            .accept(explanation, response.generation, self.now);
                    }
                    Err(error) => self.explanation.fail(error, self.now),
                }
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
            "git-head-plan" => {
                match response.result {
                    Ok(bytes) => match crate::project::parse_plan_bytes(&bytes) {
                        Ok(plan) => {
                            self.head_plan
                                .accept(Some(plan), response.generation, self.now);
                        }
                        Err(_) => self.head_plan.fail(
                            BackendError::Refused(
                                "the task source at the campaign head does not parse".to_owned(),
                            ),
                            self.now,
                        ),
                    },
                    Err(error) => self.head_plan.fail(error, self.now),
                }
                true
            }
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
                    Screen::Changes => self.changes_screen(ui),
                    Screen::Setup => self.setup(ui),
                    Screen::Reports => self.placeholder(ui, Screen::Reports),
                });
        });
        if self.diagnosis.loading
            || self.status.loading
            || self.head_plan.loading
            || self.preflight.loading
            || self.changes.loading
            || self.records.loading
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
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("Refresh (F5)"))
                    .clicked()
                {
                    self.refresh_diagnosis();
                }
            });
        });
    }

    fn navigation(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .resizable(false)
            .default_size(170.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                for screen in Screen::ALL {
                    if ui
                        .selectable_label(self.screen == screen, screen.label())
                        .clicked()
                    {
                        self.screen = screen;
                    }
                }
                ui.add_space(12.0);
                ui.separator();
                ui.small("Ctrl+1 to Ctrl+6 switch screens");
                ui.small(format!("Uptime {}s", self.started_at.elapsed().as_secs()));
            });
    }

    fn status_bar(&self, root: &mut egui::Ui) {
        egui::Panel::bottom("status-bar").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                match &self.connection {
                    Connection::Ready(_) => {
                        ui.label(format!(
                            "Coordinator: {}",
                            self.binary_path
                                .as_ref()
                                .map_or_else(String::new, |path| path.display().to_string())
                        ));
                    }
                    Connection::Unavailable(error) => {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("Coordinator unavailable: {error}"),
                        );
                    }
                }
                if let Some((_, message)) = &self.status_line {
                    ui.separator();
                    ui.label(message);
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
            ui.label(
                "Open a repository configuration to see its identity, readiness and work plan.",
            );
            self.open_controls(ui);
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        egui::Grid::new("overview-grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
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
                        egui::Color32::from_rgb(180, 120, 0),
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
                ui.colored_label(egui::Color32::RED, error);
            }
            if browser.truncated {
                ui.small("Listing truncated; navigate into a narrower directory.");
            }
            egui::ScrollArea::vertical()
                .max_height(240.0)
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
            .max_height(360.0)
            .show(ui, |ui| {
                for row in &rows {
                    if last_sprint != Some(row.sprint_id.as_str()) {
                        last_sprint = Some(row.sprint_id.as_str());
                        last_story = None;
                        ui.strong(format!(
                            "Sprint {} - {}",
                            row.sprint_id,
                            index.sprint_title(&row.sprint_id).unwrap_or("")
                        ));
                    }
                    if row.story_id.as_deref() != last_story {
                        last_story = row.story_id.as_deref();
                        if let Some(story) = last_story {
                            ui.label(format!(
                                "Story {} - {}",
                                story,
                                index.story_title(story).unwrap_or("")
                            ));
                        }
                    }
                    let is_selected = selected.as_deref() == Some(row.id.as_str());
                    let mut label = row_label(row);
                    if let Some(task) = overlay.get(&row.id) {
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
                    let response = ui.selectable_label(is_selected, label);
                    if response.clicked() {
                        selected = Some(row.id.clone());
                    }
                }
            });
        self.plan_filter = filter;
        self.selected_item = selected;
        if let Some(row) = self
            .selected_item
            .as_ref()
            .and_then(|id| index.rows().iter().find(|row| &row.id == id))
        {
            ui.separator();
            item_detail(ui, row, index);
        }
    }

    fn placeholder(&self, ui: &mut egui::Ui, screen: Screen) {
        ui.heading(screen.label());
        if self.project.is_none() {
            ui.label("Open a repository first.");
        } else {
            ui.label("This screen is not implemented in this build.");
        }
    }

    fn open_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Configuration file");
            let edit = egui::TextEdit::singleline(&mut self.config_input)
                .hint_text("/absolute/path/codingmage.toml")
                .desired_width(420.0);
            let response = ui.add(edit);
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
        .spacing([12.0, 6.0])
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
        ui.label("Search");
        ui.add(
            egui::TextEdit::singleline(&mut filter.query)
                .hint_text("identifier or title")
                .desired_width(240.0),
        );
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
    format!("{checkbox} {kind} {} {}{readiness}", row.id, row.title)
}

fn item_detail(ui: &mut egui::Ui, row: &PlanRow, index: &PlanIndex) {
    ui.heading(format!("Item {}", row.id));
    egui::Grid::new("item-detail")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Title");
            ui.label(&row.title);
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
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(60, 30, 30))
        .show(ui, |ui| {
            ui.colored_label(egui::Color32::from_rgb(255, 180, 180), title);
            ui.monospace(detail);
            ui.label(action);
        });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}
