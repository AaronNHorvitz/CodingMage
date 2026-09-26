//! Manual, redacted support bundles: the existing content-minimized records, nothing more.
//!
//! A bundle is written only on explicit operator request into a new private directory. It
//! contains the configuration view, campaign status, blocker explanation, final report and
//! mission status exactly as the corresponding commands print them, plus a digest manifest and a
//! README naming what is absent. It never includes logs, prompts, provider output, source text,
//! credentials or any host identity, and nothing is transmitted anywhere.

use std::{
    fs,
    path::{Path, PathBuf},
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::Config;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    RuntimeError, campaign_blocker_explanation, campaign_status, private_directory,
    team_campaign::team_campaign_report, team_mission::campaign_mission_status,
};

const MANIFEST_VERSION: u16 = 1;

/// One file retained in a bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportBundleEntry {
    /// File name inside the bundle directory.
    pub name: String,
    /// SHA-256 of the file bytes.
    pub sha256: String,
    /// Byte length.
    pub bytes: u64,
}

/// Digest manifest written beside the bundle files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportBundleManifest {
    /// Closed manifest version.
    pub schema_version: u16,
    /// Coordinator version that wrote the bundle.
    pub coordinator_version: String,
    /// Exact campaign identity.
    pub campaign_id: String,
    /// Exact repository identity.
    pub repository_id: String,
    /// Digest of the campaign authority.
    pub authority_sha256: String,
    /// Files present, sorted by name.
    pub files: Vec<SupportBundleEntry>,
    /// Records that had no durable state to export, sorted by name.
    pub absent: Vec<String>,
    /// Always true: the bundle contains only content-minimized records.
    pub redacted: bool,
    /// Always false: nothing was sent anywhere.
    pub uploaded: bool,
}

/// Writes a redacted support bundle into `output`, which must not exist yet.
///
/// # Errors
///
/// Returns [`RuntimeError::Spec`] when the output exists or its parent is not a directory,
/// [`RuntimeError::State`] when a record cannot be written, and the underlying error when the
/// campaign authority cannot be validated.
#[allow(clippy::too_many_lines)]
pub fn export_support_bundle(
    config: &Config,
    spec: &CampaignSpec,
    codingmage_binary: &Path,
    output: &Path,
) -> Result<SupportBundleManifest, RuntimeError> {
    spec.verify().map_err(RuntimeError::Campaign)?;
    let authority_sha256 = spec.authority_sha256().map_err(RuntimeError::Campaign)?;
    if !output.is_absolute()
        || output.exists()
        || !output.parent().is_some_and(Path::is_dir)
        || output
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(RuntimeError::Spec);
    }
    let status = campaign_status(config, spec, codingmage_binary)?;
    let explanation = if status.is_some() {
        campaign_blocker_explanation(config, spec, codingmage_binary)?
    } else {
        None
    };
    let parallel = spec.multi_agent.as_ref().is_some_and(|policy| {
        policy.execution_mode == codingmage_campaign::CampaignExecutionMode::Parallel
    });
    let report = if parallel {
        match team_campaign_report(config, spec, codingmage_binary) {
            Ok(report) => report,
            Err(RuntimeError::State) => None,
            Err(error) => return Err(error),
        }
    } else {
        // Final reports exist only for parallel campaigns; a serial campaign has none.
        None
    };
    let mission = match campaign_mission_status(config, spec, codingmage_binary) {
        Ok(mission) => Some(mission),
        Err(RuntimeError::State) => None,
        Err(error) => return Err(error),
    };

    private_directory(output)?;
    let mut files = Vec::new();
    let mut absent = Vec::new();
    write_record(
        output,
        "configuration.json",
        &config.redacted_view(),
        &mut files,
    )?;
    write_optional(
        output,
        "campaign-status.json",
        status.as_ref(),
        &mut files,
        &mut absent,
    )?;
    write_optional(
        output,
        "campaign-explain-blocker.json",
        explanation.as_ref(),
        &mut files,
        &mut absent,
    )?;
    write_optional(
        output,
        "campaign-report.json",
        report.as_ref(),
        &mut files,
        &mut absent,
    )?;
    write_optional(
        output,
        "mission-status.json",
        mission.as_ref(),
        &mut files,
        &mut absent,
    )?;
    files.sort_by(|left, right| left.name.cmp(&right.name));
    absent.sort();
    let manifest = SupportBundleManifest {
        schema_version: MANIFEST_VERSION,
        coordinator_version: env!("CARGO_PKG_VERSION").to_owned(),
        campaign_id: spec.campaign_id.clone(),
        repository_id: spec.repository_id.clone(),
        authority_sha256,
        files,
        absent,
        redacted: true,
        uploaded: false,
    };
    let readme = format!(
        "CodingMage support bundle for campaign {}\n\nThis directory was written on explicit \
         request and was not transmitted anywhere. It contains only the content-minimized records \
         that the campaign-status, campaign-explain-blocker, campaign-report, \
         campaign-mission-status and doctor commands already expose: identities, digests, counts \
         and stable codes. It never contains logs, prompts, provider output, repository source, \
         credentials or host identities. Records listed under `absent` in manifest.json had no \
         durable state at export time.\n\nAbsent: {}\n",
        spec.campaign_id,
        if manifest.absent.is_empty() {
            "none".to_owned()
        } else {
            manifest.absent.join(", ")
        }
    );
    write_bytes(output, "README.txt", readme.as_bytes())?;
    let encoded = serde_json::to_vec_pretty(&manifest).map_err(|_| RuntimeError::State)?;
    write_bytes(output, "manifest.json", &encoded)?;
    Ok(manifest)
}

fn write_optional<T: Serialize>(
    output: &Path,
    name: &str,
    record: Option<&T>,
    files: &mut Vec<SupportBundleEntry>,
    absent: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    if let Some(record) = record {
        write_record(output, name, record, files)
    } else {
        absent.push(name.to_owned());
        Ok(())
    }
}

fn write_record<T: Serialize>(
    output: &Path,
    name: &str,
    record: &T,
    files: &mut Vec<SupportBundleEntry>,
) -> Result<(), RuntimeError> {
    let encoded = serde_json::to_vec_pretty(record).map_err(|_| RuntimeError::State)?;
    write_bytes(output, name, &encoded)?;
    files.push(SupportBundleEntry {
        name: name.to_owned(),
        sha256: hex(&Sha256::digest(&encoded)),
        bytes: u64::try_from(encoded.len()).map_err(|_| RuntimeError::State)?,
    });
    Ok(())
}

fn write_bytes(output: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, RuntimeError> {
    let path = output.join(name);
    if path.exists() {
        return Err(RuntimeError::State);
    }
    fs::write(&path, bytes).map_err(|_| RuntimeError::State)?;
    Ok(path)
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
