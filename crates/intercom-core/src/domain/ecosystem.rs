//! Smart home ecosystem domain model.
//!
//! Protocol-agnostic value objects and status representations for smart home ecosystems.

pub use edf_core::domain::ecosystem::{
    ClusterInfo, EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo,
    FabricInfo, FabricLabel, FabricSubscription, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS,
    MAX_ECOSYSTEM_PROPERTIES,
};

/// Wireless boot mode selected from persisted commissioning state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirelessBootMode {
    /// Device already has an active controller fabric; run Wi-Fi-only operational stack.
    OperationalWifiOnly,
    /// Device is uncommissioned; run concurrent BLE + Wi-Fi commissioning stack.
    BleWifiCoex,
}

/// Select the wireless boot mode based on whether a commissioned fabric is present.
pub const fn select_wireless_boot_mode(is_commissioned: bool) -> WirelessBootMode {
    if is_commissioned {
        WirelessBootMode::OperationalWifiOnly
    } else {
        WirelessBootMode::BleWifiCoex
    }
}

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

    #[test]
    fn wireless_boot_mode_uses_wifi_only_when_commissioned() {
        assert_eq!(
            select_wireless_boot_mode(true),
            WirelessBootMode::OperationalWifiOnly
        );
    }

    #[test]
    fn wireless_boot_mode_uses_ble_wifi_coex_when_uncommissioned() {
        assert_eq!(
            select_wireless_boot_mode(false),
            WirelessBootMode::BleWifiCoex
        );
    }
}
