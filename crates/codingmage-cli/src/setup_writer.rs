//! Public, repository-bound guided Setup writes through the coordinator executable.

use std::io::Read;

use codingmage_core::{RepositoryAuthorization, load_config};
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments, executable_parent, report_writer};

const MAX_AUTHORIZATION_BYTES: usize = 1024 * 1024;

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
