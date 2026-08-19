//! Explicit platform capability and evidence boundary.

use std::fmt;

/// Supported or planned native platform family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    /// Fedora and compatible Linux distributions using user systemd and process groups.
    Linux,
    /// Apple Silicon macOS using launch agents, process groups, and Keychain references.
    MacOs,
    /// Windows 11 using job objects, NTFS identity, and credential references.
    Windows,
}

/// Truthful implementation/evidence level for one platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportLevel {
    /// Implemented and locally exercised on the current native platform.
    NativeTested,
    /// Contract implemented but native execution evidence is absent.
    ImplementedUntested,
    /// Requirements exist but executable support does not.
    Planned,
}

/// Native process-containment primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessContainment {
    /// Unix process group with parent-start identity and resource limits.
    LinuxProcessGroup,
    /// Darwin process group plus parent identity and resource limits.
    DarwinProcessGroup,
    /// Windows job object with kill-on-close and process limits.
    WindowsJobObject,
}

/// Native background-service primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceManager {
    /// Unprivileged systemd user service.
    SystemdUser,
    /// Per-user launch agent.
    LaunchAgent,
    /// Per-user scheduled task or explicitly approved service.
    WindowsTask,
}

/// Native credential-reference primitive. Raw secrets never cross this contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialStore {
    /// Freedesktop Secret Service item reference.
    SecretService,
    /// macOS Keychain item reference.
    Keychain,
    /// Windows Credential Manager item reference.
    CredentialManager,
}

/// Native local monitoring and control transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorTransport {
    /// Private Unix-domain socket owned by the current Linux user.
    LinuxUnixSocket,
    /// Private Unix-domain socket owned by the current macOS user.
    DarwinUnixSocket,
    /// Current-user Windows named pipe with an explicit access control list.
    WindowsNamedPipe,
}

/// Reviewable platform capabilities without a claim of native execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformCapabilities {
    /// Platform family.
    pub platform: Platform,
    /// Current evidence level.
    pub support: SupportLevel,
    /// Process-containment primitive.
    pub process: ProcessContainment,
    /// Background-service primitive.
    pub service: ServiceManager,
    /// Credential-reference primitive.
    pub credentials: CredentialStore,
    /// Local monitoring/control transport.
    pub monitoring: MonitorTransport,
    /// Whether physical filesystem identity has an implementation.
    pub filesystem_identity: bool,
    /// Whether native lifecycle tests have actually run.
    pub native_lifecycle_evidence: bool,
}

/// Platform adapter exposes capability facts and content-free command plans only.
pub trait PlatformAdapter {
    /// Returns immutable capabilities.
    fn capabilities(&self) -> PlatformCapabilities;
    /// Returns the native executable and literal arguments for installing a user service.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Unsupported`] when executable support is not available.
    fn install_service_plan(&self) -> Result<CommandPlan, PlatformError>;
    /// Returns a namespaced reference format for one credential item, never its value.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::InvalidReference`] for a noncanonical label.
    fn credential_reference(&self, label: &str) -> Result<String, PlatformError>;
}

/// Literal native command plan, not shell text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    /// Absolute executable path.
    pub executable: &'static str,
    /// Fixed literal arguments.
    pub arguments: Vec<String>,
}

/// Fedora/Ubuntu reference adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxAdapter;

impl PlatformAdapter for LinuxAdapter {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            platform: Platform::Linux,
            support: if cfg!(target_os = "linux") {
                SupportLevel::NativeTested
            } else {
                SupportLevel::ImplementedUntested
            },
            process: ProcessContainment::LinuxProcessGroup,
            service: ServiceManager::SystemdUser,
            credentials: CredentialStore::SecretService,
            monitoring: MonitorTransport::LinuxUnixSocket,
            filesystem_identity: true,
            native_lifecycle_evidence: cfg!(target_os = "linux"),
        }
    }

    fn install_service_plan(&self) -> Result<CommandPlan, PlatformError> {
        Ok(CommandPlan {
            executable: "/usr/bin/systemctl",
            arguments: vec!["--user".to_owned(), "daemon-reload".to_owned()],
        })
    }

    fn credential_reference(&self, label: &str) -> Result<String, PlatformError> {
        reference("secret-service", label)
    }
}

/// macOS adapter contract. Native evidence must be supplied on supported Apple hardware.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacOsAdapter;

impl PlatformAdapter for MacOsAdapter {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            platform: Platform::MacOs,
            support: SupportLevel::ImplementedUntested,
            process: ProcessContainment::DarwinProcessGroup,
            service: ServiceManager::LaunchAgent,
            credentials: CredentialStore::Keychain,
            monitoring: MonitorTransport::DarwinUnixSocket,
            filesystem_identity: true,
            native_lifecycle_evidence: false,
        }
    }

    fn install_service_plan(&self) -> Result<CommandPlan, PlatformError> {
        Ok(CommandPlan {
            executable: "/bin/launchctl",
            arguments: vec![
                "bootstrap".to_owned(),
                "gui/$UID".to_owned(),
                "$CODINGMAGE_PLIST".to_owned(),
            ],
        })
    }

    fn credential_reference(&self, label: &str) -> Result<String, PlatformError> {
        reference("keychain", label)
    }
}

/// Windows requirements adapter. Executable commands remain unavailable until implemented.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsPlan;

impl PlatformAdapter for WindowsPlan {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            platform: Platform::Windows,
            support: SupportLevel::Planned,
            process: ProcessContainment::WindowsJobObject,
            service: ServiceManager::WindowsTask,
            credentials: CredentialStore::CredentialManager,
            monitoring: MonitorTransport::WindowsNamedPipe,
            filesystem_identity: false,
            native_lifecycle_evidence: false,
        }
    }

    fn install_service_plan(&self) -> Result<CommandPlan, PlatformError> {
        Err(PlatformError::Unsupported)
    }

    fn credential_reference(&self, label: &str) -> Result<String, PlatformError> {
        reference("credential-manager-planned", label)
    }
}

fn reference(namespace: &str, label: &str) -> Result<String, PlatformError> {
    if label.is_empty()
        || label.len() > 128
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(PlatformError::InvalidReference);
    }
    Ok(format!("codingmage:{namespace}:{label}"))
}

/// Stable platform error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformError {
    /// Requested executable behavior is not implemented.
    Unsupported,
    /// Credential label was noncanonical.
    InvalidReference,
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unsupported => "codingmage.platform.unsupported",
            Self::InvalidReference => "codingmage.platform.invalid_reference",
        })
    }
}

impl std::error::Error for PlatformError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_is_the_only_native_evidence_on_linux() {
        let linux = LinuxAdapter.capabilities();
        assert_eq!(linux.platform, Platform::Linux);
        assert_eq!(linux.monitoring, MonitorTransport::LinuxUnixSocket);
        assert_eq!(linux.support, SupportLevel::NativeTested);
        assert!(linux.native_lifecycle_evidence);
        for capabilities in [MacOsAdapter.capabilities(), WindowsPlan.capabilities()] {
            assert!(!capabilities.native_lifecycle_evidence);
        }
    }

    #[test]
    fn mac_contract_is_implemented_without_claiming_native_evidence() {
        let mac = MacOsAdapter;
        assert_eq!(
            mac.capabilities().support,
            SupportLevel::ImplementedUntested
        );
        assert_eq!(
            mac.install_service_plan().unwrap().executable,
            "/bin/launchctl"
        );
        assert_eq!(
            mac.credential_reference("github-personal").unwrap(),
            "codingmage:keychain:github-personal"
        );
    }

    #[test]
    fn windows_commands_remain_unimplemented_and_references_are_bounded() {
        assert_eq!(
            WindowsPlan.install_service_plan(),
            Err(PlatformError::Unsupported)
        );
        for label in ["", "../escape", "contains space", &"x".repeat(129)] {
            assert_eq!(
                LinuxAdapter.credential_reference(label),
                Err(PlatformError::InvalidReference)
            );
        }
    }
}
