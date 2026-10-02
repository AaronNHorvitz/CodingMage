//! Private, integrity-bound retention of inert director priority proposals.

use std::{fmt, path::Path};

use codingmage_campaign::{
    DirectorContext, DirectorInput, DirectorProposal, DirectorProposalError,
};
use codingmage_state::{IntegrityDocument, IntegrityDocumentError};
use sha2::{Digest as _, Sha256};

/// Coordinator-side storage boundary for exact source-bound director proposals.
pub struct DirectorProposalStore;

impl DirectorProposalStore {
    /// Writes one validated proposal to private state, keyed by its canonical digest.
    ///
    /// The proposal remains inert data. Admission and delegated-domain policy are separate.
    ///
    /// # Errors
    ///
    /// Refuses stale source, malformed proposal or failed durable storage.
    pub fn write(
        root: &Path,
        proposal: &DirectorProposal,
        input: &DirectorInput,
        context: &DirectorContext<'_>,
    ) -> Result<String, DirectorStoreError> {
        let input_sha256 = input.sha256(context).map_err(DirectorStoreError::Packet)?;
        proposal
            .verify(input, &input_sha256)
            .map_err(DirectorStoreError::Packet)?;
        let digest = proposal_digest(proposal)?;
        IntegrityDocument::write_atomic(root, &file_name(&digest), proposal.clone(), |value| {
            value.verify(input, &input_sha256).is_ok()
        })
        .map_err(DirectorStoreError::Storage)?;
        Ok(digest)
    }

    /// Reopens a proposal only when bytes, identity and fresh source still agree.
    ///
    /// # Errors
    ///
    /// Refuses changed authority, malformed input or tampered private state.
    pub fn load(
        root: &Path,
        digest: &str,
        input: &DirectorInput,
        context: &DirectorContext<'_>,
    ) -> Result<DirectorProposal, DirectorStoreError> {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(DirectorStoreError::Identity);
        }
        let input_sha256 = input.sha256(context).map_err(DirectorStoreError::Packet)?;
        let document =
            IntegrityDocument::<DirectorProposal>::load(root, &file_name(digest), |value| {
                value.verify(input, &input_sha256).is_ok()
            })
            .map_err(DirectorStoreError::Storage)?;
        if proposal_digest(&document.payload)? != digest {
            return Err(DirectorStoreError::Identity);
        }
        Ok(document.payload)
    }
}

fn proposal_digest(proposal: &DirectorProposal) -> Result<String, DirectorStoreError> {
    let bytes = serde_json::to_vec(proposal).map_err(|_| DirectorStoreError::Identity)?;
    let digest = Sha256::digest(bytes);
    Ok(super::hex(&digest))
}

fn file_name(digest: &str) -> String {
    format!("director-proposal-{digest}.json")
}

/// Content-free proposal storage failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectorStoreError {
    /// Proposed identity or encoding is invalid.
    Identity,
    /// The source-bound packet is stale or malformed.
    Packet(DirectorProposalError),
    /// The private document failed an integrity or I/O check.
    Storage(IntegrityDocumentError),
}

impl fmt::Display for DirectorStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Identity => "codingmage.director_store.identity",
            Self::Packet(_) => "codingmage.director_store.packet",
            Self::Storage(_) => "codingmage.director_store.storage",
        })
    }
}

impl std::error::Error for DirectorStoreError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use codingmage_campaign::{
        CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
        CampaignPublication, CampaignSpec, DIRECTOR_PACKET_VERSION, DurablePodScheduler,
        InvolvementMode, MISSION_VERSION, MissionBudgets, MissionCharter, RemainingDirectorLimits,
    };
    use codingmage_plan::TaskPlan;

    use super::*;

    const SOURCE: &str = "# Tasks\n\n## Sprint 1 - Work\n\n**Sprint goal:** Finish work.\n\n### Story 1.1 - Build\n\n- [ ] **Task 1.1.1 - Build**\n  - [ ] **Sub-task 1.1.1.1:** Ready.\n\n- [ ] **AC 1.1:** Accept.\n";

    fn authority(plan: &TaskPlan) -> (CampaignSpec, MissionCharter) {
        let provider = CampaignProvider {
            executable: PathBuf::from("/bin/true"),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        let spec = CampaignSpec {
            version: 3,
            campaign_id: "director-store-campaign".to_owned(),
            repository_id: "director-store-repository".to_owned(),
            repository_path: PathBuf::from("/tmp/director-store-repository"),
            initial_commit: "a".repeat(40),
            task_source_sha256: plan.source_sha256.clone(),
            operator_authorization_sha256: "b".repeat(64),
            max_parallel_pods: 1,
            max_units: 10,
            limits: CampaignLimits {
                provider_attempts: 10,
                malformed_report_repairs: 1,
                correction_rounds: 2,
                process_invocations: 20,
                output_bytes: 100_000,
                retained_state_bytes: 100_000,
                execution_elapsed_ms: 10_000,
            },
            team_lead: provider.clone(),
            implementer: provider.clone(),
            implementer_authentication: CampaignAuthentication::Bare,
            reviewer: provider,
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["unit".to_owned()],
            }],
            campaign_branch: "codingmage/director-store-campaign".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        };
        let mission = MissionCharter {
            version: MISSION_VERSION,
            mission_id: "director-store-mission".to_owned(),
            generation: 1,
            campaign_id: spec.campaign_id.clone(),
            repository_id: spec.repository_id.clone(),
            initial_commit: spec.initial_commit.clone(),
            task_source_sha256: spec.task_source_sha256.clone(),
            operator_authorization_sha256: spec.operator_authorization_sha256.clone(),
            campaign_authority_sha256: spec.authority_sha256().unwrap(),
            objective: "Finish approved work".to_owned(),
            success_criteria: vec!["Approved work passes".to_owned()],
            exclusions: Vec::new(),
            architecture_invariants: Vec::new(),
            decision_domains: Vec::new(),
            command_registry: vec!["unit".to_owned()],
            involvement: InvolvementMode::HandsOff,
            budgets: MissionBudgets {
                max_decisions: 10,
                max_decision_retries: 2,
                max_no_progress_cycles: 3,
            },
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
            revocation_epoch: 0,
        };
        mission.verify(&spec).unwrap();
        (spec, mission)
    }

    #[test]
    fn private_proposal_round_trip_refuses_stale_or_tampered_source() {
        let plan = TaskPlan::parse(SOURCE.as_bytes()).unwrap();
        let (spec, mission) = authority(&plan);
        let mut scheduler = DurablePodScheduler::new(&spec).unwrap();
        scheduler.begin_generation(&["1.1.1.1".to_owned()]).unwrap();
        let context = DirectorContext {
            spec: &spec,
            mission: &mission,
            plan: &plan,
            scheduler: scheduler.snapshot(),
            campaign_head: &spec.initial_commit,
        };
        let input = DirectorInput::build(
            &context,
            Vec::new(),
            RemainingDirectorLimits {
                provider_attempts: spec.limits.provider_attempts,
                process_invocations: spec.limits.process_invocations,
                output_bytes: spec.limits.output_bytes,
                retained_state_bytes: spec.limits.retained_state_bytes,
                execution_elapsed_ms: spec.limits.execution_elapsed_ms,
            },
        )
        .unwrap();
        let proposal = DirectorProposal {
            version: DIRECTOR_PACKET_VERSION,
            source: input.source.clone(),
            input_sha256: input.sha256(&context).unwrap(),
            priority_order: input.ready_task_ids.clone(),
            attention_criteria: vec![0],
        };
        let root = std::env::temp_dir().join(format!(
            "codingmage-director-store-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let digest = DirectorProposalStore::write(&root, &proposal, &input, &context).unwrap();
        let mut invented = proposal.clone();
        invented.priority_order = vec!["9.9.9.9".to_owned()];
        assert_eq!(
            DirectorProposalStore::write(&root, &invented, &input, &context),
            Err(DirectorStoreError::Packet(DirectorProposalError::Proposal))
        );
        assert_eq!(
            DirectorProposalStore::load(&root, &digest, &input, &context),
            Ok(proposal.clone())
        );
        let changed = DirectorContext {
            campaign_head: &"c".repeat(40),
            ..context
        };
        assert_eq!(
            DirectorProposalStore::load(&root, &digest, &input, &changed),
            Err(DirectorStoreError::Packet(DirectorProposalError::Source))
        );
        let path = root.join(file_name(&digest));
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 1;
        fs::write(path, bytes).unwrap();
        assert!(matches!(
            DirectorProposalStore::load(&root, &digest, &input, &context),
            Err(DirectorStoreError::Storage(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
