//! Smart Intercom - Configuration Constants
//!
//! Timing constants, endpoint IDs, device identification, and partition ranges.

// ============================================================================
// Device identity (Basic Information cluster)
// ============================================================================

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

/// How long the BOOT button must be held to trigger a factory reset (ms)
pub const FACTORY_RESET_HOLD_MS: u64 = 5000;

// ----------------------------------------------------------------------------
// LED patterns — asymmetric duty cycles (on_ms / off_ms)
//
// Duty cycle increases with system readiness:
//   Initializing (17%) → Initialized (30%) → Connecting (50%) → Connected (90%) → Ready (solid)
// Error and FactoryReset break the pattern intentionally to stand out.
// ----------------------------------------------------------------------------

/// Initializing: slow heartbeat — brief flash, long dark (~17% duty)
pub const INITIALIZING_ON_MS: u64 = 200;
pub const INITIALIZING_OFF_MS: u64 = 1000;

/// Initialized: faster heartbeat — hardware ready, waiting (30% duty)
pub const INITIALIZED_ON_MS: u64 = 300;
pub const INITIALIZED_OFF_MS: u64 = 700;

/// Connecting: even medium blink — actively connecting (50% duty)
pub const CONNECTING_ON_MS: u64 = 500;
pub const CONNECTING_OFF_MS: u64 = 500;

/// Connected: mostly on, brief wink — almost ready (90% duty)
pub const CONNECTED_ON_MS: u64 = 900;
pub const CONNECTED_OFF_MS: u64 = 100;

/// Error: burst pattern — 2 quick flashes then a long pause (irregular, unmistakable)
pub const ERROR_ON_MS: u64 = 100;
pub const ERROR_OFF_MS: u64 = 100;
pub const ERROR_BURST_COUNT: u8 = 2;
pub const ERROR_PAUSE_MS: u64 = 700;

/// Factory reset: ultra-rapid strobe — critical action in progress (50% duty, very fast)
pub const FACTORY_RESET_ON_MS: u64 = 50;
pub const FACTORY_RESET_OFF_MS: u64 = 50;

/// Duration of each half of the ring indication blink cycle (ms)
pub const RING_BLINK_MS: u64 = 100;

/// Number of blinks for ring indication
pub const RING_BLINK_COUNT: u8 = 2;

/// Duration of each half of the unlock indication blink cycle (ms)
pub const UNLOCK_BLINK_MS: u64 = 200;

/// Number of blinks for unlock indication
pub const UNLOCK_BLINK_COUNT: u8 = 2;

/// Delay before reboot after factory reset (ms) — gives indicator time to blink
pub const FACTORY_RESET_REBOOT_DELAY_MS: u64 = 2000;

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
