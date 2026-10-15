//! Concrete smart home ecosystem status provider for Matter protocol.

use intercom_core::ports::ecosystem::EcosystemInfoDriver;

/// Concrete smart home ecosystem status provider for Matter protocol.
pub struct FirmwareEcosystemInfo;

impl EcosystemInfoDriver for FirmwareEcosystemInfo {
    fn is_paired(&self) -> bool {
        true
    }

    fn protocol_name(&self) -> &'static str {
        "Matter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_ecosystem_info_matter() {
        // Arrange
        let eco = FirmwareEcosystemInfo;

        // Act
        let paired = eco.is_paired();
        let protocol = eco.protocol_name();

        // Assert
        assert!(paired);
        assert_eq!(protocol, "Matter");
    }
}
