//! Hostile-safe Git inventory and exact owned-worktree lifecycle operations.

mod command;
mod commit;
mod inventory;
mod policy;
mod review;
mod worktree;

#[cfg(test)]
mod test_support;

pub use commit::{CommitError, CommitReceipt, commit_owned_changes};
pub use inventory::{
    Inventory, InventoryError, OperationState, RepositoryCondition, inventory_repository,
};
pub use policy::{GitPolicyError, validate_requested_command};
pub use review::{ReviewLocation, ReviewScope, ReviewScopeError};
pub use worktree::{
    OwnedWorktree, WorktreeError, WorktreeManifest, WorktreeStatus, create_owned_worktree,
    remove_owned_worktree,
};
