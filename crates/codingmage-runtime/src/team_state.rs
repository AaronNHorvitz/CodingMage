//! Durable multi-agent campaign snapshot and intent/observation reconciliation.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use codingmage_campaign::TeamCampaignSnapshot;
use codingmage_contracts::{EvidenceId, RepositoryId, RunId, TaskId};
use codingmage_state::{
    DurableIdentities, EffectClass, EventKind, EventOutcome, IntegrityDocument,
    IntegrityDocumentError, Journal, JournalEvent, RedactedField,
};

const DOCUMENT_NAME: &str = "team-campaign.json";
const INTENT_PHASE: &str = "team_checkpoint_intent";
const OBSERVED_PHASE: &str = "team_checkpoint_observed";

/// Private durable owner for one multi-agent campaign projection.
pub struct TeamStateStore {
    root: PathBuf,
    journal: Journal,
    repository_id: RepositoryId,
    run_id: RunId,
    task_id: TaskId,
}

impl TeamStateStore {
    /// Opens one exact private state root and its single-writer journal.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateStoreError`] for invalid identity, ownership, or journal state.
    pub fn open(
        root: &Path,
        owner_id: &str,
        repository_id: RepositoryId,
        run_id: RunId,
    ) -> Result<Self, TeamStateStoreError> {
        let task_id =
            TaskId::new("campaign-team-state").map_err(|_| TeamStateStoreError::Identity)?;
        let journal = Journal::open(root, owner_id).map_err(|_| TeamStateStoreError::Journal)?;
        Ok(Self {
            root: root.to_path_buf(),
            journal,
            repository_id,
            run_id,
            task_id,
        })
    }

    /// Persists one complete validated snapshot through intent, atomic document, and observation.
    ///
    /// Exact replay of the current snapshot is observational and appends no record.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateStoreError`] for invalid projection, journal, or durable I/O behavior.
    pub fn persist(
        &mut self,
        snapshot: &TeamCampaignSnapshot,
        timestamp_ms: u64,
    ) -> Result<String, TeamStateStoreError> {
        snapshot
            .verify()
            .map_err(|_| TeamStateStoreError::Projection)?;
        let digest = snapshot
            .sha256()
            .map_err(|_| TeamStateStoreError::Projection)?;
        if let Some(current) = self.load_document()? {
            let current_digest = current
                .sha256()
                .map_err(|_| TeamStateStoreError::Projection)?;
            if current_digest == digest && self.last_observed_digest().as_deref() == Some(&digest) {
                return Ok(digest);
            }
        }
        self.journal
            .append(self.event(
                INTENT_PHASE,
                EffectClass::StateChanging,
                &digest,
                timestamp_ms,
            ))
            .map_err(|_| TeamStateStoreError::Journal)?;
        IntegrityDocument::write_atomic(&self.root, DOCUMENT_NAME, snapshot.clone(), |value| {
            value.verify().is_ok()
        })
        .map_err(TeamStateStoreError::Document)?;
        self.journal
            .append(self.event(
                OBSERVED_PHASE,
                EffectClass::Idempotent,
                &digest,
                timestamp_ms.saturating_add(1),
            ))
            .map_err(|_| TeamStateStoreError::Journal)?;
        self.journal
            .write_snapshot()
            .map_err(|_| TeamStateStoreError::Journal)?;
        Ok(digest)
    }

    /// Loads and reconciles the current document against the append-only intent journal.
    ///
    /// A crash after atomic replacement but before observation appends the missing observation once.
    /// A pending intent whose digest does not match the current document remains blocked.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStateStoreError`] for malformed, unbound, or contradictory state.
    pub fn load_reconciled(
        &mut self,
        timestamp_ms: u64,
    ) -> Result<Option<TeamCampaignSnapshot>, TeamStateStoreError> {
        let current = self.load_document()?;
        let Some(snapshot) = current else {
            if self.last_team_event().is_some() {
                return Err(TeamStateStoreError::Recovery);
            }
            return Ok(None);
        };
        let digest = snapshot
            .sha256()
            .map_err(|_| TeamStateStoreError::Projection)?;
        let Some((phase, journal_digest)) = self.last_team_event() else {
            return Err(TeamStateStoreError::Recovery);
        };
        if phase == OBSERVED_PHASE {
            if journal_digest != digest {
                return Err(TeamStateStoreError::Recovery);
            }
            return Ok(Some(snapshot));
        }
        if phase != INTENT_PHASE || journal_digest != digest {
            return Err(TeamStateStoreError::Recovery);
        }
        self.journal
            .append(self.event(
                OBSERVED_PHASE,
                EffectClass::Idempotent,
                &digest,
                timestamp_ms,
            ))
            .map_err(|_| TeamStateStoreError::Journal)?;
        self.journal
            .write_snapshot()
            .map_err(|_| TeamStateStoreError::Journal)?;
        Ok(Some(snapshot))
    }

    fn load_document(&self) -> Result<Option<TeamCampaignSnapshot>, TeamStateStoreError> {
        let path = self.root.join(DOCUMENT_NAME);
        if fs::symlink_metadata(&path).is_err() {
            return Ok(None);
        }
        IntegrityDocument::<TeamCampaignSnapshot>::load(&self.root, DOCUMENT_NAME, |value| {
            value.verify().is_ok()
        })
        .map(|document| Some(document.payload))
        .map_err(TeamStateStoreError::Document)
    }

    fn event(
        &self,
        phase: &str,
        effect: EffectClass,
        digest: &str,
        timestamp_ms: u64,
    ) -> JournalEvent {
        JournalEvent {
            timestamp_ms,
            run_id: self.run_id.clone(),
            task_id: self.task_id.clone(),
            repository_id: self.repository_id.clone(),
            identities: DurableIdentities::default(),
            kind: if phase == INTENT_PHASE {
                EventKind::Transition {
                    phase: phase.to_owned(),
                    effect,
                }
            } else {
                EventKind::EffectObserved {
                    phase: phase.to_owned(),
                }
            },
            outcome: EventOutcome::Succeeded,
            evidence: vec![digest_evidence(digest)],
            redactions: vec![
                RedactedField::new("provider_output").expect("static redaction is valid"),
                RedactedField::new("source_content").expect("static redaction is valid"),
                RedactedField::new("credential").expect("static redaction is valid"),
            ],
        }
    }

    fn last_observed_digest(&self) -> Option<String> {
        self.last_team_event()
            .and_then(|(phase, digest)| (phase == OBSERVED_PHASE).then_some(digest))
    }

    fn last_team_event(&self) -> Option<(&str, String)> {
        self.journal.records().iter().rev().find_map(|record| {
            let phase = match &record.event.kind {
                EventKind::Transition { phase, .. } | EventKind::EffectObserved { phase }
                    if matches!(phase.as_str(), INTENT_PHASE | OBSERVED_PHASE) =>
                {
                    phase.as_str()
                }
                _ => return None,
            };
            let digest = record.event.evidence.first()?.as_str();
            digest
                .strip_prefix("team-state-")
                .map(|value| (phase, value.to_owned()))
        })
    }
}

/// Content-free multi-agent state-store failure.
#[derive(Debug)]
pub enum TeamStateStoreError {
    /// Supplied coordinator identity is invalid.
    Identity,
    /// Complete team projection is invalid.
    Projection,
    /// Append-only journal operation failed.
    Journal,
    /// Atomic integrity document operation failed.
    Document(IntegrityDocumentError),
    /// Journal and atomic document cannot be reconciled safely.
    Recovery,
}

impl fmt::Display for TeamStateStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.runtime.team_state.identity",
            Self::Projection => "codingmage.runtime.team_state.projection",
            Self::Journal => "codingmage.runtime.team_state.journal",
            Self::Document(_) => "codingmage.runtime.team_state.document",
            Self::Recovery => "codingmage.runtime.team_state.recovery",
        })
    }
}

impl std::error::Error for TeamStateStoreError {}

fn digest_evidence(digest: &str) -> EvidenceId {
    EvidenceId::new(format!("team-state-{digest}")).expect("verified digest is a valid evidence id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codingmage_campaign::{
        CampaignConcurrency, DurableSchedulerSnapshot, TEAM_STATE_SCHEMA_VERSION, TaskUtilization,
        TeamCampaignSnapshot, TeamResourcePolicy, TeamResourceSnapshot,
    };
    use std::{
        collections::{BTreeMap, BTreeSet},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "codingmage-team-state-{}-{unique}",
            std::process::id()
        ))
    }

    fn snapshot() -> TeamCampaignSnapshot {
        TeamCampaignSnapshot {
            version: TEAM_STATE_SCHEMA_VERSION,
            campaign_id: "campaign-team".to_owned(),
            generation: 0,
            campaign_head: "a".repeat(40),
            task_source_sha256: "b".repeat(64),
            scheduler: DurableSchedulerSnapshot {
                version: TEAM_STATE_SCHEMA_VERSION,
                campaign_id: "campaign-team".to_owned(),
                max_parallel_pods: 1,
                generation: 0,
                next_sequence: 0,
                active: BTreeMap::new(),
                released: BTreeSet::new(),
                ready_age: BTreeMap::new(),
                follow_up_limit: Some(0),
                follow_up_bindings: BTreeMap::new(),
            },
            resources: TeamResourceSnapshot {
                version: TEAM_STATE_SCHEMA_VERSION,
                concurrency: CampaignConcurrency::default(),
                policy: TeamResourcePolicy::default(),
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                active: BTreeMap::new(),
                released: BTreeSet::new(),
                consumed: TaskUtilization::default(),
                provider_circuits: BTreeMap::new(),
                next_sequence: 0,
            },
            tasks: BTreeMap::new(),
            integration_queue: Vec::new(),
        }
    }

    #[test]
    fn complete_snapshot_persists_replays_and_loads_exactly() {
        let root = root();
        let mut store = TeamStateStore::open(
            &root,
            "owner",
            RepositoryId::new("repo-team").unwrap(),
            RunId::new("run-team").unwrap(),
        )
        .unwrap();
        assert_eq!(store.load_reconciled(1).unwrap(), None);
        let snapshot = snapshot();
        let first = store.persist(&snapshot, 2).unwrap();
        let records = store.journal.records().len();
        assert_eq!(store.persist(&snapshot, 4).unwrap(), first);
        assert_eq!(store.journal.records().len(), records);
        assert_eq!(store.load_reconciled(5).unwrap(), Some(snapshot));
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crash_after_atomic_replace_reconciles_one_observation() {
        let root = root();
        let snapshot = snapshot();
        let digest = snapshot.sha256().unwrap();
        let mut store = TeamStateStore::open(
            &root,
            "owner",
            RepositoryId::new("repo-team").unwrap(),
            RunId::new("run-team").unwrap(),
        )
        .unwrap();
        store
            .journal
            .append(store.event(INTENT_PHASE, EffectClass::StateChanging, &digest, 1))
            .unwrap();
        IntegrityDocument::write_atomic(&root, DOCUMENT_NAME, snapshot.clone(), |value| {
            value.verify().is_ok()
        })
        .unwrap();
        let before = store.journal.records().len();
        assert_eq!(store.load_reconciled(2).unwrap(), Some(snapshot.clone()));
        assert_eq!(store.journal.records().len(), before + 1);
        assert_eq!(store.load_reconciled(3).unwrap(), Some(snapshot));
        assert_eq!(store.journal.records().len(), before + 1);
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unbound_or_mismatched_document_fails_closed() {
        let root = root();
        let snapshot = snapshot();
        IntegrityDocument::write_atomic(&root, DOCUMENT_NAME, snapshot.clone(), |value| {
            value.verify().is_ok()
        })
        .unwrap();
        let mut store = TeamStateStore::open(
            &root,
            "owner",
            RepositoryId::new("repo-team").unwrap(),
            RunId::new("run-team").unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.load_reconciled(1),
            Err(TeamStateStoreError::Recovery)
        ));
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
