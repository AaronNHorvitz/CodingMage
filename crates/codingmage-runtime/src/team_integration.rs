//! Serialized, restart-reconcilable campaign integration.

use std::{collections::BTreeMap, collections::BTreeSet, fmt::Write as _, fs, path::PathBuf};

use codingmage_campaign::{CampaignTaskState, TaskMergeStrategy, TeamCampaignSnapshot};
use codingmage_codex::{CodexAdapter, CodexReviewBinding, ReviewVerdict, codex_review_schema};
use codingmage_contracts::{AgentId, EvidenceId, TaskId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_gate::{
    GateAssertion, GateEntry, GateRegistry, GateRequirement, GateRunner, GateTier, GateTrigger,
    TrustedGateDefinition,
};
use codingmage_git::{
    OwnedWorktree, commit_owned_changes, create_owned_worktree, integrate_reviewed_descendant,
    observe_owned_child_commit, prepare_reviewed_delta, release_prepared_integration,
    remove_owned_worktree,
};
use codingmage_orchestrator::reconcile_and_select_next;
use codingmage_plan::{CheckState, TaskPlan};
use codingmage_process::{CancellationToken, ProcessExecutor, ProcessProfile, ProcessRequest};
use sha2::{Digest, Sha256};

use crate::{
    ProviderSpec, RuntimeError, check_exact_line, generated_run_id, login_discovery_environment,
    private_directory, write_private_idempotent,
};

/// Immutable deterministic and independent-review evidence for one prepared integration commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationVerification {
    /// Digest of all required integration-level deterministic gates.
    pub gate_evidence_sha256: String,
    /// Digest of a fresh review of the effective prepared cumulative diff.
    pub review_evidence_sha256: String,
}

impl IntegrationVerification {
    pub(crate) fn verify(&self) -> Result<(), RuntimeError> {
        if !valid_sha256(&self.gate_evidence_sha256) || !valid_sha256(&self.review_evidence_sha256)
        {
            return Err(RuntimeError::Verification);
        }
        Ok(())
    }
}

/// Fresh integration-level gate and review adapter.
pub trait TeamIntegrationVerifier {
    /// Verifies the exact prepared commit in its still-owned immutable worktree.
    ///
    /// # Errors
    ///
    /// Returns a content-free runtime failure. A failure must not mutate the campaign worktree.
    fn verify(
        &mut self,
        task_id: &str,
        prepared_worktree: &OwnedWorktree,
        prepared_commit: &str,
    ) -> Result<IntegrationVerification, RuntimeError>;

    /// Verifies a cumulative campaign diff from an explicitly supplied immutable base.
    ///
    /// Test adapters may inherit the ordinary verification behavior. Production adapters must
    /// bind the supplied base into their read-only review request.
    ///
    /// # Errors
    ///
    /// Returns a content-free gate, review, identity, process, or cancellation failure.
    fn verify_from_base(
        &mut self,
        task_id: &str,
        prepared_worktree: &OwnedWorktree,
        _base_commit: &str,
        prepared_commit: &str,
    ) -> Result<IntegrationVerification, RuntimeError> {
        self.verify(task_id, prepared_worktree, prepared_commit)
    }
}

/// Production integration verifier composed from configured gates and a fresh Codex review.
#[derive(Clone, Debug)]
pub struct ProductionTeamIntegrationVerifier {
    config: Config,
    executor: ProcessExecutor,
    reviewer: ProviderSpec,
    schema_path: PathBuf,
    login_environment: BTreeMap<String, String>,
    cancellation: CancellationToken,
}

impl ProductionTeamIntegrationVerifier {
    /// Creates a verifier with a private process root and immutable review schema.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for unsafe roots, provider identity, or login environment.
    pub fn new(
        config: Config,
        codingmage_binary: &std::path::Path,
        private_root: &std::path::Path,
        reviewer: ProviderSpec,
        cancellation: CancellationToken,
    ) -> Result<Self, RuntimeError> {
        private_directory(private_root)?;
        let process_root = private_root.join("integration-processes");
        let executor = ProcessExecutor::new_with_guard_arguments(
            codingmage_binary,
            vec!["__process-guard".to_owned()],
            &process_root,
        )
        .map_err(|_| RuntimeError::Process)?;
        let schema_path = private_root.join("integration-review.schema.json");
        write_private_idempotent(&schema_path, codex_review_schema().as_bytes())?;
        let login_environment = login_discovery_environment()?;
        CodexAdapter::new(
            reviewer.executable.clone(),
            &reviewer.model,
            &reviewer.effort,
            schema_path.clone(),
        )
        .and_then(|adapter| adapter.with_login_environment(login_environment.clone()))
        .map_err(RuntimeError::Reviewer)?;
        Ok(Self {
            config,
            executor,
            reviewer,
            schema_path,
            login_environment,
            cancellation,
        })
    }

    fn gate_registry(&self, worktree: &std::path::Path) -> Result<GateRegistry, RuntimeError> {
        let entries = self
            .config
            .gate_commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let profile = ProcessProfile::new(
                    &command.executable,
                    [command.args.clone()],
                    std::iter::empty::<String>(),
                )
                .map_err(|_| RuntimeError::Verification)?;
                Ok(GateEntry::Available(Box::new(TrustedGateDefinition {
                    id: format!("integration-gate-{}", index.saturating_add(1)),
                    tier: GateTier::Tier2,
                    trigger: GateTrigger::EveryAttempt,
                    requirement: GateRequirement::Required,
                    resources: BTreeSet::from(["campaign-integration".to_owned()]),
                    profile,
                    request: ProcessRequest {
                        arguments: command.args.clone(),
                        working_directory: worktree.to_path_buf(),
                        environment: BTreeMap::new(),
                        stdin: Vec::new(),
                        max_output_bytes: 16 * 1024 * 1024,
                        deadline_millis: 30 * 60 * 1_000,
                        max_processes: 64,
                        max_open_files: 1_024,
                        expected_exit_codes: BTreeSet::from([0]),
                    },
                    assertions: vec![GateAssertion::OutputNotTruncated],
                })))
            })
            .collect::<Result<Vec<_>, RuntimeError>>()?;
        GateRegistry::new(entries).map_err(|_| RuntimeError::Verification)
    }

    fn verify_bound(
        &mut self,
        task_id: &str,
        prepared_worktree: &OwnedWorktree,
        base_commit: &str,
        prepared_commit: &str,
    ) -> Result<IntegrationVerification, RuntimeError> {
        if !valid_commit(base_commit) || !valid_commit(prepared_commit) {
            return Err(RuntimeError::Verification);
        }
        let registry = self.gate_registry(&prepared_worktree.manifest().path)?;
        let gates = GateRunner::new(self.executor.clone())
            .run_with_cancellation(
                &registry,
                prepared_commit,
                &BTreeSet::new(),
                &self.cancellation,
            )
            .map_err(|_| RuntimeError::Verification)?;
        if gates.blocked || gates.evidence.is_empty() {
            return Err(RuntimeError::Verification);
        }
        let gate_material = gates
            .evidence
            .iter()
            .map(|evidence| evidence.integrity_sha256.as_str())
            .collect::<Vec<_>>()
            .join("\0");
        let gate_evidence_sha256 = digest(&gate_material);
        let evidence = gates
            .evidence
            .iter()
            .map(|value| EvidenceId::new(format!("integration-{}", value.integrity_sha256)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RuntimeError::Verification)?;
        let adapter = CodexAdapter::new(
            self.reviewer.executable.clone(),
            &self.reviewer.model,
            &self.reviewer.effort,
            self.schema_path.clone(),
        )
        .and_then(|adapter| adapter.with_login_environment(self.login_environment.clone()))
        .map_err(RuntimeError::Reviewer)?;
        let binding = CodexReviewBinding {
            run_id: generated_run_id()?,
            task_id: TaskId::new(task_id.to_owned()).map_err(|_| RuntimeError::State)?,
            agent_id: AgentId::new("codex-integration-reviewer")
                .map_err(|_| RuntimeError::State)?,
            thread_id: None,
            worktree: prepared_worktree.manifest().path.clone(),
            base_commit: base_commit.to_owned(),
            target_commit: prepared_commit.to_owned(),
            evidence,
        };
        let plan = adapter
            .plan_start(
                &binding,
                "Review the exact effective campaign integration delta.",
            )
            .map_err(RuntimeError::Reviewer)?;
        let execution = adapter
            .execute_observed(&self.executor, &plan, &binding, &self.cancellation)
            .map_err(RuntimeError::Reviewer)?;
        let result = execution.report.map_err(RuntimeError::Reviewer)?;
        if result.report.verdict != ReviewVerdict::Pass
            || result.report.base_commit != base_commit
            || result.report.target_commit != prepared_commit
        {
            return Err(RuntimeError::Verification);
        }
        Ok(IntegrationVerification {
            gate_evidence_sha256,
            review_evidence_sha256: digest(&format!(
                "{}\0{}\0{}\0pass",
                result.thread_id, result.report.base_commit, result.report.target_commit
            )),
        })
    }
}

impl TeamIntegrationVerifier for ProductionTeamIntegrationVerifier {
    fn verify(
        &mut self,
        task_id: &str,
        prepared_worktree: &OwnedWorktree,
        prepared_commit: &str,
    ) -> Result<IntegrationVerification, RuntimeError> {
        self.verify_bound(
            task_id,
            prepared_worktree,
            &prepared_worktree.manifest().source_commit,
            prepared_commit,
        )
    }

    fn verify_from_base(
        &mut self,
        task_id: &str,
        prepared_worktree: &OwnedWorktree,
        base_commit: &str,
        prepared_commit: &str,
    ) -> Result<IntegrationVerification, RuntimeError> {
        self.verify_bound(task_id, prepared_worktree, base_commit, prepared_commit)
    }
}

/// Terminal observation from one serialized integration-queue step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamIntegrationOutcome {
    /// Canonical completed task.
    pub task_id: String,
    /// Exact reviewed candidate supplied by the task pod.
    pub reviewed_commit: String,
    /// Exact coordinator-created integration commit.
    pub integration_commit: String,
    /// Exact mechanical task-source completion commit.
    pub completion_commit: String,
    /// New canonical task-source digest.
    pub task_source_sha256: String,
}

/// Enqueues one accepted task and durably persists the exact canonical queue order.
///
/// # Errors
///
/// Returns [`RuntimeError`] for stale task state or persistence failure.
pub fn enqueue_team_integration<P>(
    snapshot: &mut TeamCampaignSnapshot,
    task_id: &str,
    mut persist: P,
) -> Result<(), RuntimeError>
where
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    let evidence = transition_evidence(snapshot, task_id, "integration-queued")?;
    snapshot
        .enqueue_integration(task_id, evidence)
        .map_err(|_| RuntimeError::State)?;
    persist(snapshot)
}

/// Integrates the exact queue head and reconciles every campaign-head crash boundary.
///
/// The operation prepares the reviewed delta on the current campaign head, runs fresh gates and
/// review before mutation, releases the temporary worktree, persists the exact integration commit,
/// installs that commit, creates a separate mechanical task-source completion commit, and finally
/// advances the durable projection. On restart, the observed campaign head determines which of
/// those effects already happened; no state-changing Git operation is blindly replayed.
///
/// # Errors
///
/// Returns a content-free runtime error for stale identity, conflict, failed verification, unsafe
/// task-source reconciliation, or persistence failure.
#[allow(clippy::too_many_lines)]
pub fn integrate_team_queue_head<V, P>(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    snapshot: &mut TeamCampaignSnapshot,
    verifier: &mut V,
    persist: P,
) -> Result<TeamIntegrationOutcome, RuntimeError>
where
    V: TeamIntegrationVerifier,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    integrate_team_queue_head_with_strategy(
        config,
        authorization,
        campaign,
        snapshot,
        verifier,
        TaskMergeStrategy::Squash,
        persist,
    )
}

/// Integrates the queue head using the exact configured commit strategy.
///
/// # Errors
///
/// Returns a content-free failure when the selected strategy cannot preserve the reviewed commit,
/// or when any ordinary serialized-integration precondition fails.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn integrate_team_queue_head_with_strategy<V, P>(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    snapshot: &mut TeamCampaignSnapshot,
    verifier: &mut V,
    strategy: TaskMergeStrategy,
    persist: P,
) -> Result<TeamIntegrationOutcome, RuntimeError>
where
    V: TeamIntegrationVerifier,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    integrate_team_queue_head_with_validation(
        config,
        authorization,
        campaign,
        snapshot,
        verifier,
        strategy,
        None,
        persist,
    )
}

/// Integrates the queue head and performs cumulative validation at the configured interval.
///
/// The cumulative review is completed on the prepared commit before the campaign head mutates.
/// Its immutable base is the campaign worktree's original source commit.
///
/// # Errors
///
/// Returns a content-free failure for a zero interval or any ordinary integration, gate, review,
/// identity, persistence, or repository failure.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn integrate_team_queue_head_with_validation<V, P>(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    snapshot: &mut TeamCampaignSnapshot,
    verifier: &mut V,
    strategy: TaskMergeStrategy,
    validation_interval: Option<u16>,
    mut persist: P,
) -> Result<TeamIntegrationOutcome, RuntimeError>
where
    V: TeamIntegrationVerifier,
    P: FnMut(&TeamCampaignSnapshot) -> Result<(), RuntimeError>,
{
    if validation_interval == Some(0) {
        return Err(RuntimeError::State);
    }
    snapshot.verify().map_err(|_| RuntimeError::State)?;
    let task_id = snapshot
        .integration_queue
        .first()
        .cloned()
        .ok_or(RuntimeError::State)?;
    if snapshot.tasks[&task_id].state == CampaignTaskState::IntegrationQueued {
        let evidence = transition_evidence(snapshot, &task_id, "merge-ready")?;
        snapshot
            .mark_merge_ready(&task_id, evidence)
            .map_err(|_| RuntimeError::State)?;
        persist(snapshot)?;
    }
    if snapshot.tasks[&task_id].state == CampaignTaskState::MergeReady {
        let evidence = transition_evidence(snapshot, &task_id, "integration-intent")?;
        snapshot
            .begin_integration(&task_id, evidence)
            .map_err(|_| RuntimeError::State)?;
        persist(snapshot)?;
    }
    let record = snapshot
        .tasks
        .get(&task_id)
        .cloned()
        .ok_or(RuntimeError::State)?;
    if record.state != CampaignTaskState::Integrating {
        return Err(RuntimeError::State);
    }
    let reviewed_commit = record.reviewed_commit.clone().ok_or(RuntimeError::State)?;
    let expected_head = snapshot.campaign_head.clone();
    let projected_merged_count = snapshot
        .tasks
        .values()
        .filter(|task| task.state == CampaignTaskState::Merged)
        .count()
        .saturating_add(1);
    let cumulative_validation = validation_interval
        .filter(|interval| projected_merged_count % usize::from(*interval) == 0)
        .map(|_| {
            (
                campaign.manifest().source_commit.as_str(),
                projected_merged_count,
            )
        });
    let integration_commit = if let Some(commit) = record.integration_commit.clone() {
        commit
    } else {
        let (integration_commit, verification) = match strategy {
            TaskMergeStrategy::Squash => prepare_squash_integration(
                config,
                authorization,
                campaign,
                verifier,
                &task_id,
                &expected_head,
                &record.base_commit,
                &reviewed_commit,
                &record.owned_paths,
                cumulative_validation,
            )?,
            TaskMergeStrategy::FastForwardOnly => prepare_fast_forward_integration(
                config,
                authorization,
                verifier,
                &task_id,
                &expected_head,
                &record.base_commit,
                &reviewed_commit,
                &record.owned_paths,
                cumulative_validation,
            )?,
        };
        let evidence = integration_evidence(
            &task_id,
            &integration_commit,
            &verification.gate_evidence_sha256,
            &verification.review_evidence_sha256,
        );
        snapshot
            .bind_integration_commit(&task_id, integration_commit.clone(), evidence)
            .map_err(|_| RuntimeError::State)?;
        persist(snapshot)?;
        integration_commit
    };

    let observed_head = campaign
        .observe_head(authorization)
        .map_err(|_| RuntimeError::Repository)?;
    let completion_commit = if observed_head == expected_head {
        let receipt = integrate_reviewed_descendant(
            authorization,
            campaign,
            &expected_head,
            &integration_commit,
            &record.owned_paths,
        )
        .map_err(|_| RuntimeError::Integration)?;
        if receipt.integrated_head != integration_commit {
            return Err(RuntimeError::State);
        }
        create_completion_commit(
            config,
            authorization,
            campaign,
            &task_id,
            &integration_commit,
        )?
    } else if observed_head == integration_commit {
        create_completion_commit(
            config,
            authorization,
            campaign,
            &task_id,
            &integration_commit,
        )?
    } else {
        observe_completion_commit(
            config,
            authorization,
            campaign,
            &task_id,
            &integration_commit,
            &observed_head,
        )?;
        observed_head
    };
    let task_source_sha256 = completed_task_source_digest(config, campaign, &task_id)?;
    let evidence = integration_evidence(
        &task_id,
        &completion_commit,
        &task_source_sha256,
        "completion-observed",
    );
    snapshot
        .complete_integration(
            &task_id,
            &expected_head,
            &integration_commit,
            completion_commit.clone(),
            task_source_sha256.clone(),
            evidence,
        )
        .map_err(|_| RuntimeError::State)?;
    persist(snapshot)?;
    Ok(TeamIntegrationOutcome {
        task_id,
        reviewed_commit,
        integration_commit,
        completion_commit,
        task_source_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_squash_integration<V: TeamIntegrationVerifier>(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    verifier: &mut V,
    task_id: &str,
    expected_head: &str,
    candidate_base: &str,
    reviewed_commit: &str,
    owned_paths: &[PathBuf],
    cumulative_validation: Option<(&str, usize)>,
) -> Result<(String, IntegrationVerification), RuntimeError> {
    let prepared = prepare_reviewed_delta(
        authorization,
        config,
        campaign,
        generated_run_id()?,
        TaskId::new(task_id.to_owned()).map_err(|_| RuntimeError::State)?,
        expected_head,
        candidate_base,
        reviewed_commit,
        owned_paths,
    )
    .map_err(|_| RuntimeError::Integration)?;
    let verification_result = (|| {
        let task = verifier.verify(task_id, prepared.worktree(), prepared.prepared_head())?;
        let Some((base_commit, merged_count)) = cumulative_validation else {
            return Ok(task);
        };
        let cumulative = verifier.verify_from_base(
            &format!("campaign-batch-{merged_count}"),
            prepared.worktree(),
            base_commit,
            prepared.prepared_head(),
        )?;
        combined_verification(&task, &cumulative)
    })();
    let released = release_prepared_integration(authorization, prepared)
        .map_err(|_| RuntimeError::Integration)?;
    let verification = verification_result?;
    verification.verify()?;
    if released.previous_head != expected_head
        || released.candidate_base != candidate_base
        || released.reviewed_head != reviewed_commit
        || released.allowed_paths != owned_paths
    {
        return Err(RuntimeError::State);
    }
    Ok((released.prepared_head, verification))
}

#[allow(clippy::too_many_arguments)]
fn prepare_fast_forward_integration<V: TeamIntegrationVerifier>(
    config: &Config,
    authorization: &RepositoryAuthorization,
    verifier: &mut V,
    task_id: &str,
    expected_head: &str,
    candidate_base: &str,
    reviewed_commit: &str,
    owned_paths: &[PathBuf],
    cumulative_validation: Option<(&str, usize)>,
) -> Result<(String, IntegrationVerification), RuntimeError> {
    if candidate_base != expected_head {
        return Err(RuntimeError::Integration);
    }
    let mut verification_worktree = create_owned_worktree(
        authorization,
        config,
        generated_run_id()?,
        TaskId::new(task_id.to_owned()).map_err(|_| RuntimeError::State)?,
        candidate_base,
    )
    .map_err(|_| RuntimeError::Integration)?;
    let attempt_result = (|| {
        let receipt = integrate_reviewed_descendant(
            authorization,
            &verification_worktree,
            candidate_base,
            reviewed_commit,
            owned_paths,
        )
        .map_err(|_| RuntimeError::Integration)?;
        if receipt.integrated_head != reviewed_commit {
            return Err(RuntimeError::State);
        }
        let task = verifier.verify(task_id, &verification_worktree, reviewed_commit)?;
        let verification = if let Some((base_commit, merged_count)) = cumulative_validation {
            let cumulative = verifier.verify_from_base(
                &format!("campaign-batch-{merged_count}"),
                &verification_worktree,
                base_commit,
                reviewed_commit,
            )?;
            combined_verification(&task, &cumulative)?
        } else {
            task
        };
        verification.verify()?;
        Ok(verification)
    })();
    let released = remove_owned_worktree(authorization, &mut verification_worktree)
        .map_err(|_| RuntimeError::Integration);
    released?;
    let verification = attempt_result?;
    Ok((reviewed_commit.to_owned(), verification))
}

fn combined_verification(
    task: &IntegrationVerification,
    cumulative: &IntegrationVerification,
) -> Result<IntegrationVerification, RuntimeError> {
    task.verify()?;
    cumulative.verify()?;
    Ok(IntegrationVerification {
        gate_evidence_sha256: digest(&format!(
            "{}\0{}",
            task.gate_evidence_sha256, cumulative.gate_evidence_sha256
        )),
        review_evidence_sha256: digest(&format!(
            "{}\0{}",
            task.review_evidence_sha256, cumulative.review_evidence_sha256
        )),
    })
}

fn create_completion_commit(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    task_id: &str,
    integration_commit: &str,
) -> Result<String, RuntimeError> {
    let task_path = campaign.manifest().path.join(&config.task_source);
    let metadata = fs::symlink_metadata(&task_path).map_err(|_| RuntimeError::Plan)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RuntimeError::Authority);
    }
    let before = fs::read(&task_path).map_err(|_| RuntimeError::Plan)?;
    let plan = TaskPlan::parse(&before).map_err(|_| RuntimeError::Plan)?;
    let selected = plan.select_exact(task_id).map_err(|_| RuntimeError::Plan)?;
    let after =
        check_exact_line(&before, selected.item.anchor.line).map_err(|_| RuntimeError::Plan)?;
    let completion_evidence =
        EvidenceId::new(format!("team-integration-{}", &integration_commit[..16]))
            .map_err(|_| RuntimeError::State)?;
    reconcile_and_select_next(
        &before,
        &after,
        task_id,
        &completion_evidence,
        &BTreeSet::new(),
    )
    .map_err(|_| RuntimeError::Plan)?;
    fs::write(&task_path, &after).map_err(|_| RuntimeError::Plan)?;
    let receipt = commit_owned_changes(
        authorization,
        campaign,
        integration_commit,
        std::slice::from_ref(&config.task_source),
    )
    .map_err(|_| RuntimeError::Repository)?;
    if receipt.changed_paths != [config.task_source.clone()] {
        return Err(RuntimeError::Repository);
    }
    Ok(receipt.commit)
}

fn observe_completion_commit(
    config: &Config,
    authorization: &RepositoryAuthorization,
    campaign: &OwnedWorktree,
    task_id: &str,
    integration_commit: &str,
    completion_commit: &str,
) -> Result<(), RuntimeError> {
    let receipt = observe_owned_child_commit(
        authorization,
        campaign,
        integration_commit,
        completion_commit,
        std::slice::from_ref(&config.task_source),
    )
    .map_err(|_| RuntimeError::Repository)?;
    if receipt.changed_paths != [config.task_source.clone()] {
        return Err(RuntimeError::Repository);
    }
    let _ = completed_task_source_digest(config, campaign, task_id)?;
    Ok(())
}

fn completed_task_source_digest(
    config: &Config,
    campaign: &OwnedWorktree,
    task_id: &str,
) -> Result<String, RuntimeError> {
    let source = fs::read(campaign.manifest().path.join(&config.task_source))
        .map_err(|_| RuntimeError::Plan)?;
    let plan = TaskPlan::parse(&source).map_err(|_| RuntimeError::Plan)?;
    if !plan
        .items
        .iter()
        .any(|item| item.id == task_id && item.state == CheckState::Checked)
    {
        return Err(RuntimeError::Plan);
    }
    Ok(plan.source_sha256)
}

fn transition_evidence(
    snapshot: &TeamCampaignSnapshot,
    task_id: &str,
    phase: &str,
) -> Result<String, RuntimeError> {
    let record = snapshot.tasks.get(task_id).ok_or(RuntimeError::State)?;
    Ok(digest(&format!(
        "{}\0{}\0{}\0{}\0{phase}",
        snapshot.campaign_id, task_id, snapshot.campaign_head, record.next_transition
    )))
}

fn integration_evidence(task_id: &str, commit: &str, left: &str, right: &str) -> String {
    digest(&format!("{task_id}\0{commit}\0{left}\0{right}"))
}

fn digest(value: &str) -> String {
    let bytes = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    };

    use codingmage_campaign::{
        AdmissionDecision, CampaignAuthentication, CampaignConcurrency, CampaignExecutionMode,
        CampaignGateTier, CampaignLimits, CampaignProvider, CampaignPublication,
        CampaignTaskRecord, CampaignTaskTransition, DestinationPromotionPolicy,
        DurablePodScheduler, MultiAgentPolicy, PodProposal, PodRisk, TEAM_STATE_SCHEMA_VERSION,
        TaskIntegrationPolicy, TaskMergeStrategy, TaskPublicationMode, TeamResourceController,
        TeamResourcePolicy,
    };
    use codingmage_contracts::{AgentId, RunId};
    use codingmage_core::{
        AgentProfile, CapabilityPolicy, CommandSpec, PublicationMode, PublicationPolicy,
    };
    use codingmage_git::{create_owned_worktree, remove_owned_worktree};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
    const TASK_ID: &str = "23.4.4.1";
    const TASKS: &str = "# Tasks\n\n## Sprint 23 - Campaigns\n\n**Sprint goal:** Integrate safely.\n\n### Story 23.4 - Integration\n\n- [ ] **Task 23.4.4 - Serialize work**\n  - [ ] **Sub-task 23.4.4.1:** Integrate one reviewed candidate safely.\n";

    struct Fixture {
        root: PathBuf,
        config: Config,
        authorization: RepositoryAuthorization,
        campaign: OwnedWorktree,
        task_worktree: OwnedWorktree,
        snapshot: TeamCampaignSnapshot,
    }

    impl Fixture {
        #[allow(clippy::too_many_lines)]
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "codingmage-team-integration-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            let source = root.join("source");
            let target = root.join("target");
            let scratch = root.join("scratch");
            let state = root.join("state");
            for path in [&source, &target, &scratch, &state] {
                std::fs::create_dir_all(path).unwrap();
            }
            git(&target, &["init", "-b", "main"]);
            std::fs::write(target.join("TASKS.md"), TASKS).unwrap();
            std::fs::write(target.join("code.txt"), "before\n").unwrap();
            git(&target, &["add", "TASKS.md", "code.txt"]);
            git_commit(&target, "fixture");
            let initial = git_output(&target, &["rev-parse", "HEAD"]);
            let config = Config {
                version: 1,
                target_path: target,
                task_source: PathBuf::from("TASKS.md"),
                default_branch: "main".to_owned(),
                integration_branch: "codingmage/task".to_owned(),
                scratch_root: scratch,
                state_root: state,
                agent_profiles: vec![AgentProfile {
                    id: AgentId::new("fixture-agent").unwrap(),
                    provider: "fake".to_owned(),
                    model: "fixture".to_owned(),
                }],
                correction_limit: 3,
                gate_commands: vec![CommandSpec {
                    executable: PathBuf::from("/usr/bin/git"),
                    args: vec!["diff".to_owned(), "--check".to_owned()],
                }],
                capabilities: CapabilityPolicy::default(),
                publication: PublicationPolicy {
                    mode: PublicationMode::LocalOnly,
                },
                allow_parent_discovery: false,
            };
            let authorization = RepositoryAuthorization::authorize(&config, &source).unwrap();
            let task_worktree = create_owned_worktree(
                &authorization,
                &config,
                RunId::new("run-task-candidate").unwrap(),
                TaskId::new(TASK_ID).unwrap(),
                &initial,
            )
            .unwrap();
            std::fs::write(task_worktree.manifest().path.join("code.txt"), "after\n").unwrap();
            let candidate = commit_owned_changes(
                &authorization,
                &task_worktree,
                &initial,
                &[PathBuf::from("code.txt")],
            )
            .unwrap()
            .commit;
            let mut campaign_config = config.clone();
            campaign_config.integration_branch = "codingmage/campaign".to_owned();
            let campaign = create_owned_worktree(
                &authorization,
                &campaign_config,
                RunId::new("run-campaign-root").unwrap(),
                TaskId::new("campaign-root").unwrap(),
                &initial,
            )
            .unwrap();
            let spec = campaign_spec(&config, &authorization, &initial);
            let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
            let generation = scheduler.begin_generation(&[TASK_ID.to_owned()]).unwrap();
            let proposal = PodProposal::seal(
                PodProposal {
                    version: 3,
                    task_id: TASK_ID.to_owned(),
                    task_source_sha256: spec.task_source_sha256.clone(),
                    owned_paths: vec![PathBuf::from("code.txt")],
                    dependencies: Vec::new(),
                    gate_tiers: vec!["focused".to_owned()],
                    test_resources: vec!["integration-fixture".to_owned()],
                    expected_artifacts: vec![PathBuf::from("code.txt")],
                    risk: PodRisk::Routine,
                    rationale_summary: "bounded fixture".to_owned(),
                    proposal_sha256: "0".repeat(64),
                },
                &spec,
            )
            .unwrap();
            let AdmissionDecision::Admitted(lease) = scheduler
                .admit(&spec, generation, &initial, &proposal)
                .unwrap()
            else {
                panic!("fixture lease must be admitted");
            };
            let mut record = CampaignTaskRecord::planned(
                spec.campaign_id.clone(),
                TASK_ID.to_owned(),
                initial.clone(),
            )
            .unwrap();
            transition(&mut record, CampaignTaskState::Ready, "1");
            record.propose(generation, "2".repeat(64)).unwrap();
            record.bind_lease(&lease, "3".repeat(64)).unwrap();
            record
                .bind_run("run-task-candidate".to_owned(), "4".repeat(64))
                .unwrap();
            record
                .bind_worktree(
                    task_worktree.manifest().worktree_id.as_str().to_owned(),
                    task_worktree.manifest().branch.clone(),
                    "5".repeat(64),
                )
                .unwrap();
            record.candidate_commit = Some(candidate.clone());
            record.reviewed_commit = Some(candidate);
            transition(&mut record, CampaignTaskState::LocalGates, "6");
            transition(&mut record, CampaignTaskState::Reviewing, "7");
            transition(&mut record, CampaignTaskState::PublicationReady, "8");
            let snapshot = TeamCampaignSnapshot {
                version: TEAM_STATE_SCHEMA_VERSION,
                campaign_id: spec.campaign_id.clone(),
                generation,
                campaign_head: initial,
                task_source_sha256: spec.task_source_sha256.clone(),
                scheduler: scheduler.snapshot().clone(),
                resources: TeamResourceController::new(&spec)
                    .unwrap()
                    .snapshot()
                    .clone(),
                tasks: BTreeMap::from([(TASK_ID.to_owned(), record)]),
                integration_queue: Vec::new(),
            };
            snapshot.verify().unwrap();
            Self {
                root,
                config,
                authorization,
                campaign,
                task_worktree,
                snapshot,
            }
        }

        fn bind_prepared(&mut self) -> String {
            self.snapshot
                .mark_merge_ready(TASK_ID, "a".repeat(64))
                .unwrap();
            self.snapshot
                .begin_integration(TASK_ID, "b".repeat(64))
                .unwrap();
            let record = self.snapshot.tasks[TASK_ID].clone();
            let prepared = prepare_reviewed_delta(
                &self.authorization,
                &self.config,
                &self.campaign,
                RunId::new("run-prepared-crash").unwrap(),
                TaskId::new(TASK_ID).unwrap(),
                &self.snapshot.campaign_head,
                &record.base_commit,
                record.reviewed_commit.as_deref().unwrap(),
                &record.owned_paths,
            )
            .unwrap();
            let released = release_prepared_integration(&self.authorization, prepared).unwrap();
            self.snapshot
                .bind_integration_commit(TASK_ID, released.prepared_head.clone(), "c".repeat(64))
                .unwrap();
            released.prepared_head
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = remove_owned_worktree(&self.authorization, &mut self.task_worktree);
            let _ = remove_owned_worktree(&self.authorization, &mut self.campaign);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    // The verifier receives the worktree but deliberately has no repository-authority accessor.
    // Tests therefore use a verifier that only checks the immutable manifest source/head binding.
    struct ManifestVerifier(AtomicUsize);

    impl TeamIntegrationVerifier for ManifestVerifier {
        fn verify(
            &mut self,
            _: &str,
            worktree: &OwnedWorktree,
            prepared_commit: &str,
        ) -> Result<IntegrationVerification, RuntimeError> {
            assert_eq!(worktree.manifest().source_commit.len(), 40);
            assert_eq!(prepared_commit.len(), 40);
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(IntegrationVerification {
                gate_evidence_sha256: "d".repeat(64),
                review_evidence_sha256: "e".repeat(64),
            })
        }
    }

    struct RejectVerifier;

    impl TeamIntegrationVerifier for RejectVerifier {
        fn verify(
            &mut self,
            _: &str,
            _: &OwnedWorktree,
            _: &str,
        ) -> Result<IntegrationVerification, RuntimeError> {
            Err(RuntimeError::Verification)
        }
    }

    #[derive(Default)]
    struct ScopeVerifier(Vec<String>);

    impl TeamIntegrationVerifier for ScopeVerifier {
        fn verify(
            &mut self,
            task_id: &str,
            _: &OwnedWorktree,
            _: &str,
        ) -> Result<IntegrationVerification, RuntimeError> {
            self.0.push(task_id.to_owned());
            Ok(IntegrationVerification {
                gate_evidence_sha256: "d".repeat(64),
                review_evidence_sha256: "e".repeat(64),
            })
        }

        fn verify_from_base(
            &mut self,
            task_id: &str,
            worktree: &OwnedWorktree,
            base_commit: &str,
            prepared_commit: &str,
        ) -> Result<IntegrationVerification, RuntimeError> {
            assert_eq!(base_commit, worktree.manifest().source_commit);
            self.verify(task_id, worktree, prepared_commit)
        }
    }

    #[test]
    fn serialized_integration_completes_from_fresh_and_every_durable_git_boundary() {
        for boundary in 0..4 {
            let mut fixture = Fixture::new();
            enqueue_team_integration(&mut fixture.snapshot, TASK_ID, |_| Ok(())).unwrap();
            let mut expected_verifications = 1;
            if boundary > 0 {
                let integration = fixture.bind_prepared();
                expected_verifications = 0;
                if boundary > 1 {
                    integrate_reviewed_descendant(
                        &fixture.authorization,
                        &fixture.campaign,
                        &fixture.snapshot.campaign_head,
                        &integration,
                        &[PathBuf::from("code.txt")],
                    )
                    .unwrap();
                }
                if boundary > 2 {
                    create_completion_commit(
                        &fixture.config,
                        &fixture.authorization,
                        &fixture.campaign,
                        TASK_ID,
                        &integration,
                    )
                    .unwrap();
                }
            }
            let mut verifier = ManifestVerifier(AtomicUsize::new(0));
            let mut persisted = 0_usize;
            let outcome = integrate_team_queue_head(
                &fixture.config,
                &fixture.authorization,
                &fixture.campaign,
                &mut fixture.snapshot,
                &mut verifier,
                |_| {
                    persisted = persisted.saturating_add(1);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(verifier.0.load(Ordering::SeqCst), expected_verifications);
            assert!(persisted >= 1);
            assert_eq!(
                fixture.snapshot.tasks[TASK_ID].state,
                CampaignTaskState::Merged
            );
            assert!(fixture.snapshot.integration_queue.is_empty());
            assert_eq!(fixture.snapshot.campaign_head, outcome.completion_commit);
            assert_eq!(
                fixture
                    .campaign
                    .observe_head(&fixture.authorization)
                    .unwrap(),
                outcome.completion_commit
            );
            assert_eq!(
                completed_task_source_digest(&fixture.config, &fixture.campaign, TASK_ID).unwrap(),
                outcome.task_source_sha256
            );
        }
    }

    #[test]
    fn configured_interval_runs_cumulative_validation_before_head_mutation() {
        let mut fixture = Fixture::new();
        enqueue_team_integration(&mut fixture.snapshot, TASK_ID, |_| Ok(())).unwrap();
        let mut verifier = ScopeVerifier::default();
        let outcome = integrate_team_queue_head_with_validation(
            &fixture.config,
            &fixture.authorization,
            &fixture.campaign,
            &mut fixture.snapshot,
            &mut verifier,
            TaskMergeStrategy::Squash,
            Some(1),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            verifier.0,
            vec![TASK_ID.to_owned(), "campaign-batch-1".to_owned()]
        );
        assert_eq!(outcome.completion_commit, fixture.snapshot.campaign_head);
    }

    #[test]
    fn fast_forward_strategy_preserves_reviewed_commit() {
        let mut fixture = Fixture::new();
        let reviewed = fixture.snapshot.tasks[TASK_ID]
            .reviewed_commit
            .clone()
            .unwrap();
        enqueue_team_integration(&mut fixture.snapshot, TASK_ID, |_| Ok(())).unwrap();
        let mut verifier = ManifestVerifier(AtomicUsize::new(0));
        let outcome = integrate_team_queue_head_with_strategy(
            &fixture.config,
            &fixture.authorization,
            &fixture.campaign,
            &mut fixture.snapshot,
            &mut verifier,
            TaskMergeStrategy::FastForwardOnly,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(outcome.integration_commit, reviewed);
        assert_eq!(verifier.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture.snapshot.tasks[TASK_ID].state,
            CampaignTaskState::Merged
        );
    }

    #[test]
    fn fast_forward_strategy_refuses_a_stale_candidate_base() {
        let mut fixture = Fixture::new();
        std::fs::write(
            fixture.campaign.manifest().path.join("code.txt"),
            "campaign advanced\n",
        )
        .unwrap();
        let advanced = commit_owned_changes(
            &fixture.authorization,
            &fixture.campaign,
            &fixture.snapshot.campaign_head,
            &[PathBuf::from("code.txt")],
        )
        .unwrap()
        .commit;
        fixture.snapshot.campaign_head = advanced;
        fixture.snapshot.verify().unwrap();
        enqueue_team_integration(&mut fixture.snapshot, TASK_ID, |_| Ok(())).unwrap();
        let mut verifier = ManifestVerifier(AtomicUsize::new(0));
        assert_eq!(
            integrate_team_queue_head_with_strategy(
                &fixture.config,
                &fixture.authorization,
                &fixture.campaign,
                &mut fixture.snapshot,
                &mut verifier,
                TaskMergeStrategy::FastForwardOnly,
                |_| Ok(()),
            ),
            Err(RuntimeError::Integration)
        );
        assert_eq!(verifier.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failed_integration_verification_releases_temporary_worktrees() {
        for strategy in [
            TaskMergeStrategy::Squash,
            TaskMergeStrategy::FastForwardOnly,
        ] {
            let mut fixture = Fixture::new();
            let before = git_output(
                &fixture.config.target_path,
                &["worktree", "list", "--porcelain"],
            )
            .matches("worktree ")
            .count();
            enqueue_team_integration(&mut fixture.snapshot, TASK_ID, |_| Ok(())).unwrap();
            assert_eq!(
                integrate_team_queue_head_with_strategy(
                    &fixture.config,
                    &fixture.authorization,
                    &fixture.campaign,
                    &mut fixture.snapshot,
                    &mut RejectVerifier,
                    strategy,
                    |_| Ok(()),
                ),
                Err(RuntimeError::Verification)
            );
            let after = git_output(
                &fixture.config.target_path,
                &["worktree", "list", "--porcelain"],
            )
            .matches("worktree ")
            .count();
            assert_eq!(after, before);
        }
    }

    fn campaign_spec(
        config: &Config,
        authorization: &RepositoryAuthorization,
        initial: &str,
    ) -> codingmage_campaign::CampaignSpec {
        let plan = TaskPlan::parse(TASKS.as_bytes()).unwrap();
        codingmage_campaign::CampaignSpec {
            version: 3,
            campaign_id: "campaign-integration".to_owned(),
            repository_id: authorization.identity().repository_id.as_str().to_owned(),
            repository_path: config.target_path.clone(),
            initial_commit: initial.to_owned(),
            task_source_sha256: plan.source_sha256,
            operator_authorization_sha256: "f".repeat(64),
            max_parallel_pods: 1,
            max_units: 10,
            limits: CampaignLimits {
                provider_attempts: 10,
                malformed_report_repairs: 2,
                correction_rounds: 3,
                process_invocations: 100,
                output_bytes: 1_000_000,
                retained_state_bytes: 1_000_000,
                execution_elapsed_ms: 1_000_000,
            },
            team_lead: provider("codex"),
            implementer: provider("claude"),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider("codex"),
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["workspace".to_owned()],
            }],
            campaign_branch: "codingmage/campaign".to_owned(),
            allowed_paths: vec![PathBuf::from("code.txt")],
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: Some(MultiAgentPolicy {
                version: 1,
                execution_mode: CampaignExecutionMode::Parallel,
                publication_mode: TaskPublicationMode::LocalOnly,
                task_integration_policy: TaskIntegrationPolicy::AutoToCampaignBranch,
                destination_promotion_policy: DestinationPromotionPolicy::HumanRequired,
                task_merge_strategy: TaskMergeStrategy::Squash,
                github: None,
                concurrency: CampaignConcurrency {
                    claude_implementers: 1,
                    codex_team_leads: 1,
                    codex_reviewers: 1,
                    test_workers: 1,
                    github_writers: 1,
                    integration_workers: 1,
                },
                resources: TeamResourcePolicy::default(),
                max_campaign_tokens: 1_000_000,
                max_task_tokens: 100_000,
                max_task_correction_cycles: 3,
                max_follow_up_tasks: 10,
                integration_validation_interval: 1,
            }),
        }
    }

    fn provider(name: &str) -> CampaignProvider {
        CampaignProvider {
            executable: PathBuf::from(format!("/usr/bin/{name}")),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        }
    }

    fn transition(record: &mut CampaignTaskRecord, to: CampaignTaskState, digit: &str) {
        record
            .transition(&CampaignTaskTransition {
                sequence: record.next_transition,
                campaign_id: record.campaign_id.clone(),
                task_id: record.task_id.clone(),
                generation: record.generation,
                from: record.state,
                to,
                evidence_sha256: digit.repeat(64),
            })
            .unwrap();
    }

    fn git(directory: &Path, arguments: &[&str]) {
        assert!(
            Command::new("/usr/bin/git")
                .current_dir(directory)
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }

    fn git_commit(directory: &Path, message: &str) {
        git(
            directory,
            &[
                "-c",
                "user.name=CodingMage Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                message,
            ],
        );
    }

    fn git_output(directory: &Path, arguments: &[&str]) -> String {
        let output = Command::new("/usr/bin/git")
            .current_dir(directory)
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
}
