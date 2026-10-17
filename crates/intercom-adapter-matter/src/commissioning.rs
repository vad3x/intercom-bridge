//! Matter commissioning parameters generation.
//!
//! Derives unique, deterministic Matter commissioning parameters (Passcode and
//! Discriminator) from the hardware MAC address instead of using hardcoded test values.

use rs_matter::BasicCommData;

// Re-export pure domain functions and constants from intercom-core
pub use intercom_core::domain::commissioning::{
    discriminator_from_mac, hostname_from_mac, is_forbidden_passcode, mac_hash, passcode_from_mac,
    serial_no_from_mac, FORBIDDEN_PASSCODES, MAX_DISCRIMINATOR, MAX_PASSCODE, MIN_DISCRIMINATOR,
    MIN_PASSCODE, TEST_DISCRIMINATOR,
};

// ============================================================================
// Public Functions
// ============================================================================

/// Derives `BasicCommData` from a 6-byte hardware MAC address.
pub fn comm_data_from_mac(mac: &[u8; 6]) -> BasicCommData {
    let passcode = passcode_from_mac(mac);
    let discriminator = discriminator_from_mac(mac);

    BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_valid_passcode_for_known_mac() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let passcode = passcode_from_mac(&mac);

        // Assert
        assert!(passcode >= MIN_PASSCODE);
        assert!(passcode <= MAX_PASSCODE);
        assert!(!is_forbidden_passcode(passcode));
    }

    #[test]
    fn derives_valid_discriminator_not_equal_to_test_default() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let discriminator = discriminator_from_mac(&mac);

        // Assert
        assert!(discriminator >= MIN_DISCRIMINATOR);
        assert!(discriminator <= MAX_DISCRIMINATOR);
        assert_ne!(discriminator, TEST_DISCRIMINATOR);
    }

    #[test]
    fn different_macs_produce_different_pairing_parameters() {
        // Arrange
        let mac1 = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
        let mac2 = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x45];

        // Act
        let code1 = passcode_from_mac(&mac1);
        let code2 = passcode_from_mac(&mac2);
        let disc1 = discriminator_from_mac(&mac1);
        let disc2 = discriminator_from_mac(&mac2);

        // Assert
        assert_ne!((code1, disc1), (code2, disc2));
    }

    #[test]
    fn comm_data_from_mac_builds_expected_structure() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let comm_data = comm_data_from_mac(&mac);

        // Assert
        assert_eq!(comm_data.discriminator, discriminator_from_mac(&mac));
        let expected_pw: [u8; 4] = passcode_from_mac(&mac).to_le_bytes();
        assert_eq!(comm_data.password.access(), &expected_pw);
    }

    #[test]
    fn hostname_from_mac_formats_lowercase_suffix() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let hostname_buf = hostname_from_mac(&mac);
        let hostname = core::str::from_utf8(&hostname_buf).unwrap();

        // Assert
        assert_eq!(hostname, "intercom-bridge-7844");
    }
}
