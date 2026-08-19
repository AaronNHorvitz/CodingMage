//! Authority and orchestration policy for `CodingMage`.

mod config;
mod repository;

pub use codingmage_contracts as contracts;
pub use config::{
    AgentProfile, CapabilityGrant, CapabilityPolicy, CommandSpec, Config, ConfigLoadError,
    EffectiveConfigView, PublicationMode, PublicationPolicy, load_config,
};
pub use repository::{
    RemoteIdentity, RepositoryAuthorization, RepositoryAuthorizationError, RepositoryIdentity,
};

/// Returns the coordinator contract version implemented by this core.
#[must_use]
pub const fn contract_version() -> u16 {
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn contract_version_is_nonzero() {
        assert_ne!(super::contract_version(), 0);
    }
}
