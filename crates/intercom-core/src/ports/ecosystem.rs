//! Outbound port traits for smart home ecosystem integrations.

pub use edf_core::ports::ecosystem::EcosystemInfoDriver;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ecosystem::EcosystemStatus;

    struct DummyEcosystem;

    impl EcosystemInfoDriver for DummyEcosystem {
        fn status(&self) -> Option<EcosystemStatus> {
            None
        }
    }

    #[test]
    fn consumer_ecosystem_trait_is_the_foundation_trait() {
        // Arrange
        let driver = DummyEcosystem;

        // Act
        let shared: &dyn edf_core::ports::ecosystem::EcosystemInfoDriver = &driver;

        // Assert
        assert!(shared.status().is_none());
    }
}
