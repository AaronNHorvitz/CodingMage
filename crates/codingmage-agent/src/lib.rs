//! Provider-neutral, authority-free agent contracts and deterministic fakes.

use std::{collections::BTreeSet, fmt};

use codingmage_contracts::{AgentId, AttemptId, RunId, TaskId};
use serde::{Deserialize, Serialize};

const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 10_000;
const MAX_UNTRUSTED_SUMMARY_BYTES: usize = 4096;

/// Role assigned by the coordinator for one invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Implement one bounded work packet.
    Implementation,
    /// Review one exact commit without write authority.
    Review,
    /// Correct accepted findings in the implementation worktree.
    Correction,
    /// Verify an exact corrected commit and evidence set.
    Verification,
    /// Perform content-free queue or capability administration.
    Administrative,
}

/// Operation exposed by every provider adapter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOperation {
    /// Probe supported provider behavior.
    Probe,
    /// Start a new exact session.
    Start,
    /// Continue one exact retained session.
    Continue,
    /// Cancel one exact active session.
    Cancel,
    /// Observe provider-reported usage.
    ObserveUsage,
}

/// Individually advertised adapter capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapability {
    /// Structured noninteractive invocation.
    StructuredOutput,
    /// Ordered event streaming.
    EventStream,
    /// Exact session continuation.
    SessionContinuation,
    /// Explicit cancellation.
    Cancellation,
    /// Usage observation.
    UsageObservation,
}

/// Content-minimized capability result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCapabilities {
    /// Adapter name.
    pub provider: String,
    /// Exact provider version or fingerprint when exposed.
    pub version: String,
    /// Supported operations.
    pub capabilities: BTreeSet<AgentCapability>,
}

/// One coordinator-authored adapter request with no repository or publication handle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRequest {
    /// Contract version.
    pub version: u16,
    /// Bound run.
    pub run_id: RunId,
    /// Bound task.
    pub task_id: TaskId,
    /// Configured agent profile.
    pub agent_id: AgentId,
    /// Assigned role.
    pub role: AgentRole,
    /// Requested operation.
    pub operation: AgentOperation,
    /// Exact prior session for continue, cancel, or usage operations.
    pub session_id: Option<AttemptId>,
    /// SHA-256 of separately bounded provider input.
    pub input_sha256: String,
}

/// One schema-valid event emitted by a provider adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEvent {
    /// Event schema version.
    pub version: u16,
    /// Zero-based contiguous sequence.
    pub sequence: u64,
    /// Exact session identity.
    pub session_id: AttemptId,
    /// Untrusted event body.
    pub event: AgentEventKind,
}

/// Untrusted provider event body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentEventKind {
    /// Provider accepted the exact session.
    Started,
    /// Bounded provider-authored progress text; never interpreted as authority.
    Progress {
        /// Untrusted summary retained only for immediate display.
        summary: String,
    },
    /// Provider-reported usage counters.
    Usage {
        /// Provider-reported input units.
        input_units: u64,
        /// Provider-reported output units.
        output_units: u64,
    },
    /// One terminal provider claim.
    Final {
        /// Provider result requiring independent verification.
        result: AgentFinal,
    },
}

/// Provider terminal classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFinalStatus {
    /// Provider claims its assigned response is complete.
    Completed,
    /// Provider reports a stable blocker code.
    Blocked,
    /// Provider reports failure.
    Failed,
    /// Provider reports a quota or rate limit.
    Quota,
    /// Provider reports cancellation.
    Cancelled,
}

/// Claims that remain untrusted until deterministic and coordinator gates verify them.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderClaims {
    /// Claimed coherent commit identifier.
    pub commit: Option<String>,
    /// Provider claims tests passed.
    pub tests_passed: bool,
    /// Provider claims a merge occurred; bootstrap policy must reject this effect.
    pub merge_completed: bool,
    /// Provider claims a release was published; bootstrap policy must reject this effect.
    pub release_published: bool,
}

/// Final provider response without state-transition authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFinal {
    /// Provider terminal classification.
    pub status: AgentFinalStatus,
    /// Untrusted implementation or review claims.
    #[serde(default)]
    pub claims: ProviderClaims,
    /// Stable blocker category without source or credential text.
    pub blocker_code: Option<String>,
}

/// Ordered, schema-valid transcript. It still contains only provider claims.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedTranscript {
    events: Vec<AgentEvent>,
    final_result: AgentFinal,
}

impl NormalizedTranscript {
    /// Returns the ordered events for display or coordinator inspection.
    #[must_use]
    pub fn events(&self) -> &[AgentEvent] {
        &self.events
    }

    /// Returns the terminal provider claim, which cannot advance state by itself.
    #[must_use]
    pub const fn final_result(&self) -> &AgentFinal {
        &self.final_result
    }
}

/// Provider-reported aggregate usage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AgentUsage {
    /// Provider-reported input units.
    pub input_units: u64,
    /// Provider-reported output units.
    pub output_units: u64,
}

/// Content-free adapter failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterError {
    /// Request did not match operation or session rules.
    InvalidRequest,
    /// Provider output was malformed, unordered, contradictory, or excessive.
    InvalidOutput,
    /// Scripted provider failure.
    Provider,
    /// Provider quota or rate limit.
    Quota,
    /// Exact operation was cancelled.
    Cancelled,
    /// Adapter invocation timed out.
    Timeout,
    /// No scripted response remained.
    Exhausted,
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "codingmage.provider.invalid_request",
            Self::InvalidOutput => "codingmage.provider.invalid_output",
            Self::Provider => "codingmage.provider.failed",
            Self::Quota => "codingmage.provider.quota",
            Self::Cancelled => "codingmage.provider.cancelled",
            Self::Timeout => "codingmage.provider.timeout",
            Self::Exhausted => "codingmage.provider.fixture_exhausted",
        })
    }
}

impl std::error::Error for AdapterError {}

/// Provider-neutral adapter operations. No method receives Git, repository, or state authority.
pub trait AgentAdapter {
    /// Reports supported adapter capabilities.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when the capability probe fails or is malformed.
    fn probe(&self) -> Result<AgentCapabilities, AdapterError>;
    /// Starts one new exact session.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] for an invalid request or provider failure.
    fn start(&mut self, request: &AgentRequest) -> Result<Vec<u8>, AdapterError>;
    /// Continues one exact retained session.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when session identity, request, or provider behavior is invalid.
    fn continue_session(&mut self, request: &AgentRequest) -> Result<Vec<u8>, AdapterError>;
    /// Cancels one exact active session.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when session identity is stale or cancellation fails.
    fn cancel(&mut self, request: &AgentRequest) -> Result<(), AdapterError>;
    /// Returns provider-reported usage for one exact session.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] when the session is not exact or usage is unavailable.
    fn observe_usage(&self, request: &AgentRequest) -> Result<AgentUsage, AdapterError>;
}

/// One deterministic fake-provider step.
#[derive(Clone, Debug)]
pub struct FakeStep {
    /// Exact expected operation.
    pub operation: AgentOperation,
    /// Scripted raw provider output or failure.
    pub response: Result<Vec<u8>, AdapterError>,
}

/// Deterministic stateful adapter used before live provider integration.
#[derive(Clone, Debug)]
pub struct FakeAdapter {
    capabilities: AgentCapabilities,
    steps: Vec<FakeStep>,
    next_step: usize,
    session: Option<AttemptId>,
    usage: AgentUsage,
}

impl FakeAdapter {
    /// Creates a fake adapter with an exact script.
    #[must_use]
    pub fn new(provider: &str, steps: Vec<FakeStep>) -> Self {
        Self {
            capabilities: AgentCapabilities {
                provider: provider.to_owned(),
                version: "fixture-1".to_owned(),
                capabilities: BTreeSet::from([
                    AgentCapability::StructuredOutput,
                    AgentCapability::EventStream,
                    AgentCapability::SessionContinuation,
                    AgentCapability::Cancellation,
                    AgentCapability::UsageObservation,
                ]),
            },
            steps,
            next_step: 0,
            session: None,
            usage: AgentUsage::default(),
        }
    }

    fn execute_step(&mut self, request: &AgentRequest) -> Result<Vec<u8>, AdapterError> {
        validate_request(request)?;
        let step = self
            .steps
            .get(self.next_step)
            .ok_or(AdapterError::Exhausted)?;
        if step.operation != request.operation {
            return Err(AdapterError::InvalidRequest);
        }
        self.next_step += 1;
        step.response.clone()
    }
}

impl AgentAdapter for FakeAdapter {
    fn probe(&self) -> Result<AgentCapabilities, AdapterError> {
        Ok(self.capabilities.clone())
    }

    fn start(&mut self, request: &AgentRequest) -> Result<Vec<u8>, AdapterError> {
        if request.operation != AgentOperation::Start || request.session_id.is_some() {
            return Err(AdapterError::InvalidRequest);
        }
        let output = self.execute_step(request)?;
        let transcript = normalize_events(&output, None)?;
        let session = transcript.events[0].session_id.clone();
        self.usage = usage_from(&transcript);
        self.session = Some(session);
        Ok(output)
    }

    fn continue_session(&mut self, request: &AgentRequest) -> Result<Vec<u8>, AdapterError> {
        if request.operation != AgentOperation::Continue || request.session_id != self.session {
            return Err(AdapterError::InvalidRequest);
        }
        let output = self.execute_step(request)?;
        let transcript = normalize_events(&output, self.session.as_ref())?;
        self.usage = usage_from(&transcript);
        Ok(output)
    }

    fn cancel(&mut self, request: &AgentRequest) -> Result<(), AdapterError> {
        if request.operation != AgentOperation::Cancel || request.session_id != self.session {
            return Err(AdapterError::InvalidRequest);
        }
        let _ = self.execute_step(request)?;
        self.session = None;
        Ok(())
    }

    fn observe_usage(&self, request: &AgentRequest) -> Result<AgentUsage, AdapterError> {
        if request.operation != AgentOperation::ObserveUsage || request.session_id != self.session {
            return Err(AdapterError::InvalidRequest);
        }
        Ok(self.usage)
    }
}

/// Normalizes newline-delimited provider events and enforces exact order and session identity.
///
/// # Errors
///
/// Returns [`AdapterError::InvalidOutput`] for malformed, unknown, unordered, contradictory,
/// excessive, unterminated, or wrong-session output.
pub fn normalize_events(
    bytes: &[u8],
    expected_session: Option<&AttemptId>,
) -> Result<NormalizedTranscript, AdapterError> {
    if bytes.is_empty() || bytes.len() > MAX_EVENT_BYTES {
        return Err(AdapterError::InvalidOutput);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| AdapterError::InvalidOutput)?;
    let mut events = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if index >= MAX_EVENTS || line.is_empty() {
            return Err(AdapterError::InvalidOutput);
        }
        let event: AgentEvent =
            serde_json::from_str(line).map_err(|_| AdapterError::InvalidOutput)?;
        if event.version != 1 || event.sequence != index as u64 {
            return Err(AdapterError::InvalidOutput);
        }
        if expected_session.is_some_and(|expected| expected != &event.session_id)
            || events
                .first()
                .is_some_and(|first: &AgentEvent| first.session_id != event.session_id)
        {
            return Err(AdapterError::InvalidOutput);
        }
        if let AgentEventKind::Progress { summary } = &event.event
            && summary.len() > MAX_UNTRUSTED_SUMMARY_BYTES
        {
            return Err(AdapterError::InvalidOutput);
        }
        events.push(event);
    }
    if !matches!(
        events.first().map(|event| &event.event),
        Some(AgentEventKind::Started)
    ) {
        return Err(AdapterError::InvalidOutput);
    }
    let finals = events
        .iter()
        .filter_map(|event| match &event.event {
            AgentEventKind::Final { result } => Some(result),
            _ => None,
        })
        .collect::<Vec<_>>();
    if finals.len() != 1
        || !matches!(
            events.last().map(|event| &event.event),
            Some(AgentEventKind::Final { .. })
        )
    {
        return Err(AdapterError::InvalidOutput);
    }
    let final_result = finals[0].clone();
    validate_final(&final_result)?;
    Ok(NormalizedTranscript {
        events,
        final_result,
    })
}

fn validate_request(request: &AgentRequest) -> Result<(), AdapterError> {
    if request.version != 1
        || request.input_sha256.len() != 64
        || !request
            .input_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AdapterError::InvalidRequest);
    }
    Ok(())
}

fn validate_final(result: &AgentFinal) -> Result<(), AdapterError> {
    let commit_valid = result.claims.commit.as_ref().is_none_or(|commit| {
        matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let contradictory = match result.status {
        AgentFinalStatus::Completed => result.blocker_code.is_some(),
        AgentFinalStatus::Blocked => {
            result.blocker_code.is_none()
                || result.claims.commit.is_some()
                || result.claims.tests_passed
        }
        AgentFinalStatus::Failed | AgentFinalStatus::Quota | AgentFinalStatus::Cancelled => {
            result.claims.commit.is_some()
                || result.claims.tests_passed
                || result.claims.merge_completed
                || result.claims.release_published
        }
    };
    if !commit_valid || contradictory {
        return Err(AdapterError::InvalidOutput);
    }
    Ok(())
}

fn usage_from(transcript: &NormalizedTranscript) -> AgentUsage {
    transcript
        .events
        .iter()
        .fold(AgentUsage::default(), |mut total, event| {
            if let AgentEventKind::Usage {
                input_units,
                output_units,
            } = event.event
            {
                total.input_units = total.input_units.saturating_add(input_units);
                total.output_units = total.output_units.saturating_add(output_units);
            }
            total
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(operation: AgentOperation, session_id: Option<AttemptId>) -> AgentRequest {
        AgentRequest {
            version: 1,
            run_id: RunId::new("run-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            agent_id: AgentId::new("agent-1").unwrap(),
            role: AgentRole::Implementation,
            operation,
            session_id,
            input_sha256: "a".repeat(64),
        }
    }

    fn transcript(session: &AttemptId, final_result: AgentFinal) -> Vec<u8> {
        let events = [
            AgentEvent {
                version: 1,
                sequence: 0,
                session_id: session.clone(),
                event: AgentEventKind::Started,
            },
            AgentEvent {
                version: 1,
                sequence: 1,
                session_id: session.clone(),
                event: AgentEventKind::Usage {
                    input_units: 10,
                    output_units: 4,
                },
            },
            AgentEvent {
                version: 1,
                sequence: 2,
                session_id: session.clone(),
                event: AgentEventKind::Final {
                    result: final_result,
                },
            },
        ];
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    fn completed() -> AgentFinal {
        AgentFinal {
            status: AgentFinalStatus::Completed,
            claims: ProviderClaims {
                commit: Some("a".repeat(40)),
                tests_passed: true,
                merge_completed: false,
                release_published: false,
            },
            blocker_code: None,
        }
    }

    #[test]
    fn ordered_events_normalize_but_claims_gain_no_authority() {
        let session = AttemptId::new("attempt-1").unwrap();
        let mut result = completed();
        result.claims.merge_completed = true;
        result.claims.release_published = true;
        let normalized = normalize_events(&transcript(&session, result), Some(&session)).unwrap();
        assert_eq!(normalized.events().len(), 3);
        assert!(normalized.final_result().claims.tests_passed);
        assert!(normalized.final_result().claims.merge_completed);
        assert!(normalized.final_result().claims.release_published);
    }

    #[test]
    fn malformed_unordered_wrong_session_and_unknown_fields_fail() {
        let session = AttemptId::new("attempt-1").unwrap();
        let mut unordered = transcript(&session, completed());
        let text = String::from_utf8(unordered.clone()).unwrap();
        unordered = text
            .replacen("\"sequence\":1", "\"sequence\":9", 1)
            .into_bytes();
        assert_eq!(
            normalize_events(&unordered, Some(&session)),
            Err(AdapterError::InvalidOutput)
        );
        assert_eq!(
            normalize_events(b"not-json", None),
            Err(AdapterError::InvalidOutput)
        );
        assert_eq!(
            normalize_events(
                &transcript(&session, completed()),
                Some(&AttemptId::new("attempt-2").unwrap())
            ),
            Err(AdapterError::InvalidOutput)
        );
        let unknown = String::from_utf8(transcript(&session, completed()))
            .unwrap()
            .replacen("{\"version\":1", "{\"unknown\":1,\"version\":1", 1);
        assert_eq!(
            normalize_events(unknown.as_bytes(), None),
            Err(AdapterError::InvalidOutput)
        );
    }

    #[test]
    fn contradictory_terminal_results_fail() {
        let session = AttemptId::new("attempt-1").unwrap();
        let contradictory = AgentFinal {
            status: AgentFinalStatus::Blocked,
            claims: ProviderClaims {
                commit: Some("b".repeat(40)),
                tests_passed: true,
                ..ProviderClaims::default()
            },
            blocker_code: Some("external".to_owned()),
        };
        assert_eq!(
            normalize_events(&transcript(&session, contradictory), None),
            Err(AdapterError::InvalidOutput)
        );
    }

    #[test]
    fn scripted_start_continue_usage_and_cancel_are_exact() {
        let session = AttemptId::new("attempt-1").unwrap();
        let steps = vec![
            FakeStep {
                operation: AgentOperation::Start,
                response: Ok(transcript(&session, completed())),
            },
            FakeStep {
                operation: AgentOperation::Continue,
                response: Ok(transcript(&session, completed())),
            },
            FakeStep {
                operation: AgentOperation::Cancel,
                response: Ok(Vec::new()),
            },
        ];
        let mut adapter = FakeAdapter::new("fake", steps);
        assert!(
            adapter
                .probe()
                .unwrap()
                .capabilities
                .contains(&AgentCapability::EventStream)
        );
        adapter
            .start(&request(AgentOperation::Start, None))
            .unwrap();
        adapter
            .continue_session(&request(AgentOperation::Continue, Some(session.clone())))
            .unwrap();
        assert_eq!(
            adapter
                .observe_usage(&request(
                    AgentOperation::ObserveUsage,
                    Some(session.clone())
                ))
                .unwrap(),
            AgentUsage {
                input_units: 10,
                output_units: 4
            }
        );
        adapter
            .cancel(&request(AgentOperation::Cancel, Some(session)))
            .unwrap();
    }

    #[test]
    fn failure_quota_timeout_and_cancel_scenarios_are_deterministic() {
        for expected in [
            AdapterError::Provider,
            AdapterError::Quota,
            AdapterError::Timeout,
            AdapterError::Cancelled,
        ] {
            let mut adapter = FakeAdapter::new(
                "fake",
                vec![FakeStep {
                    operation: AgentOperation::Start,
                    response: Err(expected),
                }],
            );
            assert_eq!(
                adapter.start(&request(AgentOperation::Start, None)),
                Err(expected)
            );
        }
    }

    #[test]
    fn implementation_and_review_scripts_remain_role_neutral() {
        for role in [AgentRole::Implementation, AgentRole::Review] {
            let session = AttemptId::new(match role {
                AgentRole::Implementation => "implementation-session",
                _ => "review-session",
            })
            .unwrap();
            let mut adapter = FakeAdapter::new(
                "fake",
                vec![FakeStep {
                    operation: AgentOperation::Start,
                    response: Ok(transcript(&session, completed())),
                }],
            );
            let mut start = request(AgentOperation::Start, None);
            start.role = role;
            let output = adapter.start(&start).unwrap();
            assert_eq!(
                normalize_events(&output, Some(&session))
                    .unwrap()
                    .events()
                    .len(),
                3
            );
        }
    }
}
