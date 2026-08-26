//! Bounded process execution through a parent-watching Linux guard.

mod runtime;

#[doc(hidden)]
pub use runtime::guard_entry;
pub use runtime::{
    CancellationToken, CapturedStream, DescendantCleanup, ExecutableIdentity, ProcessError,
    ProcessExecutor, ProcessOutcome, ProcessProfile, ProcessRequest, ProcessResult,
    observe_control_residue, recover_orphaned_controls,
};
