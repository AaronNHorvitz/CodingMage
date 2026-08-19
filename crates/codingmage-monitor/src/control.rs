use std::{collections::BTreeMap, fmt};

use codingmage_contracts::{RepositoryId, RunId, TaskId};
use codingmage_state::{EventKind, EventOutcome, Journal, JournalEvent, JournalRecord};
use serde::{Deserialize, Serialize};

use crate::{StatusLabel, StatusView};

/// Read-only operator commands with no lifecycle authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadCommand {
    /// Return current status.
    Status,
    /// Return status containing the current stable blocker.
    ExplainBlocker,
    /// Return the configured diff artifact reference.
    OpenDiff,
    /// Return the configured log artifact reference.
    OpenLog,
    /// Return local health state.
    Doctor,
}

/// Same-user read request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadRequest {
    /// Effective local user supplied by an authenticated transport.
    pub requester_uid: u32,
    /// Exact run being observed.
    pub run_id: RunId,
    /// Read-only command.
    pub command: ReadCommand,
}

/// Content-minimized read response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadResponse {
    /// Requested command.
    pub command: ReadCommand,
    /// Current status for status and blocker views.
    pub status: Option<StatusView>,
    /// Stable artifact reference for diff or log views.
    pub artifact: Option<StatusLabel>,
    /// Local health for doctor.
    pub healthy: Option<bool>,
}

/// State-changing lifecycle controls.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    /// Pause after the current safe boundary.
    Pause,
    /// Resume a paused run.
    Resume,
    /// Finish the current unit and stop before claiming another.
    StopAfterUnit,
    /// Cancel the exact active run while preserving durable recovery state.
    Cancel,
}

impl ControlAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::StopAfterUnit => "stop_after_unit",
            Self::Cancel => "cancel",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            "stop_after_unit" => Some(Self::StopAfterUnit),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// One exact idempotent control request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    /// Canonical caller-generated idempotency identity.
    pub request_id: StatusLabel,
    /// Effective local user supplied by an authenticated transport.
    pub requester_uid: u32,
    /// Exact active run.
    pub run_id: RunId,
    /// Requested lifecycle action.
    pub action: ControlAction,
}

/// Current lifecycle-control state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlState {
    /// Run is paused.
    pub paused: bool,
    /// Coordinator must stop before claiming another unit.
    pub stop_after_unit: bool,
    /// Exact run is cancelled.
    pub cancelled: bool,
}

/// Idempotent result of one accepted request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlOutcome {
    /// True only when this request changed state on first application.
    pub changed: bool,
    /// Resulting control state.
    pub state: ControlState,
}

#[derive(Clone, Copy, Debug)]
struct MutationGrant;

/// Exact-run control authority reconstructed from durable events.
#[derive(Debug)]
pub struct ControlEngine {
    owner_uid: u32,
    repository_id: RepositoryId,
    run_id: RunId,
    task_id: TaskId,
    state: ControlState,
    applied: BTreeMap<StatusLabel, (ControlAction, ControlOutcome)>,
}

impl ControlEngine {
    /// Creates an engine for one exact local user and active run.
    #[must_use]
    pub fn new(
        owner_uid: u32,
        repository_id: RepositoryId,
        run_id: RunId,
        task_id: TaskId,
    ) -> Self {
        Self {
            owner_uid,
            repository_id,
            run_id,
            task_id,
            state: ControlState::default(),
            applied: BTreeMap::new(),
        }
    }

    /// Reconstructs controls from accepted journal records for the exact target and run.
    ///
    /// # Errors
    ///
    /// Returns [`ControlError`] for malformed action/request labels or contradictory duplicates.
    pub fn recover(
        owner_uid: u32,
        repository_id: RepositoryId,
        run_id: RunId,
        task_id: TaskId,
        records: &[JournalRecord],
    ) -> Result<Self, ControlError> {
        let mut engine = Self::new(owner_uid, repository_id, run_id, task_id);
        for record in records {
            if record.event.repository_id != engine.repository_id
                || record.event.run_id != engine.run_id
                || record.event.task_id != engine.task_id
                || record.event.outcome != EventOutcome::Succeeded
            {
                continue;
            }
            let EventKind::ControlApplied { request_id, action } = &record.event.kind else {
                continue;
            };
            let request_id = StatusLabel::new(request_id.clone())?;
            let action = ControlAction::parse(action).ok_or(ControlError::InvalidRecord)?;
            engine.apply_recovered(request_id, action)?;
        }
        Ok(engine)
    }

    /// Returns current lifecycle controls.
    #[must_use]
    pub const fn state(&self) -> ControlState {
        self.state
    }

    /// Handles a same-user read without creating any mutation authority.
    ///
    /// # Errors
    ///
    /// Returns [`ControlError::Unauthorized`] for a different user or run.
    pub fn read(
        &self,
        request: &ReadRequest,
        status: &StatusView,
    ) -> Result<ReadResponse, ControlError> {
        self.authenticate(request.requester_uid, &request.run_id)?;
        let response = match request.command {
            ReadCommand::Status | ReadCommand::ExplainBlocker => ReadResponse {
                command: request.command,
                status: Some(status.clone()),
                artifact: None,
                healthy: None,
            },
            ReadCommand::OpenDiff => ReadResponse {
                command: request.command,
                status: None,
                artifact: Some(StatusLabel::new("artifact.diff")?),
                healthy: None,
            },
            ReadCommand::OpenLog => ReadResponse {
                command: request.command,
                status: None,
                artifact: Some(StatusLabel::new("artifact.log")?),
                healthy: None,
            },
            ReadCommand::Doctor => ReadResponse {
                command: request.command,
                status: None,
                artifact: None,
                healthy: Some(true),
            },
        };
        Ok(response)
    }

    /// Applies one authorized lifecycle request after durably journaling it.
    ///
    /// # Errors
    ///
    /// Returns an authorization, duplicate-conflict, terminal-state, or journal error.
    pub fn apply(
        &mut self,
        request: &ControlRequest,
        journal: &mut Journal,
        timestamp_ms: u64,
    ) -> Result<ControlOutcome, ControlError> {
        let _grant = self.authorize_mutation(request)?;
        if let Some((prior_action, prior_outcome)) = self.applied.get(&request.request_id) {
            if *prior_action != request.action {
                return Err(ControlError::DuplicateConflict);
            }
            return Ok(ControlOutcome {
                changed: false,
                state: prior_outcome.state,
            });
        }
        let next = transition(self.state, request.action)?;
        journal
            .append(JournalEvent {
                timestamp_ms,
                run_id: self.run_id.clone(),
                task_id: self.task_id.clone(),
                repository_id: self.repository_id.clone(),
                kind: EventKind::ControlApplied {
                    request_id: request.request_id.as_str().to_owned(),
                    action: request.action.label().to_owned(),
                },
                outcome: EventOutcome::Succeeded,
                evidence: Vec::new(),
                redactions: Vec::new(),
            })
            .map_err(|_| ControlError::Journal)?;
        journal
            .write_snapshot()
            .map_err(|_| ControlError::Journal)?;
        let changed = next != self.state;
        self.state = next;
        let outcome = ControlOutcome {
            changed,
            state: next,
        };
        self.applied
            .insert(request.request_id.clone(), (request.action, outcome));
        Ok(outcome)
    }

    fn authenticate(&self, requester_uid: u32, run_id: &RunId) -> Result<(), ControlError> {
        if requester_uid != self.owner_uid || run_id != &self.run_id {
            return Err(ControlError::Unauthorized);
        }
        Ok(())
    }

    fn authorize_mutation(&self, request: &ControlRequest) -> Result<MutationGrant, ControlError> {
        self.authenticate(request.requester_uid, &request.run_id)?;
        Ok(MutationGrant)
    }

    fn apply_recovered(
        &mut self,
        request_id: StatusLabel,
        action: ControlAction,
    ) -> Result<(), ControlError> {
        if let Some((prior, _)) = self.applied.get(&request_id) {
            return if *prior == action {
                Ok(())
            } else {
                Err(ControlError::DuplicateConflict)
            };
        }
        let prior_state = self.state;
        self.state = transition(self.state, action)?;
        let outcome = ControlOutcome {
            changed: self.state != prior_state,
            state: self.state,
        };
        self.applied.insert(request_id, (action, outcome));
        Ok(())
    }
}

fn transition(
    mut state: ControlState,
    action: ControlAction,
) -> Result<ControlState, ControlError> {
    if state.cancelled && action != ControlAction::Cancel {
        return Err(ControlError::Terminal);
    }
    match action {
        ControlAction::Pause => state.paused = true,
        ControlAction::Resume => state.paused = false,
        ControlAction::StopAfterUnit => state.stop_after_unit = true,
        ControlAction::Cancel => state.cancelled = true,
    }
    Ok(state)
}

/// Operator-control failure without request or user content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlError {
    /// Local user or exact run did not match.
    Unauthorized,
    /// One request identity was reused for a different action.
    DuplicateConflict,
    /// Cancelled state rejects further non-cancel controls.
    Terminal,
    /// Durable state write failed.
    Journal,
    /// Durable control record was malformed.
    InvalidRecord,
    /// Status field was invalid.
    InvalidStatus,
}

impl From<crate::MonitorError> for ControlError {
    fn from(_value: crate::MonitorError) -> Self {
        Self::InvalidStatus
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unauthorized => "codingmage.control.unauthorized",
            Self::DuplicateConflict => "codingmage.control.duplicate_conflict",
            Self::Terminal => "codingmage.control.terminal",
            Self::Journal => "codingmage.control.journal",
            Self::InvalidRecord => "codingmage.control.invalid_record",
            Self::InvalidStatus => "codingmage.control.invalid_status",
        })
    }
}

impl std::error::Error for ControlError {}
