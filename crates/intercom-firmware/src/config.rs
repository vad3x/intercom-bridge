//! Smart Intercom - Firmware Composition Root Configuration
//!
//! Target hardware version, product identification, and flash memory layout
//! specific to the ESP32-C6 Intercom Bridge firmware.

use core::ops::Range;

// ============================================================================
// Device identity (Basic Information cluster)
// ============================================================================

/// Hardware version string.
pub const HW_VER_STR: &str = "1.1";

/// Numeric hardware version derived from `HW_VER_STR`.
pub const HW_VER: u16 = parse_version(HW_VER_STR) as u16;

/// Vendor name shown in smart home ecosystem controllers.
pub const VENDOR_NAME: &str = "Smart Intercom";

/// Product name shown in smart home ecosystem controllers.
pub const PRODUCT_NAME: &str = "Intercom Controller";

/// Device name shown in smart home ecosystem controllers.
pub const DEVICE_NAME: &str = "Smart Intercom";

// ============================================================================
// Flash persistence & Partition Layout (from partitions.csv)
// ============================================================================

/// Start offset of the `otadata` partition in flash.
#[allow(dead_code)]
pub const OTA_DATA_OFFSET: u32 = 0x00f000;
/// Size of the `otadata` partition (8 KiB = 2 sectors).
#[allow(dead_code)]
pub const OTA_DATA_SIZE: u32 = 0x002000;

/// Start offset of `ota_0` app partition (Slot A).
#[allow(dead_code)]
pub const OTA_0_OFFSET: u32 = 0x020000;
/// Start offset of `ota_1` app partition (Slot B).
#[allow(dead_code)]
pub const OTA_1_OFFSET: u32 = 0x370000;
/// Size of each app partition (3.4 MiB = 896 sectors × 4 KiB).
#[allow(dead_code)]
pub const OTA_SLOT_SIZE: u32 = 0x350000;

/// Start offset of the `matter_kvs` partition in flash (from partitions.csv).
pub const MATTER_KVS_OFFSET: u32 = 0x6c0000;

/// Size of the `matter_kvs` partition (24 KiB = 6 sectors × 4 KiB).
pub const MATTER_KVS_SIZE: u32 = 0x6000;

/// Flash range for the Matter KV blob store.
pub const MATTER_KVS_RANGE: Range<u32> =
    MATTER_KVS_OFFSET..MATTER_KVS_OFFSET + MATTER_KVS_SIZE;

// ============================================================================
// OTA Firmware Update Security
// ============================================================================

/// Ed25519 public key used to verify incoming OTA firmware images.
pub const OTA_PUBLIC_KEY: [u8; 32] = [
    0xc5, 0xcc, 0x7f, 0x68, 0xac, 0xce, 0x5d, 0xa4, 0x0c, 0x92, 0xa9, 0xf9,
    0x09, 0xaf, 0xaa, 0xcf, 0xb6, 0x42, 0xa3, 0x62, 0xf4, 0xae, 0xc0, 0x09,
    0xfc, 0xb8, 0x40, 0x16, 0x39, 0x05, 0xc9, 0xa8,
];

// ============================================================================
// Private helpers
// ============================================================================

/// Parse a dotted version string (e.g. `"1.2.3"`, `"1.1"`) into a numeric
/// value at compile time: `major * 10000 + minor * 100 + patch`.
const fn parse_version(version: &str) -> u32 {
    let bytes = version.as_bytes();
    let mut i = 0;
    let mut parts = [0u32; 3];
    let mut part = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            part += 1;
        } else {
            parts[part] = parts[part] * 10 + (bytes[i] - b'0') as u32;
        }
        i += 1;
    }
    parts[0] * 10000 + parts[1] * 100 + parts[2]
}
