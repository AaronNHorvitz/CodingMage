//! Read-only outcome report assembled from exact coordinator and repository projections.

use std::{fs, path::Path};

use codingmage_campaign::{CampaignExecutionMode, CampaignSpec};
use codingmage_core::{Config, RepositoryAuthorization, load_config};
use codingmage_git::read_authorized_changes;
use codingmage_runtime::{
    CampaignStatus, campaign_blocker_explanation, campaign_run_records, campaign_status,
    team_campaign_report,
};
use serde_json::{Value, json};

use crate::{CliError, ParsedArguments, executable_parent, report_writer};

const REPORT_SCHEMA_VERSION: u16 = 1;
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;

/// Builds the source-bound report without writing a destination.
pub(super) fn inspect(arguments: &[String]) -> Result<String, CliError> {
    let parsed =
        ParsedArguments::new_with_optional(arguments, &["config", "campaign"], &["include-paths"])?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    assemble(
        &config,
        &spec,
        &executable,
        boolean(&parsed, "include-paths")?,
    )
}

/// Writes one source-bound report only after explicit destination and privacy choices.
pub(super) fn export(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(
        arguments,
        &["config", "campaign", "output"],
        &["include-paths", "overwrite"],
    )?;
    let config = load_config(&parsed.absolute_file("config")?).map_err(|_| CliError::Config)?;
    let spec = CampaignSpec::load(&parsed.absolute_file("campaign")?)
        .map_err(|_| CliError::InvalidArgument)?;
    let destination = parsed.absolute_path("output")?;
    let include_paths = boolean(&parsed, "include-paths")?;
    let overwrite = boolean(&parsed, "overwrite")?;
    report_writer::validate(&destination, &config.target_path)?;
    let executable = std::env::current_exe().map_err(|_| CliError::Internal)?;
    let report = assemble(&config, &spec, &executable, include_paths)?;
    let receipt = serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "campaign_id": spec.campaign_id,
        "repository_id": spec.repository_id,
        "written": true,
        "bytes": report.len(),
        "repository_paths_requested": include_paths,
    }))
    .map_err(|_| CliError::Internal)?;
    report_writer::write(
        &destination,
        &config.target_path,
        report.as_bytes(),
        overwrite,
    )?;
    Ok(receipt)
}

fn boolean(parsed: &ParsedArguments, name: &str) -> Result<bool, CliError> {
    match parsed.optional_value(name) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err(CliError::Usage),
    }
}

fn assemble(
    config: &Config,
    spec: &CampaignSpec,
    executable: &Path,
    include_paths: bool,
) -> Result<String, CliError> {
    spec.verify().map_err(|_| CliError::InvalidArgument)?;
    let authority = RepositoryAuthorization::authorize(config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    if authority.identity().repository_id.as_str() != spec.repository_id
        || fs::canonicalize(&config.target_path).map_err(|_| CliError::Repository)?
            != spec.repository_path
    {
        return Err(CliError::Repository);
    }
    let before = campaign_status(config, spec, executable).map_err(CliError::Runtime)?;
    let blocker = if before.is_some() {
        campaign_blocker_explanation(config, spec, executable).map_err(CliError::Runtime)?
    } else {
        None
    };
    let final_report = if before.is_some()
        && spec
            .multi_agent
            .as_ref()
            .is_some_and(|policy| policy.execution_mode == CampaignExecutionMode::Parallel)
    {
        team_campaign_report(config, spec, executable).map_err(CliError::Runtime)?
    } else {
        None
    };
    let runs = if before.is_some() {
        let encoded = campaign_run_records(config, spec, executable).map_err(CliError::Runtime)?;
        let projection: Value = serde_json::from_str(&encoded).map_err(|_| CliError::Internal)?;
        if projection["schema_version"].as_u64() != Some(1)
            || projection["campaign_id"].as_str() != Some(spec.campaign_id.as_str())
            || projection["repository_id"].as_str() != Some(spec.repository_id.as_str())
            || projection["head"].as_str() != before.as_ref().map(|status| status.head.as_str())
            || projection["updated_at_ms"].as_u64()
                != before.as_ref().map(|status| status.updated_at_ms)
        {
            return Err(CliError::StaleObservation);
        }
        Some(projection)
    } else {
        None
    };
    let changes = change_projection(&authority, spec, before.as_ref(), include_paths)?;
    let after = campaign_status(config, spec, executable).map_err(CliError::Runtime)?;
    if !same_observation(before.as_ref(), after.as_ref()) {
        return Err(CliError::StaleObservation);
    }
    if blocker
        .as_ref()
        .is_some_and(|value| value.campaign_id != spec.campaign_id)
        || final_report.as_ref().is_some_and(|value| {
            value.campaign_id != spec.campaign_id
                || value.repository_id != spec.repository_id
                || value.initial_commit != spec.initial_commit
        })
    {
        return Err(CliError::StaleObservation);
    }
    let contains_paths = changes["repository_paths_included"]
        .as_bool()
        .unwrap_or(false);
    let output = json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "campaign_id": spec.campaign_id,
        "repository_id": spec.repository_id,
        "authority_sha256": spec.authority_sha256().map_err(|_| CliError::Internal)?,
        "initial_commit": spec.initial_commit,
        "observed_head": before.as_ref().map(|status| status.head.as_str()),
        "status": before,
        "blockers": blocker,
        "final_report": final_report,
        "changes": changes,
        "run_records": runs,
        "repository_paths_included": contains_paths,
        "limits": [
            "This is a coordinator and repository projection, not independent review or delivery.",
            "Review finding text and full gate logs are not retained by this command.",
            "Displayed change counts may be lower bounds when the source projection was truncated.",
            "No UI-only admission or last-invocation observation is claimed.",
        ],
    });
    let encoded = serde_json::to_string_pretty(&output).map_err(|_| CliError::Internal)?;
    if encoded.len() > MAX_REPORT_BYTES {
        return Err(CliError::Refused);
    }
    Ok(encoded)
}

fn change_projection(
    authority: &RepositoryAuthorization,
    spec: &CampaignSpec,
    status: Option<&CampaignStatus>,
    include_paths: bool,
) -> Result<Value, CliError> {
    let Some(status) = status else {
        return Ok(json!({
            "observed": false,
            "head": null,
            "commits": [],
            "commits_truncated": false,
            "changed_file_count": null,
            "files_truncated": false,
            "changed_files": null,
            "repository_paths_included": false,
        }));
    };
    let observed = read_authorized_changes(authority, &spec.initial_commit, &status.head)
        .map_err(|_| CliError::Repository)?;
    let contains_paths = include_paths && !observed.files.is_empty();
    Ok(json!({
        "observed": true,
        "head": status.head,
        "commits": observed.commits,
        "commits_truncated": observed.commits_truncated,
        "changed_file_count": observed.files.len(),
        "files_truncated": observed.files_truncated,
        "changed_files": include_paths.then_some(observed.files),
        "repository_paths_included": contains_paths,
    }))
}

fn same_observation(before: Option<&CampaignStatus>, after: Option<&CampaignStatus>) -> bool {
    let normalize = |status: Option<&CampaignStatus>| {
        status.cloned().map(|mut value| {
            // Elapsed time advances independently of a durable coordinator checkpoint.
            value.elapsed_ms = 0;
            value
        })
    };
    normalize(before) == normalize(after)
}
