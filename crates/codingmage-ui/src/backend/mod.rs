//! Bounded client of the existing `codingmage` command boundary.

pub mod cli;
pub mod models;
pub mod worker;

pub use cli::{BackendError, CoordinatorBinary, explain_code};
pub use worker::{Binding, Generation, Job, QueueError, Request, Response, Worker};
