//! Public, repository-bound guided Setup writes through the coordinator executable.

use std::{fs, io::Read, path::Path};

use codingmage_campaign::CampaignSpec;
use codingmage_core::{RepositoryAuthorization, load_config};
use codingmage_git::{inventory_repository, read_authorized_blob};
use codingmage_plan::TaskPlan;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments, executable_parent, report_writer};

const MAX_AUTHORIZATION_BYTES: usize = 1024 * 1024;
const MAX_CAMPAIGN_BYTES: usize = 1024 * 1024;

/// Writes the exact, bounded authorization record supplied on private stdin.
pub(super) fn authorization(arguments: &[String], input: impl Read) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(
        arguments,
        &["config", "repository-id", "output"],
        &["overwrite"],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let authority = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    if authority.identity().repository_id.as_str() != parsed.value("repository-id")? {
        return Err(CliError::StaleObservation);
    }
    let output = parsed.absolute_path("output")?;
    let overwrite = match parsed.optional_value("overwrite") {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(CliError::Usage),
    };
    report_writer::validate(&output, &config.target_path)?;
    let mut bytes = Vec::new();
    input
        .take(MAX_AUTHORIZATION_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::InvalidArgument)?;
    if bytes.len() > MAX_AUTHORIZATION_BYTES
        || !std::str::from_utf8(&bytes)
            .is_ok_and(|text| !text.trim().is_empty() && !text.contains('\0'))
    {
        return Err(CliError::InvalidArgument);
    }
    report_writer::write_guarded(&output, &config.target_path, &bytes, overwrite, || {
        authority
            .revalidate()
            .map_err(|_| CliError::StaleObservation)
    })?;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        use std::fmt::Write as _;
        write!(&mut digest, "{byte:02x}").map_err(|_| CliError::Internal)?;
    }
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "repository_id": authority.identity().repository_id.as_str(),
        "written": true,
        "bytes": bytes.len(),
        "sha256": digest,
    }))
    .map_err(|_| CliError::Internal)
}

/// Publishes exact campaign specification bytes from private stdin under observed authority.
pub(super) fn campaign(arguments: &[String], input: impl Read) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(
        arguments,
        &[
            "config",
            "repository-id",
            "head",
            "task-source-sha256",
            "authorization",
            "output",
        ],
        &["overwrite"],
    )?;
    let config_path = parsed.absolute_file("config")?;
    let config = load_config(&config_path).map_err(|_| CliError::Config)?;
    let authority = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    let repository_id = parsed.value("repository-id")?;
    let head = parsed.value("head")?;
    let task_sha256 = parsed.value("task-source-sha256")?;
    let record = parsed.absolute_file("authorization")?;
    let output = parsed.absolute_path("output")?;
    let overwrite = match parsed.optional_value("overwrite") {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(CliError::Usage),
    };
    if authority.identity().repository_id.as_str() != repository_id {
        return Err(CliError::StaleObservation);
    }
    report_writer::validate(&output, &config.target_path)?;
    let output_leaf = normalized_leaf(&output)?;
    let config_leaf = normalized_leaf(&config_path)?;
    let record_leaf = normalized_leaf(&record)?;
    if output_leaf == config_leaf || output_leaf == record_leaf || config_leaf == record_leaf {
        return Err(CliError::Refused);
    }
    let record_sha256 = observed_record_digest(&record, &config.target_path)?;
    observe_campaign_source(&config, &authority, head, task_sha256)?;

    let mut bytes = Vec::new();
    input
        .take(MAX_CAMPAIGN_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::InvalidArgument)?;
    let spec = CampaignSpec::parse_bytes(&bytes).map_err(|_| CliError::InvalidArgument)?;
    if spec.repository_id != repository_id
        || spec.repository_path != config.target_path
        || spec.initial_commit != head
        || spec.task_source_sha256 != task_sha256
        || spec.operator_authorization_sha256 != record_sha256
    {
        return Err(CliError::StaleObservation);
    }
    report_writer::write_guarded(&output, &config.target_path, &bytes, overwrite, || {
        authority
            .revalidate()
            .map_err(|_| CliError::StaleObservation)?;
        if load_config(&config_path).map_err(|_| CliError::StaleObservation)? != config
            || observed_record_digest(&record, &config.target_path)
                .map_err(|_| CliError::StaleObservation)?
                != record_sha256
        {
            return Err(CliError::StaleObservation);
        }
        observe_campaign_source(&config, &authority, head, task_sha256)
    })?;
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "repository_id": repository_id,
        "campaign_id": spec.campaign_id,
        "head": head,
        "written": true,
        "bytes": bytes.len(),
        "sha256": digest_hex(&bytes)?,
    }))
    .map_err(|_| CliError::Internal)
}

fn observed_record_digest(path: &Path, repository: &Path) -> Result<String, CliError> {
    report_writer::validate(path, repository)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| CliError::InvalidArgument)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err(CliError::InvalidArgument);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| CliError::InvalidArgument)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::InvalidArgument)?;
    if bytes.len() > 1024 * 1024 {
        return Err(CliError::InvalidArgument);
    }
    if !std::str::from_utf8(&bytes)
        .is_ok_and(|text| !text.trim().is_empty() && !text.contains('\0'))
    {
        return Err(CliError::InvalidArgument);
    }
    digest_hex(&bytes)
}

fn normalized_leaf(path: &Path) -> Result<std::path::PathBuf, CliError> {
    let parent = path.parent().ok_or(CliError::InvalidArgument)?;
    let leaf = path.file_name().ok_or(CliError::InvalidArgument)?;
    Ok(fs::canonicalize(parent)
        .map_err(|_| CliError::InvalidArgument)?
        .join(leaf))
}

fn observe_campaign_source(
    config: &codingmage_core::Config,
    authority: &RepositoryAuthorization,
    head: &str,
    expected_sha256: &str,
) -> Result<(), CliError> {
    let inventory = inventory_repository(authority).map_err(|_| CliError::StaleObservation)?;
    if inventory.head != head || !inventory.condition.is_clean() {
        return Err(CliError::StaleObservation);
    }
    let blob = read_authorized_blob(authority, head, &config.task_source)
        .map_err(|_| CliError::StaleObservation)?;
    let plan = TaskPlan::parse(&blob).map_err(|_| CliError::Plan)?;
    if plan.source_sha256 != expected_sha256 {
        return Err(CliError::StaleObservation);
    }
    Ok(())
}

fn digest_hex(bytes: &[u8]) -> Result<String, CliError> {
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        write!(&mut digest, "{byte:02x}").map_err(|_| CliError::Internal)?;
    }
    Ok(digest)
}
