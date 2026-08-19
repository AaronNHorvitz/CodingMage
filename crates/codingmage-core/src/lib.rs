//! Authority and orchestration policy for `CodingMage`.

pub use codingmage_contracts as contracts;

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
