//! Deterministic engineering-risk classification, routing, and bounded escalation.

use std::{collections::BTreeSet, fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Risk strength selected from content-free engineering signals.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Routine, localized work.
    Routine,
    /// Material change requiring stronger implementation or review.
    Elevated,
    /// Security, architecture, final-gate, or repeatedly failing work.
    Critical,
}

/// Provider role for one decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingRole {
    /// Code or documentation implementation.
    Implementation,
    /// Independent senior review.
    Review,
    /// Mechanical administration without gate authority.
    Administration,
}

/// Named provider family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Claude Code implementation adapter.
    Claude,
    /// Codex review adapter.
    Codex,
    /// Deterministic local code.
    Deterministic,
}

/// Fixed model profile selected by policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelProfile {
    /// Routine Claude implementation.
    ClaudeSonnet,
    /// Elevated Claude implementation.
    ClaudeOpus,
    /// Routine Codex senior review.
    CodexTerraHigh,
    /// Critical Codex senior review.
    CodexSolHigh,
    /// Optional low-authority Codex administration.
    CodexLuna,
    /// No model; deterministic coordinator logic.
    Deterministic,
}

impl ModelProfile {
    const fn provider(self) -> Provider {
        match self {
            Self::ClaudeSonnet | Self::ClaudeOpus => Provider::Claude,
            Self::CodexTerraHigh | Self::CodexSolHigh | Self::CodexLuna => Provider::Codex,
            Self::Deterministic => Provider::Deterministic,
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Deterministic => 0,
            Self::CodexLuna => 1,
            Self::ClaudeSonnet | Self::CodexTerraHigh => 2,
            Self::ClaudeOpus | Self::CodexSolHigh => 3,
        }
    }
}

/// Provider-reported operational observations retained without prompts or source text.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceFeedback {
    /// Elapsed provider time.
    pub elapsed_millis: u64,
    /// Retried provider attempts.
    pub retries: u32,
    /// Correction rounds already used.
    pub correction_count: u32,
    /// Deterministic gate failures.
    pub gate_failures: u32,
    /// Unresolved review findings.
    pub review_findings: u32,
    /// Provider-exposed input units; unknown remains absent.
    pub input_units: Option<u64>,
    /// Provider-exposed output units; unknown remains absent.
    pub output_units: Option<u64>,
}

/// Content-free inputs to deterministic risk classification.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiskInput {
    /// Trusted task labels.
    pub labels: BTreeSet<String>,
    /// Changed relative paths.
    pub changed_paths: Vec<PathBuf>,
    /// Direct dependency breadth.
    pub dependency_count: u32,
    /// Number of changed files.
    pub changed_files: u32,
    /// Added and removed lines.
    pub changed_lines: u64,
    /// Prior provider or correction failures.
    pub prior_failures: u32,
    /// Unresolved finding count.
    pub unresolved_findings: u32,
    /// Signals not recognized by this policy version.
    pub unknown_signals: BTreeSet<String>,
    /// Whether this is a mandatory final story or release gate.
    pub final_gate: bool,
    /// Whether reviewers currently disagree.
    pub disputed: bool,
}

/// Optional operator pin with an auditable content-free reason.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorOverride {
    /// Exact requested profile.
    pub profile: ModelProfile,
    /// Stable reason code.
    pub reason_code: String,
}

/// Explainable immutable routing result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingDecision {
    /// Provider family.
    pub provider: Provider,
    /// Assigned role.
    pub role: RoutingRole,
    /// Requested policy profile.
    pub profile: ModelProfile,
    /// Requested effort label.
    pub effort: String,
    /// Requested speed label.
    pub speed: String,
    /// Deterministic risk level.
    pub risk: RiskLevel,
    /// Stable reason codes in sorted order.
    pub reason_codes: BTreeSet<String>,
    /// Conditions that trigger later escalation.
    pub escalation_conditions: BTreeSet<String>,
    /// Exact provider-resolved model identity when exposed.
    pub resolved_model: Option<String>,
    /// Operator reason when a valid pin was used.
    pub override_reason: Option<String>,
}

impl RoutingDecision {
    /// Records the exact provider-reported model identity.
    ///
    /// # Errors
    ///
    /// Returns [`RoutingError::InvalidResolvedModel`] for empty, excessive, or control-bearing
    /// identities.
    pub fn record_resolved_model(&mut self, identity: &str) -> Result<(), RoutingError> {
        if identity.is_empty() || identity.len() > 256 || identity.chars().any(char::is_control) {
            return Err(RoutingError::InvalidResolvedModel);
        }
        self.resolved_model = Some(identity.to_owned());
        Ok(())
    }
}

/// Deterministic routing policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingPolicy {
    /// Profiles currently available to this run.
    pub available_profiles: BTreeSet<ModelProfile>,
    /// Failure count that raises implementation strength.
    pub failure_escalation_threshold: u32,
    /// Correction count that raises implementation strength.
    pub correction_escalation_threshold: u32,
}

impl RoutingPolicy {
    /// Produces one deterministic route without fallback.
    ///
    /// # Errors
    ///
    /// Returns [`RoutingError`] when policy is invalid, the exact required profile is unavailable,
    /// or an operator override would weaken a mandatory decision.
    pub fn route(
        &self,
        role: RoutingRole,
        input: &RiskInput,
        feedback: &PerformanceFeedback,
        operator_override: Option<&OperatorOverride>,
    ) -> Result<RoutingDecision, RoutingError> {
        if self.failure_escalation_threshold == 0 || self.correction_escalation_threshold == 0 {
            return Err(RoutingError::InvalidPolicy);
        }
        let (risk, mut reasons) = classify(input, feedback, self);
        let required = profile_for(role, risk, input.final_gate || input.disputed);
        let mut selected = required;
        let mut override_reason = None;
        if let Some(operator_override) = operator_override {
            if !valid_reason(&operator_override.reason_code)
                || operator_override.profile.provider() != required.provider()
                || operator_override.profile.rank() < required.rank()
                || (input.final_gate && operator_override.profile != ModelProfile::CodexSolHigh)
            {
                return Err(RoutingError::OverrideDenied);
            }
            selected = operator_override.profile;
            override_reason = Some(operator_override.reason_code.clone());
            reasons.insert("operator-pin".to_owned());
        }
        if !self.available_profiles.contains(&selected) {
            return Err(RoutingError::ProfileUnavailable);
        }
        let (effort, speed) = profile_controls(selected);
        Ok(RoutingDecision {
            provider: selected.provider(),
            role,
            profile: selected,
            effort: effort.to_owned(),
            speed: speed.to_owned(),
            risk,
            reason_codes: reasons,
            escalation_conditions: BTreeSet::from([
                "configured-failure-threshold".to_owned(),
                "configured-correction-threshold".to_owned(),
                "material-review-disagreement".to_owned(),
            ]),
            resolved_model: None,
            override_reason,
        })
    }
}

/// Content-free routing failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingError {
    /// Threshold or availability policy is malformed.
    InvalidPolicy,
    /// Required profile is not available; no fallback occurred.
    ProfileUnavailable,
    /// Operator pin is malformed, cross-provider, or weaker than mandatory policy.
    OverrideDenied,
    /// Provider-resolved model identity is unsafe to retain.
    InvalidResolvedModel,
}

impl fmt::Display for RoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPolicy => "codingmage.routing.invalid_policy",
            Self::ProfileUnavailable => "codingmage.routing.profile_unavailable",
            Self::OverrideDenied => "codingmage.routing.override_denied",
            Self::InvalidResolvedModel => "codingmage.routing.invalid_resolved_model",
        })
    }
}

impl std::error::Error for RoutingError {}

fn classify(
    input: &RiskInput,
    feedback: &PerformanceFeedback,
    policy: &RoutingPolicy,
) -> (RiskLevel, BTreeSet<String>) {
    let mut level = RiskLevel::Routine;
    let mut reasons = BTreeSet::new();
    for path in &input.changed_paths {
        let value = path.to_string_lossy().to_ascii_lowercase();
        if elevated_path(&value) {
            level = RiskLevel::Critical;
            reasons.insert("elevated-path".to_owned());
        }
    }
    if input.labels.iter().any(|label| elevated_label(label)) {
        level = RiskLevel::Critical;
        reasons.insert("elevated-label".to_owned());
    }
    if input.dependency_count > 4 || input.changed_files > 12 || input.changed_lines > 800 {
        level = level.max(RiskLevel::Elevated);
        reasons.insert("broad-diff".to_owned());
    }
    if input.prior_failures >= policy.failure_escalation_threshold
        || feedback.gate_failures >= policy.failure_escalation_threshold
        || feedback.correction_count >= policy.correction_escalation_threshold
    {
        level = RiskLevel::Critical;
        reasons.insert("failure-escalation".to_owned());
    }
    if input.unresolved_findings > 0 || feedback.review_findings > 0 {
        level = level.max(RiskLevel::Elevated);
        reasons.insert("unresolved-findings".to_owned());
    }
    if !input.unknown_signals.is_empty() {
        level = level.max(RiskLevel::Elevated);
        reasons.insert("unknown-signal".to_owned());
    }
    if input.final_gate {
        level = RiskLevel::Critical;
        reasons.insert("final-gate".to_owned());
    }
    if input.disputed {
        level = RiskLevel::Critical;
        reasons.insert("disputed".to_owned());
    }
    if reasons.is_empty() {
        reasons.insert("routine-local-change".to_owned());
    }
    (level, reasons)
}

fn elevated_path(path: &str) -> bool {
    [
        "security",
        "auth",
        "credential",
        "crypto",
        "concurr",
        "process",
        "git",
        "journal",
        "state",
        "persist",
        "platform",
        "package",
        "release",
        "architect",
    ]
    .iter()
    .any(|signal| path.contains(signal))
}

fn elevated_label(label: &str) -> bool {
    matches!(
        label.to_ascii_lowercase().as_str(),
        "security"
            | "authentication"
            | "credentials"
            | "cryptography"
            | "concurrency"
            | "process-control"
            | "git-mutation"
            | "persistence"
            | "cross-platform"
            | "packaging"
            | "release"
            | "architecture"
    )
}

const fn profile_for(role: RoutingRole, risk: RiskLevel, mandatory_review: bool) -> ModelProfile {
    match role {
        RoutingRole::Implementation => match risk {
            RiskLevel::Routine => ModelProfile::ClaudeSonnet,
            RiskLevel::Elevated | RiskLevel::Critical => ModelProfile::ClaudeOpus,
        },
        RoutingRole::Review => {
            if mandatory_review || matches!(risk, RiskLevel::Critical) {
                ModelProfile::CodexSolHigh
            } else {
                ModelProfile::CodexTerraHigh
            }
        }
        RoutingRole::Administration => ModelProfile::Deterministic,
    }
}

const fn profile_controls(profile: ModelProfile) -> (&'static str, &'static str) {
    match profile {
        ModelProfile::ClaudeSonnet
        | ModelProfile::ClaudeOpus
        | ModelProfile::CodexTerraHigh
        | ModelProfile::CodexSolHigh => ("high", "standard"),
        ModelProfile::CodexLuna => ("medium", "fast"),
        ModelProfile::Deterministic => ("none", "local"),
    }
}

fn valid_reason(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RoutingPolicy {
        RoutingPolicy {
            available_profiles: BTreeSet::from([
                ModelProfile::ClaudeSonnet,
                ModelProfile::ClaudeOpus,
                ModelProfile::CodexTerraHigh,
                ModelProfile::CodexSolHigh,
                ModelProfile::CodexLuna,
                ModelProfile::Deterministic,
            ]),
            failure_escalation_threshold: 2,
            correction_escalation_threshold: 2,
        }
    }

    #[test]
    fn fixed_corpus_is_deterministic_and_security_never_routes_weakly() {
        let input = RiskInput {
            changed_paths: vec![PathBuf::from("src/security/auth.rs")],
            ..RiskInput::default()
        };
        let first = policy()
            .route(
                RoutingRole::Implementation,
                &input,
                &PerformanceFeedback::default(),
                None,
            )
            .unwrap();
        let second = policy()
            .route(
                RoutingRole::Implementation,
                &input,
                &PerformanceFeedback::default(),
                None,
            )
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.profile, ModelProfile::ClaudeOpus);
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Review,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .profile,
            ModelProfile::CodexSolHigh
        );
    }

    #[test]
    fn routine_routes_to_sonnet_and_terra_while_admin_is_deterministic() {
        let input = RiskInput::default();
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Implementation,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .profile,
            ModelProfile::ClaudeSonnet
        );
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Review,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .profile,
            ModelProfile::CodexTerraHigh
        );
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Administration,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .profile,
            ModelProfile::Deterministic
        );
    }

    #[test]
    fn unknown_failure_dispute_and_final_gate_only_raise_strength() {
        let mut input = RiskInput {
            unknown_signals: BTreeSet::from(["future-signal".to_owned()]),
            ..RiskInput::default()
        };
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Review,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .risk,
            RiskLevel::Elevated
        );
        input.prior_failures = 2;
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Implementation,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .risk,
            RiskLevel::Critical
        );
        input.final_gate = true;
        assert_eq!(
            policy()
                .route(
                    RoutingRole::Review,
                    &input,
                    &PerformanceFeedback::default(),
                    None
                )
                .unwrap()
                .profile,
            ModelProfile::CodexSolHigh
        );
    }

    #[test]
    fn unavailable_profile_stops_without_fallback() {
        let mut limited = policy();
        limited.available_profiles.remove(&ModelProfile::ClaudeOpus);
        let input = RiskInput {
            labels: BTreeSet::from(["architecture".to_owned()]),
            ..RiskInput::default()
        };
        assert_eq!(
            limited.route(
                RoutingRole::Implementation,
                &input,
                &PerformanceFeedback::default(),
                None
            ),
            Err(RoutingError::ProfileUnavailable)
        );
    }

    #[test]
    fn feedback_escalates_and_usage_unknown_is_preserved() {
        let feedback = PerformanceFeedback {
            correction_count: 2,
            input_units: None,
            output_units: None,
            ..PerformanceFeedback::default()
        };
        let decision = policy()
            .route(
                RoutingRole::Implementation,
                &RiskInput::default(),
                &feedback,
                None,
            )
            .unwrap();
        assert_eq!(decision.profile, ModelProfile::ClaudeOpus);
        assert!(decision.reason_codes.contains("failure-escalation"));
        assert_eq!(feedback.input_units, None);
    }

    #[test]
    fn operator_can_pin_stronger_but_not_weaken_or_cross_provider() {
        let stronger = OperatorOverride {
            profile: ModelProfile::ClaudeOpus,
            reason_code: "operator-security-review".to_owned(),
        };
        let decision = policy()
            .route(
                RoutingRole::Implementation,
                &RiskInput::default(),
                &PerformanceFeedback::default(),
                Some(&stronger),
            )
            .unwrap();
        assert_eq!(
            decision.override_reason.as_deref(),
            Some("operator-security-review")
        );
        let weaker = OperatorOverride {
            profile: ModelProfile::CodexTerraHigh,
            reason_code: "save-capacity".to_owned(),
        };
        let input = RiskInput {
            final_gate: true,
            ..RiskInput::default()
        };
        assert_eq!(
            policy().route(
                RoutingRole::Review,
                &input,
                &PerformanceFeedback::default(),
                Some(&weaker)
            ),
            Err(RoutingError::OverrideDenied)
        );
    }

    #[test]
    fn resolved_model_identity_is_bounded_and_explicit() {
        let mut decision = policy()
            .route(
                RoutingRole::Review,
                &RiskInput::default(),
                &PerformanceFeedback::default(),
                None,
            )
            .unwrap();
        assert_eq!(decision.resolved_model, None);
        decision
            .record_resolved_model("gpt-5.6-terra-2026-08-01")
            .unwrap();
        assert_eq!(
            decision.resolved_model.as_deref(),
            Some("gpt-5.6-terra-2026-08-01")
        );
        assert_eq!(
            decision.record_resolved_model(""),
            Err(RoutingError::InvalidResolvedModel)
        );
    }
}
