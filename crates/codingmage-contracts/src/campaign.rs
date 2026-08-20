use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Model-proposed pod risk; deterministic policy may only escalate it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PodRisk {
    /// Bounded implementation with no shared security or architecture contract.
    Routine,
    /// Shared contract, architecture, security, concurrency, or publication-sensitive work.
    High,
}

/// Untrusted model-authored proposal before deterministic sealing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamLeadProposal {
    /// Exact dependency-ready task identifier.
    pub task_id: String,
    /// Dependencies claimed from the canonical plan.
    pub dependencies: Vec<String>,
    /// Exact requested write roots.
    pub owned_paths: Vec<PathBuf>,
    /// Required operator-defined gate tiers.
    pub gate_tiers: Vec<String>,
    /// Shared test resources that must be leased exclusively.
    pub test_resources: Vec<String>,
    /// Expected outputs under the requested write roots.
    pub expected_artifacts: Vec<PathBuf>,
    /// Proposed risk, subject to deterministic escalation.
    pub risk: PodRisk,
    /// Concise inspectable summary; never used as authority.
    pub rationale_summary: String,
}

/// Bounded request for an operator decision when no proposal is independently safe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionBlocker {
    /// Stable content-free reason code.
    pub code: String,
    /// Concise inspectable question with no hidden reasoning.
    pub summary: String,
}

/// Strict read-only campaign-lead response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamLeadReport {
    /// Exact starting commit observed by the lead.
    pub campaign_head: String,
    /// Exact canonical task source observed by the lead.
    pub task_source_sha256: String,
    /// Bounded proposals selected only from the supplied ready set.
    pub proposals: Vec<TeamLeadProposal>,
    /// Present only when no proposal can be made without an operator decision.
    pub human_decision: Option<HumanDecisionBlocker>,
}
