//! Stable wire contracts shared across `CodingMage` ownership boundaries.

mod campaign;
mod error;
mod host;
mod identifier;

pub use campaign::{
    HumanDecisionBlocker, LeadBlockedDisposition, LeadBlockedReason, LeadDeferredDisposition,
    LeadDeferredReason, LeadDispositionKind, LeadHumanDecisionReason, LeadReconsiderationTrigger,
    LeadTaskBinding, PodRisk, TeamLeadProposal, TeamLeadReport,
};
pub use error::{ErrorCategory, ErrorCode, ErrorCodeError, ErrorMetadata, PublicError};
pub use host::{
    HOST_PROTOCOL_VERSION, HostBlocker, HostCapability, HostContractError, HostControl,
    HostControlOperation, HostOperation, HostRequest, HostRunState, HostStatus,
    MAX_HOST_OPERATIONS,
};
pub use identifier::{
    AgentId, AttemptId, ClientId, EvidenceId, IdentifierError, RepositoryId, RequestId, ReviewId,
    RunId, TaskId, WorktreeId,
};
