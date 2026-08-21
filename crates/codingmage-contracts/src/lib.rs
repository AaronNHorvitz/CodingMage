//! Stable wire contracts shared across `CodingMage` ownership boundaries.

mod campaign;
mod error;
mod identifier;

pub use campaign::{
    HumanDecisionBlocker, LeadBlockedDisposition, LeadBlockedReason, LeadDeferredDisposition,
    LeadDeferredReason, LeadDispositionKind, LeadHumanDecisionReason, LeadReconsiderationTrigger,
    LeadTaskBinding, PodRisk, TeamLeadProposal, TeamLeadReport,
};
pub use error::{ErrorCategory, ErrorCode, ErrorCodeError, ErrorMetadata, PublicError};
pub use identifier::{
    AgentId, AttemptId, EvidenceId, IdentifierError, RepositoryId, ReviewId, RunId, TaskId,
    WorktreeId,
};
