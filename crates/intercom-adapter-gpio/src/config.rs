//! Smart Intercom - GPIO Adapter Configuration
//!
//! Shared lifecycle timing and LED blink patterns are re-exported from
//! `edf-gpio`; only Intercom-specific ring and door timings remain local.

// ============================================================================
// Shared Reset, Debounce & Lifecycle Indicator Patterns
// ============================================================================

pub use edf_gpio::config::{
    ACTIVITY_PULSE_MS, BOOTING_OFF_MS, BOOTING_ON_MS, BUTTON_DEBOUNCE_MS, BUTTON_POLL_INTERVAL_MS,
    COMMISSIONING_OFF_MS, COMMISSIONING_ON_MS, ECOSYSTEM_CONNECTING_OFF_MS,
    ECOSYSTEM_CONNECTING_ON_MS, ECOSYSTEM_RESET_HOLD_MS, ECOSYSTEM_RESET_OFF_MS,
    ECOSYSTEM_RESET_ON_MS, ERROR_BURST_COUNT, ERROR_OFF_MS, ERROR_ON_MS, ERROR_PAUSE_MS,
    FACTORY_RESET_HOLD_MS, FACTORY_RESET_OFF_MS, FACTORY_RESET_ON_MS, STARTING_OFF_MS,
    STARTING_ON_MS, UPDATING_STROBE_MS, WIFI_CONNECTING_OFF_MS, WIFI_CONNECTING_ON_MS,
};

// ============================================================================
// Intercom-Specific Electrical Actuation & Sensing Timings
// ============================================================================

/// Duration of physical door strike relay pulse in milliseconds (PhotoMOS).
pub const UNLOCK_PULSE_MS: u64 = 5000;

/// Debounce time for ring optocoupler input in milliseconds.
pub const RING_DEBOUNCE_MS: u64 = 30;

/// Cooldown between ring optocoupler events in milliseconds.
pub const RING_COOLDOWN_MS: u64 = 2500;

/// Duration of each half of the ring indication blink cycle (ms).
pub const RING_BLINK_MS: u64 = 100;

/// Number of blinks for ring indication.
pub const RING_BLINK_COUNT: u8 = 2;

/// Duration of each half of the unlock indication blink cycle (ms).
pub const UNLOCK_BLINK_MS: u64 = 200;

/// Number of blinks for unlock indication.
pub const UNLOCK_BLINK_COUNT: u8 = 2;
