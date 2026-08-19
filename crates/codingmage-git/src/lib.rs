//! Hostile-safe Git inventory and exact owned-worktree lifecycle operations.

mod command;
mod inventory;
mod policy;
mod worktree;

#[cfg(test)]
mod test_support;

pub use inventory::{
    Inventory, InventoryError, OperationState, RepositoryCondition, inventory_repository,
};
pub use policy::{GitPolicyError, validate_requested_command};
pub use worktree::{
    OwnedWorktree, WorktreeError, WorktreeManifest, WorktreeStatus, create_owned_worktree,
    remove_owned_worktree,
};
