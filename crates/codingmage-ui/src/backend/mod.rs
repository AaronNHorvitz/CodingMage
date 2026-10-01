//! Bounded client of the existing `codingmage` command boundary.

pub mod cli;
mod export_process;
pub mod models;
pub mod worker;

pub use cli::{BackendError, CoordinatorBinary, FailureState, explain_code};
pub use worker::{Binding, Generation, Job, PrivateInput, QueueError, Request, Response, Worker};
