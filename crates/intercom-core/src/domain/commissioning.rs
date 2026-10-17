//! Matter commissioning parameters generation.
//!
//! Derives unique, deterministic Matter commissioning parameters (Passcode and
//! Discriminator) from the hardware MAC address instead of using hardcoded test values.

// ============================================================================
// Public Constants
// ============================================================================

/// Forbidden passcodes per Matter specification (Section 5.1.7.1).
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

/// Standard length of formatted serial number `"IC-XXXXXXXXXXXX"`.
pub const MAC_SERIAL_NO_LEN: usize = 15;

/// Standard length of formatted DHCP hostname `"intercom-bridge-xxxx"`.
pub const MAC_HOSTNAME_LEN: usize = 20;

// ============================================================================
// Public Functions
// ============================================================================

/// Derives a standard serial number buffer `"IC-XXXXXXXXXXXX"` from a 6-byte MAC address.
pub const fn serial_no_from_mac(mac: &[u8; 6]) -> [u8; MAC_SERIAL_NO_LEN] {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; MAC_SERIAL_NO_LEN];
    buf[0] = b'I';
    buf[1] = b'C';
    buf[2] = b'-';

    let mut i = 0;
    while i < 6 {
        let byte = mac[i];
        buf[3 + i * 2] = HEX_CHARS[(byte >> 4) as usize];
        buf[3 + i * 2 + 1] = HEX_CHARS[(byte & 0x0F) as usize];
        i += 1;
    }
    buf
}

/// Derives a standard DHCP hostname buffer `"intercom-bridge-xxxx"` from a 6-byte MAC address.
pub const fn hostname_from_mac(mac: &[u8; 6]) -> [u8; MAC_HOSTNAME_LEN] {
    const PREFIX: &[u8] = b"intercom-bridge-";
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; MAC_HOSTNAME_LEN];

    let mut i = 0;
    while i < PREFIX.len() {
        buf[i] = PREFIX[i];
        i += 1;
    }

    buf[16] = HEX_CHARS[(mac[4] >> 4) as usize];
    buf[17] = HEX_CHARS[(mac[4] & 0x0F) as usize];
    buf[18] = HEX_CHARS[(mac[5] >> 4) as usize];
    buf[19] = HEX_CHARS[(mac[5] & 0x0F) as usize];

    buf
}

/// Derives a valid 8-digit Matter passcode (PIN) from a 6-byte MAC address.
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

/// 64-bit FNV-1a hash over MAC bytes for deterministic distribution.
pub fn mac_hash(mac: &[u8; 6]) -> u64 {
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
    fn hostname_from_mac_formats_lowercase_suffix() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let hostname_buf = hostname_from_mac(&mac);
        let hostname = core::str::from_utf8(&hostname_buf).unwrap();

        // Assert
        assert_eq!(hostname, "intercom-bridge-7844");
    }

    #[test]
    fn serial_no_from_mac_formats_expected_prefix_and_hex() {
        // Arrange
        let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];

        // Act
        let serial_buf = serial_no_from_mac(&mac);
        let serial = core::str::from_utf8(&serial_buf).unwrap();

        // Assert
        assert_eq!(serial, "IC-98A316A67844");
    }
}
