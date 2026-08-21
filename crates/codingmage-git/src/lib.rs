//! Hostile-safe Git inventory and exact owned-worktree lifecycle operations.

mod command;
mod commit;
mod integration;
mod inventory;
mod policy;
mod review;
mod worktree;

#[cfg(test)]
mod test_support;

pub use commit::{
    CommitError, CommitReceipt, commit_owned_changes, observe_owned_child_commit,
    reobserve_owned_commit,
};
pub use integration::{
    IntegrationError, IntegrationReceipt, IntegrationTransferReceipt, PreparedIntegration,
    PreparedIntegrationReceipt, install_prepared_integration, integrate_reviewed_delta,
    integrate_reviewed_descendant, prepare_reviewed_delta, release_prepared_integration,
};
pub use inventory::{
    Inventory, InventoryError, OperationState, RepositoryCondition, inventory_repository,
};
pub use policy::{GitPolicyError, validate_requested_command};
pub use review::{ReadOnlyScope, ReviewLocation, ReviewScope, ReviewScopeError};
pub use worktree::{
    OwnedWorktree, WorktreeError, WorktreeManifest, WorktreeStatus, create_owned_worktree,
    remove_owned_worktree,
};
