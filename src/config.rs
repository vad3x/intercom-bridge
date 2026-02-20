//! Smart Intercom - Configuration Constants
//!
//! Mirrors the C++ `app_config.h` values from `sdkconfig.defaults`.
//!
//! GPIO pin selection is done in `main.rs` via ESP-HAL peripheral tokens
//! (`peripherals.GPIO2`, etc.). These constants define timing and endpoint IDs.

// ============================================================================
// Device identity (Basic Information cluster)
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

/// Hardware version string.
pub const HW_VER_STR: &str = "1.1";

/// Numeric hardware version derived from `HW_VER_STR`.
pub const HW_VER: u16 = parse_version(HW_VER_STR) as u16;

/// Human-readable software version string from `Cargo.toml`.
pub const SW_VER_STR: &str = env!("CARGO_PKG_VERSION");

/// Numeric software version derived from `Cargo.toml` semver at compile time.
pub const SW_VER: u32 = parse_version(SW_VER_STR);

/// Device serial number.
pub const SERIAL_NO: &str = "IC-00001";

/// Vendor name shown in Matter/HomeKit.
pub const VENDOR_NAME: &str = "Smart Intercom";

/// Product name shown in Matter/HomeKit.
pub const PRODUCT_NAME: &str = "Intercom Controller";

/// Device name shown in Matter/HomeKit.
pub const DEVICE_NAME: &str = "Smart Intercom";

// ============================================================================
// Timing
// ============================================================================

/// Duration of unlock pulse in milliseconds
pub const UNLOCK_PULSE_MS: u64 = 5000;

/// Debounce time for ring detection in milliseconds
pub const RING_DEBOUNCE_MS: u64 = 30;

/// Cooldown between ring events in milliseconds
pub const RING_COOLDOWN_MS: u64 = 2500;

/// How long occupancy stays active after ring in milliseconds
pub const RING_HOLD_MS: u64 = 15000;

/// Auto-relock time in seconds (Matter state only, door springs back mechanically)
pub const AUTO_RELOCK_SEC: u64 = 4;

pub const INITIALIZING_BLINK_MS: u64 = 300;

/// Duration of each half of the connecting blink cycle
pub const CONNECTING_BLINK_MS: u64 = 1000;

/// Duration of each half of the connecting blink cycle
pub const ERROR_BLINK_MS: u64 = 200;

/// Duration of each half of the ring indication blink cycle (ms)
pub const RING_BLINK_MS: u64 = 100;

/// Number of blinks for ring indication
pub const RING_BLINK_COUNT: u8 = 2;

/// Duration of each half of the unlock indication blink cycle (ms)
pub const UNLOCK_BLINK_MS: u64 = 200;

/// Number of blinks for unlock indication
pub const UNLOCK_BLINK_COUNT: u8 = 2;

// ============================================================================
// Matter endpoints
// ============================================================================

/// Matter endpoint ID for the door lock cluster
pub const DOOR_LOCK_ENDPOINT_ID: u16 = 1;

/// Matter endpoint ID for the occupancy sensor cluster
pub const OCCUPANCY_SENSOR_ENDPOINT_ID: u16 = 2;

// ============================================================================
// Flash persistence (Matter commissioning data)
// ============================================================================

/// Start offset of the `matter_kvs` partition in flash (from partitions.csv).
pub const MATTER_KVS_OFFSET: u32 = 0x310000;

/// Size of the `matter_kvs` partition (24 KiB = 6 sectors × 4 KiB).
pub const MATTER_KVS_SIZE: u32 = 0x6000;

/// Flash range for the Matter KV blob store.
pub const MATTER_KVS_RANGE: core::ops::Range<u32> =
    MATTER_KVS_OFFSET..MATTER_KVS_OFFSET + MATTER_KVS_SIZE;
