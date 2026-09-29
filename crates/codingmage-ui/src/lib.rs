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
pub mod setup;
pub mod state_dir;
pub mod workplan;

pub use app::{App, Screen};
