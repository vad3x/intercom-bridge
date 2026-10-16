//! Smart Intercom - Matter Adapter Configuration
//!
//! Protocol-specific endpoint IDs, device serial formatting, and constants
//! defined by the Matter Specification.

use intercom_core::domain::EcosystemId;

// ============================================================================
// Ecosystem Identity
// ============================================================================

/// Strongly-typed identifier for the Matter smart home ecosystem.
pub const MATTER_ECOSYSTEM: EcosystemId = EcosystemId::new("Matter");

// ============================================================================
// Matter Endpoints
// ============================================================================

/// Matter endpoint ID for the door lock cluster
pub const DOOR_LOCK_ENDPOINT_ID: u16 = 1;

/// Matter endpoint ID for the occupancy sensor cluster
pub const OCCUPANCY_SENSOR_ENDPOINT_ID: u16 = 2;

// ============================================================================
// Identity & Serial Number Formatting (Matter Spec 11.1.5.8)
// ============================================================================

/// Device serial number prefix.
pub const SERIAL_NO_PREFIX: &str = "IC-";

/// Length of a MAC-derived serial number (`"IC-"` + 12 uppercase hex digits).
pub const MAC_SERIAL_NO_LEN: usize = 15;

/// Formats a 6-byte MAC address into a deterministic, standards-compliant serial number buffer:
/// `"IC-XXXXXXXXXXXX"`.
///
/// Guaranteed to be 15 ASCII characters, unique per device hardware MAC, and well within
/// Matter's 32-character limit (Matter Specification Section 11.1.5.8).
pub const fn format_serial_no(mac: &[u8; 6]) -> [u8; MAC_SERIAL_NO_LEN] {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut buf = [0u8; MAC_SERIAL_NO_LEN];
    buf[0] = b'I';
    buf[1] = b'C';
    buf[2] = b'-';
    let mut i = 0;
    while i < 6 {
        buf[3 + i * 2] = HEX[(mac[i] >> 4) as usize];
        buf[3 + i * 2 + 1] = HEX[(mac[i] & 0x0F) as usize];
        i += 1;
    }
    buf
}

/// Default DHCP hostname prefix.
pub const HOSTNAME_PREFIX: &str = "intercom-bridge-";

/// Length of a MAC-derived DHCP hostname (`"intercom-bridge-"` + 4 lowercase hex digits).
pub const MAC_HOSTNAME_LEN: usize = 20;

/// Formats the last 2 bytes of a 6-byte MAC address into a deterministic DHCP hostname:
/// `"intercom-bridge-xxxx"` (e.g. `"intercom-bridge-7844"`).
pub const fn format_hostname(mac: &[u8; 6]) -> [u8; MAC_HOSTNAME_LEN] {
    const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";
    let prefix = HOSTNAME_PREFIX.as_bytes();
    let mut buf = [0u8; MAC_HOSTNAME_LEN];
    let mut i = 0;
    while i < prefix.len() {
        buf[i] = prefix[i];
        i += 1;
    }
    buf[16] = HEX_LOWER[(mac[4] >> 4) as usize];
    buf[17] = HEX_LOWER[(mac[4] & 0x0F) as usize];
    buf[18] = HEX_LOWER[(mac[5] >> 4) as usize];
    buf[19] = HEX_LOWER[(mac[5] & 0x0F) as usize];
    buf
}

// ============================================================================
// Subscription Interval Policies (Matter Spec 8.5.1)
// ============================================================================

/// Maximum reporting interval granted to Matter subscriptions in seconds.
///
/// Apple Home requests 600s (10 min). Clamping this to 120s (2 min) forces the
/// Apple Home Hub to check in and re-establish CASE within 2 minutes of an ESP32 reboot.
pub const MAX_SUBSCRIPTION_INTERVAL_SECS: u16 = 120;
