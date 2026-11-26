//! Hardware platform selection and lifecycle abstraction.

#[cfg(target_os = "none")]
pub mod c6;
#[cfg(not(target_os = "none"))]
pub mod mock;

/// Peripheral ownership transferred from platform startup to the application.
pub struct PlatformDrivers<P> {
    pub peripherals: P,
}

/// Initializes a hardware target and provides target-level reset control.
pub trait PlatformDriver: Sized {
    type Drivers;

    fn init() -> (Self, Self::Drivers);

    fn software_reset() -> !;
}

#[cfg(target_os = "none")]
pub type ActivePlatform = c6::C6Platform;
#[cfg(not(target_os = "none"))]
pub type ActivePlatform = mock::MockPlatform;

#[cfg(target_os = "none")]
pub use c6::board::{
    active_slot, free_heap_bytes, ip_address, is_slot_valid, mac_address, wifi_rssi, wifi_ssid,
    wire_board, BoardHardware,
};
#[cfg(target_os = "none")]
pub use c6::wireless::{
    enet_stack, init_esp_network, BoardWireless, EnetMatterBuffers, EspWifiDriver,
};

#[cfg(not(target_os = "none"))]
pub use mock::{
    active_slot, free_heap_bytes, ip_address, is_slot_valid, mac_address, wifi_rssi, wifi_ssid,
};

/// Build an `embassy_net::DhcpConfig` with DHCPv4 Option 12 hostname when non-empty and <= 32 bytes.
pub fn dhcp_config_with_hostname(hostname: &str) -> embassy_net::DhcpConfig {
    let mut dhcp = embassy_net::DhcpConfig::default();
    if !hostname.is_empty() {
        if let Ok(parsed) = heapless::String::try_from(hostname) {
            dhcp.hostname = Some(parsed);
        }
    }
    dhcp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dhcp_config_with_hostname_sets_non_empty_hostname() {
        let cfg = dhcp_config_with_hostname("intercom-bridge-7844");
        assert_eq!(cfg.hostname.as_deref(), Some("intercom-bridge-7844"));
    }

    #[test]
    fn dhcp_config_with_hostname_ignores_empty_hostname() {
        let cfg = dhcp_config_with_hostname("");
        assert_eq!(cfg.hostname, None);
    }
}
