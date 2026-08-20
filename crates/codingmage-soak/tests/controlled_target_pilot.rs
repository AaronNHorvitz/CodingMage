//! Disposable controlled-target fake-agent pilot and repository-preservation evidence.

use std::{
    collections::BTreeSet,
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_contracts::{EvidenceId, RunId, TaskId};
use codingmage_orchestrator::{
    OneUnitCoordinator, OrchestrationError, ReviewOutcome, TaskState, VerificationOutcome,
    WorkflowPort,
};
use codingmage_plan::TaskPlan;
use codingmage_soak::materialize_controlled_target_pilot_fixture;

struct FakeAgents {
    calls: Vec<&'static str>,
}

impl FakeAgents {
    fn evidence(&mut self, value: &'static str) -> Result<EvidenceId, OrchestrationError> {
        self.calls.push(value);
        EvidenceId::new(value).map_err(|_| OrchestrationError::Evidence)
    }
}

impl WorkflowPort for FakeAgents {
    fn claim(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("claim")
    }

    fn start_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("start")
    }

    fn finish_implementation(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("implementation")
    }

    fn verify_local(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        Ok((VerificationOutcome::Pass, self.evidence("local")?))
    }

    fn review(&mut self) -> Result<(ReviewOutcome, EvidenceId), OrchestrationError> {
        Ok((ReviewOutcome::Pass, self.evidence("review")?))
    }

    fn correct(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("correction")
    }

    fn verify_final(&mut self) -> Result<(VerificationOutcome, EvidenceId), OrchestrationError> {
        Ok((VerificationOutcome::Pass, self.evidence("final")?))
    }

    fn checkpoint(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("checkpoint")
    }

    fn reconcile_completion(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("complete")
    }

    fn release(&mut self) -> Result<EvidenceId, OrchestrationError> {
        self.evidence("release")
    }
}

fn observe(repository: &std::path::Path) -> (Vec<u8>, Vec<u8>) {
    let head = Command::new("/usr/bin/git")
        .current_dir(repository)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let status = Command::new("/usr/bin/git")
        .current_dir(repository)
        .args(["status", "--porcelain=v2", "--branch"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert!(status.status.success());
    (head.stdout, status.stdout)
}

#[test]
fn fake_controlled_target_pilot_completes_without_repository_mutation() {
    let root = std::env::temp_dir().join(format!(
        "codingmage-controlled-target-pilot-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let fixture = materialize_controlled_target_pilot_fixture(&root).unwrap();
    let before = observe(&fixture.root);
    let source = fs::read(fixture.root.join("TASKS.md")).unwrap();
    let plan = TaskPlan::parse(&source).unwrap();
    let selected = plan.select_next(&BTreeSet::new()).unwrap();
    assert_eq!(selected.item.id, "0.1.1.1");

    let mut coordinator = OneUnitCoordinator::new(
        RunId::new("controlled-target-pilot-run").unwrap(),
        TaskId::new("controlled-target-preview").unwrap(),
    );
    let mut agents = FakeAgents { calls: Vec::new() };
    assert_eq!(coordinator.run(&mut agents), Ok(TaskState::Complete));
    assert_eq!(
        agents.calls,
        [
            "claim",
            "start",
            "implementation",
            "local",
            "review",
            "final",
            "checkpoint",
            "complete",
            "release"
        ]
    );
    assert_eq!(observe(&fixture.root), before);
    fs::remove_dir_all(fixture.root).unwrap();
}
