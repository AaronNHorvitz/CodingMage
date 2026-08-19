//! Structured finding lifecycle, correction packets, and independent-review policy.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use codingmage_contracts::AgentId;
use serde::{Deserialize, Serialize};

/// Finding classification that keeps suggestions outside mandatory scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// Verified implementation defect.
    Defect,
    /// External prerequisite.
    ExternalBlocker,
    /// Optional nonblocking improvement.
    Suggestion,
}

/// Durable finding state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Newly validated finding.
    Open,
    /// Implementer accepted correction responsibility.
    Accepted,
    /// Correction commit claims to address it.
    Corrected,
    /// Independent verification passed.
    Verified,
    /// Reviewer and implementer disagree.
    Disputed,
    /// Reviewer withdrew it.
    Withdrawn,
    /// External prerequisite prevents action.
    Blocked,
}

/// One finding bound to the exact reviewed commit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// Stable provider-independent identifier.
    pub id: String,
    /// Exact reviewed commit.
    pub reviewed_commit: String,
    /// Defect, blocker, or suggestion.
    pub kind: FindingKind,
    /// Current lifecycle state.
    pub state: FindingState,
    /// Exact correction commit when corrected.
    pub correction_commit: Option<String>,
    /// Whether relevant code or test evidence changed.
    pub relevant_change: bool,
    /// Stable explanation code when verification needs no relevant change.
    pub no_change_reason: Option<String>,
}

/// Exact collection of deduplicated findings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FindingLedger {
    findings: BTreeMap<(String, String), Finding>,
}

impl FindingLedger {
    /// Registers one open finding, deduplicated by reviewed commit and stable ID.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError`] for malformed or duplicate findings.
    pub fn register(&mut self, finding: Finding) -> Result<(), ReviewError> {
        if !valid_id(&finding.id)
            || !valid_commit(&finding.reviewed_commit)
            || finding.state != FindingState::Open
            || finding.correction_commit.is_some()
            || finding.relevant_change
            || finding.no_change_reason.is_some()
        {
            return Err(ReviewError::InvalidFinding);
        }
        let key = (finding.reviewed_commit.clone(), finding.id.clone());
        if self.findings.insert(key, finding).is_some() {
            return Err(ReviewError::DuplicateFinding);
        }
        Ok(())
    }

    /// Applies one legal finding transition.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError`] for missing findings, illegal transitions, stale commits, or
    /// unsupported verification claims.
    pub fn transition(
        &mut self,
        reviewed_commit: &str,
        finding_id: &str,
        next: FindingState,
        correction_commit: Option<&str>,
        relevant_change: bool,
        no_change_reason: Option<&str>,
    ) -> Result<(), ReviewError> {
        let finding = self
            .findings
            .get_mut(&(reviewed_commit.to_owned(), finding_id.to_owned()))
            .ok_or(ReviewError::MissingFinding)?;
        if !legal_finding_transition(finding.state, next) {
            return Err(ReviewError::InvalidTransition);
        }
        if next == FindingState::Corrected {
            let commit = correction_commit.ok_or(ReviewError::InvalidCorrection)?;
            if !valid_commit(commit) || commit == reviewed_commit {
                return Err(ReviewError::InvalidCorrection);
            }
            finding.correction_commit = Some(commit.to_owned());
            finding.relevant_change = relevant_change;
            finding.no_change_reason = no_change_reason.map(str::to_owned);
        } else if correction_commit.is_some() || relevant_change || no_change_reason.is_some() {
            return Err(ReviewError::InvalidCorrection);
        }
        if next == FindingState::Verified
            && !finding.relevant_change
            && !finding.no_change_reason.as_deref().is_some_and(valid_id)
        {
            return Err(ReviewError::UnprovenVerification);
        }
        finding.state = next;
        Ok(())
    }

    /// Returns one exact finding.
    #[must_use]
    pub fn get(&self, reviewed_commit: &str, finding_id: &str) -> Option<&Finding> {
        self.findings
            .get(&(reviewed_commit.to_owned(), finding_id.to_owned()))
    }

    /// Builds a mandatory correction packet from accepted defects only.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError`] if IDs are missing, stale, duplicated, optional, or not accepted.
    pub fn correction_packet(
        &self,
        reviewed_commit: &str,
        expected_correction_base: &str,
        finding_ids: &[String],
        unchanged_scope_sha256: &str,
        requested_tests: Vec<Vec<String>>,
    ) -> Result<CorrectionPacket, ReviewError> {
        if !valid_commit(reviewed_commit)
            || !valid_commit(expected_correction_base)
            || reviewed_commit != expected_correction_base
            || !valid_sha256(unchanged_scope_sha256)
            || finding_ids.is_empty()
            || requested_tests.is_empty()
        {
            return Err(ReviewError::InvalidCorrection);
        }
        let mut unique = BTreeSet::new();
        for id in finding_ids {
            let finding = self
                .get(reviewed_commit, id)
                .ok_or(ReviewError::MissingFinding)?;
            if finding.kind != FindingKind::Defect || finding.state != FindingState::Accepted {
                return Err(ReviewError::InvalidCorrection);
            }
            if !unique.insert(id.clone()) {
                return Err(ReviewError::DuplicateFinding);
            }
        }
        Ok(CorrectionPacket {
            reviewed_commit: reviewed_commit.to_owned(),
            correction_base: expected_correction_base.to_owned(),
            finding_ids: unique.into_iter().collect(),
            unchanged_scope_sha256: unchanged_scope_sha256.to_owned(),
            requested_tests,
        })
    }
}

/// Bounded correction request retaining unchanged task scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionPacket {
    /// Exact reviewed commit.
    pub reviewed_commit: String,
    /// Exact correction base.
    pub correction_base: String,
    /// Accepted mandatory finding IDs.
    pub finding_ids: Vec<String>,
    /// Hash of unchanged canonical task scope.
    pub unchanged_scope_sha256: String,
    /// Literal requested test vectors.
    pub requested_tests: Vec<Vec<String>>,
}

/// Decision after one correction round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundDecision {
    /// Another bounded correction may begin.
    Continue,
    /// Implementation or review profile must escalate before continuing.
    Escalate,
    /// Bound was reached; autonomous work stops in dispute.
    Dispute,
}

/// Correction-loop budget with a default maximum of three rounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrectionBudget {
    maximum_rounds: u8,
    used_rounds: u8,
}

impl Default for CorrectionBudget {
    fn default() -> Self {
        Self {
            maximum_rounds: 3,
            used_rounds: 0,
        }
    }
}

impl CorrectionBudget {
    /// Creates an explicitly bounded policy.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError`] unless the bound is between one and three.
    pub fn new(maximum_rounds: u8) -> Result<Self, ReviewError> {
        if !(1..=3).contains(&maximum_rounds) {
            return Err(ReviewError::InvalidBudget);
        }
        Ok(Self {
            maximum_rounds,
            used_rounds: 0,
        })
    }

    /// Consumes one failed review/correction round.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError::BudgetExhausted`] if called after the dispute boundary.
    pub fn consume_failed_round(&mut self) -> Result<RoundDecision, ReviewError> {
        if self.used_rounds >= self.maximum_rounds {
            return Err(ReviewError::BudgetExhausted);
        }
        self.used_rounds += 1;
        Ok(if self.used_rounds >= self.maximum_rounds {
            RoundDecision::Dispute
        } else if self.used_rounds + 1 == self.maximum_rounds {
            RoundDecision::Escalate
        } else {
            RoundDecision::Continue
        })
    }
}

/// Authorship and final-review binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndependentReview {
    /// Agent that authored the material correction.
    pub author: AgentId,
    /// Agent selected for final review.
    pub reviewer: AgentId,
    /// Whether Codex performed an emergency correction.
    pub codex_emergency_correction: bool,
    /// Whether a human supplies the independent review.
    pub human_reviewer: bool,
}

impl IndependentReview {
    /// Enforces independent final review.
    ///
    /// # Errors
    ///
    /// Returns [`ReviewError::SelfReview`] for sole self-review or an unreviewed Codex correction.
    pub fn validate(&self) -> Result<(), ReviewError> {
        if self.author == self.reviewer && !self.human_reviewer {
            return Err(ReviewError::SelfReview);
        }
        if self.codex_emergency_correction
            && !self.human_reviewer
            && !self.reviewer.as_str().starts_with("claude-")
        {
            return Err(ReviewError::SelfReview);
        }
        Ok(())
    }
}

/// Content-free review lifecycle failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewError {
    /// Finding shape or initial state is invalid.
    InvalidFinding,
    /// Finding identity already exists for the reviewed commit.
    DuplicateFinding,
    /// Finding does not exist at the exact commit.
    MissingFinding,
    /// Requested lifecycle transition is illegal.
    InvalidTransition,
    /// Correction commit, scope, IDs, or tests are invalid.
    InvalidCorrection,
    /// Verification lacks a relevant change or valid explanation.
    UnprovenVerification,
    /// Correction-round configuration is invalid.
    InvalidBudget,
    /// Correction budget already ended in dispute.
    BudgetExhausted,
    /// Author would be the sole final reviewer.
    SelfReview,
}

impl fmt::Display for ReviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFinding => "codingmage.review.invalid_finding",
            Self::DuplicateFinding => "codingmage.review.duplicate_finding",
            Self::MissingFinding => "codingmage.review.missing_finding",
            Self::InvalidTransition => "codingmage.review.invalid_transition",
            Self::InvalidCorrection => "codingmage.review.invalid_correction",
            Self::UnprovenVerification => "codingmage.review.unproven_verification",
            Self::InvalidBudget => "codingmage.review.invalid_budget",
            Self::BudgetExhausted => "codingmage.review.budget_exhausted",
            Self::SelfReview => "codingmage.review.self_review",
        })
    }
}

impl std::error::Error for ReviewError {}

const fn legal_finding_transition(from: FindingState, to: FindingState) -> bool {
    matches!(
        (from, to),
        (
            FindingState::Open,
            FindingState::Accepted
                | FindingState::Disputed
                | FindingState::Withdrawn
                | FindingState::Blocked
        ) | (
            FindingState::Accepted,
            FindingState::Corrected | FindingState::Disputed | FindingState::Blocked
        ) | (
            FindingState::Corrected,
            FindingState::Verified | FindingState::Disputed
        ) | (
            FindingState::Disputed,
            FindingState::Accepted | FindingState::Withdrawn | FindingState::Blocked
        )
    )
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, kind: FindingKind) -> Finding {
        Finding {
            id: id.to_owned(),
            reviewed_commit: "a".repeat(40),
            kind,
            state: FindingState::Open,
            correction_commit: None,
            relevant_change: false,
            no_change_reason: None,
        }
    }

    #[test]
    fn lifecycle_deduplicates_and_requires_proven_verification() {
        let mut ledger = FindingLedger::default();
        ledger
            .register(finding("F-1", FindingKind::Defect))
            .unwrap();
        assert_eq!(
            ledger.register(finding("F-1", FindingKind::Defect)),
            Err(ReviewError::DuplicateFinding)
        );
        ledger
            .transition(
                &"a".repeat(40),
                "F-1",
                FindingState::Accepted,
                None,
                false,
                None,
            )
            .unwrap();
        ledger
            .transition(
                &"a".repeat(40),
                "F-1",
                FindingState::Corrected,
                Some(&"b".repeat(40)),
                false,
                None,
            )
            .unwrap();
        assert_eq!(
            ledger.transition(
                &"a".repeat(40),
                "F-1",
                FindingState::Verified,
                None,
                false,
                None
            ),
            Err(ReviewError::UnprovenVerification)
        );
    }

    #[test]
    fn relevant_change_or_valid_explanation_allows_verification() {
        for (id, changed, reason) in [
            ("F-1", true, None),
            ("F-2", false, Some("false-positive-proven")),
        ] {
            let mut ledger = FindingLedger::default();
            ledger.register(finding(id, FindingKind::Defect)).unwrap();
            ledger
                .transition(
                    &"a".repeat(40),
                    id,
                    FindingState::Accepted,
                    None,
                    false,
                    None,
                )
                .unwrap();
            ledger
                .transition(
                    &"a".repeat(40),
                    id,
                    FindingState::Corrected,
                    Some(&"b".repeat(40)),
                    changed,
                    reason,
                )
                .unwrap();
            ledger
                .transition(
                    &"a".repeat(40),
                    id,
                    FindingState::Verified,
                    None,
                    false,
                    None,
                )
                .unwrap();
            assert_eq!(
                ledger.get(&"a".repeat(40), id).unwrap().state,
                FindingState::Verified
            );
        }
    }

    #[test]
    fn correction_packet_excludes_suggestions_and_preserves_scope() {
        let mut ledger = FindingLedger::default();
        ledger
            .register(finding("F-1", FindingKind::Defect))
            .unwrap();
        ledger
            .register(finding("S-1", FindingKind::Suggestion))
            .unwrap();
        ledger
            .transition(
                &"a".repeat(40),
                "F-1",
                FindingState::Accepted,
                None,
                false,
                None,
            )
            .unwrap();
        ledger
            .transition(
                &"a".repeat(40),
                "S-1",
                FindingState::Accepted,
                None,
                false,
                None,
            )
            .unwrap();
        let packet = ledger
            .correction_packet(
                &"a".repeat(40),
                &"a".repeat(40),
                &["F-1".to_owned()],
                &"c".repeat(64),
                vec![vec!["cargo".to_owned(), "test".to_owned()]],
            )
            .unwrap();
        assert_eq!(packet.finding_ids, ["F-1"]);
        assert_eq!(
            ledger.correction_packet(
                &"a".repeat(40),
                &"a".repeat(40),
                &["S-1".to_owned()],
                &"c".repeat(64),
                vec![vec!["cargo".to_owned()]]
            ),
            Err(ReviewError::InvalidCorrection)
        );
    }

    #[test]
    fn default_budget_escalates_then_stops_after_three() {
        let mut budget = CorrectionBudget::default();
        assert_eq!(budget.consume_failed_round(), Ok(RoundDecision::Continue));
        assert_eq!(budget.consume_failed_round(), Ok(RoundDecision::Escalate));
        assert_eq!(budget.consume_failed_round(), Ok(RoundDecision::Dispute));
        assert_eq!(
            budget.consume_failed_round(),
            Err(ReviewError::BudgetExhausted)
        );
    }

    #[test]
    fn material_correction_cannot_be_solely_self_reviewed() {
        let same = AgentId::new("codex-reviewer").unwrap();
        assert_eq!(
            IndependentReview {
                author: same.clone(),
                reviewer: same,
                codex_emergency_correction: false,
                human_reviewer: false
            }
            .validate(),
            Err(ReviewError::SelfReview)
        );
        assert_eq!(
            IndependentReview {
                author: AgentId::new("codex-fixer").unwrap(),
                reviewer: AgentId::new("claude-reviewer").unwrap(),
                codex_emergency_correction: true,
                human_reviewer: false
            }
            .validate(),
            Ok(())
        );
        assert_eq!(
            IndependentReview {
                author: AgentId::new("codex-fixer").unwrap(),
                reviewer: AgentId::new("codex-reviewer").unwrap(),
                codex_emergency_correction: true,
                human_reviewer: false
            }
            .validate(),
            Err(ReviewError::SelfReview)
        );
    }
}
