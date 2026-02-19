//! Smart Intercom - Configuration Constants
//!
//! Mirrors the C++ `app_config.h` values from `sdkconfig.defaults`.
//!
//! GPIO pin selection is done in `main.rs` via ESP-HAL peripheral tokens
//! (`peripherals.GPIO2`, etc.). These constants define timing and endpoint IDs.

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

// ============================================================================
// Matter endpoints
// ============================================================================

/// Matter endpoint ID for the door lock cluster
pub const DOOR_LOCK_ENDPOINT_ID: u16 = 1;

/// Matter endpoint ID for the occupancy sensor cluster
pub const OCCUPANCY_SENSOR_ENDPOINT_ID: u16 = 2;
