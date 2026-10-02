//! Native Linux desktop workspace over the existing `CodingMage` command boundary.
//!
//! The interface presents repository, work-plan and campaign state read through the installed
//! `codingmage` executable and the existing parser libraries. It requests explicit controls
//! through the same command boundary and never enforces policy or grants authority itself.

pub mod admission;
pub mod app;
pub mod backend;
pub mod browser;
pub mod campaign;
pub mod command;
pub mod content;
pub mod controls;
pub mod design;
pub mod fonts;
pub mod launch;
mod messages;
pub mod observed;
pub mod project;
pub mod readiness;
pub mod records;
pub mod report;
mod report_export;
pub mod setup;
mod setup_config_process;
mod setup_export_process;
pub mod state_dir;
pub mod workplan;

pub use app::{App, Screen};

/// Runs the private one-shot report writer process selected by the desktop executable.
///
/// This is a process isolation boundary for an already authorized local export. It is not
/// a public coordinator command or a source of campaign authority.
#[must_use]
pub fn run_report_export_helper() -> std::process::ExitCode {
    report_export::helper_main()
}

/// Runs the isolated one-shot Setup export supervisor selected by the desktop executable.
#[must_use]
pub fn run_setup_export_helper() -> std::process::ExitCode {
    setup_export_process::helper_main()
}

/// Runs the isolated guided-configuration supervisor selected by the desktop executable.
#[must_use]
pub fn run_setup_config_helper() -> std::process::ExitCode {
    setup_config_process::helper_main()
}
