//! Real-process campaign Setup publication through the coordinator's public command.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use codingmage_campaign::{
    CampaignAuthentication, CampaignGateTier, CampaignLimits, CampaignProvider,
    CampaignPublication, CampaignSpec,
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

struct Fixture {
    root: PathBuf,
    target: PathBuf,
    config: PathBuf,
    record: PathBuf,
    repository_id: String,
    head: String,
    task_sha256: String,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codingmage-setup-campaign-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = root.join("repository");
        let workspace = root.join("workspace");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir(&workspace).unwrap();
        git(&target, &["init", "--initial-branch=main"]);
        git(&target, &["config", "user.name", "CodingMage Fixture"]);
        git(
            &target,
            &["config", "user.email", "fixture@invalid.example"],
        );
        fs::write(target.join("TASKS.md"), "# Tasks\n\n## Sprint 0 - Start\n\n**Sprint goal:** Start safely.\n\n### Story 0.1 - First\n\n- [ ] **Task 0.1.1 - Work**\n  - [ ] **Sub-task 0.1.1.1:** Complete the fixture operation.\n").unwrap();
        git(&target, &["add", "TASKS.md"]);
        git(&target, &["commit", "-m", "fixture"]);
        let config = workspace.join("codingmage.toml");
        let initialized = run(
            &[
                "init",
                "--repo",
                target.to_str().unwrap(),
                "--config",
                config.to_str().unwrap(),
                "--scratch",
                root.join("scratch").to_str().unwrap(),
                "--state",
                root.join("state").to_str().unwrap(),
            ],
            b"",
        );
        assert!(
            initialized.status.success(),
            "{}",
            String::from_utf8_lossy(&initialized.stderr)
        );
        let diagnosed = run(&["doctor", "--config", config.to_str().unwrap()], b"");
        assert!(
            diagnosed.status.success(),
            "{}",
            String::from_utf8_lossy(&diagnosed.stderr)
        );
        let diagnosis: Value = serde_json::from_slice(&diagnosed.stdout).unwrap();
        let repository_id = diagnosis["repository_id"].as_str().unwrap().to_owned();
        let head = diagnosis["head"].as_str().unwrap().to_owned();
        let task_sha256 = diagnosis["task_source_sha256"].as_str().unwrap().to_owned();
        let record = workspace.join("authorization.txt");
        let authorized = run(
            &[
                "setup-write-authorization",
                "--config",
                config.to_str().unwrap(),
                "--repository-id",
                &repository_id,
                "--output",
                record.to_str().unwrap(),
            ],
            b"Synthetic owner authorization for disposable fixture.",
        );
        assert!(
            authorized.status.success(),
            "{}",
            String::from_utf8_lossy(&authorized.stderr)
        );
        Self {
            root,
            target,
            config,
            record,
            repository_id,
            head,
            task_sha256,
        }
    }

    fn spec(&self) -> CampaignSpec {
        let provider = CampaignProvider {
            executable: PathBuf::from("/usr/bin/git"),
            model: "fixture".to_owned(),
            effort: "high".to_owned(),
        };
        let digest = digest(&fs::read(&self.record).unwrap());
        CampaignSpec {
            version: 3,
            campaign_id: "setup-fixture".to_owned(),
            repository_id: self.repository_id.clone(),
            repository_path: self.target.clone(),
            initial_commit: self.head.clone(),
            task_source_sha256: self.task_sha256.clone(),
            operator_authorization_sha256: digest,
            max_parallel_pods: 1,
            max_units: 1,
            limits: CampaignLimits {
                provider_attempts: 1000,
                malformed_report_repairs: 100,
                correction_rounds: 100,
                process_invocations: 10000,
                output_bytes: 1_073_741_824,
                retained_state_bytes: 1_073_741_824,
                execution_elapsed_ms: 86_400_000,
            },
            team_lead: provider.clone(),
            implementer: provider.clone(),
            implementer_authentication: CampaignAuthentication::ExistingLogin,
            reviewer: provider,
            gate_tiers: vec![CampaignGateTier {
                name: "focused".to_owned(),
                profiles: vec!["configured-gates".to_owned()],
            }],
            campaign_branch: "codingmage/setup-fixture".to_owned(),
            allowed_paths: vec![PathBuf::from("src")],
            task_path_authority: Vec::new(),
            denied_paths: Vec::new(),
            protected_branches: vec!["main".to_owned()],
            publication: CampaignPublication::LocalOnly,
            multi_agent: None,
        }
    }

    fn publish(
        &self,
        output: &Path,
        bytes: &[u8],
        overwrite: bool,
        repository_id: &str,
        head: &str,
        task_sha256: &str,
    ) -> Output {
        run(
            &[
                "setup-write-campaign",
                "--config",
                self.config.to_str().unwrap(),
                "--repository-id",
                repository_id,
                "--head",
                head,
                "--task-source-sha256",
                task_sha256,
                "--authorization",
                self.record.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--overwrite",
                if overwrite { "true" } else { "false" },
            ],
            bytes,
        )
    }

    fn write(&self, output: &Path, bytes: &[u8], overwrite: bool) -> Output {
        self.publish(
            output,
            bytes,
            overwrite,
            &self.repository_id,
            &self.head,
            &self.task_sha256,
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .current_dir(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codingmage"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(input);
    child.wait_with_output().unwrap()
}

fn digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

#[test]
fn exact_campaign_bytes_are_private_bound_and_explicitly_replaceable() {
    let fixture = Fixture::new();
    let output = fixture.root.join("workspace/campaign.toml");
    let first = fixture.spec();
    let first_bytes = toml::to_string_pretty(&first).unwrap();
    let result = fixture.write(&output, first_bytes.as_bytes(), false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&output).unwrap(), first_bytes.as_bytes());
    assert_eq!(CampaignSpec::load(&output).unwrap(), first);
    let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["repository_id"], fixture.repository_id);
    assert_eq!(receipt["campaign_id"], "setup-fixture");
    assert_eq!(receipt["head"], fixture.head);
    assert_eq!(receipt["sha256"], digest(first_bytes.as_bytes()));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("operator_authorization_sha256"));

    let mut changed = first;
    changed.max_units = 2;
    let changed_bytes = toml::to_string_pretty(&changed).unwrap();
    let refused = fixture.write(&output, changed_bytes.as_bytes(), false);
    assert_eq!(refused.stderr, b"codingmage.cli.refused\n");
    assert_eq!(fs::read(&output).unwrap(), first_bytes.as_bytes());
    let replaced = fixture.write(&output, changed_bytes.as_bytes(), true);
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    assert_eq!(fs::read(&output).unwrap(), changed_bytes.as_bytes());
}

#[test]
fn stale_and_malformed_campaign_inputs_never_publish() {
    let fixture = Fixture::new();
    let output = fixture.root.join("workspace/campaign.toml");
    let spec = fixture.spec();
    let bytes = toml::to_string_pretty(&spec).unwrap();
    let stale_id = fixture.publish(
        &output,
        bytes.as_bytes(),
        false,
        "other",
        &fixture.head,
        &fixture.task_sha256,
    );
    assert_eq!(stale_id.stderr, b"codingmage.cli.stale_observation\n");
    let stale_head = fixture.publish(
        &output,
        bytes.as_bytes(),
        false,
        &fixture.repository_id,
        &"a".repeat(40),
        &fixture.task_sha256,
    );
    assert_eq!(stale_head.stderr, b"codingmage.cli.stale_observation\n");
    let stale_plan = fixture.publish(
        &output,
        bytes.as_bytes(),
        false,
        &fixture.repository_id,
        &fixture.head,
        &"b".repeat(64),
    );
    assert_eq!(stale_plan.stderr, b"codingmage.cli.stale_observation\n");
    let mut mismatched = spec.clone();
    mismatched.operator_authorization_sha256 = "c".repeat(64);
    let mismatch = fixture.write(
        &output,
        toml::to_string_pretty(&mismatched).unwrap().as_bytes(),
        false,
    );
    assert_eq!(mismatch.stderr, b"codingmage.cli.stale_observation\n");
    for invalid in [
        b"\xff".to_vec(),
        b"version = 3\nunknown = true\n".to_vec(),
        vec![b'x'; 1024 * 1024 + 1],
    ] {
        let refused = fixture.write(&output, &invalid, false);
        assert_eq!(refused.stderr, b"codingmage.cli.invalid_argument\n");
    }
    assert!(!output.exists());
    let inside = fixture.target.join("campaign.toml");
    let refused = fixture.write(&inside, bytes.as_bytes(), false);
    assert_eq!(refused.stderr, b"codingmage.cli.refused\n");
    assert!(!inside.exists());

    for protected in [&fixture.config, &fixture.record] {
        let original = fs::read(protected).unwrap();
        let refused = fixture.write(protected, bytes.as_bytes(), true);
        assert_eq!(refused.stderr, b"codingmage.cli.refused\n");
        assert_eq!(fs::read(protected).unwrap(), original);
    }
    let config_as_record = run(
        &[
            "setup-write-campaign",
            "--config",
            fixture.config.to_str().unwrap(),
            "--repository-id",
            &fixture.repository_id,
            "--head",
            &fixture.head,
            "--task-source-sha256",
            &fixture.task_sha256,
            "--authorization",
            fixture.config.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
        bytes.as_bytes(),
    );
    assert_eq!(config_as_record.stderr, b"codingmage.cli.refused\n");
    assert!(!output.exists());

    fs::write(&fixture.record, b"changed authorization record").unwrap();
    let refused = fixture.write(&output, bytes.as_bytes(), false);
    assert_eq!(refused.stderr, b"codingmage.cli.stale_observation\n");
    assert!(!output.exists());
}

#[test]
fn changed_head_and_task_blob_are_not_adopted_from_a_stale_form() {
    let fixture = Fixture::new();
    let output = fixture.root.join("workspace/campaign.toml");
    let bytes = toml::to_string_pretty(&fixture.spec()).unwrap();
    fs::write(
        fixture.target.join("TASKS.md"),
        "# Changed but uncommitted\n",
    )
    .unwrap();
    let dirty = fixture.write(&output, bytes.as_bytes(), false);
    assert_eq!(dirty.stderr, b"codingmage.cli.stale_observation\n");
    git(&fixture.target, &["add", "TASKS.md"]);
    git(&fixture.target, &["commit", "-m", "changed task source"]);
    let changed = fixture.write(&output, bytes.as_bytes(), false);
    assert_eq!(changed.stderr, b"codingmage.cli.stale_observation\n");
    assert!(!output.exists());
}
