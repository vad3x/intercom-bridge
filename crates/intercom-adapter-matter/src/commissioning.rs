//! Matter commissioning identity for Intercom Bridge.
//!
//! Intercom prefixes are applied to the shared foundation identity derivation so
//! the device and the host pairing tool produce the same commissioning values.

pub use edf_matter::identity::{DeviceIdentity, IdentityError, IdentityParams};

/// Intercom serial-number prefix.
pub const SERIAL_PREFIX: &str = "IC";

/// Intercom DHCP hostname prefix.
pub const HOSTNAME_PREFIX: &str = "intercom-bridge";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirelessBootMode {
    /// Uncommissioned boot: run concurrent BLE + Wi-Fi coexistence (`run_coex`).
    BleWifiCoex,
    /// Commissioned boot: run operational Wi-Fi only (`run_wifi_only`), reclaiming ~50 KiB BLE heap.
    OperationalWifiOnly,
}

pub const fn select_wireless_boot_mode(has_active_fabric: bool) -> WirelessBootMode {
    if has_active_fabric {
        WirelessBootMode::OperationalWifiOnly
    } else {
        WirelessBootMode::BleWifiCoex
    }
}

/// Derive the device and host pairing identity for an Intercom MAC address.
pub fn derive_intercom_identity(mac: &[u8; 6]) -> Result<DeviceIdentity, IdentityError> {
    edf_matter::identity::derive_device_identity(&IdentityParams {
        mac,
        serial_prefix: SERIAL_PREFIX,
        hostname_prefix: HOSTNAME_PREFIX,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

    #[test]
    fn intercom_identity_preserves_pairing_vectors() {
        // Arrange
        let mac = MAC;

        // Act
        let identity = derive_intercom_identity(&mac).unwrap();

        // Assert
        assert_eq!(
            (
                identity.passcode,
                identity.discriminator,
                identity.serial_str(),
                identity.hostname_str(),
            ),
            (21_710_079, 3096, "IC-98A316A67844", "intercom-bridge-7844")
        );
    }

    #[test]
    fn intercom_comm_data_uses_foundation_identity() {
        // Arrange
        let mac = MAC;
        let identity = derive_intercom_identity(&mac).unwrap();

        // Act
        let comm_data = identity.comm_data();

        // Assert
        assert_eq!(comm_data.discriminator, identity.discriminator);
        assert_eq!(
            comm_data.password.access(),
            &identity.passcode.to_le_bytes()
        );
    }
}

#[cfg(test)]
mod boot_mode_tests {
    use super::*;

    #[test]
    fn commissioned_fabric_selects_wifi_only_and_uncommissioned_selects_ble_coex() {
        // Arrange
        let fabric_absent = false;
        let fabric_present = true;

        // Act
        let uncommissioned = select_wireless_boot_mode(fabric_absent);
        let commissioned = select_wireless_boot_mode(fabric_present);

        // Assert
        assert_eq!(uncommissioned, WirelessBootMode::BleWifiCoex);
        assert_eq!(commissioned, WirelessBootMode::OperationalWifiOnly);
    }
}
