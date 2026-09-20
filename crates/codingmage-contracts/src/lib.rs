//! Stable wire contracts shared across `CodingMage` ownership boundaries.

mod campaign;
mod context;
mod error;
mod host;
mod identifier;
mod transport;

pub use campaign::{
    HumanDecisionBlocker, LeadBlockedDisposition, LeadBlockedReason, LeadDeferredDisposition,
    LeadDeferredReason, LeadDispositionKind, LeadHumanDecisionReason, LeadReconsiderationTrigger,
    LeadTaskBinding, PodRisk, TeamLeadProposal, TeamLeadReport,
};
pub use context::{
    ContextCaller, ContextContentPolicy, ContextDirection, ContextError, ContextGrant,
    ContextLimits, ContextProvenance, ContextRecord, MAX_CONTEXT_ENTRY_BYTES,
    MAX_CONTEXT_KEY_CHARS, MAX_CONTEXT_READ_ENTRIES, MAX_CONTEXT_SOURCE_CHARS,
    MAX_CONTEXT_TIMEOUT_MS, MIN_CONTEXT_ENTRY_BYTES, MIN_CONTEXT_TIMEOUT_MS,
};
pub use error::{ErrorCategory, ErrorCode, ErrorCodeError, ErrorMetadata, PublicError};
pub use host::{
    HOST_PROTOCOL_VERSION, HostBlocker, HostCapability, HostContractError, HostControl,
    HostControlOperation, HostOperation, HostRequest, HostRunState, HostStatus,
    MAX_HOST_OPERATIONS,
};
pub use identifier::{
    AgentId, AttemptId, ClientId, ContextNamespace, ContextOperationId, EvidenceId,
    IdentifierError, RepositoryId, RequestId, ReviewId, RunId, TaskId, WorktreeId,
};
pub use transport::{
    DEFAULT_MAX_FRAME_BYTES, DEFAULT_MAX_REQUESTS_PER_CONNECTION, DEFAULT_READ_TIMEOUT_MS,
    DEFAULT_WRITE_TIMEOUT_MS, FRAME_HEADER_LEN, HostTransportError, MAX_MAX_FRAME_BYTES,
    MAX_REQUESTS_PER_CONNECTION, MAX_TIMEOUT_MS, MIN_MAX_FRAME_BYTES, MIN_TIMEOUT_MS,
    TRANSPORT_KIND, TransportLimits, decode_frame, encode_frame, validate_peer_directory,
};
