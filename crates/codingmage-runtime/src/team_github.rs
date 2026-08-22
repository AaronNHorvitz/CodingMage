//! Guarded production GitHub task-publication adapter.

use std::{collections::BTreeSet, path::PathBuf};

use codingmage_campaign::{CampaignSpec, GitHubCampaignPolicy};
use codingmage_contracts::{EvidenceId, TaskId};
use codingmage_core::{CapabilityGrant, Config, PublicationMode};
use codingmage_github::{GitHubIdentity, TaskIssue, TaskPullRequest};
use codingmage_process::{
    CancellationToken, DescendantCleanup, ProcessExecutor, ProcessOutcome, ProcessProfile,
    ProcessRequest,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    CampaignBranchObservation, CampaignBranchPublicationRequest, CiObservation, IssueObservation,
    PullRequestObservation, PushObservation, TaskCompletionObservation, TaskPublicationRequest,
    TeamPublicationError, TeamPublicationPort, login_discovery_environment, private_directory,
};

const MAX_REMOTE_OUTPUT: u64 = 4 * 1024 * 1024;
const REMOTE_DEADLINE_MS: u64 = 2 * 60 * 1_000;

/// Production publication port backed by exact guarded `gh` and `git` commands.
#[derive(Clone, Debug)]
pub struct GhCliPublicationPort {
    policy: GitHubCampaignPolicy,
    campaign_id: String,
    campaign_branch_prefix: String,
    initial_commit: String,
    target_path: PathBuf,
    executor: ProcessExecutor,
    environment: std::collections::BTreeMap<String, String>,
    cancellation: CancellationToken,
    task_merge_allowed: bool,
}

impl GhCliPublicationPort {
    /// Creates and probes an exact authenticated GitHub publication boundary.
    ///
    /// # Errors
    ///
    /// Returns a content-free authority or transport failure before any write operation.
    pub fn new(
        config: &Config,
        spec: &CampaignSpec,
        codingmage_binary: &std::path::Path,
        private_root: &std::path::Path,
        cancellation: CancellationToken,
    ) -> Result<Self, TeamPublicationError> {
        spec.verify().map_err(|_| TeamPublicationError::Authority)?;
        let policy = spec
            .multi_agent
            .as_ref()
            .and_then(|multi| multi.github.clone())
            .ok_or(TeamPublicationError::Authority)?;
        if config.target_path != spec.repository_path
            || config.publication.mode != PublicationMode::DraftPullRequest
            || config.capabilities.network != CapabilityGrant::Allowed
            || config.capabilities.push != CapabilityGrant::Allowed
            || config.capabilities.issues != CapabilityGrant::Allowed
            || config.capabilities.pull_requests != CapabilityGrant::Allowed
        {
            return Err(TeamPublicationError::Authority);
        }
        private_directory(private_root).map_err(|_| TeamPublicationError::Authority)?;
        let executor = ProcessExecutor::new_with_guard_arguments(
            codingmage_binary,
            vec!["__process-guard".to_owned()],
            &private_root.join("processes"),
        )
        .map_err(|_| TeamPublicationError::Authority)?;
        let environment =
            login_discovery_environment().map_err(|_| TeamPublicationError::Authority)?;
        let mut port = Self {
            policy,
            campaign_id: spec.campaign_id.clone(),
            campaign_branch_prefix: spec.campaign_branch.clone(),
            initial_commit: spec.initial_commit.clone(),
            target_path: config.target_path.clone(),
            executor,
            environment,
            cancellation,
            task_merge_allowed: config.capabilities.task_merge == CapabilityGrant::Allowed,
        };
        port.probe_identity()?;
        port.verify_remote_url()?;
        Ok(port)
    }

    fn probe_identity(&mut self) -> Result<(), TeamPublicationError> {
        let account = self.run_gh(
            vec![
                "api".to_owned(),
                "--hostname".to_owned(),
                self.policy.host.clone(),
                "user".to_owned(),
                "--jq".to_owned(),
                ".login".to_owned(),
            ],
            Vec::new(),
        )?;
        if text(&account)? != self.policy.account {
            return Err(TeamPublicationError::Identity);
        }
        let repository = self.run_gh(
            vec![
                "repo".to_owned(),
                "view".to_owned(),
                self.repository_selector(),
                "--json".to_owned(),
                "nameWithOwner".to_owned(),
                "--jq".to_owned(),
                ".nameWithOwner".to_owned(),
            ],
            Vec::new(),
        )?;
        if text(&repository)? != self.repository_selector() {
            return Err(TeamPublicationError::Identity);
        }
        Ok(())
    }

    fn verify_remote_url(&mut self) -> Result<(), TeamPublicationError> {
        let output = self.run_git(
            vec![
                "remote".to_owned(),
                "get-url".to_owned(),
                "--push".to_owned(),
                self.policy.remote.clone(),
            ],
            Vec::new(),
        )?;
        let (host, owner, repository) = parse_remote_url(&text(&output)?)?;
        if host != self.policy.host
            || owner != self.policy.owner
            || repository != self.policy.repository
        {
            return Err(TeamPublicationError::Identity);
        }
        Ok(())
    }

    fn issue_record(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<Option<GhIssue>, TeamPublicationError> {
        if let Some(number) = request.issue_number {
            let output = self.run_gh(
                vec![
                    "issue".to_owned(),
                    "view".to_owned(),
                    number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--json".to_owned(),
                    "number,body,state".to_owned(),
                ],
                Vec::new(),
            )?;
            return parse_one(&output).map(Some);
        }
        let marker = format!(
            "<!-- codingmage:task:start {}:{} -->",
            request.campaign_id, request.task_id
        );
        let output = self.run_gh(
            vec![
                "issue".to_owned(),
                "list".to_owned(),
                "--repo".to_owned(),
                self.repository_selector(),
                "--state".to_owned(),
                "all".to_owned(),
                "--search".to_owned(),
                format!("\"{marker}\" in:body"),
                "--json".to_owned(),
                "number,body,state".to_owned(),
                "--limit".to_owned(),
                "2".to_owned(),
            ],
            Vec::new(),
        )?;
        unique_record(&output, |issue: &GhIssue| issue.body.contains(&marker))
    }

    fn pull_request_record(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<Option<GhPullRequest>, TeamPublicationError> {
        if let Some(number) = request.pull_request_number {
            let output = self.run_gh(
                vec![
                    "pr".to_owned(),
                    "view".to_owned(),
                    number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--json".to_owned(),
                    "number,body,baseRefName,headRefName,headRefOid,isDraft,state".to_owned(),
                ],
                Vec::new(),
            )?;
            return parse_one(&output).map(Some);
        }
        let output = self.run_gh(
            vec![
                "pr".to_owned(),
                "list".to_owned(),
                "--repo".to_owned(),
                self.repository_selector(),
                "--state".to_owned(),
                "all".to_owned(),
                "--head".to_owned(),
                request.task_branch.clone(),
                "--base".to_owned(),
                request.campaign_branch.clone(),
                "--json".to_owned(),
                "number,body,baseRefName,headRefName,headRefOid,isDraft,state".to_owned(),
                "--limit".to_owned(),
                "2".to_owned(),
            ],
            Vec::new(),
        )?;
        unique_record(&output, |_| true)
    }

    fn render_issue(
        request: &TaskPublicationRequest,
        existing: &str,
    ) -> Result<String, TeamPublicationError> {
        let dependencies = request
            .dependencies
            .iter()
            .cloned()
            .map(TaskId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| TeamPublicationError::Authority)?;
        TaskIssue {
            campaign_id: request.campaign_id.clone(),
            task_id: TaskId::new(request.task_id.clone())
                .map_err(|_| TeamPublicationError::Authority)?,
            parent: None,
            dependencies,
            pod_id: request.pod_id.clone(),
            authorized_paths: request.owned_paths.clone(),
            required_gates: request.gate_tiers.clone(),
            state: request.state.clone(),
            branch: request.task_branch.clone(),
            pull_request: request.pull_request_number,
            candidate_commit: Some(request.reviewed_commit.clone()),
            review_result: Some("pass".to_owned()),
            blocker: None,
            evidence: publication_evidence(request)?,
        }
        .merge_into(existing)
        .map_err(|_| TeamPublicationError::Authority)
    }

    fn render_pull_request(
        &self,
        request: &TaskPublicationRequest,
        existing: &str,
    ) -> Result<String, TeamPublicationError> {
        let issue_number = request
            .issue_number
            .ok_or(TeamPublicationError::Authority)?;
        TaskPullRequest {
            campaign_id: request.campaign_id.clone(),
            task_id: TaskId::new(request.task_id.clone())
                .map_err(|_| TeamPublicationError::Authority)?,
            issue_number,
            base_branch: request.campaign_branch.clone(),
            head_branch: request.task_branch.clone(),
            reviewed_commit: request.reviewed_commit.clone(),
            requirements: vec![request.task_id.clone()],
            tests: gate_evidence(request)?,
            review_result: "pass".to_owned(),
            correction_history: request.correction_sessions.clone(),
            integration_status: request.state.clone(),
            risk_notes: Vec::new(),
        }
        .merge_into(&self.github_identity(request), existing)
        .map_err(|_| TeamPublicationError::Authority)
    }

    fn observe_branch(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<Option<String>, TeamPublicationError> {
        self.observe_named_branch(&request.task_branch)
    }

    fn observe_named_branch(
        &mut self,
        branch: &str,
    ) -> Result<Option<String>, TeamPublicationError> {
        let reference = format!("refs/heads/{branch}");
        let output = self.run_git(
            vec![
                "ls-remote".to_owned(),
                "--heads".to_owned(),
                self.policy.remote.clone(),
                reference.clone(),
            ],
            Vec::new(),
        )?;
        let body = text(&output)?;
        if body.is_empty() {
            return Ok(None);
        }
        let mut lines = body.lines();
        let fields = lines
            .next()
            .ok_or(TeamPublicationError::Identity)?
            .split_whitespace()
            .collect::<Vec<_>>();
        if lines.next().is_some()
            || fields.len() != 2
            || fields[1] != reference
            || !valid_commit(fields[0])
        {
            return Err(TeamPublicationError::Identity);
        }
        Ok(Some(fields[0].to_owned()))
    }

    fn github_identity(&self, request: &TaskPublicationRequest) -> GitHubIdentity {
        GitHubIdentity {
            account: self.policy.account.clone(),
            host: self.policy.host.clone(),
            owner: self.policy.owner.clone(),
            repository: self.policy.repository.clone(),
            branch: request.task_branch.clone(),
            campaign_branch: request.campaign_branch.clone(),
            protected_branch: self.policy.destination_branch.clone(),
        }
    }

    fn repository_selector(&self) -> String {
        format!("{}/{}", self.policy.owner, self.policy.repository)
    }

    fn run_gh(
        &self,
        arguments: Vec<String>,
        stdin: Vec<u8>,
    ) -> Result<Vec<u8>, TeamPublicationError> {
        self.run_process(&self.policy.cli_executable, arguments, stdin)
    }

    fn run_git(
        &self,
        arguments: Vec<String>,
        stdin: Vec<u8>,
    ) -> Result<Vec<u8>, TeamPublicationError> {
        self.run_process(std::path::Path::new("/usr/bin/git"), arguments, stdin)
    }

    fn run_process(
        &self,
        executable: &std::path::Path,
        arguments: Vec<String>,
        stdin: Vec<u8>,
    ) -> Result<Vec<u8>, TeamPublicationError> {
        let profile = ProcessProfile::new(
            executable,
            [arguments.clone()],
            self.environment.keys().cloned(),
        )
        .map_err(|_| TeamPublicationError::Authority)?;
        let result = self
            .executor
            .execute(
                &profile,
                &ProcessRequest {
                    arguments,
                    working_directory: self.target_path.clone(),
                    environment: self.environment.clone(),
                    stdin,
                    max_output_bytes: MAX_REMOTE_OUTPUT,
                    deadline_millis: REMOTE_DEADLINE_MS,
                    max_processes: 16,
                    max_open_files: 256,
                    expected_exit_codes: BTreeSet::from([0]),
                },
                &self.cancellation,
            )
            .map_err(|_| TeamPublicationError::Uncertain)?;
        if result.outcome != ProcessOutcome::Succeeded
            || result.stdout.truncated
            || result.stderr.truncated
            || result.descendant_cleanup == DescendantCleanup::Uncertain
        {
            return Err(TeamPublicationError::Uncertain);
        }
        Ok(result.stdout.retained)
    }

    fn reconcile_completed_issue(
        &mut self,
        request: &TaskPublicationRequest,
        issue_number: u64,
    ) -> Result<GhIssue, TeamPublicationError> {
        let issue = self
            .issue_record(request)?
            .ok_or(TeamPublicationError::Identity)?;
        if issue.number != issue_number {
            return Err(TeamPublicationError::Identity);
        }
        let expected_body = Self::render_issue(request, &issue.body)?;
        if issue.body != expected_body {
            let attempt = self.run_gh(
                vec![
                    "issue".to_owned(),
                    "edit".to_owned(),
                    issue_number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ],
                expected_body.clone().into_bytes(),
            );
            if self
                .issue_record(request)?
                .as_ref()
                .map(|value| &value.body)
                != Some(&expected_body)
            {
                return Err(reconciled_write_error(attempt.is_err()));
            }
        }
        if self
            .issue_record(request)?
            .is_some_and(|value| value.state == "OPEN")
        {
            let attempt = self.run_gh(
                vec![
                    "issue".to_owned(),
                    "close".to_owned(),
                    issue_number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                ],
                Vec::new(),
            );
            if self
                .issue_record(request)?
                .is_none_or(|value| value.state != "CLOSED")
            {
                return Err(reconciled_write_error(attempt.is_err()));
            }
        }
        let observed = self
            .issue_record(request)?
            .ok_or(TeamPublicationError::Identity)?;
        if observed.number != issue_number
            || observed.state != "CLOSED"
            || observed.body != Self::render_issue(request, &observed.body)?
        {
            return Err(TeamPublicationError::Identity);
        }
        Ok(observed)
    }

    fn reconcile_completed_pull_request(
        &mut self,
        request: &TaskPublicationRequest,
        pull_request_number: u64,
    ) -> Result<GhPullRequest, TeamPublicationError> {
        let pull_request = self
            .pull_request_record(request)?
            .ok_or(TeamPublicationError::Identity)?;
        if pull_request.number != pull_request_number
            || pull_request.base_ref_name != request.campaign_branch
            || pull_request.head_ref_name != request.task_branch
            || pull_request.head_ref_oid != request.reviewed_commit
        {
            return Err(TeamPublicationError::Identity);
        }
        let expected_body = self.render_pull_request(request, &pull_request.body)?;
        if pull_request.body != expected_body {
            let attempt = self.run_gh(
                vec![
                    "pr".to_owned(),
                    "edit".to_owned(),
                    pull_request_number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ],
                expected_body.clone().into_bytes(),
            );
            if self
                .pull_request_record(request)?
                .as_ref()
                .map(|value| &value.body)
                != Some(&expected_body)
            {
                return Err(reconciled_write_error(attempt.is_err()));
            }
        }
        if self
            .pull_request_record(request)?
            .is_some_and(|value| value.state == "OPEN")
        {
            let attempt = self.run_gh(
                vec![
                    "pr".to_owned(),
                    "close".to_owned(),
                    pull_request_number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                ],
                Vec::new(),
            );
            if self
                .pull_request_record(request)?
                .is_none_or(|value| !matches!(value.state.as_str(), "CLOSED" | "MERGED"))
            {
                return Err(reconciled_write_error(attempt.is_err()));
            }
        }
        let observed = self
            .pull_request_record(request)?
            .ok_or(TeamPublicationError::Identity)?;
        if observed.number != pull_request_number
            || !matches!(observed.state.as_str(), "CLOSED" | "MERGED")
            || observed.body != self.render_pull_request(request, &observed.body)?
        {
            return Err(TeamPublicationError::Identity);
        }
        Ok(observed)
    }
}

impl TeamPublicationPort for GhCliPublicationPort {
    fn ensure_campaign_branch(
        &mut self,
        request: &CampaignBranchPublicationRequest,
    ) -> Result<CampaignBranchObservation, TeamPublicationError> {
        if request.campaign_id != self.campaign_id
            || request.campaign_branch_prefix != self.campaign_branch_prefix
            || request.initial_commit != self.initial_commit
            || request.campaign_branch == self.policy.destination_branch
            || request.campaign_branch != self.campaign_branch_prefix
                && !request
                    .campaign_branch
                    .strip_prefix(&self.campaign_branch_prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            || !valid_commit(&request.campaign_commit)
            || !valid_commit(&request.initial_commit)
            || !request.includes_task_integration
                && request.campaign_commit != request.initial_commit
            || request.includes_task_integration && !self.task_merge_allowed
        {
            return Err(TeamPublicationError::Authority);
        }
        self.verify_remote_url()?;
        if self
            .observe_named_branch(&request.campaign_branch)?
            .as_deref()
            != Some(&request.campaign_commit)
        {
            let refspec = format!(
                "{}:refs/heads/{}",
                request.campaign_commit, request.campaign_branch
            );
            let attempt = self.run_git(
                vec![
                    "push".to_owned(),
                    "--porcelain".to_owned(),
                    self.policy.remote.clone(),
                    refspec,
                ],
                Vec::new(),
            );
            let observed = self.observe_named_branch(&request.campaign_branch)?;
            if observed.as_deref() != Some(&request.campaign_commit) {
                return Err(if attempt.is_err() {
                    TeamPublicationError::Uncertain
                } else {
                    TeamPublicationError::Identity
                });
            }
        }
        Ok(CampaignBranchObservation {
            branch: request.campaign_branch.clone(),
            commit: request.campaign_commit.clone(),
            evidence_sha256: digest(&format!(
                "{}\0{}\0{}",
                request.campaign_id, request.campaign_branch, request.campaign_commit
            )),
        })
    }

    fn ensure_issue(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<IssueObservation, TeamPublicationError> {
        let current = self.issue_record(request)?;
        let body = Self::render_issue(request, current.as_ref().map_or("", |value| &value.body))?;
        if current.as_ref().is_none_or(|value| value.body != body) {
            let arguments = if let Some(issue) = &current {
                vec![
                    "issue".to_owned(),
                    "edit".to_owned(),
                    issue.number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ]
            } else {
                vec![
                    "issue".to_owned(),
                    "create".to_owned(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--title".to_owned(),
                    format!("[{}] {}", request.task_id, request.title),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ]
            };
            if self.run_gh(arguments, body.into_bytes()).is_err()
                && self.issue_record(request)?.is_none()
            {
                return Err(TeamPublicationError::Uncertain);
            }
        }
        let observed = self
            .issue_record(request)?
            .ok_or(TeamPublicationError::Uncertain)?;
        let expected = Self::render_issue(request, &observed.body)?;
        if observed.body != expected {
            return Err(TeamPublicationError::Identity);
        }
        Ok(IssueObservation {
            number: observed.number,
            evidence_sha256: digest(&format!("{}\0{}", observed.number, observed.body)),
        })
    }

    fn ensure_reviewed_branch(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<PushObservation, TeamPublicationError> {
        self.verify_remote_url()?;
        if self.observe_branch(request)?.as_deref() != Some(&request.reviewed_commit) {
            let refspec = format!(
                "{}:refs/heads/{}",
                request.reviewed_commit, request.task_branch
            );
            let attempt = self.run_git(
                vec![
                    "push".to_owned(),
                    "--porcelain".to_owned(),
                    self.policy.remote.clone(),
                    refspec,
                ],
                Vec::new(),
            );
            let observed = self.observe_branch(request)?;
            if observed.as_deref() != Some(&request.reviewed_commit) {
                return Err(if attempt.is_err() {
                    TeamPublicationError::Uncertain
                } else {
                    TeamPublicationError::Identity
                });
            }
        }
        Ok(PushObservation {
            branch: request.task_branch.clone(),
            commit: request.reviewed_commit.clone(),
            evidence_sha256: digest(&format!(
                "{}\0{}\0{}",
                self.policy.remote, request.task_branch, request.reviewed_commit
            )),
        })
    }

    fn ensure_pull_request(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<PullRequestObservation, TeamPublicationError> {
        let current = self.pull_request_record(request)?;
        if current.as_ref().is_some_and(|value| {
            value.base_ref_name != request.campaign_branch
                || value.head_ref_name != request.task_branch
                || value.head_ref_oid != request.reviewed_commit
                || !value.is_draft
                || value.state != "OPEN"
        }) {
            return Err(TeamPublicationError::Identity);
        }
        let body =
            self.render_pull_request(request, current.as_ref().map_or("", |value| &value.body))?;
        if current.as_ref().is_none_or(|value| value.body != body) {
            let arguments = if let Some(pull_request) = &current {
                vec![
                    "pr".to_owned(),
                    "edit".to_owned(),
                    pull_request.number.to_string(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ]
            } else {
                vec![
                    "pr".to_owned(),
                    "create".to_owned(),
                    "--repo".to_owned(),
                    self.repository_selector(),
                    "--draft".to_owned(),
                    "--base".to_owned(),
                    request.campaign_branch.clone(),
                    "--head".to_owned(),
                    request.task_branch.clone(),
                    "--title".to_owned(),
                    format!("[{}] {}", request.task_id, request.title),
                    "--body-file".to_owned(),
                    "-".to_owned(),
                ]
            };
            if self.run_gh(arguments, body.into_bytes()).is_err()
                && self.pull_request_record(request)?.is_none()
            {
                return Err(TeamPublicationError::Uncertain);
            }
        }
        let observed = self
            .pull_request_record(request)?
            .ok_or(TeamPublicationError::Uncertain)?;
        if observed.base_ref_name != request.campaign_branch
            || observed.head_ref_name != request.task_branch
            || observed.head_ref_oid != request.reviewed_commit
            || !observed.is_draft
            || observed.state != "OPEN"
            || observed.body != self.render_pull_request(request, &observed.body)?
        {
            return Err(TeamPublicationError::Identity);
        }
        Ok(PullRequestObservation {
            number: observed.number,
            base_branch: observed.base_ref_name,
            head_branch: observed.head_ref_name,
            head_commit: observed.head_ref_oid,
            evidence_sha256: digest(&format!("{}\0{}", observed.number, observed.body)),
        })
    }

    fn observe_ci(
        &mut self,
        request: &TaskPublicationRequest,
    ) -> Result<CiObservation, TeamPublicationError> {
        let endpoint = format!(
            "repos/{}/{}/commits/{}/check-runs",
            self.policy.owner, self.policy.repository, request.reviewed_commit
        );
        let output = self.run_gh(
            vec![
                "api".to_owned(),
                "--hostname".to_owned(),
                self.policy.host.clone(),
                endpoint,
                "--jq".to_owned(),
                ".check_runs | map({name:.name,status:.status,conclusion:.conclusion})".to_owned(),
            ],
            Vec::new(),
        )?;
        let mut checks: Vec<GhCheck> =
            serde_json::from_slice(&output).map_err(|_| TeamPublicationError::Identity)?;
        checks.sort_by(|left, right| left.name.cmp(&right.name));
        let selected = self
            .policy
            .required_checks
            .iter()
            .map(|required| {
                let matches = checks
                    .iter()
                    .filter(|check| &check.name == required)
                    .collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err(TeamPublicationError::Identity);
                }
                Ok(matches[0])
            })
            .collect::<Result<Vec<_>, _>>()?;
        if selected.iter().any(|check| check.status != "completed") {
            return Ok(CiObservation::Pending);
        }
        let material = selected
            .iter()
            .map(|check| {
                format!(
                    "{}:{}:{}",
                    check.name,
                    check.status,
                    check.conclusion.as_deref().unwrap_or("none")
                )
            })
            .collect::<Vec<_>>()
            .join("\0");
        let evidence_sha256 = digest(&format!("{}\0{material}", request.reviewed_commit));
        if selected
            .iter()
            .all(|check| check.conclusion.as_deref() == Some("success"))
        {
            Ok(CiObservation::Passed {
                commit: request.reviewed_commit.clone(),
                evidence_sha256,
            })
        } else {
            Ok(CiObservation::Failed {
                commit: request.reviewed_commit.clone(),
                evidence_sha256,
            })
        }
    }

    fn ensure_task_completion(
        &mut self,
        request: &TaskPublicationRequest,
        integration_commit: &str,
        completion_commit: &str,
    ) -> Result<TaskCompletionObservation, TeamPublicationError> {
        if !self.task_merge_allowed
            || request.state != "merged"
            || !valid_commit(integration_commit)
            || !valid_commit(completion_commit)
            || self
                .observe_named_branch(&request.campaign_branch)?
                .as_deref()
                != Some(completion_commit)
        {
            return Err(TeamPublicationError::Authority);
        }
        let issue_number = request
            .issue_number
            .ok_or(TeamPublicationError::Authority)?;
        let pull_request_number = request
            .pull_request_number
            .ok_or(TeamPublicationError::Authority)?;

        let _ = self.reconcile_completed_issue(request, issue_number)?;
        let final_pull_request =
            self.reconcile_completed_pull_request(request, pull_request_number)?;
        Ok(TaskCompletionObservation {
            issue_number,
            pull_request_number,
            pull_request_merged: final_pull_request.state == "MERGED",
            evidence_sha256: digest(&format!(
                "{}\0{}\0{}\0{}\0{}",
                request.campaign_id,
                request.task_id,
                integration_commit,
                completion_commit,
                final_pull_request.state
            )),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhIssue {
    number: u64,
    body: String,
    state: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhPullRequest {
    number: u64,
    body: String,
    base_ref_name: String,
    head_ref_name: String,
    head_ref_oid: String,
    is_draft: bool,
    state: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhCheck {
    name: String,
    status: String,
    conclusion: Option<String>,
}

fn publication_evidence(
    request: &TaskPublicationRequest,
) -> Result<Vec<EvidenceId>, TeamPublicationError> {
    request
        .gate_evidence_sha256
        .iter()
        .map(|value| EvidenceId::new(format!("gate-{value}")))
        .chain(
            request
                .review_evidence_sha256
                .iter()
                .map(|value| EvidenceId::new(format!("review-{value}"))),
        )
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TeamPublicationError::Authority)
}

const fn reconciled_write_error(attempt_failed: bool) -> TeamPublicationError {
    if attempt_failed {
        TeamPublicationError::Uncertain
    } else {
        TeamPublicationError::Identity
    }
}

fn gate_evidence(
    request: &TaskPublicationRequest,
) -> Result<Vec<EvidenceId>, TeamPublicationError> {
    request
        .gate_evidence_sha256
        .iter()
        .map(|value| EvidenceId::new(format!("gate-{value}")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TeamPublicationError::Authority)
}

fn parse_one<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, TeamPublicationError> {
    serde_json::from_slice(bytes).map_err(|_| TeamPublicationError::Identity)
}

fn unique_record<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    predicate: impl Fn(&T) -> bool,
) -> Result<Option<T>, TeamPublicationError> {
    let mut values = serde_json::from_slice::<Vec<T>>(bytes)
        .map_err(|_| TeamPublicationError::Identity)?
        .into_iter()
        .filter(predicate)
        .collect::<Vec<_>>();
    if values.len() > 1 {
        return Err(TeamPublicationError::Identity);
    }
    Ok(values.pop())
}

fn parse_remote_url(url: &str) -> Result<(String, String, String), TeamPublicationError> {
    let without_suffix = url.strip_suffix(".git").unwrap_or(url);
    let path = if let Some(rest) = without_suffix.strip_prefix("https://") {
        if rest.contains('@') {
            return Err(TeamPublicationError::Identity);
        }
        rest.to_owned()
    } else if let Some(rest) = without_suffix.strip_prefix("ssh://git@") {
        rest.to_owned()
    } else if let Some(rest) = without_suffix.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        return Err(TeamPublicationError::Identity);
    };
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|value| value.is_empty()) {
        return Err(TeamPublicationError::Identity);
    }
    Ok((
        parts[0].to_owned(),
        parts[1].to_owned(),
        parts[2].to_owned(),
    ))
}

fn text(bytes: &[u8]) -> Result<String, TeamPublicationError> {
    let value = std::str::from_utf8(bytes)
        .map(str::trim)
        .map_err(|_| TeamPublicationError::Identity)?;
    if value.contains('\0') {
        return Err(TeamPublicationError::Identity);
    }
    Ok(value.to_owned())
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn digest(value: &str) -> String {
    let bytes = Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_supported_remote_urls_parse_without_credentials() {
        for (value, expected) in [
            (
                "https://github.com/owner/repository.git",
                ("github.com", "owner", "repository"),
            ),
            (
                "git@github.com:owner/repository.git",
                ("github.com", "owner", "repository"),
            ),
            (
                "ssh://git@github.com/owner/repository.git",
                ("github.com", "owner", "repository"),
            ),
        ] {
            let parsed = parse_remote_url(value).unwrap();
            assert_eq!(
                (parsed.0.as_str(), parsed.1.as_str(), parsed.2.as_str()),
                expected
            );
        }
        for denied in [
            "https://user@github.com/owner/repository.git",
            "file:///tmp/repository",
            "https://github.com/owner/repository/extra.git",
        ] {
            assert_eq!(
                parse_remote_url(denied),
                Err(TeamPublicationError::Identity)
            );
        }
    }
}
