//! Freshness tracking for every backend observation shown on screen.

use std::time::{Duration, Instant};

use crate::backend::{BackendError, Generation};

/// Age after which a successful observation is labelled stale.
pub const STALE_AFTER: Duration = Duration::from_secs(30);

/// One backend observation with its freshness.
#[derive(Clone, Debug)]
pub struct Observed<T> {
    /// Last successful value, retained through later failures.
    pub value: Option<T>,
    /// When the value was observed.
    pub observed_at: Option<Instant>,
    /// Generation the value belongs to.
    pub generation: Option<Generation>,
    /// Most recent failure after the retained value, if any.
    pub last_error: Option<(Instant, BackendError)>,
    /// A request is in flight.
    pub loading: bool,
}

impl<T> Default for Observed<T> {
    fn default() -> Self {
        Self {
            value: None,
            observed_at: None,
            generation: None,
            last_error: None,
            loading: false,
        }
    }
}

/// Presentation freshness derived from an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    /// Nothing has been requested yet.
    NotRequested,
    /// A request is in flight and no value exists yet.
    Loading,
    /// A fresh value exists.
    Live,
    /// A value exists but its last refresh failed or it is older than [`STALE_AFTER`].
    Stale,
    /// No value exists and the last request failed.
    Failed,
}

impl Freshness {
    /// Short label for status lines.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotRequested => "not requested",
            Self::Loading => "loading",
            Self::Live => "live",
            Self::Stale => "stale",
            Self::Failed => "failed",
        }
    }
}

impl<T> Observed<T> {
    /// Records a successful observation.
    pub fn accept(&mut self, value: T, generation: Generation, now: Instant) {
        self.value = Some(value);
        self.observed_at = Some(now);
        self.generation = Some(generation);
        self.last_error = None;
        self.loading = false;
    }

    /// Records a failed refresh while retaining any earlier value.
    pub fn fail(&mut self, error: BackendError, now: Instant) {
        self.last_error = Some((now, error));
        self.loading = false;
    }

    /// Forgets everything; used when the selection changes.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Current freshness.
    #[must_use]
    pub fn freshness(&self, now: Instant) -> Freshness {
        match (&self.value, &self.last_error, self.loading) {
            (None, None, true) => Freshness::Loading,
            (None, None, false) => Freshness::NotRequested,
            (None, Some(_), _) => Freshness::Failed,
            (Some(_), Some(_), _) => Freshness::Stale,
            (Some(_), None, _) => {
                if self
                    .observed_at
                    .is_some_and(|observed| now.duration_since(observed) > STALE_AFTER)
                {
                    Freshness::Stale
                } else {
                    Freshness::Live
                }
            }
        }
    }

    /// Age of the retained value.
    #[must_use]
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.observed_at
            .map(|observed| now.duration_since(observed))
    }
}

/// Renders an age as a compact human string.
#[must_use]
pub fn age_label(age: Option<Duration>) -> String {
    match age {
        None => "never".to_owned(),
        Some(age) if age < Duration::from_secs(1) => "just now".to_owned(),
        Some(age) if age < Duration::from_secs(90) => format!("{}s ago", age.as_secs()),
        Some(age) if age < Duration::from_mins(90) => format!("{}m ago", age.as_secs() / 60),
        Some(age) => format!("{}h ago", age.as_secs() / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_transitions_are_distinct() {
        let now = Instant::now();
        let mut observed: Observed<u8> = Observed::default();
        assert_eq!(observed.freshness(now), Freshness::NotRequested);
        observed.loading = true;
        assert_eq!(observed.freshness(now), Freshness::Loading);
        observed.fail(BackendError::Timeout, now);
        assert_eq!(observed.freshness(now), Freshness::Failed);
        observed.accept(1, Generation(1), now);
        assert_eq!(observed.freshness(now), Freshness::Live);
        assert_eq!(
            observed.freshness(now + STALE_AFTER + Duration::from_secs(1)),
            Freshness::Stale
        );
        observed.fail(BackendError::Timeout, now);
        assert_eq!(observed.freshness(now), Freshness::Stale);
        assert_eq!(observed.value, Some(1));
        observed.clear();
        assert_eq!(observed.freshness(now), Freshness::NotRequested);
        assert_eq!(age_label(Some(Duration::from_secs(5))), "5s ago");
        assert_eq!(age_label(None), "never");
    }
}
