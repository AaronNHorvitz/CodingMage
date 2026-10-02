//! Public, repository-bound guided Setup writes through the coordinator executable.

use std::{
    fs::{self, File},
    io::{Read, Seek as _, SeekFrom},
    os::unix::fs::{DirBuilderExt as _, MetadataExt as _},
    path::{Component, Path, PathBuf},
};

use codingmage_campaign::CampaignSpec;
use codingmage_core::{Config, RepositoryAuthorization, load_config};
use codingmage_git::{inventory_repository, read_authorized_blob};
use codingmage_plan::TaskPlan;
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::{CliError, ParsedArguments, executable_parent, report_writer};

const MAX_AUTHORIZATION_BYTES: usize = 1024 * 1024;
const MAX_CAMPAIGN_BYTES: usize = 1024 * 1024;
const MAX_CONFIG_BYTES: usize = 1024 * 1024;

/// Publishes one guided configuration through the public coordinator boundary.
///
/// Missing scratch/state roots may be created as private direct children of the configuration
/// directory. A failure after that creation can leave empty roots for the operator to inspect.
pub(super) fn configuration(arguments: &[String], input: impl Read) -> Result<String, CliError> {
    let parsed =
        ParsedArguments::new_with_optional(arguments, &["repo", "output"], &["overwrite"])?;
    let repository = parsed.absolute_directory("repo")?;
    let output = parsed.absolute_path("output")?;
    let overwrite = match parsed.optional_value("overwrite") {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(CliError::Usage),
    };
    report_writer::validate(&output, &repository)?;
    let mut previous = match fs::symlink_metadata(&output) {
        Ok(metadata) if !overwrite || !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err(CliError::Refused);
        }
        Ok(_) => {
            let mut held = ObservedFile::open(&output, MAX_CONFIG_BYTES as u64)?;
            let original = load_config(&output).map_err(|_| CliError::Refused)?;
            if original.target_path != repository {
                return Err(CliError::Refused);
            }
            let bytes = held.exact_bytes()?;
            if toml::from_str::<Config>(
                std::str::from_utf8(&bytes).map_err(|_| CliError::StaleObservation)?,
            )
            .map_err(|_| CliError::StaleObservation)?
                != original
            {
                return Err(CliError::StaleObservation);
            }
            Some((held, bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(CliError::Refused),
    };
    let mut bytes = Vec::new();
    input
        .take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CliError::InvalidArgument)?;
    if bytes.is_empty() || bytes.len() > MAX_CONFIG_BYTES {
        return Err(CliError::InvalidArgument);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| CliError::InvalidArgument)?;
    let config: Config = toml::from_str(text).map_err(|_| CliError::Config)?;
    if config.target_path != repository {
        return Err(CliError::StaleObservation);
    }
    let roots = prepare_configuration_roots(&config, &output, &repository)?;
    let authority = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    let expected = authority.identity().clone();
    let mut checked_previous = false;
    report_writer::write_guarded_validated(
        &output,
        &repository,
        &bytes,
        overwrite,
        |candidate| {
            if load_config(candidate).map_err(|_| CliError::Config)? == config {
                Ok(())
            } else {
                Err(CliError::StaleObservation)
            }
        },
        || {
            authority
                .revalidate()
                .map_err(|_| CliError::StaleObservation)?;
            check_configuration_roots(&roots)?;
            if !checked_previous {
                if let Some((held, original)) = previous.as_mut()
                    && held.exact_bytes()? != *original
                {
                    return Err(CliError::StaleObservation);
                }
                checked_previous = true;
            }
            if RepositoryAuthorization::authorize(&config, &executable_parent()?)
                .map_err(|_| CliError::StaleObservation)?
                .identity()
                != &expected
            {
                return Err(CliError::StaleObservation);
            }
            Ok(())
        },
    )?;
    verify_published_configuration(&output, &bytes, &authority, &roots)?;
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "repository_id": expected.repository_id.as_str(),
        "head": expected.initial_head,
        "written": true,
        "bytes": bytes.len(),
        "sha256": digest_hex(&bytes)?,
    }))
    .map_err(|_| CliError::Internal)
}

fn verify_published_configuration(
    output: &Path,
    bytes: &[u8],
    authority: &RepositoryAuthorization,
    roots: &[(PathBuf, u64, u64)],
) -> Result<(), CliError> {
    let mut published = ObservedFile::open(output, MAX_CONFIG_BYTES as u64)
        .map_err(|_| CliError::UncertainWrite)?;
    if published
        .exact_bytes()
        .map_err(|_| CliError::UncertainWrite)?
        != bytes
        || authority.revalidate().is_err()
        || check_configuration_roots(roots).is_err()
    {
        return Err(CliError::UncertainWrite);
    }
    Ok(())
}

fn prepare_configuration_roots(
    config: &Config,
    output: &Path,
    repository: &Path,
) -> Result<Vec<(PathBuf, u64, u64)>, CliError> {
    let output_parent = fs::canonicalize(output.parent().ok_or(CliError::InvalidArgument)?)
        .map_err(|_| CliError::Refused)?;
    let mut roots = Vec::new();
    for root in [&config.scratch_root, &config.state_root] {
        if !root.is_absolute()
            || root
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CliError::InvalidArgument);
        }
        match fs::symlink_metadata(root) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = root.parent().ok_or(CliError::InvalidArgument)?;
                if fs::canonicalize(parent).map_err(|_| CliError::Refused)? != output_parent {
                    return Err(CliError::Refused);
                }
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(root)
                    .map_err(|_| CliError::Refused)?;
            }
            Ok(_) | Err(_) => return Err(CliError::Refused),
        }
        report_writer::validate(&root.join(".codingmage-root-check"), repository)?;
        let metadata = fs::symlink_metadata(root).map_err(|_| CliError::Refused)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CliError::Refused);
        }
        roots.push((root.clone(), metadata.dev(), metadata.ino()));
    }
    Ok(roots)
}

fn check_configuration_roots(roots: &[(PathBuf, u64, u64)]) -> Result<(), CliError> {
    for (path, device, inode) in roots {
        let metadata = fs::symlink_metadata(path).map_err(|_| CliError::StaleObservation)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || (metadata.dev(), metadata.ino()) != (*device, *inode)
        {
            return Err(CliError::StaleObservation);
        }
    }
    Ok(())
}

// A held no-follow descriptor binds the bytes and physical identity to the
// named file. Publication checks reject a moved leaf and re-read mutable bytes.
struct ObservedFile {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
    max_bytes: u64,
}

impl ObservedFile {
    fn open(path: &Path, max_bytes: u64) -> Result<Self, CliError> {
        let file = File::from(
            open(
                path,
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| CliError::InvalidArgument)?,
        );
        let metadata = file.metadata().map_err(|_| CliError::InvalidArgument)?;
        if !metadata.is_file() || metadata.len() > max_bytes {
            return Err(CliError::InvalidArgument);
        }
        let observed = Self {
            path: path.to_path_buf(),
            file,
            identity: (metadata.dev(), metadata.ino()),
            max_bytes,
        };
        observed
            .revalidate()
            .map_err(|_| CliError::InvalidArgument)?;
        Ok(observed)
    }

    fn revalidate(&self) -> Result<(), CliError> {
        let named = fs::symlink_metadata(&self.path).map_err(|_| CliError::StaleObservation)?;
        let held = self
            .file
            .metadata()
            .map_err(|_| CliError::StaleObservation)?;
        if !named.is_file()
            || named.file_type().is_symlink()
            || named.len() > self.max_bytes
            || !held.is_file()
            || held.len() > self.max_bytes
            || self.identity != (named.dev(), named.ino())
            || self.identity != (held.dev(), held.ino())
        {
            return Err(CliError::StaleObservation);
        }
        Ok(())
    }

    fn authorization_observation(&mut self) -> Result<(String, usize), CliError> {
        self.revalidate()?;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|_| CliError::InvalidArgument)?;
        let mut bytes = Vec::new();
        (&self.file)
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::InvalidArgument)?;
        if bytes.len() as u64 > self.max_bytes
            || !std::str::from_utf8(&bytes)
                .is_ok_and(|text| !text.trim().is_empty() && !text.contains('\0'))
        {
            return Err(CliError::InvalidArgument);
        }
        self.revalidate()?;
        Ok((digest_hex(&bytes)?, bytes.len()))
    }

    fn exact_bytes(&mut self) -> Result<Vec<u8>, CliError> {
        self.revalidate()?;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|_| CliError::StaleObservation)?;
        let mut bytes = Vec::new();
        (&self.file)
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::StaleObservation)?;
        self.revalidate()?;
        if bytes.len() as u64 > self.max_bytes {
            return Err(CliError::StaleObservation);
        }
        Ok(bytes)
    }
}

/// Copies an exact, validated Setup source through guarded public publication.
pub(super) fn export_copy(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new_with_optional(
        arguments,
        &["config", "repository-id", "source", "output"],
        &["campaign-authority-sha256", "overwrite"],
    )?;
    let config_path = parsed.absolute_file("config")?;
    let config = load_config(&config_path).map_err(|_| CliError::Config)?;
    let mut config_file = ObservedFile::open(&config_path, MAX_CAMPAIGN_BYTES as u64)
        .map_err(|_| CliError::StaleObservation)?;
    let config_bytes = config_file.exact_bytes()?;
    let config_sha256 = digest_hex(&config_bytes)?;
    let authority = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    let repository_id = parsed.value("repository-id")?;
    if authority.identity().repository_id.as_str() != repository_id {
        return Err(CliError::StaleObservation);
    }
    let source_path = parsed.absolute_file("source")?;
    let output = parsed.absolute_path("output")?;
    let overwrite = match parsed.optional_value("overwrite") {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(CliError::Usage),
    };
    report_writer::validate(&source_path, &config.target_path)?;
    report_writer::validate(&output, &config.target_path)?;
    if normalized_leaf(&source_path)? == normalized_leaf(&output)?
        || normalized_leaf(&config_path)? == normalized_leaf(&output)?
    {
        return Err(CliError::Refused);
    }
    let mut source_file = ObservedFile::open(&source_path, MAX_CAMPAIGN_BYTES as u64)?;
    let bytes = source_file.exact_bytes()?;
    let config_source = source_file.identity == config_file.identity;
    if config_source {
        if parsed.optional_value("campaign-authority-sha256").is_some() || bytes != config_bytes {
            return Err(CliError::StaleObservation);
        }
    } else {
        let expected = parsed
            .optional_value("campaign-authority-sha256")
            .ok_or(CliError::Usage)?;
        let spec = CampaignSpec::parse_bytes(&bytes).map_err(|_| CliError::InvalidArgument)?;
        if spec.repository_id != repository_id
            || spec.repository_path != config.target_path
            || spec
                .authority_sha256()
                .map_err(|_| CliError::InvalidArgument)?
                != expected
        {
            return Err(CliError::StaleObservation);
        }
    }
    refuse_protected_output(&output, &config_file, &source_file)?;
    let sha256 = digest_hex(&bytes)?;
    report_writer::write_guarded(&output, &config.target_path, &bytes, overwrite, || {
        authority
            .revalidate()
            .map_err(|_| CliError::StaleObservation)?;
        config_file.revalidate()?;
        source_file.revalidate()?;
        refuse_protected_output(&output, &config_file, &source_file)?;
        if load_config(&config_path).map_err(|_| CliError::StaleObservation)? != config
            || digest_hex(&config_file.exact_bytes()?)? != config_sha256
            || digest_hex(&source_file.exact_bytes()?)? != sha256
        {
            return Err(CliError::StaleObservation);
        }
        Ok(())
    })?;
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "repository_id": repository_id,
        "written": true,
        "bytes": bytes.len(),
        "sha256": sha256,
    }))
    .map_err(|_| CliError::Internal)
}

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
    let config_file = ObservedFile::open(&config_path, MAX_CAMPAIGN_BYTES as u64)
        .map_err(|_| CliError::StaleObservation)?;
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
    report_writer::validate(&record, &config.target_path)?;
    let mut record_file = ObservedFile::open(&record, MAX_AUTHORIZATION_BYTES as u64)?;
    if config_file.identity == record_file.identity {
        return Err(CliError::Refused);
    }
    refuse_protected_output(&output, &config_file, &record_file)?;
    let record_sha256 = record_file.authorization_observation()?.0;
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
        config_file.revalidate()?;
        record_file.revalidate()?;
        refuse_protected_output(&output, &config_file, &record_file)?;
        if load_config(&config_path).map_err(|_| CliError::StaleObservation)? != config
            || record_file.authorization_observation()?.0 != record_sha256
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

/// Inspects a bounded external authorization record for guided campaign authoring.
pub(super) fn inspect_authorization(arguments: &[String]) -> Result<String, CliError> {
    let parsed = ParsedArguments::new(
        arguments,
        &[
            "config",
            "repository-id",
            "head",
            "task-source-sha256",
            "authorization",
        ],
    )?;
    let config_path = parsed.absolute_file("config")?;
    let config = load_config(&config_path).map_err(|_| CliError::Config)?;
    let config_file = ObservedFile::open(&config_path, MAX_CAMPAIGN_BYTES as u64)
        .map_err(|_| CliError::StaleObservation)?;
    let authority = RepositoryAuthorization::authorize(&config, &executable_parent()?)
        .map_err(|_| CliError::Repository)?;
    let repository_id = parsed.value("repository-id")?;
    let head = parsed.value("head")?;
    let task_sha256 = parsed.value("task-source-sha256")?;
    let record = parsed.absolute_file("authorization")?;
    if authority.identity().repository_id.as_str() != repository_id {
        return Err(CliError::StaleObservation);
    }
    report_writer::validate(&record, &config.target_path)?;
    if normalized_leaf(&record)? == normalized_leaf(&config_path)? {
        return Err(CliError::Refused);
    }
    let mut record_file = ObservedFile::open(&record, MAX_AUTHORIZATION_BYTES as u64)?;
    if config_file.identity == record_file.identity {
        return Err(CliError::Refused);
    }
    observe_campaign_source(&config, &authority, head, task_sha256)?;
    let (sha256, bytes) = record_file.authorization_observation()?;
    authority
        .revalidate()
        .map_err(|_| CliError::StaleObservation)?;
    config_file.revalidate()?;
    record_file.revalidate()?;
    if load_config(&config_path).map_err(|_| CliError::StaleObservation)? != config
        || record_file
            .authorization_observation()
            .map_err(|_| CliError::StaleObservation)?
            != (sha256.clone(), bytes)
    {
        return Err(CliError::StaleObservation);
    }
    observe_campaign_source(&config, &authority, head, task_sha256)?;
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "repository_id": repository_id,
        "head": head,
        "task_source_sha256": task_sha256,
        "observed": true,
        "bytes": bytes,
        "sha256": sha256,
    }))
    .map_err(|_| CliError::Internal)
}

fn refuse_protected_output(
    output: &Path,
    config: &ObservedFile,
    record: &ObservedFile,
) -> Result<(), CliError> {
    match fs::symlink_metadata(output) {
        Ok(metadata) if metadata.is_file() => {
            let identity = (metadata.dev(), metadata.ino());
            if identity == config.identity || identity == record.identity {
                return Err(CliError::Refused);
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(CliError::StaleObservation),
    }
    Ok(())
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
