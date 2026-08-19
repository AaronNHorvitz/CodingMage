use std::{collections::VecDeque, fmt};

use codingmage_contracts::{AgentId, AttemptId, RepositoryId, RunId, TaskId};
use serde::{Deserialize, Serialize};

const MAX_EVENTS: usize = 4096;
const MIN_PROGRESS_INTERVAL_MS: u64 = 250;

/// A metric that distinguishes an unavailable value from a measured zero.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "availability", content = "value", rename_all = "snake_case")]
pub enum Known<T> {
    /// Provider or coordinator exposed a measured value.
    Known(T),
    /// No trustworthy value is available.
    Unknown,
}

/// Validated content-free status label.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct StatusLabel(String);

impl StatusLabel {
    /// Creates a bounded canonical status label.
    ///
    /// # Errors
    ///
    /// Returns [`MonitorError::InvalidLabel`] for content-bearing or malformed text.
    pub fn new(value: impl Into<String>) -> Result<Self, MonitorError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
            || value.starts_with('/')
            || value.contains("..")
        {
            return Err(MonitorError::InvalidLabel);
        }
        Ok(Self(value))
    }

    /// Returns the canonical text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StatusLabel {
    type Error = MonitorError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<StatusLabel> for String {
    fn from(value: StatusLabel) -> Self {
        value.0
    }
}

/// Validated input used to construct one status snapshot.
#[derive(Clone, Debug)]
pub struct StatusInput {
    /// Exact run.
    pub run_id: RunId,
    /// Authorized target.
    pub target: RepositoryId,
    /// Current task.
    pub task: TaskId,
    /// Stable lifecycle state.
    pub state: StatusLabel,
    /// Active agent.
    pub agent: Option<AgentId>,
    /// Resolved model identifier.
    pub model: Option<StatusLabel>,
    /// Owned branch identifier.
    pub branch: Option<StatusLabel>,
    /// Exact commit identifier.
    pub commit: Option<StatusLabel>,
    /// Stable command identifier, never arguments or output.
    pub command: Option<StatusLabel>,
    /// Current gate identifier.
    pub gate: Option<StatusLabel>,
    /// Open finding count.
    pub findings: u32,
    /// Completed correction rounds.
    pub correction_count: u32,
    /// Elapsed monotonic milliseconds.
    pub elapsed_ms: u64,
    /// Pause category.
    pub pause: Option<StatusLabel>,
    /// Stable blocker code.
    pub blocker: Option<StatusLabel>,
    /// Provider-reported used units.
    pub usage: Known<u64>,
    /// Provider-reported reset as Unix milliseconds.
    pub reset_at_ms: Known<u64>,
}

/// Stable serializable operator status without source, output, or credential content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusView {
    run_id: RunId,
    target: RepositoryId,
    task: TaskId,
    state: StatusLabel,
    agent: Option<AgentId>,
    model: Option<StatusLabel>,
    branch: Option<StatusLabel>,
    commit: Option<StatusLabel>,
    command: Option<StatusLabel>,
    gate: Option<StatusLabel>,
    findings: u32,
    correction_count: u32,
    elapsed_ms: u64,
    pause: Option<StatusLabel>,
    blocker: Option<StatusLabel>,
    usage: Known<u64>,
    reset_at_ms: Known<u64>,
}

impl From<StatusInput> for StatusView {
    fn from(input: StatusInput) -> Self {
        Self {
            run_id: input.run_id,
            target: input.target,
            task: input.task,
            state: input.state,
            agent: input.agent,
            model: input.model,
            branch: input.branch,
            commit: input.commit,
            command: input.command,
            gate: input.gate,
            findings: input.findings,
            correction_count: input.correction_count,
            elapsed_ms: input.elapsed_ms,
            pause: input.pause,
            blocker: input.blocker,
            usage: input.usage,
            reset_at_ms: input.reset_at_ms,
        }
    }
}

impl StatusView {
    /// Serializes stable status JSON.
    ///
    /// # Errors
    ///
    /// Returns [`MonitorError::Encoding`] if serialization fails.
    pub fn to_json(&self) -> Result<String, MonitorError> {
        serde_json::to_string(self).map_err(|_| MonitorError::Encoding)
    }

    /// Renders a compact terminal view suitable for a VS Code integrated terminal.
    #[must_use]
    pub fn render_terminal(&self) -> String {
        let unknown = "unknown";
        let usage = match self.usage {
            Known::Known(value) => value.to_string(),
            Known::Unknown => unknown.to_owned(),
        };
        let reset = match self.reset_at_ms {
            Known::Known(value) => value.to_string(),
            Known::Unknown => unknown.to_owned(),
        };
        format!(
            "CodingMage run={} target={} task={}\nstate={} agent={} model={}\nbranch={} commit={} command={} gate={}\nfindings={} corrections={} elapsed_ms={}\npause={} blocker={} usage={} reset_at_ms={}",
            self.run_id,
            self.target,
            self.task,
            self.state.as_str(),
            self.agent.as_ref().map_or(unknown, AgentId::as_str),
            label_or_unknown(self.model.as_ref()),
            label_or_unknown(self.branch.as_ref()),
            label_or_unknown(self.commit.as_ref()),
            label_or_unknown(self.command.as_ref()),
            label_or_unknown(self.gate.as_ref()),
            self.findings,
            self.correction_count,
            self.elapsed_ms,
            label_or_unknown(self.pause.as_ref()),
            label_or_unknown(self.blocker.as_ref()),
            usage,
            reset,
        )
    }
}

fn label_or_unknown(label: Option<&StatusLabel>) -> &str {
    label.map_or("unknown", StatusLabel::as_str)
}

/// One immutable accepted status event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvent {
    /// Contiguous stream sequence.
    pub sequence: u64,
    /// Exact operation correlation identity.
    pub correlation_id: AttemptId,
    /// Monotonic event time supplied by the coordinator.
    pub monotonic_ms: u64,
    /// Content-minimized current state.
    pub status: StatusView,
}

/// Reconnect response containing current state and retained events after a cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorSnapshot {
    /// Authoritative current status even when old events aged out.
    pub current: StatusView,
    /// Ordered retained events after the requested sequence.
    pub events: Vec<MonitorEvent>,
    /// True when the requested cursor predates retained history.
    pub history_gap: bool,
}

/// Bounded in-memory stream that has no coordinator mutation handle.
#[derive(Debug)]
pub struct StatusStream {
    current: StatusView,
    events: VecDeque<MonitorEvent>,
    next_sequence: u64,
    last_emitted_ms: Option<u64>,
}

impl StatusStream {
    /// Starts a stream with an authoritative initial status.
    #[must_use]
    pub fn new(current: StatusView) -> Self {
        Self {
            current,
            events: VecDeque::new(),
            next_sequence: 0,
            last_emitted_ms: None,
        }
    }

    /// Updates current status and emits at the bounded rate.
    ///
    /// Returns `None` when a noisy update is coalesced into current state.
    #[must_use]
    pub fn publish(
        &mut self,
        correlation_id: AttemptId,
        monotonic_ms: u64,
        status: StatusView,
    ) -> Option<MonitorEvent> {
        self.current = status.clone();
        let coalesced = self.last_emitted_ms.is_some_and(|last| {
            monotonic_ms >= last && monotonic_ms - last < MIN_PROGRESS_INTERVAL_MS
        });
        if coalesced {
            return None;
        }
        let event = MonitorEvent {
            sequence: self.next_sequence,
            correlation_id,
            monotonic_ms,
            status,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.last_emitted_ms = Some(monotonic_ms);
        self.events.push_back(event.clone());
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
        Some(event)
    }

    /// Attaches or reconnects after an optional last-seen sequence without changing execution.
    #[must_use]
    pub fn attach(&self, after: Option<u64>) -> MonitorSnapshot {
        let first = self.events.front().map(|event| event.sequence);
        let history_gap = match (after, first) {
            (Some(cursor), Some(first)) => cursor.saturating_add(1) < first,
            _ => false,
        };
        let events = self
            .events
            .iter()
            .filter(|event| after.is_none_or(|cursor| event.sequence > cursor))
            .cloned()
            .collect();
        MonitorSnapshot {
            current: self.current.clone(),
            events,
            history_gap,
        }
    }
}

/// Status validation or encoding failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorError {
    /// Status label was content-bearing, oversized, or malformed.
    InvalidLabel,
    /// Stable serialization failed.
    Encoding,
}

impl fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLabel => "codingmage.monitor.invalid_label",
            Self::Encoding => "codingmage.monitor.encoding",
        })
    }
}

impl std::error::Error for MonitorError {}
