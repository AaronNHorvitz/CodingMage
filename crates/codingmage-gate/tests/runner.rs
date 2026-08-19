//! Live bounded-process integration coverage for the deterministic gate runner.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use codingmage_gate::{
    GateAssertion, GateEntry, GateError, GateOutcome, GateProgressKind, GateRegistry,
    GateRequirement, GateRunner, GateTier, GateTrigger, TrustedGateDefinition, UnavailableGate,
};
use codingmage_process::{ProcessExecutor, ProcessProfile, ProcessRequest};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
    executor: ProcessExecutor,
    executable: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-gate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let guard = PathBuf::from(env!("CARGO_BIN_EXE_codingmage-gate-guard"));
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_codingmage-gate-fixture"));
        let executor = ProcessExecutor::new(&guard, &root.join("control")).unwrap();
        Self {
            root,
            executor,
            executable,
        }
    }

    fn gate(&self, id: &str, mode: &str, required: bool) -> TrustedGateDefinition {
        let arguments = vec![mode.to_owned()];
        TrustedGateDefinition {
            id: id.to_owned(),
            tier: GateTier::Tier1,
            trigger: GateTrigger::EveryAttempt,
            requirement: if required {
                GateRequirement::Required
            } else {
                GateRequirement::Optional
            },
            resources: BTreeSet::from([id.to_owned()]),
            profile: ProcessProfile::new(&self.executable, [arguments.clone()], []).unwrap(),
            request: ProcessRequest {
                arguments,
                working_directory: self.root.clone(),
                environment: BTreeMap::new(),
                stdin: Vec::new(),
                max_output_bytes: 1024,
                deadline_millis: 500,
                max_processes: 4,
                max_open_files: 64,
                expected_exit_codes: BTreeSet::from([0]),
            },
            assertions: vec![GateAssertion::StderrBytes { value: 0 }],
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn pass_failure_timeout_and_output_limit_are_truthful() {
    let fixture = Fixture::new();
    let registry = GateRegistry::new(vec![
        GateEntry::Available(Box::new(fixture.gate("pass", "pass", true))),
        GateEntry::Available(Box::new(fixture.gate("fail", "fail", false))),
        GateEntry::Available(Box::new(fixture.gate("sleep", "sleep", false))),
        GateEntry::Available(Box::new(fixture.gate("noisy", "noisy", false))),
    ])
    .unwrap();
    let run = GateRunner::new(fixture.executor.clone())
        .run(&registry, &"a".repeat(40), &BTreeSet::new())
        .unwrap();
    assert!(!run.blocked);
    assert_eq!(run.evidence[0].outcome, GateOutcome::Passed);
    assert!(
        run.evidence[1..]
            .iter()
            .all(|item| item.outcome == GateOutcome::Failed)
    );
    assert!(run.evidence.iter().all(|item| item.verify().is_ok()));
}

#[test]
fn required_failure_cancels_other_live_gate_and_blocks() {
    let fixture = Fixture::new();
    let mut failing = fixture.gate("required", "fail", true);
    failing.resources = BTreeSet::from(["failure".to_owned()]);
    let mut sleeping = fixture.gate("sleeping", "sleep", false);
    sleeping.resources = BTreeSet::from(["sleep".to_owned()]);
    let mut later = fixture.gate("later", "pass", true);
    later.resources = failing.resources.clone();
    let registry = GateRegistry::new(vec![
        GateEntry::Available(Box::new(failing)),
        GateEntry::Available(Box::new(sleeping)),
        GateEntry::Available(Box::new(later)),
    ])
    .unwrap();
    let run = GateRunner::new(fixture.executor.clone())
        .run(&registry, &"b".repeat(40), &BTreeSet::new())
        .unwrap();
    assert!(run.blocked);
    assert_eq!(run.evidence[0].outcome, GateOutcome::Failed);
    assert_eq!(
        run.evidence[1].process.as_ref().unwrap().process_outcome,
        "cancelled"
    );
    assert_eq!(run.evidence[2].outcome, GateOutcome::SkippedWithPolicy);
    assert_eq!(
        run.evidence[2].reason_code.as_deref(),
        Some("prior-required-failure")
    );
}

#[test]
fn unavailable_required_blocks_and_optional_skip_is_explicit() {
    let registry = GateRegistry::new(vec![
        GateEntry::Unavailable(UnavailableGate {
            id: "native-macos".to_owned(),
            tier: GateTier::Tier4,
            requirement: GateRequirement::Required,
            reason_code: "hardware-unavailable".to_owned(),
        }),
        GateEntry::Unavailable(UnavailableGate {
            id: "advisory".to_owned(),
            tier: GateTier::Tier3,
            requirement: GateRequirement::Optional,
            reason_code: "tool-unavailable".to_owned(),
        }),
    ])
    .unwrap();
    let fixture = Fixture::new();
    let skips = BTreeSet::from(["advisory".to_owned()]);
    let run = GateRunner::new(fixture.executor.clone())
        .run(&registry, &"c".repeat(40), &skips)
        .unwrap();
    assert!(run.blocked);
    assert_eq!(run.evidence[0].outcome, GateOutcome::Unavailable);
    assert_eq!(run.evidence[1].outcome, GateOutcome::SkippedWithPolicy);
}

#[test]
fn exit_zero_without_expected_assertion_cannot_pass() {
    let fixture = Fixture::new();
    let mut gate = fixture.gate("assertion", "pass", true);
    gate.assertions = vec![GateAssertion::StdoutBytes { value: 999 }];
    let registry = GateRegistry::new(vec![GateEntry::Available(Box::new(gate))]).unwrap();
    let run = GateRunner::new(fixture.executor.clone())
        .run(&registry, &"d".repeat(40), &BTreeSet::new())
        .unwrap();
    assert!(run.blocked);
    assert_eq!(run.evidence[0].process.as_ref().unwrap().exit_code, 0);
    assert_eq!(run.evidence[0].outcome, GateOutcome::Failed);
}

#[test]
fn every_evidence_field_mutation_breaks_integrity() {
    let fixture = Fixture::new();
    let registry = GateRegistry::new(vec![GateEntry::Available(Box::new(
        fixture.gate("pass", "pass", true),
    ))])
    .unwrap();
    let run = GateRunner::new(fixture.executor.clone())
        .run(&registry, &"e".repeat(40), &BTreeSet::new())
        .unwrap();
    let original = run.evidence[0].clone();
    let mut mutations = Vec::new();
    let mut changed = original.clone();
    changed.gate_id.push('x');
    mutations.push(changed);
    let mut changed = original.clone();
    changed.source_commit = "f".repeat(40);
    mutations.push(changed);
    let mut changed = original.clone();
    changed.outcome = GateOutcome::Failed;
    mutations.push(changed);
    let mut changed = original.clone();
    changed.process.as_mut().unwrap().stdout_bytes += 1;
    mutations.push(changed);
    let mut changed = original;
    changed.integrity_sha256.replace_range(..1, "0");
    mutations.push(changed);
    assert!(
        mutations
            .iter()
            .all(|evidence| evidence.verify() == Err(GateError::Evidence))
    );
}

#[test]
fn progress_is_content_free_contiguous_and_bounded() {
    let fixture = Fixture::new();
    let registry = GateRegistry::new(vec![
        GateEntry::Available(Box::new(fixture.gate("one", "pass", true))),
        GateEntry::Available(Box::new(fixture.gate("two", "pass", true))),
    ])
    .unwrap();
    let mut progress = Vec::new();
    let run = GateRunner::new(fixture.executor.clone())
        .run_with_progress(&registry, &"f".repeat(40), &BTreeSet::new(), |event| {
            progress.push(event.clone());
        })
        .unwrap();
    assert!(!run.blocked);
    assert_eq!(progress.len(), 4);
    assert_eq!(
        progress
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert!(
        progress[..2]
            .iter()
            .all(|event| event.kind == GateProgressKind::Scheduled && event.outcome.is_none())
    );
    assert!(
        progress[2..]
            .iter()
            .all(|event| event.kind == GateProgressKind::Finished && event.outcome.is_some())
    );
}
