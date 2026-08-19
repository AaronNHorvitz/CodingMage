//! Stable wire contracts shared across `CodingMage` ownership boundaries.

mod error;
mod identifier;

pub use error::{ErrorCategory, ErrorCode, ErrorCodeError, ErrorMetadata, PublicError};
pub use identifier::{
    AgentId, AttemptId, EvidenceId, IdentifierError, RepositoryId, ReviewId, RunId, TaskId,
    WorktreeId,
};
