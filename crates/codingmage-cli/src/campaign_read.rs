//! Bounded campaign authority snapshot for native selection.

use codingmage_campaign::CampaignSpec;
use serde_json::json;

use crate::{CliError, ParsedArguments};

const MAX_RESPONSE_BYTES: usize = 1_048_575;

pub(super) fn select(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(arguments, &["campaign"])?;
    let campaign_path = parsed.absolute_path("campaign")?;
    let (spec, source_bytes, source_sha256) =
        CampaignSpec::load_with_source_binding(&campaign_path)
            .map_err(|_| CliError::InvalidArgument)?;
    let authority_sha256 = spec
        .authority_sha256()
        .map_err(|_| CliError::InvalidArgument)?;
    let bytes = serde_json::to_vec(&json!({
        "schema_version": 2,
        "campaign_path": campaign_path,
        "source_bytes": source_bytes,
        "source_sha256": source_sha256,
        "authority_sha256": authority_sha256,
        "spec": spec,
    }))
    .map_err(|_| CliError::Internal)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(CliError::Refused);
    }
    String::from_utf8(bytes).map_err(|_| CliError::Internal)
}
