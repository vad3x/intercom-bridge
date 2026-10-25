//! Smart Intercom - GPIO Adapter Configuration
//!
//! Intercom owns its reset hold policy and compile-time LED timing profile,
//! while the foundation supplies the generic button and LED drivers. Only
//! Intercom-specific ring and door timings remain below.

// ============================================================================
// Intercom Reset Hold Policy
// ============================================================================

/// How long the BOOT button must be held to trigger ecosystem reset in
/// milliseconds (unpairs the active smart home ecosystem, preserves Wi-Fi).
pub const ECOSYSTEM_RESET_HOLD_MS: u64 = 5000;

/// How long the BOOT button must be held to trigger full factory reset in
/// milliseconds (erases both Wi-Fi credentials and ecosystem fabrics).
pub const FACTORY_RESET_HOLD_MS: u64 = 10000;

// ============================================================================
// Intercom LED Timing Profile
// ============================================================================

// Every field is spelled out so Intercom owns its profile explicitly.
edf_gpio::led_profile! {
    /// Compile-time LED pattern timings for the Intercom Bridge indicator.
    pub struct IntercomLedProfile;

    booting_on_ms: 200,
    booting_off_ms: 1000,
    starting_on_ms: 300,
    starting_off_ms: 700,
    commissioning_on_ms: 250,
    commissioning_off_ms: 250,
    wifi_connecting_on_ms: 500,
    wifi_connecting_off_ms: 500,
    ecosystem_connecting_on_ms: 900,
    ecosystem_connecting_off_ms: 100,
    error_on_ms: 100,
    error_off_ms: 100,
    error_burst_count: 2,
    error_pause_ms: 700,
    ecosystem_reset_on_ms: 250,
    ecosystem_reset_off_ms: 250,
    factory_reset_on_ms: 50,
    factory_reset_off_ms: 50,
    updating_strobe_ms: 50,
    activity_pulse_ms: 150,
}

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
