//! Smart home ecosystem domain model.
//!
//! Protocol-agnostic value objects and status representations for smart home ecosystems.

pub use edf_core::domain::ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo, FabricInfo,
    FabricLabel, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS, MAX_ECOSYSTEM_PROPERTIES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_status_is_the_foundation_status() {
        // Arrange
        let status = EcosystemStatus::new(EcosystemId::new("sample"), true);

        // Act
        let shared: edf_core::EcosystemStatus = status;

        // Assert
        assert_eq!(
            (shared.ecosystem_type.as_str(), shared.paired),
            ("sample", true)
        );
    }
}
