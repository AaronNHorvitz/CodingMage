//! Durable ledger of control requests issued by this interface.
//!
//! Every request identity is generated once and retained, so a retry after a timeout replays
//! the same identity (which the coordinator treats idempotently) instead of creating a second
//! intent. A request that is still pending blocks another request for the same campaign.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    admission::now_ms,
    state_dir::{StateError, read_private, write_private},
};

/// Closed control actions the coordinator accepts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    /// Pause admission at the next boundary.
    Pause,
    /// Clear the operator pause.
    Resume,
    /// Finish the current unit and stop.
    StopAfterUnit,
    /// Cancel the campaign and terminate owned descendants.
    Cancel,
}

impl ControlAction {
    /// Every action in display order.
    pub const ALL: [Self; 4] = [Self::Pause, Self::Resume, Self::StopAfterUnit, Self::Cancel];

    /// Stable code passed to `campaign-control`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::StopAfterUnit => "stop_after_unit",
            Self::Cancel => "cancel",
        }
    }

    /// Button label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::Resume => "Resume",
            Self::StopAfterUnit => "Stop after unit",
            Self::Cancel => "Cancel campaign",
        }
    }

    /// What the coordinator does with the request.
    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::Pause => {
                "records a pause intent; the coordinator stops admitting units at the next boundary and exits"
            }
            Self::Resume => "clears the operator pause; start the coordinator again to continue",
            Self::StopAfterUnit => {
                "the coordinator finishes the active unit, checkpoints and exits"
            }
            Self::Cancel => {
                "the coordinator terminates only its owned provider and gate processes, records cancellation and exits; durable state is retained"
            }
        }
    }
}

/// Outcome recorded for one request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResult {
    /// True when the coordinator created the intent on this invocation.
    pub created: bool,
    /// Stable failure code when the request failed.
    pub code: Option<String>,
    /// Unix milliseconds when the outcome was observed.
    pub observed_at_ms: u64,
}

/// One ledger entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlEntry {
    /// Caller-generated idempotency identity.
    pub request_id: String,
    /// Requested action.
    pub action: ControlAction,
    /// Campaign authority the request was bound to.
    pub authority_sha256: String,
    /// Unix milliseconds when first requested.
    pub requested_at_ms: u64,
    /// Attempts issued for this identity.
    pub attempts: u32,
    /// Outcome, absent while pending.
    pub result: Option<ControlResult>,
}

/// Ledger for one campaign.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlLedger {
    /// Document version.
    #[serde(default = "version")]
    pub version: u16,
    /// Entries in request order.
    #[serde(default)]
    pub entries: Vec<ControlEntry>,
}

impl Default for ControlLedger {
    fn default() -> Self {
        Self {
            version: version(),
            entries: Vec::new(),
        }
    }
}

const fn version() -> u16 {
    1
}

/// Why a control request was refused before reaching the coordinator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlRefusal {
    /// Another request is still pending for this campaign.
    Pending(String),
    /// No campaign is selected.
    NoCampaign,
    /// The cancel action needs a second confirmation.
    ConfirmCancel,
}

impl std::fmt::Display for ControlRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending(request_id) => write!(
                formatter,
                "request {request_id} is still pending; wait for its outcome before another control"
            ),
            Self::NoCampaign => formatter.write_str("select a campaign first"),
            Self::ConfirmCancel => formatter.write_str("press Cancel campaign again to confirm"),
        }
    }
}

impl std::error::Error for ControlRefusal {}

impl ControlLedger {
    fn path(project_dir: &Path, campaign_id: &str) -> PathBuf {
        project_dir
            .join("controls")
            .join(format!("{campaign_id}.json"))
    }

    /// Loads the ledger for one campaign; missing means empty.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the document exists but is invalid.
    pub fn load(project_dir: &Path, campaign_id: &str) -> Result<Self, StateError> {
        let path = Self::path(project_dir, campaign_id);
        if !path.exists() {
            return Ok(Self {
                version: 1,
                entries: Vec::new(),
            });
        }
        let bytes = read_private(&path)?;
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| StateError::Invalid)?;
        if value.version != 1 {
            return Err(StateError::Invalid);
        }
        Ok(value)
    }

    /// Persists the ledger.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when persistence fails.
    pub fn save(&self, project_dir: &Path, campaign_id: &str) -> Result<(), StateError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| StateError::Invalid)?;
        write_private(&Self::path(project_dir, campaign_id), &bytes)
    }

    /// Pending entry, if any.
    #[must_use]
    pub fn pending(&self) -> Option<&ControlEntry> {
        self.entries.iter().find(|entry| entry.result.is_none())
    }

    /// Chooses the request identity for an action: a retryable earlier identity for the same
    /// action and authority, or a fresh one.
    ///
    /// # Errors
    ///
    /// Returns [`ControlRefusal::Pending`] while another request has no outcome.
    pub fn begin(
        &mut self,
        action: ControlAction,
        authority_sha256: &str,
    ) -> Result<String, ControlRefusal> {
        if let Some(pending) = self.pending() {
            return Err(ControlRefusal::Pending(pending.request_id.clone()));
        }
        let retryable = self.entries.iter_mut().rev().find(|entry| {
            entry.action == action
                && entry.authority_sha256 == authority_sha256
                && entry.result.as_ref().is_some_and(|result| {
                    !result.created
                        && result.code.as_deref().is_some_and(|code| {
                            matches!(
                                code,
                                "codingmage.ui.timeout"
                                    | "codingmage.ui.spawn"
                                    | "codingmage.ui.cancelled"
                                    | "codingmage.ui.outcome_unknown"
                            )
                        })
                })
        });
        if let Some(entry) = retryable {
            entry.attempts += 1;
            entry.result = None;
            return Ok(entry.request_id.clone());
        }
        let request_id = format!("ui-{}-{}", action.code().replace('_', "-"), fresh_suffix());
        self.entries.push(ControlEntry {
            request_id: request_id.clone(),
            action,
            authority_sha256: authority_sha256.to_owned(),
            requested_at_ms: now_ms(),
            attempts: 1,
            result: None,
        });
        Ok(request_id)
    }

    /// Marks every pending entry as having an unknown outcome so it can be replayed with the
    /// same identity; used when the interface reconnects after losing the response.
    pub fn mark_pending_unknown(&mut self) {
        for entry in &mut self.entries {
            if entry.result.is_none() {
                entry.result = Some(ControlResult {
                    created: false,
                    code: Some("codingmage.ui.outcome_unknown".to_owned()),
                    observed_at_ms: now_ms(),
                });
            }
        }
    }

    /// Records the outcome of a request.
    pub fn finish(&mut self, request_id: &str, created: bool, code: Option<String>) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.request_id == request_id)
        {
            entry.result = Some(ControlResult {
                created,
                code,
                observed_at_ms: now_ms(),
            });
        }
    }
}

fn fresh_suffix() -> String {
    use sha2::{Digest as _, Sha256};
    let seed = format!(
        "{}-{}-{:?}",
        now_ms(),
        std::process::id(),
        std::time::Instant::now()
    );
    crate::project::hex(&Sha256::digest(seed.as_bytes()))[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_blocks_pending_requests_and_replays_retryable_identities() {
        let mut ledger = ControlLedger::default();
        let first = ledger.begin(ControlAction::Pause, "a").unwrap();
        assert!(first.starts_with("ui-pause-"));
        assert!(matches!(
            ledger.begin(ControlAction::Resume, "a"),
            Err(ControlRefusal::Pending(_))
        ));
        ledger.finish(&first, false, Some("codingmage.ui.timeout".to_owned()));
        let retried = ledger.begin(ControlAction::Pause, "a").unwrap();
        assert_eq!(retried, first);
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].attempts, 2);
        ledger.finish(&first, true, None);
        let fresh = ledger.begin(ControlAction::Pause, "a").unwrap();
        assert_ne!(fresh, first);
        ledger.finish(&fresh, false, Some("codingmage.runtime.state".to_owned()));
        let after_terminal = ledger.begin(ControlAction::Pause, "a").unwrap();
        assert_ne!(after_terminal, fresh, "terminal failures are not replayed");
        ledger.finish(&after_terminal, true, None);
        let lost = ledger.begin(ControlAction::Resume, "a").unwrap();
        ledger.mark_pending_unknown();
        assert!(ledger.pending().is_none());
        assert_eq!(ledger.begin(ControlAction::Resume, "a").unwrap(), lost);
        ledger.finish(&lost, false, None);
        let root =
            std::env::temp_dir().join(format!("codingmage-ui-controls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        ledger.save(&root, "c").unwrap();
        assert_eq!(ControlLedger::load(&root, "c").unwrap(), ledger);
        std::fs::remove_dir_all(root).unwrap();
    }
}
