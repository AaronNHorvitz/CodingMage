//! Durable, content-minimized state storage and recovery decisions.

mod journal;
mod recovery;
mod snapshot;

pub use journal::{
    CampaignCheckpointProjection, DurableIdentities, EffectClass, EventKind, EventOutcome, Journal,
    JournalError, JournalEvent, JournalLock, JournalRecord, MAX_RECORD_BYTES, RedactedField,
};
pub use recovery::{
    IdentitySet, LiveObservation, RecoveryDecision, RecoveryReason, reconcile_after_restart,
};
pub use snapshot::{Snapshot, SnapshotEnvelope, SnapshotError, TaskProjection};
