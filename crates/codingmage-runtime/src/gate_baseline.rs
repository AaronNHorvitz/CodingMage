//! Baseline-bound gate receipts: which candidate gate failures are new and which pre-existed.
//!
//! A gate run that passed for a commit is retained as that commit's baseline. When a later
//! candidate built on that commit runs the same gate registry, every failing gate is classified
//! as `introduced` (the baseline passed it) or `pre_existing` (the baseline failed it too). A
//! candidate without a baseline for its base commit and registry is classified `unknown`; that
//! is an honest gap, never a pass. No baseline is ever computed by running extra processes here.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use codingmage_gate::{GateOutcome, GateRun};
use codingmage_state::IntegrityDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::RuntimeError;

const BASELINE_VERSION: u16 = 1;
const COMPARISON_VERSION: u16 = 1;
const BASELINE_ROOT: &str = "gate-baselines";
const MAX_GATES: usize = 4_096;

/// One gate outcome retained in a baseline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineGate {
    /// Stable gate identity from the registry.
    pub gate_id: String,
    /// Closed outcome code: `passed`, `failed`, `unavailable` or `skipped`.
    pub outcome: String,
    /// Integrity digest of the retained gate evidence.
    pub evidence_sha256: String,
}

/// Gate outcomes observed for one exact commit and gate-registry identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateBaseline {
    /// Closed document version.
    pub version: u16,
    /// Repository the commit belongs to.
    pub repository_id: String,
    /// Exact commit the gates ran against.
    pub source_commit: String,
    /// Digest of the configured gate registry the outcomes belong to.
    pub registry_sha256: String,
    /// When the run finished, in Unix milliseconds.
    pub observed_unix_ms: u64,
    /// Gate outcomes in registry order.
    pub gates: Vec<BaselineGate>,
}

impl GateBaseline {
    /// Builds a baseline from a completed gate run.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the run carries no evidence or an inconsistent commit.
    pub fn from_run(
        repository_id: &str,
        source_commit: &str,
        registry_sha256: &str,
        run: &GateRun,
    ) -> Result<Self, RuntimeError> {
        if run.evidence.is_empty() || run.evidence.len() > MAX_GATES {
            return Err(RuntimeError::State);
        }
        let mut observed_unix_ms = 0;
        let mut gates = Vec::with_capacity(run.evidence.len());
        for evidence in &run.evidence {
            if evidence.source_commit != source_commit {
                return Err(RuntimeError::State);
            }
            observed_unix_ms = observed_unix_ms.max(evidence.ended_unix_ms);
            gates.push(BaselineGate {
                gate_id: evidence.gate_id.clone(),
                outcome: outcome_code(evidence.outcome).to_owned(),
                evidence_sha256: evidence.integrity_sha256.clone(),
            });
        }
        let baseline = Self {
            version: BASELINE_VERSION,
            repository_id: repository_id.to_owned(),
            source_commit: source_commit.to_owned(),
            registry_sha256: registry_sha256.to_owned(),
            observed_unix_ms,
            gates,
        };
        if !baseline.verify() {
            return Err(RuntimeError::State);
        }
        Ok(baseline)
    }

    fn verify(&self) -> bool {
        self.version == BASELINE_VERSION
            && !self.repository_id.is_empty()
            && valid_commit(&self.source_commit)
            && valid_sha256(&self.registry_sha256)
            && !self.gates.is_empty()
            && self.gates.len() <= MAX_GATES
            && self.gates.iter().all(|gate| {
                !gate.gate_id.is_empty()
                    && matches!(
                        gate.outcome.as_str(),
                        "passed" | "failed" | "unavailable" | "skipped"
                    )
                    && valid_sha256(&gate.evidence_sha256)
            })
            && self
                .gates
                .iter()
                .map(|gate| gate.gate_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.gates.len()
    }

    /// True when every retained gate passed, which is the only state worth reusing as a head.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.gates.iter().all(|gate| gate.outcome == "passed")
    }
}

/// Classification of one candidate gate run against the baseline of its base commit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateComparison {
    /// Closed document version.
    pub version: u16,
    /// Repository the commits belong to.
    pub repository_id: String,
    /// Exact base commit the candidate was built on.
    pub base_commit: String,
    /// Exact candidate commit whose gates were classified.
    pub candidate_commit: String,
    /// Digest of the gate registry both runs used.
    pub registry_sha256: String,
    /// `compared` when a baseline existed for the base commit, otherwise `unknown`.
    pub baseline: String,
    /// Evidence digests of the baseline gates, in registry order; empty when unknown.
    pub baseline_evidence: Vec<String>,
    /// Gates that fail on the candidate but passed at the base commit.
    pub introduced: Vec<String>,
    /// Gates that fail on the candidate and already failed at the base commit.
    pub pre_existing: Vec<String>,
    /// Gates that fail on the candidate with no baseline outcome to compare against.
    pub unclassified: Vec<String>,
    /// Gates that pass on the candidate but failed at the base commit.
    pub repaired: Vec<String>,
    /// Integrity digest of every other field, used as the receipt's evidence identity.
    pub integrity_sha256: String,
}

impl GateComparison {
    /// Classifies a candidate gate run against an optional baseline.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the run is empty, names another commit or the
    /// baseline belongs to a different registry or repository.
    pub fn classify(
        repository_id: &str,
        base_commit: &str,
        candidate_commit: &str,
        registry_sha256: &str,
        baseline: Option<&GateBaseline>,
        run: &GateRun,
    ) -> Result<Self, RuntimeError> {
        if run.evidence.is_empty() || run.evidence.len() > MAX_GATES {
            return Err(RuntimeError::State);
        }
        if let Some(baseline) = baseline
            && (baseline.repository_id != repository_id
                || baseline.source_commit != base_commit
                || baseline.registry_sha256 != registry_sha256
                || !baseline.verify())
        {
            return Err(RuntimeError::State);
        }
        let baseline_outcomes: BTreeMap<&str, &str> = baseline
            .map(|baseline| {
                baseline
                    .gates
                    .iter()
                    .map(|gate| (gate.gate_id.as_str(), gate.outcome.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let mut introduced = Vec::new();
        let mut pre_existing = Vec::new();
        let mut unclassified = Vec::new();
        let mut repaired = Vec::new();
        for evidence in &run.evidence {
            if evidence.source_commit != candidate_commit {
                return Err(RuntimeError::State);
            }
            let failed_now = evidence.outcome == GateOutcome::Failed;
            match (failed_now, baseline_outcomes.get(evidence.gate_id.as_str())) {
                (true, Some(&"passed")) => introduced.push(evidence.gate_id.clone()),
                (true, Some(&"failed")) => pre_existing.push(evidence.gate_id.clone()),
                (true, _) => unclassified.push(evidence.gate_id.clone()),
                (false, Some(&"failed")) if evidence.outcome == GateOutcome::Passed => {
                    repaired.push(evidence.gate_id.clone());
                }
                (false, _) => {}
            }
        }
        let mut comparison = Self {
            version: COMPARISON_VERSION,
            repository_id: repository_id.to_owned(),
            base_commit: base_commit.to_owned(),
            candidate_commit: candidate_commit.to_owned(),
            registry_sha256: registry_sha256.to_owned(),
            baseline: if baseline.is_some() {
                "compared"
            } else {
                "unknown"
            }
            .to_owned(),
            baseline_evidence: baseline
                .map(|baseline| {
                    baseline
                        .gates
                        .iter()
                        .map(|gate| gate.evidence_sha256.clone())
                        .collect()
                })
                .unwrap_or_default(),
            introduced,
            pre_existing,
            unclassified,
            repaired,
            integrity_sha256: String::new(),
        };
        comparison.integrity_sha256 = comparison.body_sha256()?;
        Ok(comparison)
    }

    fn body_sha256(&self) -> Result<String, RuntimeError> {
        let mut body = self.clone();
        body.integrity_sha256 = String::new();
        let bytes = serde_json::to_vec(&body).map_err(|_| RuntimeError::State)?;
        Ok(hex(&Sha256::digest(bytes)))
    }

    fn verify(&self) -> bool {
        self.version == COMPARISON_VERSION
            && !self.repository_id.is_empty()
            && valid_commit(&self.base_commit)
            && valid_commit(&self.candidate_commit)
            && valid_sha256(&self.registry_sha256)
            && matches!(self.baseline.as_str(), "compared" | "unknown")
            && (self.baseline == "compared") != self.baseline_evidence.is_empty()
            && self
                .baseline_evidence
                .iter()
                .all(|value| valid_sha256(value))
            && self
                .body_sha256()
                .is_ok_and(|digest| digest == self.integrity_sha256)
    }

    /// True when at least one failing gate is new relative to the base commit.
    #[must_use]
    pub fn introduces_failure(&self) -> bool {
        !self.introduced.is_empty()
    }
}

/// Receipt that one named regression gate was observed failing at the base commit before
/// implementation and passing on the reviewed candidate, both by the coordinator's gate runner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepairReceipt {
    /// Closed document version.
    pub version: u16,
    /// Repository the commits belong to.
    pub repository_id: String,
    /// Exact base commit where the regression was reproduced.
    pub base_commit: String,
    /// Exact candidate commit where the regression gate passed.
    pub candidate_commit: String,
    /// Digest of the gate registry the gate belongs to.
    pub registry_sha256: String,
    /// Stable identity of the regression gate.
    pub regression_gate: String,
    /// Integrity digest of the failing base observation.
    pub base_evidence_sha256: String,
    /// Integrity digest of the passing candidate observation.
    pub candidate_evidence_sha256: String,
    /// Integrity digest of every other field.
    pub integrity_sha256: String,
}

impl RepairReceipt {
    /// Builds a receipt from a failing base observation and a passing candidate observation.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when either observation names another commit or gate,
    /// the base gate did not fail or the candidate gate did not pass.
    pub fn new(
        repository_id: &str,
        registry_sha256: &str,
        regression_gate: &str,
        base: &codingmage_gate::GateEvidence,
        candidate: &codingmage_gate::GateEvidence,
    ) -> Result<Self, RuntimeError> {
        if base.gate_id != regression_gate
            || candidate.gate_id != regression_gate
            || base.outcome != GateOutcome::Failed
            || candidate.outcome != GateOutcome::Passed
            || base.source_commit == candidate.source_commit
        {
            return Err(RuntimeError::State);
        }
        let mut receipt = Self {
            version: COMPARISON_VERSION,
            repository_id: repository_id.to_owned(),
            base_commit: base.source_commit.clone(),
            candidate_commit: candidate.source_commit.clone(),
            registry_sha256: registry_sha256.to_owned(),
            regression_gate: regression_gate.to_owned(),
            base_evidence_sha256: base.integrity_sha256.clone(),
            candidate_evidence_sha256: candidate.integrity_sha256.clone(),
            integrity_sha256: String::new(),
        };
        receipt.integrity_sha256 = receipt.body_sha256()?;
        if !receipt.verify() {
            return Err(RuntimeError::State);
        }
        Ok(receipt)
    }

    fn body_sha256(&self) -> Result<String, RuntimeError> {
        let mut body = self.clone();
        body.integrity_sha256 = String::new();
        let bytes = serde_json::to_vec(&body).map_err(|_| RuntimeError::State)?;
        Ok(hex(&Sha256::digest(bytes)))
    }

    fn verify(&self) -> bool {
        self.version == COMPARISON_VERSION
            && !self.repository_id.is_empty()
            && valid_commit(&self.base_commit)
            && valid_commit(&self.candidate_commit)
            && self.base_commit != self.candidate_commit
            && valid_sha256(&self.registry_sha256)
            && !self.regression_gate.is_empty()
            && valid_sha256(&self.base_evidence_sha256)
            && valid_sha256(&self.candidate_evidence_sha256)
            && self.base_evidence_sha256 != self.candidate_evidence_sha256
            && self
                .body_sha256()
                .is_ok_and(|digest| digest == self.integrity_sha256)
    }
}

/// Durable store of baselines and comparisons beneath the private state root.
pub struct GateBaselineStore {
    root: PathBuf,
}

impl GateBaselineStore {
    /// Opens the store for one repository beneath `state_root`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the private directory cannot be created.
    pub fn open(state_root: &Path, repository_id: &str) -> Result<Self, RuntimeError> {
        if repository_id.is_empty()
            || repository_id
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
        {
            return Err(RuntimeError::State);
        }
        let root = state_root.join(BASELINE_ROOT).join(repository_id);
        crate::private_directory(&root)?;
        Ok(Self { root })
    }

    /// Loads the baseline for one commit and registry, if one was retained.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when a retained document fails verification.
    pub fn baseline(
        &self,
        source_commit: &str,
        registry_sha256: &str,
    ) -> Result<Option<GateBaseline>, RuntimeError> {
        let name = baseline_name(source_commit, registry_sha256);
        if fs::symlink_metadata(self.root.join(&name)).is_err() {
            return Ok(None);
        }
        IntegrityDocument::<GateBaseline>::load(&self.root, &name, |value| {
            value.verify()
                && value.source_commit == source_commit
                && value.registry_sha256 == registry_sha256
        })
        .map(|document| Some(document.payload))
        .map_err(|_| RuntimeError::State)
    }

    /// Retains a baseline; an identical existing document is left untouched.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the document cannot be verified or written.
    pub fn retain_baseline(&self, baseline: &GateBaseline) -> Result<bool, RuntimeError> {
        if let Some(existing) = self.baseline(&baseline.source_commit, &baseline.registry_sha256)?
            && existing.gates == baseline.gates
        {
            return Ok(false);
        }
        IntegrityDocument::write_atomic(
            &self.root,
            &baseline_name(&baseline.source_commit, &baseline.registry_sha256),
            baseline.clone(),
            GateBaseline::verify,
        )
        .map_err(|_| RuntimeError::State)?;
        Ok(true)
    }

    /// Retains a comparison for one candidate commit.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the document cannot be verified or written.
    pub fn retain_comparison(&self, comparison: &GateComparison) -> Result<(), RuntimeError> {
        IntegrityDocument::write_atomic(
            &self.root,
            &comparison_name(&comparison.candidate_commit, &comparison.registry_sha256),
            comparison.clone(),
            GateComparison::verify,
        )
        .map_err(|_| RuntimeError::State)?;
        Ok(())
    }

    /// Loads the comparison retained for one candidate commit, if any.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when a retained document fails verification.
    pub fn comparison(
        &self,
        candidate_commit: &str,
        registry_sha256: &str,
    ) -> Result<Option<GateComparison>, RuntimeError> {
        let name = comparison_name(candidate_commit, registry_sha256);
        if fs::symlink_metadata(self.root.join(&name)).is_err() {
            return Ok(None);
        }
        IntegrityDocument::<GateComparison>::load(&self.root, &name, |value| {
            value.verify()
                && value.candidate_commit == candidate_commit
                && value.registry_sha256 == registry_sha256
        })
        .map(|document| Some(document.payload))
        .map_err(|_| RuntimeError::State)
    }
}

impl GateBaselineStore {
    /// Retains a repair receipt for one candidate commit and regression gate.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when the document cannot be verified or written.
    pub fn retain_repair(&self, receipt: &RepairReceipt) -> Result<(), RuntimeError> {
        IntegrityDocument::write_atomic(
            &self.root,
            &repair_name(&receipt.candidate_commit, &receipt.regression_gate),
            receipt.clone(),
            RepairReceipt::verify,
        )
        .map_err(|_| RuntimeError::State)?;
        Ok(())
    }

    /// Loads the repair receipt retained for one candidate commit and gate, if any.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::State`] when a retained document fails verification.
    pub fn repair(
        &self,
        candidate_commit: &str,
        regression_gate: &str,
    ) -> Result<Option<RepairReceipt>, RuntimeError> {
        let name = repair_name(candidate_commit, regression_gate);
        if fs::symlink_metadata(self.root.join(&name)).is_err() {
            return Ok(None);
        }
        IntegrityDocument::<RepairReceipt>::load(&self.root, &name, |value| {
            value.verify()
                && value.candidate_commit == candidate_commit
                && value.regression_gate == regression_gate
        })
        .map(|document| Some(document.payload))
        .map_err(|_| RuntimeError::State)
    }
}

fn repair_name(candidate_commit: &str, regression_gate: &str) -> String {
    let gate: String = regression_gate
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    format!("repair-{candidate_commit}-{gate}.json")
}

fn baseline_name(source_commit: &str, registry_sha256: &str) -> String {
    format!("baseline-{source_commit}-{}.json", &registry_sha256[..16])
}

fn comparison_name(candidate_commit: &str, registry_sha256: &str) -> String {
    format!(
        "comparison-{candidate_commit}-{}.json",
        &registry_sha256[..16]
    )
}

const fn outcome_code(outcome: GateOutcome) -> &'static str {
    match outcome {
        GateOutcome::Passed => "passed",
        GateOutcome::Failed => "failed",
        GateOutcome::Unavailable => "unavailable",
        GateOutcome::SkippedWithPolicy => "skipped",
    }
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use codingmage_gate::{GateEvidence, GateRequirement, GateTier};

    use super::*;

    fn evidence(gate_id: &str, commit: &str, outcome: GateOutcome, seed: u8) -> GateEvidence {
        GateEvidence {
            version: 1,
            gate_id: gate_id.to_owned(),
            tier: GateTier::Tier2,
            requirement: GateRequirement::Required,
            source_commit: commit.to_owned(),
            started_unix_ms: 10,
            ended_unix_ms: 20,
            outcome,
            reason_code: None,
            definition: None,
            process: None,
            integrity_sha256: format!("{seed:02x}").repeat(32),
        }
    }

    fn run(commit: &str, outcomes: &[(&str, GateOutcome)]) -> GateRun {
        GateRun {
            evidence: outcomes
                .iter()
                .enumerate()
                .map(|(index, (gate_id, outcome))| {
                    evidence(gate_id, commit, *outcome, u8::try_from(index + 1).unwrap())
                })
                .collect(),
            diagnostics: Vec::new(),
            blocked: outcomes
                .iter()
                .any(|(_, outcome)| *outcome == GateOutcome::Failed),
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codingmage-gate-baseline-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CANDIDATE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const REGISTRY: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn failures_are_classified_against_the_base_commit_only() {
        let baseline = GateBaseline::from_run(
            "repo",
            BASE,
            REGISTRY,
            &run(
                BASE,
                &[
                    ("unit", GateOutcome::Passed),
                    ("lint", GateOutcome::Failed),
                    ("docs", GateOutcome::Failed),
                ],
            ),
        )
        .unwrap();
        assert!(!baseline.all_passed());
        let comparison = GateComparison::classify(
            "repo",
            BASE,
            CANDIDATE,
            REGISTRY,
            Some(&baseline),
            &run(
                CANDIDATE,
                &[
                    ("unit", GateOutcome::Failed),
                    ("lint", GateOutcome::Failed),
                    ("docs", GateOutcome::Passed),
                    ("new-gate", GateOutcome::Failed),
                ],
            ),
        )
        .unwrap();
        assert_eq!(comparison.baseline, "compared");
        assert_eq!(comparison.introduced, vec!["unit"]);
        assert_eq!(comparison.pre_existing, vec!["lint"]);
        assert_eq!(comparison.repaired, vec!["docs"]);
        assert_eq!(comparison.unclassified, vec!["new-gate"]);
        assert!(comparison.introduces_failure());
        assert!(comparison.verify());
        assert_eq!(comparison.baseline_evidence.len(), 3);
    }

    #[test]
    fn missing_baseline_is_unknown_and_never_a_pass() {
        let comparison = GateComparison::classify(
            "repo",
            BASE,
            CANDIDATE,
            REGISTRY,
            None,
            &run(CANDIDATE, &[("unit", GateOutcome::Failed)]),
        )
        .unwrap();
        assert_eq!(comparison.baseline, "unknown");
        assert_eq!(comparison.unclassified, vec!["unit"]);
        assert!(comparison.introduced.is_empty());
        assert!(!comparison.introduces_failure());
        assert!(comparison.baseline_evidence.is_empty());
        assert!(comparison.verify());
    }

    #[test]
    fn mismatched_identities_and_empty_runs_are_refused() {
        let baseline = GateBaseline::from_run(
            "repo",
            BASE,
            REGISTRY,
            &run(BASE, &[("unit", GateOutcome::Passed)]),
        )
        .unwrap();
        let candidate = run(CANDIDATE, &[("unit", GateOutcome::Failed)]);
        assert_eq!(
            GateComparison::classify(
                "other",
                BASE,
                CANDIDATE,
                REGISTRY,
                Some(&baseline),
                &candidate
            ),
            Err(RuntimeError::State)
        );
        assert_eq!(
            GateComparison::classify(
                "repo",
                CANDIDATE,
                CANDIDATE,
                REGISTRY,
                Some(&baseline),
                &candidate
            ),
            Err(RuntimeError::State)
        );
        assert_eq!(
            GateComparison::classify("repo", BASE, BASE, REGISTRY, Some(&baseline), &candidate),
            Err(RuntimeError::State),
            "the run names a different commit than the candidate"
        );
        assert_eq!(
            GateBaseline::from_run(
                "repo",
                CANDIDATE,
                REGISTRY,
                &run(BASE, &[("unit", GateOutcome::Passed)])
            ),
            Err(RuntimeError::State)
        );
        assert_eq!(
            GateBaseline::from_run("repo", BASE, REGISTRY, &run(BASE, &[])),
            Err(RuntimeError::State)
        );
        let mut tampered = GateComparison::classify(
            "repo",
            BASE,
            CANDIDATE,
            REGISTRY,
            Some(&baseline),
            &candidate,
        )
        .unwrap();
        tampered.introduced.clear();
        assert!(
            !tampered.verify(),
            "a comparison without its digest is refused"
        );
    }

    #[test]
    fn repair_receipt_requires_failing_base_and_passing_candidate() {
        let base = evidence("regress", BASE, GateOutcome::Failed, 1);
        let candidate = evidence("regress", CANDIDATE, GateOutcome::Passed, 2);
        let receipt = RepairReceipt::new("repo", REGISTRY, "regress", &base, &candidate).unwrap();
        assert!(receipt.verify());
        assert_eq!(receipt.base_commit, BASE);
        assert_eq!(receipt.candidate_commit, CANDIDATE);
        let passing_base = evidence("regress", BASE, GateOutcome::Passed, 1);
        assert_eq!(
            RepairReceipt::new("repo", REGISTRY, "regress", &passing_base, &candidate),
            Err(RuntimeError::State),
            "a base that already passes reproduced nothing"
        );
        let failing_candidate = evidence("regress", CANDIDATE, GateOutcome::Failed, 2);
        assert_eq!(
            RepairReceipt::new("repo", REGISTRY, "regress", &base, &failing_candidate),
            Err(RuntimeError::State)
        );
        let other_gate = evidence("other", CANDIDATE, GateOutcome::Passed, 2);
        assert_eq!(
            RepairReceipt::new("repo", REGISTRY, "regress", &base, &other_gate),
            Err(RuntimeError::State)
        );
        let root = temp_root("repair");
        let store = GateBaselineStore::open(&root, "repo").unwrap();
        assert!(store.repair(CANDIDATE, "regress").unwrap().is_none());
        store.retain_repair(&receipt).unwrap();
        assert_eq!(store.repair(CANDIDATE, "regress").unwrap(), Some(receipt));
        let path = root
            .join(BASELINE_ROOT)
            .join("repo")
            .join(repair_name(CANDIDATE, "regress"));
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace(&"01".repeat(32), &"03".repeat(32))).unwrap();
        assert_eq!(store.repair(CANDIDATE, "regress"), Err(RuntimeError::State));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn store_round_trips_and_refuses_tampered_documents() {
        let root = temp_root("store");
        let store = GateBaselineStore::open(&root, "repo-1").unwrap();
        assert!(store.baseline(BASE, REGISTRY).unwrap().is_none());
        let baseline = GateBaseline::from_run(
            "repo-1",
            BASE,
            REGISTRY,
            &run(BASE, &[("unit", GateOutcome::Passed)]),
        )
        .unwrap();
        assert!(store.retain_baseline(&baseline).unwrap());
        assert!(
            !store.retain_baseline(&baseline).unwrap(),
            "identical baseline is idempotent"
        );
        assert_eq!(
            store.baseline(BASE, REGISTRY).unwrap(),
            Some(baseline.clone())
        );
        let comparison = GateComparison::classify(
            "repo-1",
            BASE,
            CANDIDATE,
            REGISTRY,
            Some(&baseline),
            &run(CANDIDATE, &[("unit", GateOutcome::Passed)]),
        )
        .unwrap();
        store.retain_comparison(&comparison).unwrap();
        assert_eq!(
            store.comparison(CANDIDATE, REGISTRY).unwrap(),
            Some(comparison.clone())
        );
        let path = root
            .join(BASELINE_ROOT)
            .join("repo-1")
            .join(comparison_name(CANDIDATE, REGISTRY));
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace("\"compared\"", "\"unknown\"")).unwrap();
        assert_eq!(
            store.comparison(CANDIDATE, REGISTRY),
            Err(RuntimeError::State)
        );
        assert_eq!(
            GateBaselineStore::open(&root, "../escape").map(|_| ()),
            Err(RuntimeError::State)
        );
        let _ = fs::remove_dir_all(&root);
    }
}
