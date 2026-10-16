//! Matter commissioning parameters generation.
//!
//! Derives unique, deterministic Matter commissioning parameters (Passcode and
//! Discriminator) from the hardware MAC address instead of using hardcoded test values.

use rs_matter::BasicCommData;

// ============================================================================
// Public Constants
// ============================================================================

/// Forbidden passcodes per Matter specification (Section 5.1.7.1).
///
/// Commissioners (Apple Home, Google Home, Chip-tool) reject these repetitive
/// or sequential passcodes.
pub const FORBIDDEN_PASSCODES: [u32; 10] = [
    11111111, 22222222, 33333333, 44444444,
    55555555, 66666666, 77777777, 88888888,
    12345678, 87654321,
];

/// Minimum 8-digit passcode for generated codes (avoids small/test numbers).
pub const MIN_PASSCODE: u32 = 10_000_001;

/// Maximum allowed passcode per Matter specification (Section 5.1.7.1).
pub const MAX_PASSCODE: u32 = 99_999_998;

/// Minimum allowed discriminator (avoids 0 which may be treated as unset).
pub const MIN_DISCRIMINATOR: u16 = 1;

/// Maximum allowed 12-bit discriminator (0x0FFF).
pub const MAX_DISCRIMINATOR: u16 = 4095;

/// Well-known test discriminator used by rs-matter and chip-tool examples.
pub const TEST_DISCRIMINATOR: u16 = 3840;

// ============================================================================
// Public Functions
// ============================================================================

/// Derives a standard serial number buffer `"IC-XXXXXXXXXXXX"` from a 6-byte MAC address.
pub const fn serial_no_from_mac(mac: &[u8; 6]) -> [u8; crate::config::MAC_SERIAL_NO_LEN] {
    crate::config::format_serial_no(mac)
}

/// Derives a standard DHCP hostname buffer `"intercom-bridge-xxxx"` from a 6-byte MAC address.
pub const fn hostname_from_mac(mac: &[u8; 6]) -> [u8; crate::config::MAC_HOSTNAME_LEN] {
    crate::config::format_hostname(mac)
}

/// Derives `BasicCommData` from a 6-byte hardware MAC address.
pub fn comm_data_from_mac(mac: &[u8; 6]) -> BasicCommData {
    let passcode = passcode_from_mac(mac);
    let discriminator = discriminator_from_mac(mac);

    BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    }
}

/// Derives a valid 8-digit Matter passcode (PIN) from a 6-byte MAC address.
///
/// The resulting passcode:
/// - Is in range `10_000_001..=99_999_998`
/// - Does not collide with forbidden passcodes (e.g. `11111111`, `12345678`)
pub fn passcode_from_mac(mac: &[u8; 6]) -> u32 {
    let hash = mac_hash(mac);
    let span = (MAX_PASSCODE - MIN_PASSCODE + 1) as u64;
    let mut code = MIN_PASSCODE + ((hash % span) as u32);

    while is_forbidden_passcode(code) {
        code = if code >= MAX_PASSCODE {
            MIN_PASSCODE
        } else {
            code + 1
        };
    }

    code
}

/// Derives a valid 12-bit Matter discriminator from a 6-byte MAC address.
///
/// The resulting discriminator:
/// - Is in range `1..=4095`
/// - Does not collide with the well-known test discriminator `3840` (0x0F00)
pub fn discriminator_from_mac(mac: &[u8; 6]) -> u16 {
    let hash = mac_hash(mac);
    let mut disc = ((hash >> 32) as u16) & MAX_DISCRIMINATOR;

    if disc == 0 || disc == TEST_DISCRIMINATOR {
        disc = (disc + 1) & MAX_DISCRIMINATOR;
        if disc == 0 {
            disc = MIN_DISCRIMINATOR;
        }
    }

    disc
}

/// Checks whether a passcode is invalid or forbidden by the Matter specification.
pub fn is_forbidden_passcode(passcode: u32) -> bool {
    if passcode < 1 || passcode > MAX_PASSCODE {
        return true;
    }
    FORBIDDEN_PASSCODES.contains(&passcode)
}

// ============================================================================
// Private Helper Functions
// ============================================================================

/// 64-bit FNV-1a hash over MAC bytes for deterministic distribution.
fn mac_hash(mac: &[u8; 6]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &b in mac {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
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
