//! Concrete, fail-closed composition for one supervised `CodingMage` unit.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_claude::{
    ClaudeAdapter, ClaudeAuthentication, ClaudeCompletionReport, ClaudeSession, ClaudeWorkPacket,
};
use codingmage_codex::{CodexAdapter, CodexReviewBinding, ReviewVerdict, codex_review_schema};
use codingmage_contracts::{AgentId, AttemptId, EvidenceId, RunId, TaskId};
use codingmage_core::{Config, RepositoryAuthorization};
use codingmage_gate::{
    GateAssertion, GateEntry, GateRegistry, GateRequirement, GateRunner, GateTier, GateTrigger,
    TrustedGateDefinition,
};
use codingmage_git::{
    CommitReceipt, OwnedWorktree, commit_owned_changes, create_owned_worktree,
    inventory_repository, remove_owned_worktree,
};
use codingmage_orchestrator::{
    DurableWorkflowPort, OneUnitCoordinator, OrchestrationError, ReviewOutcome, TaskState,
    VerificationOutcome, WorkflowPort, reconcile_and_select_next,
};
use codingmage_plan::{PlanItemKind, SelectedWork, TaskPlan};
use codingmage_process::{CancellationToken, ProcessExecutor, ProcessProfile, ProcessRequest};
use codingmage_service::CoordinatorLock;
use codingmage_state::Journal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const MAX_SPEC_BYTES: u64 = 1024 * 1024;

/// Credential discovery available to one implementation invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationMode {
    /// Claude's strict bare mode; an external helper must provide authentication.
    Bare,
    /// Claude Code may use its existing login, while `CodingMage` never receives the credential.
    ExistingLogin,
}

/// Exact provider executable and model profile selected by an operator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    /// Absolute provider executable.
    pub executable: PathBuf,
    /// Provider model selector.
    pub model: String,
    /// Provider reasoning or effort selector.
    pub effort: String,
}

/// Claude-specific implementation profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementerSpec {
    /// Shared executable and model fields.
    #[serde(flatten)]
    pub provider: ProviderSpec,
    /// Credential-discovery boundary.
    pub authentication: AuthenticationMode,
    /// Literal provider-side cost ceiling for one invocation.
    pub maximum_budget_usd: String,
}

/// Explicit authority for one supervised unit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    /// Run-spec schema version.
    pub version: u16,
    /// Exact dependency-ready sub-task.
    pub task_id: String,
    /// Relative paths the implementation may change.
    pub owned_paths: Vec<PathBuf>,
    /// Whether a passing run may close the canonical task or must stop at a reviewed checkpoint.
    pub completion_policy: CompletionPolicy,
    /// Claude implementation profile.
    pub implementer: ImplementerSpec,
    /// Codex read-only review profile.
    pub reviewer: ProviderSpec,
}

/// Canonical completion authority for one run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionPolicy {
    /// Retain reviewed implementation progress without changing the canonical task checkbox.
    CandidateOnly,
    /// Require zero provider-reported limitations and create an exact completion marker.
    CloseTask,
}

impl RunSpec {
    /// Loads and validates one explicitly selected run specification.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for linked, oversized, malformed, or unsafe input.
    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        let metadata = fs::symlink_metadata(path).map_err(|_| RuntimeError::Spec)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_SPEC_BYTES
        {
            return Err(RuntimeError::Spec);
        }
        let value: Self =
            toml::from_str(&fs::read_to_string(path).map_err(|_| RuntimeError::Spec)?)
                .map_err(|_| RuntimeError::Spec)?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        if self.version != 1
            || TaskId::new(self.task_id.clone()).is_err()
            || self.owned_paths.is_empty()
            || self.owned_paths.iter().any(|path| !safe_relative(path))
            || !valid_provider(&self.implementer.provider)
            || !valid_provider(&self.reviewer)
            || !valid_budget(&self.implementer.maximum_budget_usd)
        {
            return Err(RuntimeError::Spec);
        }
        Ok(())
    }
}

/// Content-minimized terminal result for one supervised unit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutcome {
    /// Exact run identity.
    pub run_id: RunId,
    /// Exact task identity.
    pub task_id: TaskId,
    /// Terminal coordinator state.
    pub state: TaskState,
    /// Retained feature branch; absent before worktree creation.
    pub branch: Option<String>,
    /// Reviewed implementation commit; absent before coordinator commit.
    pub candidate_commit: Option<String>,
    /// Mechanical completion-marker commit; present only after a passing review.
    pub completion_commit: Option<String>,
    /// Structured review verdict when review ran.
    pub review_verdict: Option<String>,
}

/// Runs one exact supervised unit using durable intent records and bounded provider processes.
///
/// The function never merges, pushes, opens a pull request, publishes, or mutates the active
/// checkout. A passing run retains a local feature branch containing the reviewed implementation
/// and exact checklist completion marker.
///
/// # Errors
///
/// Returns [`RuntimeError`] for invalid authority, provider, Git, gate, durable-state, or lifecycle
/// behavior. Failed state is retained for diagnosis without provider output or credentials.
pub fn run_one(
    config: &Config,
    spec: RunSpec,
    codingmage_binary: &Path,
) -> Result<RunOutcome, RuntimeError> {
    spec.validate()?;
    let codingmage_binary = canonical_file(codingmage_binary)?;
    let source_root = codingmage_binary.parent().ok_or(RuntimeError::Authority)?;
    let authorization = RepositoryAuthorization::authorize(config, source_root)
        .map_err(|_| RuntimeError::Authority)?;
    let inventory = inventory_repository(&authorization).map_err(|_| RuntimeError::Repository)?;
    if !inventory.condition.is_clean() {
        return Err(RuntimeError::Repository);
    }
    let source_path = config.target_path.join(&config.task_source);
    let source = fs::read(&source_path).map_err(|_| RuntimeError::Plan)?;
    let plan = TaskPlan::parse(&source).map_err(|_| RuntimeError::Plan)?;
    let selected = plan
        .select_exact(&spec.task_id)
        .map_err(|_| RuntimeError::Plan)?;
    let run_id = generated_run_id()?;
    let task_id = TaskId::new(spec.task_id.clone()).map_err(|_| RuntimeError::Spec)?;
    let run_root = config.state_root.join("runs").join(run_id.as_str());
    private_directory(&run_root)?;
    let process_root = run_root.join("processes");
    let executor = ProcessExecutor::new_with_guard_arguments(
        &codingmage_binary,
        vec!["__process-guard".to_owned()],
        &process_root,
    )
    .map_err(|_| RuntimeError::Process)?;
    let schema_path = run_root.join("codex-review.schema.json");
    write_private_new(&schema_path, codex_review_schema().as_bytes())?;
    let mut journal = Journal::open(&run_root, format!("{}-journal", run_id.as_str()))
        .map_err(|_| RuntimeError::State)?;
    let mut port = ProductionWorkflowPort::new(ProductionInputs {
        config,
        authorization,
        selected,
        source,
        source_commit: inventory.head,
        run_id: run_id.clone(),
        task_id: task_id.clone(),
        spec,
        executor,
        schema_path,
        run_root,
    });
    let repository_id = port.authorization.identity().repository_id.clone();
    let completion_policy = port.spec.completion_policy;
    let mut coordinator = OneUnitCoordinator::new(run_id.clone(), task_id.clone());
    let result = {
        let mut durable = DurableWorkflowPort::new(
            &mut port,
            &mut journal,
            repository_id,
            run_id.clone(),
            task_id.clone(),
        );
        match completion_policy {
            CompletionPolicy::CandidateOnly => coordinator.run_to_checkpoint(&mut durable),
            CompletionPolicy::CloseTask => coordinator.run(&mut durable),
        }
    };
    let outcome = port.outcome(run_id, task_id, coordinator.state());
    result.map_err(|_| RuntimeError::Orchestration)?;
    Ok(outcome)
}

struct ProductionInputs<'a> {
    config: &'a Config,
    authorization: RepositoryAuthorization,
    selected: SelectedWork,
    source: Vec<u8>,
    source_commit: String,
    run_id: RunId,
    task_id: TaskId,
    spec: RunSpec,
    executor: ProcessExecutor,
    schema_path: PathBuf,
    run_root: PathBuf,
}

struct ProductionWorkflowPort<'a> {
    config: &'a Config,
    authorization: RepositoryAuthorization,
    selected: SelectedWork,
    source: Vec<u8>,
    source_commit: String,
    run_id: RunId,
    task_id: TaskId,
    spec: RunSpec,
    executor: ProcessExecutor,
    schema_path: PathBuf,
    run_root: PathBuf,
    lock: Option<CoordinatorLock>,
    worktree: Option<OwnedWorktree>,
    implementation: Option<ClaudeCompletionReport>,
    candidate: Option<CommitReceipt>,
    completion: Option<CommitReceipt>,
    gate_evidence: Vec<EvidenceId>,
    review_verdict: Option<ReviewVerdict>,
}

impl<'a> ProductionWorkflowPort<'a> {
    fn new(inputs: ProductionInputs<'a>) -> Self {
        Self {
            config: inputs.config,
            authorization: inputs.authorization,
            selected: inputs.selected,
            source: inputs.source,
            source_commit: inputs.source_commit,
            run_id: inputs.run_id,
            task_id: inputs.task_id,
            spec: inputs.spec,
            executor: inputs.executor,
            schema_path: inputs.schema_path,
            run_root: inputs.run_root,
            lock: None,
            worktree: None,
            implementation: None,
            candidate: None,
            completion: None,
            gate_evidence: Vec::new(),
            review_verdict: None,
        }
    }

    fn outcome(&self, run_id: RunId, task_id: TaskId, state: TaskState) -> RunOutcome {
        RunOutcome {
            run_id,
            task_id,
            state,
            branch: self
                .worktree
                .as_ref()
                .map(|owned| owned.manifest().branch.clone()),
            candidate_commit: self
                .candidate
                .as_ref()
                .map(|receipt| receipt.commit.clone()),
            completion_commit: self
                .completion
                .as_ref()
                .map(|receipt| receipt.commit.clone()),
            review_verdict: self.review_verdict.map(verdict_name).map(str::to_owned),
        }
    }

    fn worktree(&self) -> Result<&OwnedWorktree, OrchestrationError> {
        self.worktree.as_ref().ok_or(OrchestrationError::Port)
    }

    fn candidate(&self) -> Result<&CommitReceipt, OrchestrationError> {
        self.candidate.as_ref().ok_or(OrchestrationError::Port)
    }

    fn acceptance_criteria(&self) -> Vec<String> {
        let task = self.selected.item.parent_id.as_str();
        let story = task.rsplit_once('.').map_or(task, |(parent, _)| parent);
        let plan = TaskPlan::parse(&self.source).expect("validated task source");
        let criteria = plan
            .items
            .iter()
            .filter(|item| {
                item.kind == PlanItemKind::AcceptanceCriterion && item.parent_id == story
            })
            .map(|item| format!("{}: {}", item.id, item.title))
            .collect::<Vec<_>>();
        if criteria.is_empty() {
            vec![format!(
                "The exact sub-task {} is implemented and all configured gates pass.",
                self.task_id
            )]
        } else {
            criteria
        }
    }

    fn claude_adapter(&self) -> Result<ClaudeAdapter, OrchestrationError> {
        let authentication = match self.spec.implementer.authentication {
            AuthenticationMode::Bare => ClaudeAuthentication::Bare,
            AuthenticationMode::ExistingLogin => ClaudeAuthentication::ExistingLogin,
        };
        ClaudeAdapter::new(
            self.spec.implementer.provider.executable.clone(),
            &self.spec.implementer.provider.model,
            &self.spec.implementer.provider.effort,
            &self.spec.implementer.maximum_budget_usd,
        )
        .map(|adapter| adapter.with_authentication(authentication))
        .map_err(|_| OrchestrationError::Port)
    }

    fn codex_adapter(&self) -> Result<CodexAdapter, OrchestrationError> {
        CodexAdapter::new(
            self.spec.reviewer.executable.clone(),
            &self.spec.reviewer.model,
            &self.spec.reviewer.effort,
            self.schema_path.clone(),
        )
        .map_err(|_| OrchestrationError::Port)
    }

    fn claude_packet(&self) -> ClaudeWorkPacket {
        ClaudeWorkPacket {
            task_text: self.selected.item.title.clone(),
            dependencies: self.selected.item.dependencies.clone(),
            owned_paths: self.spec.owned_paths.clone(),
            acceptance_criteria: self.acceptance_criteria(),
            test_commands: self
                .config
                .gate_commands
                .iter()
                .map(|command| {
                    let mut value = vec![command.executable.display().to_string()];
                    value.extend(command.args.clone());
                    value
                })
                .collect(),
            prohibited_actions: vec![
                "Do not run Git or change TASKS.md.".to_owned(),
                "Do not merge, push, publish, release, access credentials, or use the network."
                    .to_owned(),
                "Do not modify files outside the declared owned paths.".to_owned(),
            ],
        }
    }

    fn run_gates(&mut self) -> Result<VerificationOutcome, OrchestrationError> {
        let commit = self.candidate()?.commit.clone();
        let worktree = self.worktree()?.manifest().path.clone();
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
                .map_err(|_| OrchestrationError::Port)?;
                Ok(GateEntry::Available(Box::new(TrustedGateDefinition {
                    id: format!("configured-gate-{}", index + 1),
                    tier: GateTier::Tier2,
                    trigger: GateTrigger::EveryAttempt,
                    requirement: GateRequirement::Required,
                    resources: BTreeSet::from(["target-repository".to_owned()]),
                    profile,
                    request: ProcessRequest {
                        arguments: command.args.clone(),
                        working_directory: worktree.clone(),
                        environment: BTreeMap::new(),
                        stdin: Vec::new(),
                        max_output_bytes: 16 * 1024 * 1024,
                        deadline_millis: 30 * 60 * 1000,
                        max_processes: 64,
                        max_open_files: 1024,
                        expected_exit_codes: BTreeSet::from([0]),
                    },
                    assertions: vec![GateAssertion::OutputNotTruncated],
                })))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let registry = GateRegistry::new(entries).map_err(|_| OrchestrationError::Port)?;
        let result = GateRunner::new(self.executor.clone())
            .run(&registry, &commit, &BTreeSet::new())
            .map_err(|_| OrchestrationError::Port)?;
        self.gate_evidence = result
            .evidence
            .iter()
            .map(|evidence| evidence_id(&evidence.integrity_sha256))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(if result.blocked {
            VerificationOutcome::RecoverableFailure
        } else {
            VerificationOutcome::Pass
        })
    }
}

impl WorkflowPort for ProductionWorkflowPort<'_> {
    fn claim(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.lock = Some(
            CoordinatorLock::acquire(
                &self.config.state_root.join("locks"),
                &self.authorization.identity().repository_id,
                self.run_id.as_str(),
            )
            .map_err(|_| OrchestrationError::Port)?,
        );
        evidence_id("claim")
    }

    fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
        let owned = create_owned_worktree(
            &self.authorization,
            self.config,
            self.run_id.clone(),
            self.task_id.clone(),
            &self.source_commit,
        )
        .map_err(|_| OrchestrationError::Port)?;
        let worktree = owned.manifest().path.clone();
        self.claude_adapter()?
            .probe(
                &self.executor,
                worktree.clone(),
                &CancellationToken::default(),
            )
            .map_err(|_| OrchestrationError::Port)?;
        self.codex_adapter()?
            .probe(&self.executor, worktree, &CancellationToken::default())
            .map_err(|_| OrchestrationError::Port)?;
        self.worktree = Some(owned);
        evidence_id("implementation-started")
    }

    fn finish_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
        let owned = self.worktree()?;
        let session = ClaudeSession {
            run_id: self.run_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: AgentId::new("claude-implementer").map_err(|_| OrchestrationError::Port)?,
            session_id: generated_attempt_id().map_err(|_| OrchestrationError::Port)?,
            worktree: owned.manifest().path.clone(),
            branch: owned.manifest().branch.clone(),
            source_commit: self.source_commit.clone(),
        };
        let adapter = self.claude_adapter()?;
        let plan = adapter
            .plan_start(&session, &self.claude_packet())
            .map_err(|_| OrchestrationError::Port)?;
        let (report, _) = adapter
            .execute(&self.executor, &plan, &CancellationToken::default())
            .map_err(|_| OrchestrationError::Port)?;
        if !report.ready_for_commit || report.blocker_code.is_some() || report.commit.is_some() {
            return Err(OrchestrationError::Port);
        }
        if self.spec.completion_policy == CompletionPolicy::CloseTask
            && !report.limitations.is_empty()
        {
            return Err(OrchestrationError::Port);
        }
        let receipt = commit_owned_changes(
            &self.authorization,
            owned,
            &self.source_commit,
            &self.spec.owned_paths,
        )
        .map_err(|_| OrchestrationError::Port)?;
        let claimed = report
            .changed_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let observed = receipt
            .changed_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if claimed != observed {
            return Err(OrchestrationError::Port);
        }
        let evidence = evidence_id(&receipt.commit)?;
        self.implementation = Some(report);
        self.candidate = Some(receipt);
        Ok(evidence)
    }

    fn verify_local(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        let outcome = self.run_gates()?;
        let identity = evidence_id(&format!("local-{}", self.candidate()?.commit))?;
        Ok((outcome, identity))
    }

    fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError> {
        let owned = self.worktree()?;
        let candidate = self.candidate()?;
        let binding = CodexReviewBinding {
            run_id: self.run_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: AgentId::new("codex-reviewer").map_err(|_| OrchestrationError::Port)?,
            thread_id: None,
            worktree: owned.manifest().path.clone(),
            base_commit: candidate.parent.clone(),
            target_commit: candidate.commit.clone(),
            evidence: self.gate_evidence.clone(),
        };
        let adapter = self.codex_adapter()?;
        let plan = adapter
            .plan_start(&binding, &self.selected.item.title)
            .map_err(|_| OrchestrationError::Port)?;
        let (result, _) = adapter
            .execute(
                &self.executor,
                &plan,
                &binding,
                &CancellationToken::default(),
            )
            .map_err(|_| OrchestrationError::Port)?;
        self.review_verdict = Some(result.report.verdict);
        let evidence = evidence_id(&format!(
            "review-{}-{}",
            result.thread_id, result.report.target_commit
        ))?;
        let outcome = match result.report.verdict {
            ReviewVerdict::Pass => ReviewOutcome::Pass,
            ReviewVerdict::Blocked | ReviewVerdict::ChangesRequired | ReviewVerdict::Disputed => {
                ReviewOutcome::Blocked
            }
        };
        Ok((outcome, evidence))
    }

    fn correct(&mut self) -> Result<EvidenceId, OrchestrationError> {
        Err(OrchestrationError::Port)
    }

    fn verify_final(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        let outcome = self.run_gates()?;
        let identity = evidence_id(&format!("final-{}", self.candidate()?.commit))?;
        Ok((outcome, identity))
    }

    fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError> {
        let candidate = self.candidate()?;
        let checkpoint = serde_json::json!({
            "schema_version": 1,
            "run_id": self.run_id,
            "task_id": self.task_id,
            "candidate_commit": candidate.commit,
            "review_verdict": self.review_verdict.map(verdict_name),
            "gate_evidence": self.gate_evidence,
        });
        write_private_idempotent(
            &self.run_root.join("checkpoint.json"),
            &serde_json::to_vec_pretty(&checkpoint).map_err(|_| OrchestrationError::Port)?,
        )
        .map_err(|_| OrchestrationError::Port)?;
        evidence_id(&format!("checkpoint-{}", candidate.commit))
    }

    fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError> {
        let candidate = self.candidate()?.clone();
        let owned = self.worktree()?;
        let task_path = owned.manifest().path.join(&self.config.task_source);
        let before = fs::read(&task_path).map_err(|_| OrchestrationError::Port)?;
        self.selected
            .revalidate(&before)
            .map_err(|_| OrchestrationError::PlanDrift)?;
        let after = check_exact_line(&before, self.selected.item.anchor.line)?;
        let completion_evidence = evidence_id(&format!("completion-{}", candidate.commit))?;
        reconcile_and_select_next(
            &before,
            &after,
            self.task_id.as_str(),
            &completion_evidence,
            &BTreeSet::new(),
        )?;
        fs::write(&task_path, &after).map_err(|_| OrchestrationError::Port)?;
        let receipt = commit_owned_changes(
            &self.authorization,
            owned,
            &candidate.commit,
            std::slice::from_ref(&self.config.task_source),
        )
        .map_err(|_| OrchestrationError::Port)?;
        if receipt.changed_paths != [self.config.task_source.clone()] {
            return Err(OrchestrationError::Port);
        }
        self.completion = Some(receipt);
        Ok(completion_evidence)
    }

    fn release(&mut self) -> Result<EvidenceId, OrchestrationError> {
        if let Some(mut owned) = self.worktree.take() {
            remove_owned_worktree(&self.authorization, &mut owned)
                .map_err(|_| OrchestrationError::Port)?;
            self.worktree = Some(owned);
        }
        self.lock = None;
        evidence_id("released")
    }
}

fn check_exact_line(source: &[u8], line_number: usize) -> Result<Vec<u8>, OrchestrationError> {
    let text = std::str::from_utf8(source).map_err(|_| OrchestrationError::PlanDrift)?;
    let mut lines = text
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let line = lines
        .get_mut(
            line_number
                .checked_sub(1)
                .ok_or(OrchestrationError::PlanDrift)?,
        )
        .ok_or(OrchestrationError::PlanDrift)?;
    let marker = line
        .find("- [ ] **Sub-task ")
        .ok_or(OrchestrationError::PlanDrift)?;
    line.replace_range(marker + 3..marker + 4, "x");
    Ok(lines.concat().into_bytes())
}

fn generated_run_id() -> Result<RunId, RuntimeError> {
    RunId::new(format!("run-{}", unique_hex())).map_err(|_| RuntimeError::State)
}

fn generated_attempt_id() -> Result<AttemptId, RuntimeError> {
    let value = unique_hex();
    AttemptId::new(format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    ))
    .map_err(|_| RuntimeError::State)
}

fn unique_hex() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let input = format!(
        "{}:{time}:{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    hex(&Sha256::digest(input.as_bytes()))[..32].to_owned()
}

fn evidence_id(value: &str) -> Result<EvidenceId, OrchestrationError> {
    EvidenceId::new(format!(
        "ev-{}",
        &hex(&Sha256::digest(value.as_bytes()))[..32]
    ))
    .map_err(|_| OrchestrationError::Evidence)
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

fn valid_provider(spec: &ProviderSpec) -> bool {
    canonical_file(&spec.executable).is_ok()
        && !spec.model.is_empty()
        && spec
            .model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && matches!(
            spec.effort.as_str(),
            "low" | "medium" | "high" | "xhigh" | "max"
        )
}

fn valid_budget(value: &str) -> bool {
    value.len() <= 16
        && value
            .parse::<f64>()
            .is_ok_and(|number| number > 0.0 && number <= 100.0)
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn canonical_file(path: &Path) -> Result<PathBuf, RuntimeError> {
    if !path.is_absolute() {
        return Err(RuntimeError::Spec);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| RuntimeError::Spec)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(RuntimeError::Spec);
    }
    fs::canonicalize(path).map_err(|_| RuntimeError::Spec)
}

fn private_directory(path: &Path) -> Result<(), RuntimeError> {
    fs::create_dir_all(path).map_err(|_| RuntimeError::State)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| RuntimeError::State)?;
    }
    Ok(())
}

fn write_private_new(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| RuntimeError::State)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| RuntimeError::State)?;
    }
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| RuntimeError::State)
}

fn write_private_idempotent(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_private_new(path, bytes)
        }
        Ok(_) | Err(_) => Err(RuntimeError::State),
    }
}

const fn verdict_name(verdict: ReviewVerdict) -> &'static str {
    match verdict {
        ReviewVerdict::Pass => "pass",
        ReviewVerdict::ChangesRequired => "changes_required",
        ReviewVerdict::Disputed => "disputed",
        ReviewVerdict::Blocked => "blocked",
    }
}

/// Stable content-free runtime failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// Run specification is unavailable or invalid.
    Spec,
    /// Repository authority could not be granted.
    Authority,
    /// Repository state is unsafe.
    Repository,
    /// Exact task source or selection is invalid.
    Plan,
    /// Guarded process runtime is unavailable.
    Process,
    /// Durable state could not be created or verified.
    State,
    /// One-unit orchestration failed closed.
    Orchestration,
}

impl RuntimeError {
    /// Stable public diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Spec => "codingmage.runtime.spec",
            Self::Authority => "codingmage.runtime.authority",
            Self::Repository => "codingmage.runtime.repository",
            Self::Plan => "codingmage.runtime.plan",
            Self::Process => "codingmage.runtime.process",
            Self::State => "codingmage.runtime.state",
            Self::Orchestration => "codingmage.runtime.orchestration",
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_rejects_credentials_relative_paths_and_unknown_fields() {
        let source = r#"
version = 1
task_id = "1.2.3.4"
owned_paths = ["src"]
completion_policy = "close_task"
unexpected = "secret"

[implementer]
executable = "/bin/true"
model = "opus"
effort = "high"
authentication = "existing_login"
maximum_budget_usd = "5.00"

[reviewer]
executable = "/bin/true"
model = "gpt-5.6-sol"
effort = "high"
"#;
        assert!(toml::from_str::<RunSpec>(source).is_err());
        let valid = source.replace("unexpected = \"secret\"\n", "");
        let spec: RunSpec = toml::from_str(&valid).unwrap();
        assert_eq!(spec.validate(), Ok(()));
        let escaping = valid.replace("owned_paths = [\"src\"]", "owned_paths = [\"../src\"]");
        assert_eq!(
            toml::from_str::<RunSpec>(&escaping).unwrap().validate(),
            Err(RuntimeError::Spec)
        );
    }

    #[test]
    fn exact_checkbox_change_preserves_every_other_byte() {
        let before = b"first\n  - [ ] **Sub-task 1.2.3.4:** Work.\nlast\n";
        let after = check_exact_line(before, 2).unwrap();
        assert_eq!(after, b"first\n  - [x] **Sub-task 1.2.3.4:** Work.\nlast\n");
        assert_eq!(
            check_exact_line(before, 1),
            Err(OrchestrationError::PlanDrift)
        );
    }
}
