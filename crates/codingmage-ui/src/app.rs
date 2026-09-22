//! Application shell: navigation, project selection and bounded backend observation.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    backend::{
        BackendError, Binding, CoordinatorBinary, Generation, Job, QueueError, Request, Response,
        Worker, explain_code,
        models::{Diagnosis, parse_diagnosis},
    },
    observed::{Freshness, Observed, age_label},
    project::{OpenError, Project},
    state_dir::{RecentProjects, StateError, user_config_dir},
};

/// Deadline for read-only diagnosis commands.
pub const DIAGNOSIS_DEADLINE: Duration = Duration::from_mins(1);
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
        let state_dir = user_config_dir();
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
        }
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
            campaign_id: None,
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
        match Project::open(config_path) {
            Ok(project) => {
                if let Ok(directory) = &self.state_dir {
                    let _ = self.recent.remember(directory, &project.config_path);
                }
                self.config_input = project.config_path.display().to_string();
                self.project = Some(project);
                self.set_status("opened configuration; requesting repository diagnosis");
                self.refresh_diagnosis();
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
        if response.generation != self.generation || !self.binding_matches(&response.binding) {
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
            _ => false,
        }
    }

    fn binding_matches(&self, issued: &Binding) -> bool {
        let current = self.binding();
        issued.config_path == current.config_path
            && (issued.repository_id.is_none()
                || issued.repository_id == current.repository_id
                || current.repository_id.is_none())
            && issued.campaign_id == current.campaign_id
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
                    Screen::Setup => self.setup(ui),
                    other => self.placeholder(ui, other),
                });
        });
        if self.diagnosis.loading {
            ctx.request_repaint_after(Duration::from_millis(250));
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
        ui.separator();
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
            }
            Err(error) => failure_box(
                ui,
                "Task source unavailable",
                &error.to_string(),
                "Fix the task source in the repository; the work plan stays empty until it parses.",
            ),
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

    fn setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Setup");
        ui.label("Open an existing configuration. Guided configuration is added in a later build.");
        self.open_controls(ui);
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
        });
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
