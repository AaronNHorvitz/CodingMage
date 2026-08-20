//! Trusted gate registry, resource-aware execution, and tamper-evident evidence.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_process::{
    CancellationToken, DescendantCleanup, ExecutableIdentity, ProcessExecutor, ProcessOutcome,
    ProcessProfile, ProcessRequest, ProcessResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_GATES: usize = 256;
const MAX_ASSERTIONS: usize = 32;
const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;

/// Deterministic verification depth.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateTier {
    /// Fast syntax and formatting checks.
    Tier0,
    /// Focused unit checks.
    Tier1,
    /// Package and integration checks.
    Tier2,
    /// Security, mutation, and recovery checks.
    Tier3,
    /// Release and independent-review checks.
    Tier4,
}

/// Policy importance of one gate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateRequirement {
    /// Failure blocks progression.
    Required,
    /// Policy may explicitly skip or tolerate unavailability.
    Optional,
}

/// Stable trigger selected by deterministic orchestration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateTrigger {
    /// Every implementation attempt.
    EveryAttempt,
    /// Relevant path changed.
    PathChange,
    /// Story completion candidate.
    StoryBoundary,
    /// Release candidate.
    ReleaseBoundary,
}

/// Expected observation beyond process exit status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GateAssertion {
    /// Neither retained output stream crossed its configured byte ceiling.
    OutputNotTruncated,
    /// Exact standard-output digest.
    StdoutSha256 {
        /// Expected lowercase hexadecimal digest.
        value: String,
    },
    /// Exact standard-error digest.
    StderrSha256 {
        /// Expected lowercase hexadecimal digest.
        value: String,
    },
    /// Exact standard-output byte count.
    StdoutBytes {
        /// Expected complete stream length.
        value: u64,
    },
    /// Exact standard-error byte count.
    StderrBytes {
        /// Expected complete stream length.
        value: u64,
    },
}

/// Trusted in-process gate definition. It is intentionally not deserializable.
#[derive(Clone, Debug)]
pub struct TrustedGateDefinition {
    /// Stable gate identifier.
    pub id: String,
    /// Verification tier.
    pub tier: GateTier,
    /// Deterministic trigger.
    pub trigger: GateTrigger,
    /// Required or optional policy.
    pub requirement: GateRequirement,
    /// Exact resources whose concurrent use would conflict.
    pub resources: BTreeSet<String>,
    /// Pinned process profile.
    pub profile: ProcessProfile,
    /// Exact bounded process request.
    pub request: ProcessRequest,
    /// Required observations beyond exit status.
    pub assertions: Vec<GateAssertion>,
}

impl TrustedGateDefinition {
    /// Validates one coordinator-authored definition.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::InvalidDefinition`] for missing identity, resources, assertions, or
    /// malformed expected digests.
    pub fn validate(&self) -> Result<(), GateError> {
        if !valid_id(&self.id)
            || self.resources.is_empty()
            || self.assertions.is_empty()
            || self.assertions.len() > MAX_ASSERTIONS
            || self.assertions.iter().any(|assertion| match assertion {
                GateAssertion::StdoutSha256 { value } | GateAssertion::StderrSha256 { value } => {
                    !valid_sha256(value)
                }
                GateAssertion::OutputNotTruncated
                | GateAssertion::StdoutBytes { .. }
                | GateAssertion::StderrBytes { .. } => false,
            })
        {
            return Err(GateError::InvalidDefinition);
        }
        Ok(())
    }
}

/// Explicitly unavailable gate retained as evidence rather than omitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnavailableGate {
    /// Stable gate identifier.
    pub id: String,
    /// Verification tier.
    pub tier: GateTier,
    /// Required or optional policy.
    pub requirement: GateRequirement,
    /// Content-free reason code.
    pub reason_code: String,
}

/// One trusted registry entry.
#[derive(Clone, Debug)]
pub enum GateEntry {
    /// Executable trusted definition.
    Available(Box<TrustedGateDefinition>),
    /// Explicitly unavailable gate.
    Unavailable(UnavailableGate),
}

/// Immutable trusted registry.
#[derive(Clone, Debug)]
pub struct GateRegistry {
    entries: Vec<GateEntry>,
}

impl GateRegistry {
    /// Constructs a registry from coordinator-owned definitions.
    ///
    /// # Errors
    ///
    /// Returns [`GateError`] for duplicate IDs, invalid entries, or excessive size.
    pub fn new(entries: Vec<GateEntry>) -> Result<Self, GateError> {
        if entries.is_empty() || entries.len() > MAX_GATES {
            return Err(GateError::InvalidRegistry);
        }
        let mut ids = BTreeSet::new();
        for entry in &entries {
            let id = match entry {
                GateEntry::Available(definition) => {
                    definition.validate()?;
                    definition.id.as_str()
                }
                GateEntry::Unavailable(unavailable) => {
                    if !valid_id(&unavailable.id) || !valid_id(&unavailable.reason_code) {
                        return Err(GateError::InvalidDefinition);
                    }
                    unavailable.id.as_str()
                }
            };
            if !ids.insert(id) {
                return Err(GateError::DuplicateGate);
            }
        }
        Ok(Self { entries })
    }

    /// Returns trusted entries in registry order.
    #[must_use]
    pub fn entries(&self) -> &[GateEntry] {
        &self.entries
    }
}

/// Terminal gate outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// Process and required assertions passed.
    Passed,
    /// Process, cleanup, or an assertion failed.
    Failed,
    /// Gate could not run in this environment.
    Unavailable,
    /// Optional gate was skipped by explicit policy.
    SkippedWithPolicy,
}

/// Process observation retained without stdout or stderr content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateProcessEvidence {
    /// Exact executable identity.
    pub executable: ExecutableIdentity,
    /// SHA-256 of the canonical argument vector.
    pub arguments_sha256: String,
    /// Names of explicitly granted environment entries.
    pub environment_names: BTreeSet<String>,
    /// Process terminal classification.
    pub process_outcome: String,
    /// Literal target exit code.
    pub exit_code: i32,
    /// Full stdout digest.
    pub stdout_sha256: String,
    /// Full stderr digest.
    pub stderr_sha256: String,
    /// Total stdout bytes.
    pub stdout_bytes: u64,
    /// Total stderr bytes.
    pub stderr_bytes: u64,
    /// Whether either retained stream was truncated.
    pub truncated: bool,
    /// Exact descendant cleanup result.
    pub descendant_cleanup: String,
}

/// Complete content-minimized gate-definition identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateDefinitionEvidence {
    /// Deterministic trigger.
    pub trigger: GateTrigger,
    /// Declared conflicting resources.
    pub resources: BTreeSet<String>,
    /// SHA-256 of the exact working-directory bytes.
    pub working_directory_sha256: String,
    /// SHA-256 of bounded standard input.
    pub stdin_sha256: String,
    /// Output retention ceiling per stream.
    pub max_output_bytes: u64,
    /// Wall-clock deadline.
    pub deadline_millis: u64,
    /// Process-count ceiling.
    pub max_processes: u32,
    /// Open-file ceiling.
    pub max_open_files: u64,
    /// Exit codes eligible for success before assertions.
    pub expected_exit_codes: BTreeSet<i32>,
    /// Required observations beyond exit status.
    pub assertions: Vec<GateAssertion>,
}

/// Tamper-evident outcome for one gate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvidence {
    /// Evidence schema version.
    pub version: u16,
    /// Gate identifier.
    pub gate_id: String,
    /// Verification tier.
    pub tier: GateTier,
    /// Required or optional policy.
    pub requirement: GateRequirement,
    /// Exact source commit.
    pub source_commit: String,
    /// Start time in Unix milliseconds.
    pub started_unix_ms: u64,
    /// End time in Unix milliseconds.
    pub ended_unix_ms: u64,
    /// Terminal gate outcome.
    pub outcome: GateOutcome,
    /// Content-free reason for unavailable or skipped work.
    pub reason_code: Option<String>,
    /// Complete trusted definition when an executable gate was scheduled.
    pub definition: Option<GateDefinitionEvidence>,
    /// Process evidence when execution began.
    pub process: Option<GateProcessEvidence>,
    /// Hash of all preceding fields.
    pub integrity_sha256: String,
}

impl GateEvidence {
    /// Recomputes and verifies evidence integrity.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::Evidence`] after any field mutation.
    pub fn verify(&self) -> Result<(), GateError> {
        if self.version != 1 || !valid_commit(&self.source_commit) {
            return Err(GateError::Evidence);
        }
        let mut unsigned = self.clone();
        unsigned.integrity_sha256.clear();
        let encoded = serde_json::to_vec(&unsigned).map_err(|_| GateError::Evidence)?;
        if sha256(&encoded) == self.integrity_sha256 {
            Ok(())
        } else {
            Err(GateError::Evidence)
        }
    }
}

/// Complete ordered gate run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateRun {
    /// Evidence in registry order.
    pub evidence: Vec<GateEvidence>,
    /// Ephemeral bounded output for failed gates, never part of durable evidence.
    pub diagnostics: Vec<GateDiagnostic>,
    /// Whether any required gate failed or was unavailable.
    pub blocked: bool,
}

/// Bounded gate output returned only to the active correction loop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateDiagnostic {
    /// Stable gate identity.
    pub gate_id: String,
    /// Retained standard output, bounded independently from process evidence.
    pub stdout: String,
    /// Retained standard error, bounded independently from process evidence.
    pub stderr: String,
    /// Whether either source stream exceeded the diagnostic bound.
    pub truncated: bool,
}

struct GateExecution {
    evidence: GateEvidence,
    diagnostic: Option<GateDiagnostic>,
}

/// Bounded gate-progress phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateProgressKind {
    /// Gate entered the deterministic schedule.
    Scheduled,
    /// Gate reached one terminal evidence record.
    Finished,
}

/// Content-free progress notification emitted at most twice per gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateProgress {
    /// Contiguous run-local sequence.
    pub sequence: u64,
    /// Stable gate identifier.
    pub gate_id: String,
    /// Progress phase.
    pub kind: GateProgressKind,
    /// Terminal outcome only for [`GateProgressKind::Finished`].
    pub outcome: Option<GateOutcome>,
}

/// Concurrent runner around the shared bounded process executor.
#[derive(Clone, Debug)]
pub struct GateRunner {
    executor: ProcessExecutor,
}

impl GateRunner {
    /// Creates a runner for one process executor.
    #[must_use]
    pub const fn new(executor: ProcessExecutor) -> Self {
        Self { executor }
    }

    /// Runs conflict-free gates concurrently and cancels a live batch after a required failure.
    ///
    /// Optional unavailable gates require an explicit skip identifier; required unavailable gates
    /// always block. Evidence is returned in registry order.
    ///
    /// # Errors
    ///
    /// Returns [`GateError`] for invalid source identity, skip policy, definitions, or worker loss.
    pub fn run(
        &self,
        registry: &GateRegistry,
        source_commit: &str,
        optional_skips: &BTreeSet<String>,
    ) -> Result<GateRun, GateError> {
        self.run_with_progress(registry, source_commit, optional_skips, |_| {})
    }

    /// Runs gates while emitting content-free, rate-bounded progress notifications.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::run`]. Observers receive exactly one scheduled and one
    /// finished event per registry entry and have no cancellation or mutation authority.
    #[allow(clippy::too_many_lines)]
    pub fn run_with_progress(
        &self,
        registry: &GateRegistry,
        source_commit: &str,
        optional_skips: &BTreeSet<String>,
        mut observe: impl FnMut(&GateProgress),
    ) -> Result<GateRun, GateError> {
        if !valid_commit(source_commit) {
            return Err(GateError::InvalidSource);
        }
        let mut progress_sequence = 0_u64;
        for entry in registry.entries() {
            let gate_id = match entry {
                GateEntry::Available(definition) => definition.id.clone(),
                GateEntry::Unavailable(unavailable) => unavailable.id.clone(),
            };
            observe(&GateProgress {
                sequence: progress_sequence,
                gate_id,
                kind: GateProgressKind::Scheduled,
                outcome: None,
            });
            progress_sequence = progress_sequence.saturating_add(1);
        }
        let known: BTreeSet<&str> = registry
            .entries()
            .iter()
            .map(|entry| match entry {
                GateEntry::Available(definition) => definition.id.as_str(),
                GateEntry::Unavailable(unavailable) => unavailable.id.as_str(),
            })
            .collect();
        if optional_skips.iter().any(|id| !known.contains(id.as_str())) {
            return Err(GateError::InvalidSkip);
        }
        let mut indexed = Vec::new();
        let mut immediate = BTreeMap::new();
        for (index, entry) in registry.entries().iter().enumerate() {
            match entry {
                GateEntry::Available(definition) => {
                    indexed.push((index, definition.as_ref().clone()));
                }
                GateEntry::Unavailable(unavailable) => {
                    let skipped = unavailable.requirement == GateRequirement::Optional
                        && optional_skips.contains(&unavailable.id);
                    let evidence = unavailable_evidence(unavailable, source_commit, skipped)?;
                    observe(&GateProgress {
                        sequence: progress_sequence,
                        gate_id: evidence.gate_id.clone(),
                        kind: GateProgressKind::Finished,
                        outcome: Some(evidence.outcome),
                    });
                    progress_sequence = progress_sequence.saturating_add(1);
                    immediate.insert(index, evidence);
                }
            }
        }

        let mut completed = immediate;
        let mut diagnostics = BTreeMap::new();
        for batch in conflict_free_batches(indexed) {
            if completed.values().any(blocking_evidence) {
                for (index, definition) in batch {
                    let evidence =
                        skipped_evidence(&definition, source_commit, "prior-required-failure")?;
                    observe(&GateProgress {
                        sequence: progress_sequence,
                        gate_id: evidence.gate_id.clone(),
                        kind: GateProgressKind::Finished,
                        outcome: Some(evidence.outcome),
                    });
                    progress_sequence = progress_sequence.saturating_add(1);
                    completed.insert(index, evidence);
                }
                continue;
            }
            let cancellation = CancellationToken::default();
            let (sender, receiver) = mpsc::channel();
            thread::scope(|scope| {
                for (index, definition) in batch {
                    let sender = sender.clone();
                    let executor = self.executor.clone();
                    let cancellation = cancellation.clone();
                    let source_commit = source_commit.to_owned();
                    scope.spawn(move || {
                        let evidence =
                            execute_gate(&executor, &definition, &source_commit, &cancellation);
                        let blocking = evidence
                            .as_ref()
                            .is_ok_and(|execution| blocking_evidence(&execution.evidence));
                        let _ = sender.send((index, evidence, blocking));
                    });
                }
                drop(sender);
                for (index, execution, blocking) in receiver {
                    if blocking {
                        cancellation.cancel();
                    }
                    let execution = execution?;
                    let evidence = execution.evidence;
                    if let Some(diagnostic) = execution.diagnostic {
                        diagnostics.insert(index, diagnostic);
                    }
                    observe(&GateProgress {
                        sequence: progress_sequence,
                        gate_id: evidence.gate_id.clone(),
                        kind: GateProgressKind::Finished,
                        outcome: Some(evidence.outcome),
                    });
                    progress_sequence = progress_sequence.saturating_add(1);
                    completed.insert(index, evidence);
                }
                Ok::<(), GateError>(())
            })?;
        }
        let mut evidence: Vec<(usize, GateEvidence)> = completed.into_iter().collect();
        evidence.sort_by_key(|(index, _)| *index);
        let evidence: Vec<GateEvidence> = evidence.into_iter().map(|(_, item)| item).collect();
        let blocked = evidence.iter().any(blocking_evidence);
        let diagnostics = diagnostics.into_values().collect();
        Ok(GateRun {
            evidence,
            diagnostics,
            blocked,
        })
    }
}

/// Content-free gate failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateError {
    /// Registry is empty or excessive.
    InvalidRegistry,
    /// Definition is malformed or lacks assertions.
    InvalidDefinition,
    /// Gate identifier occurs more than once.
    DuplicateGate,
    /// Source commit is malformed.
    InvalidSource,
    /// Skip policy names an unknown gate.
    InvalidSkip,
    /// Process failed before trustworthy evidence existed.
    Process,
    /// Evidence serialization or integrity failed.
    Evidence,
}

impl fmt::Display for GateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRegistry => "codingmage.gate.invalid_registry",
            Self::InvalidDefinition => "codingmage.gate.invalid_definition",
            Self::DuplicateGate => "codingmage.gate.duplicate",
            Self::InvalidSource => "codingmage.gate.invalid_source",
            Self::InvalidSkip => "codingmage.gate.invalid_skip",
            Self::Process => "codingmage.gate.process",
            Self::Evidence => "codingmage.gate.evidence",
        })
    }
}

impl std::error::Error for GateError {}

fn conflict_free_batches(
    definitions: Vec<(usize, TrustedGateDefinition)>,
) -> Vec<Vec<(usize, TrustedGateDefinition)>> {
    type GateBatch = (BTreeSet<String>, Vec<(usize, TrustedGateDefinition)>);
    let mut batches: Vec<GateBatch> = Vec::new();
    for (index, definition) in definitions {
        let definition_resources = definition.resources.clone();
        if let Some((resources, entries)) = batches
            .iter_mut()
            .find(|(resources, _)| resources.is_disjoint(&definition.resources))
        {
            resources.extend(definition_resources);
            entries.push((index, definition));
        } else {
            batches.push((definition_resources, vec![(index, definition)]));
        }
    }
    batches.into_iter().map(|(_, entries)| entries).collect()
}

fn execute_gate(
    executor: &ProcessExecutor,
    definition: &TrustedGateDefinition,
    source_commit: &str,
    cancellation: &CancellationToken,
) -> Result<GateExecution, GateError> {
    definition.validate()?;
    let started = unix_millis()?;
    let result = executor
        .execute(&definition.profile, &definition.request, cancellation)
        .map_err(|_| GateError::Process)?;
    let assertions_pass = definition
        .assertions
        .iter()
        .all(|assertion| assertion_matches(assertion, &result));
    let passed = result.outcome == ProcessOutcome::Succeeded && assertions_pass;
    let ended = unix_millis()?;
    let evidence = sign_evidence(GateEvidence {
        version: 1,
        gate_id: definition.id.clone(),
        tier: definition.tier,
        requirement: definition.requirement,
        source_commit: source_commit.to_owned(),
        started_unix_ms: started,
        ended_unix_ms: ended,
        outcome: if passed {
            GateOutcome::Passed
        } else {
            GateOutcome::Failed
        },
        reason_code: None,
        definition: Some(definition_evidence(definition)),
        process: Some(process_evidence(definition, &result)?),
        integrity_sha256: String::new(),
    })?;
    let diagnostic = (!passed).then(|| GateDiagnostic {
        gate_id: definition.id.clone(),
        stdout: bounded_diagnostic(&result.stdout.retained),
        stderr: bounded_diagnostic(&result.stderr.retained),
        truncated: result.stdout.retained.len() > MAX_DIAGNOSTIC_BYTES
            || result.stderr.retained.len() > MAX_DIAGNOSTIC_BYTES
            || result.stdout.truncated
            || result.stderr.truncated,
    });
    Ok(GateExecution {
        evidence,
        diagnostic,
    })
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_DIAGNOSTIC_BYTES)]).into_owned()
}

fn unavailable_evidence(
    gate: &UnavailableGate,
    source_commit: &str,
    skipped: bool,
) -> Result<GateEvidence, GateError> {
    let now = unix_millis()?;
    sign_evidence(GateEvidence {
        version: 1,
        gate_id: gate.id.clone(),
        tier: gate.tier,
        requirement: gate.requirement,
        source_commit: source_commit.to_owned(),
        started_unix_ms: now,
        ended_unix_ms: now,
        outcome: if skipped {
            GateOutcome::SkippedWithPolicy
        } else {
            GateOutcome::Unavailable
        },
        reason_code: Some(gate.reason_code.clone()),
        definition: None,
        process: None,
        integrity_sha256: String::new(),
    })
}

fn skipped_evidence(
    gate: &TrustedGateDefinition,
    source_commit: &str,
    reason_code: &str,
) -> Result<GateEvidence, GateError> {
    let now = unix_millis()?;
    sign_evidence(GateEvidence {
        version: 1,
        gate_id: gate.id.clone(),
        tier: gate.tier,
        requirement: gate.requirement,
        source_commit: source_commit.to_owned(),
        started_unix_ms: now,
        ended_unix_ms: now,
        outcome: GateOutcome::SkippedWithPolicy,
        reason_code: Some(reason_code.to_owned()),
        definition: Some(definition_evidence(gate)),
        process: None,
        integrity_sha256: String::new(),
    })
}

fn process_evidence(
    definition: &TrustedGateDefinition,
    result: &ProcessResult,
) -> Result<GateProcessEvidence, GateError> {
    let arguments =
        serde_json::to_vec(&definition.request.arguments).map_err(|_| GateError::Evidence)?;
    Ok(GateProcessEvidence {
        executable: definition.profile.executable().clone(),
        arguments_sha256: sha256(&arguments),
        environment_names: definition.request.environment.keys().cloned().collect(),
        process_outcome: process_outcome_name(result.outcome).to_owned(),
        exit_code: result.exit_code,
        stdout_sha256: result.stdout.sha256.clone(),
        stderr_sha256: result.stderr.sha256.clone(),
        stdout_bytes: result.stdout.total_bytes,
        stderr_bytes: result.stderr.total_bytes,
        truncated: result.stdout.truncated || result.stderr.truncated,
        descendant_cleanup: cleanup_name(result.descendant_cleanup).to_owned(),
    })
}

fn definition_evidence(definition: &TrustedGateDefinition) -> GateDefinitionEvidence {
    GateDefinitionEvidence {
        trigger: definition.trigger,
        resources: definition.resources.clone(),
        working_directory_sha256: sha256(
            definition
                .request
                .working_directory
                .as_os_str()
                .as_encoded_bytes(),
        ),
        stdin_sha256: sha256(&definition.request.stdin),
        max_output_bytes: definition.request.max_output_bytes,
        deadline_millis: definition.request.deadline_millis,
        max_processes: definition.request.max_processes,
        max_open_files: definition.request.max_open_files,
        expected_exit_codes: definition.request.expected_exit_codes.clone(),
        assertions: definition.assertions.clone(),
    }
}

fn assertion_matches(assertion: &GateAssertion, result: &ProcessResult) -> bool {
    match assertion {
        GateAssertion::OutputNotTruncated => !result.stdout.truncated && !result.stderr.truncated,
        GateAssertion::StdoutSha256 { value } => &result.stdout.sha256 == value,
        GateAssertion::StderrSha256 { value } => &result.stderr.sha256 == value,
        GateAssertion::StdoutBytes { value } => result.stdout.total_bytes == *value,
        GateAssertion::StderrBytes { value } => result.stderr.total_bytes == *value,
    }
}

fn blocking_evidence(evidence: &GateEvidence) -> bool {
    evidence.requirement == GateRequirement::Required && evidence.outcome != GateOutcome::Passed
}

fn sign_evidence(mut evidence: GateEvidence) -> Result<GateEvidence, GateError> {
    let encoded = serde_json::to_vec(&evidence).map_err(|_| GateError::Evidence)?;
    evidence.integrity_sha256 = sha256(&encoded);
    Ok(evidence)
}

const fn process_outcome_name(outcome: ProcessOutcome) -> &'static str {
    match outcome {
        ProcessOutcome::Succeeded => "succeeded",
        ProcessOutcome::Failed => "failed",
        ProcessOutcome::TimedOut => "timed_out",
        ProcessOutcome::Cancelled => "cancelled",
        ProcessOutcome::OutputLimit => "output_limit",
        ProcessOutcome::ParentLost => "parent_lost",
        ProcessOutcome::RuntimeFailure => "runtime_failure",
    }
}

const fn cleanup_name(cleanup: DescendantCleanup) -> &'static str {
    match cleanup {
        DescendantCleanup::NotRequired => "not_required",
        DescendantCleanup::Verified => "verified",
        DescendantCleanup::Uncertain => "uncertain",
    }
}

fn unix_millis() -> Result<u64, GateError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GateError::Evidence)?
        .as_millis();
    u64::try_from(millis).map_err(|_| GateError::Evidence)
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

fn sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
