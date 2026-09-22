//! Readiness checks evaluated locally before `campaign-preflight`, and the preflight observation.
//!
//! The local checks mirror conditions the coordinator's preflight enforces but reports only as
//! stable codes. They are presentation: preflight remains the authority and always runs before
//! admission.

use std::{fs, path::Path};

use codingmage_campaign::{CampaignAuthentication, CampaignPublication, CampaignSpec};
use codingmage_core::{Config, PublicationMode};
use sha2::{Digest as _, Sha256};

use crate::{
    backend::models::{Diagnosis, PreflightReport},
    project::{file_sha256, hex},
    setup::MAX_AUTHORIZATION_BYTES,
    workplan::PlanCounts,
};

/// Open sub-tasks the coordinator's preflight requires.
pub const REQUIRED_OPEN_SUBTASKS: usize = 10;
/// Accepted-outcome ceiling the coordinator's preflight requires.
pub const REQUIRED_MAX_UNITS: u32 = 10;

/// Outcome of one local check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    /// The condition holds.
    Pass,
    /// The condition does not hold.
    Fail,
    /// The condition could not be evaluated locally.
    Unknown,
}

impl CheckStatus {
    /// Display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
        }
    }
}

/// One local readiness check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Check {
    /// Short name.
    pub name: &'static str,
    /// Outcome.
    pub status: CheckStatus,
    /// Observed detail.
    pub detail: String,
    /// What to do when it fails.
    pub action: &'static str,
}

/// Inputs to the local checks.
#[derive(Clone, Debug)]
pub struct ReadinessInput<'a> {
    /// Opened configuration.
    pub config: &'a Config,
    /// Selected campaign.
    pub spec: &'a CampaignSpec,
    /// Latest diagnosis, when observed.
    pub diagnosis: Option<&'a Diagnosis>,
    /// Plan counts from the active checkout, when the source parsed.
    pub plan_counts: Option<&'a PlanCounts>,
    /// Authorization record path selected by the owner, when known.
    pub authorization_record: Option<&'a Path>,
}

/// Evaluates every local check.
#[must_use]
pub fn evaluate(input: &ReadinessInput<'_>) -> Vec<Check> {
    let mut checks = Vec::new();
    let spec = input.spec;
    let config = input.config;
    diagnosis_checks(input, &mut checks);
    checks.push(check(
        "default branch protected",
        spec.protected_branches
            .iter()
            .any(|protected| protected == &config.default_branch),
        format!(
            "default {} in protected {}",
            config.default_branch,
            spec.protected_branches.join(", ")
        ),
        "Add the default branch to the campaign's protected branches.",
    ));
    match input.plan_counts {
        Some(counts) => checks.push(check(
            "open sub-tasks",
            counts.open_subtasks >= REQUIRED_OPEN_SUBTASKS,
            format!(
                "{} open sub-tasks (preflight requires at least {REQUIRED_OPEN_SUBTASKS})",
                counts.open_subtasks
            ),
            "Add open sub-tasks to the task source; the controlled-target boundary requires ten.",
        )),
        None => checks.push(Check {
            name: "open sub-tasks",
            status: CheckStatus::Unknown,
            detail: "task source did not parse".to_owned(),
            action: "Fix the task source first.",
        }),
    }
    checks.push(check(
        "controlled-target authority",
        spec.max_parallel_pods == 1
            && spec.max_units == REQUIRED_MAX_UNITS
            && spec.publication == CampaignPublication::LocalOnly
            && spec.implementer_authentication == CampaignAuthentication::ExistingLogin
            && spec.multi_agent.is_none(),
        format!(
            "pods {}, accepted-outcome ceiling {}, publication {:?}, authentication {:?}, multi-agent policy {}",
            spec.max_parallel_pods,
            spec.max_units,
            spec.publication,
            spec.implementer_authentication,
            if spec.multi_agent.is_some() { "present" } else { "absent" }
        ),
        "Preflight admits one pod, exactly ten accepted outcomes, local-only publication and existing-login authentication.",
    ));
    file_checks(input, &mut checks);
    checks
}

fn diagnosis_checks(input: &ReadinessInput<'_>, checks: &mut Vec<Check>) {
    let spec = input.spec;
    let config = input.config;
    match input.diagnosis {
        Some(diagnosis) => {
            checks.push(check(
                "repository identity",
                diagnosis.repository_id == spec.repository_id,
                format!(
                    "campaign {} versus observed {}",
                    spec.repository_id, diagnosis.repository_id
                ),
                "Author the campaign for the opened repository.",
            ));
            checks.push(check(
                "initial commit",
                diagnosis.head == spec.initial_commit,
                format!(
                    "campaign {} versus checkout head {}",
                    short(&spec.initial_commit),
                    short(&diagnosis.head)
                ),
                "Check out the campaign's initial commit or author a new campaign at the current head.",
            ));
            checks.push(check(
                "task-source digest",
                diagnosis.task_source_sha256 == spec.task_source_sha256,
                format!(
                    "campaign {} versus checkout {}",
                    short(&spec.task_source_sha256),
                    short(&diagnosis.task_source_sha256)
                ),
                "Restore the task source bound by the campaign or author a new campaign.",
            ));
            checks.push(check(
                "clean checkout",
                diagnosis.clean && !diagnosis.unsafe_checkout_features,
                format!(
                    "clean: {}, unsafe checkout features: {}",
                    diagnosis.clean, diagnosis.unsafe_checkout_features
                ),
                "Commit or stash user changes and remove unsupported checkout features.",
            ));
            let branch = diagnosis.branch.as_deref();
            let dedicated = branch.is_some_and(|branch| {
                branch != config.default_branch
                    && !spec
                        .protected_branches
                        .iter()
                        .any(|protected| protected == branch)
            });
            checks.push(check(
                "dedicated branch",
                dedicated,
                format!(
                    "checked out {} (default {}, protected {})",
                    branch.unwrap_or("detached head"),
                    config.default_branch,
                    spec.protected_branches.join(", ")
                ),
                "Check out a dedicated non-protected branch for the campaign.",
            ));
            checks.push(check(
                "external capabilities denied",
                diagnosis.configuration.capabilities.all_denied()
                    && config.publication.mode == PublicationMode::LocalOnly,
                format!("publication {:?}", config.publication.mode),
                "Preflight admits only local-only configurations with every external capability denied.",
            ));
        }
        None => checks.push(Check {
            name: "repository diagnosis",
            status: CheckStatus::Unknown,
            detail: "no live diagnosis".to_owned(),
            action: "Refresh the overview so the coordinator can observe the repository.",
        }),
    }
}

fn file_checks(input: &ReadinessInput<'_>, checks: &mut Vec<Check>) {
    let spec = input.spec;
    let config = input.config;
    for (role, provider) in [
        ("team lead", &spec.team_lead),
        ("implementer", &spec.implementer),
        ("reviewer", &spec.reviewer),
    ] {
        let present =
            fs::symlink_metadata(&provider.executable).is_ok_and(|metadata| metadata.is_file());
        checks.push(Check {
            name: match role {
                "team lead" => "team lead executable",
                "implementer" => "implementer executable",
                _ => "reviewer executable",
            },
            status: if present {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            detail: format!(
                "{} with model {} at effort {}",
                provider.executable.display(),
                provider.model,
                provider.effort
            ),
            action: "Install the provider CLI at the configured absolute path; the interface never substitutes another executable or model.",
        });
    }
    checks.push(match input.authorization_record {
        Some(record) => {
            let inside = record.starts_with(&config.target_path);
            match file_sha256(record, MAX_AUTHORIZATION_BYTES) {
                Some(digest) if digest == spec.operator_authorization_sha256 && !inside => Check {
                    name: "authorization record",
                    status: CheckStatus::Pass,
                    detail: format!("{} matches the bound digest", record.display()),
                    action: "",
                },
                Some(_) if inside => Check {
                    name: "authorization record",
                    status: CheckStatus::Fail,
                    detail: format!("{} is inside the repository", record.display()),
                    action: "Keep the authorization record outside the target repository.",
                },
                Some(_) => Check {
                    name: "authorization record",
                    status: CheckStatus::Fail,
                    detail: format!("{} does not match the bound digest", record.display()),
                    action: "Select the exact record the campaign was authored against, or re-author the campaign.",
                },
                None => Check {
                    name: "authorization record",
                    status: CheckStatus::Fail,
                    detail: format!("{} is missing or unreadable", record.display()),
                    action: "Write the owner's authorization record in Setup.",
                },
            }
        }
        None => Check {
            name: "authorization record",
            status: CheckStatus::Unknown,
            detail: "no record selected".to_owned(),
            action: "Select or write the owner's authorization record in Setup.",
        },
    });
}

fn check(name: &'static str, pass: bool, detail: String, action: &'static str) -> Check {
    Check {
        name,
        status: if pass {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail,
        action,
    }
}

fn short(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

/// A preflight report with the digest of its exact bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreflightObservation {
    /// Parsed report.
    pub report: PreflightReport,
    /// SHA-256 of the exact bytes the coordinator wrote.
    pub report_sha256: String,
    /// Exact bytes, retained for export and acknowledgment.
    pub bytes: Vec<u8>,
}

impl PreflightObservation {
    /// Wraps parsed output with its digest.
    #[must_use]
    pub fn new(report: PreflightReport, bytes: Vec<u8>) -> Self {
        Self {
            report_sha256: hex(&Sha256::digest(&bytes)),
            report,
            bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_digest_helpers_are_stable() {
        assert_eq!(short("abcdefghijklmnop"), "abcdefghijkl");
        assert_eq!(short("abc"), "abc");
    }
}
