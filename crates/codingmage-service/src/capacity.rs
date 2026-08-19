use std::fmt;

use codingmage_contracts::{RepositoryId, RunId, TaskId};
use codingmage_state::{EventKind, EventOutcome, Journal, JournalEvent, JournalRecord};
use serde::{Deserialize, Serialize};

/// Structured provider failure when exposed by an adapter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredFailure {
    /// Credential is absent or expired.
    Authentication,
    /// Account or model quota is exhausted.
    Quota,
    /// Provider rate limit is active.
    RateLimit,
    /// Provider is temporarily overloaded.
    Overload,
    /// Network transport failed before a response.
    Network,
    /// Provider output did not match its schema.
    Malformed,
    /// Provider reported a nonrecoverable failure.
    Terminal,
}

/// Raw content-free capacity inputs normalized by policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapacityInput {
    /// Structured adapter classification, preferred when available.
    pub structured: Option<StructuredFailure>,
    /// HTTP status supplied by an approved adapter.
    pub http_status: Option<u16>,
    /// Provider reset instant as Unix milliseconds.
    pub reset_at_ms: Option<u64>,
    /// Retry delay supplied by the provider.
    pub retry_after_ms: Option<u64>,
}

/// Normalized failure class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityClass {
    /// Credentials require operator repair.
    Authentication,
    /// Capacity is exhausted until a reset or retry.
    Quota,
    /// Network failure may recover.
    Network,
    /// Provider overload may recover.
    Overload,
    /// Response was malformed.
    Malformed,
    /// Provider explicitly ended retries.
    Terminal,
}

impl CapacityClass {
    const fn label(self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::Quota => "quota",
            Self::Network => "network",
            Self::Overload => "overload",
            Self::Malformed => "malformed",
            Self::Terminal => "terminal",
        }
    }
}

/// Provider-reported metrics, each explicitly optional.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapacityMetrics {
    /// Used provider units.
    pub used_units: Option<u64>,
    /// Remaining provider units.
    pub remaining_units: Option<u64>,
    /// Cost in provider-reported micros.
    pub cost_micros: Option<u64>,
    /// Reset as Unix milliseconds.
    pub reset_at_ms: Option<u64>,
}

/// Classifies structured data first and HTTP status only as an explicit fallback.
#[must_use]
pub fn classify_capacity(input: CapacityInput) -> CapacityClass {
    if let Some(structured) = input.structured {
        return match structured {
            StructuredFailure::Authentication => CapacityClass::Authentication,
            StructuredFailure::Quota | StructuredFailure::RateLimit => CapacityClass::Quota,
            StructuredFailure::Overload => CapacityClass::Overload,
            StructuredFailure::Network => CapacityClass::Network,
            StructuredFailure::Malformed => CapacityClass::Malformed,
            StructuredFailure::Terminal => CapacityClass::Terminal,
        };
    }
    match input.http_status {
        Some(401 | 403) => CapacityClass::Authentication,
        Some(429) => CapacityClass::Quota,
        Some(500 | 502 | 503 | 504) => CapacityClass::Overload,
        Some(400..=499) => CapacityClass::Terminal,
        _ => CapacityClass::Malformed,
    }
}

/// Persisted retry counters and deadline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetryState {
    /// Number of failed attempts already scheduled.
    pub attempt: u32,
    /// Next allowed Unix millisecond.
    pub next_at_ms: Option<u64>,
    /// Most recent class.
    pub last_class: Option<CapacityClass>,
    /// Terminal policy prevents further attempts.
    pub terminal: bool,
}

impl RetryState {
    /// Returns true only when policy permits an attempt at this instant.
    #[must_use]
    pub fn ready(self, now_ms: u64) -> bool {
        !self.terminal && self.next_at_ms.is_none_or(|deadline| now_ms >= deadline)
    }
}

/// Retry or terminal decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    /// Pause without polling until the exact deadline.
    PauseUntil {
        /// One-based retry attempt.
        attempt: u32,
        /// Earliest next attempt.
        next_at_ms: u64,
    },
    /// Stop and require operator or later-run intervention.
    Stop {
        /// Stable reason.
        class: CapacityClass,
    },
}

/// Bounded deterministic retry policy.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    base_delay_ms: u64,
    max_delay_ms: u64,
    max_attempts: u32,
    jitter_bound_ms: u64,
}

/// Retry policy validation or persistence failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryError {
    /// Retry bounds are zero, inverted, or excessive.
    InvalidPolicy,
    /// Durable retry state could not be written.
    Journal,
}

impl fmt::Display for RetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPolicy => "codingmage.retry.invalid_policy",
            Self::Journal => "codingmage.retry.journal",
        })
    }
}

impl std::error::Error for RetryError {}

impl RetryPolicy {
    /// Creates a bounded policy.
    ///
    /// # Errors
    ///
    /// Returns [`RetryError::InvalidPolicy`] when bounds are zero, inverted, or excessive.
    pub const fn new(
        base_delay_ms: u64,
        max_delay_ms: u64,
        max_attempts: u32,
        jitter_bound_ms: u64,
    ) -> Result<Self, RetryError> {
        if base_delay_ms == 0
            || max_delay_ms < base_delay_ms
            || max_attempts == 0
            || max_attempts > 100
            || jitter_bound_ms > max_delay_ms
        {
            return Err(RetryError::InvalidPolicy);
        }
        Ok(Self {
            base_delay_ms,
            max_delay_ms,
            max_attempts,
            jitter_bound_ms,
        })
    }

    /// Selects a bounded retry or terminal stop without sleeping or spinning.
    #[must_use]
    pub fn decide(
        self,
        state: RetryState,
        class: CapacityClass,
        now_ms: u64,
        exposed_reset_ms: Option<u64>,
        jitter_seed: u64,
    ) -> RetryDecision {
        let attempt = state.attempt.saturating_add(1);
        if matches!(
            class,
            CapacityClass::Authentication | CapacityClass::Terminal
        ) || attempt > self.max_attempts
        {
            return RetryDecision::Stop { class };
        }
        let jitter = if self.jitter_bound_ms == 0 {
            0
        } else {
            jitter_seed % (self.jitter_bound_ms + 1)
        };
        let next_at_ms = if let Some(reset) = exposed_reset_ms {
            reset.max(now_ms).saturating_add(jitter)
        } else {
            let exponent = attempt.saturating_sub(1).min(63);
            let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
            let delay = self
                .base_delay_ms
                .saturating_mul(multiplier)
                .min(self.max_delay_ms);
            now_ms.saturating_add(delay).saturating_add(jitter)
        };
        RetryDecision::PauseUntil {
            attempt,
            next_at_ms,
        }
    }

    /// Persists the retry decision and updates the caller's state only after durable success.
    ///
    /// # Errors
    ///
    /// Returns [`RetryError::Journal`] when journal or snapshot persistence fails.
    pub fn persist(
        decision: RetryDecision,
        class: CapacityClass,
        identities: (&RepositoryId, &RunId, &TaskId),
        timestamp_ms: u64,
        journal: &mut Journal,
        state: &mut RetryState,
    ) -> Result<(), RetryError> {
        let (attempt, next_at_ms) = match decision {
            RetryDecision::PauseUntil {
                attempt,
                next_at_ms,
            } => (attempt, Some(next_at_ms)),
            RetryDecision::Stop { .. } => (state.attempt.saturating_add(1), None),
        };
        journal
            .append(JournalEvent {
                timestamp_ms,
                repository_id: identities.0.clone(),
                run_id: identities.1.clone(),
                task_id: identities.2.clone(),
                kind: EventKind::RetryScheduled {
                    attempt,
                    next_at_ms,
                    reason: class.label().to_owned(),
                },
                outcome: if next_at_ms.is_some() {
                    EventOutcome::Blocked
                } else {
                    EventOutcome::Failed
                },
                evidence: Vec::new(),
                redactions: Vec::new(),
            })
            .map_err(|_| RetryError::Journal)?;
        journal.write_snapshot().map_err(|_| RetryError::Journal)?;
        *state = RetryState {
            attempt,
            next_at_ms,
            last_class: Some(class),
            terminal: next_at_ms.is_none(),
        };
        Ok(())
    }

    /// Recovers the latest retry state for exact durable identities.
    #[must_use]
    pub fn recover(
        records: &[JournalRecord],
        repository_id: &RepositoryId,
        run_id: &RunId,
        task_id: &TaskId,
    ) -> RetryState {
        records
            .iter()
            .filter(|record| {
                &record.event.repository_id == repository_id
                    && &record.event.run_id == run_id
                    && &record.event.task_id == task_id
            })
            .filter_map(|record| {
                let EventKind::RetryScheduled {
                    attempt,
                    next_at_ms,
                    reason,
                } = &record.event.kind
                else {
                    return None;
                };
                let class = match reason.as_str() {
                    "authentication" => CapacityClass::Authentication,
                    "quota" => CapacityClass::Quota,
                    "network" => CapacityClass::Network,
                    "overload" => CapacityClass::Overload,
                    "malformed" => CapacityClass::Malformed,
                    "terminal" => CapacityClass::Terminal,
                    _ => return None,
                };
                Some(RetryState {
                    attempt: *attempt,
                    next_at_ms: *next_at_ms,
                    last_class: Some(class),
                    terminal: next_at_ms.is_none(),
                })
            })
            .next_back()
            .unwrap_or_default()
    }
}
