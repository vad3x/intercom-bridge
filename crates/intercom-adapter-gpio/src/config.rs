//! Smart Intercom - GPIO Adapter Configuration
//!
//! Electrical timing constants, debounce intervals, and LED blink patterns
//! specific to the hardware GPIO peripheral drivers.

// ============================================================================
// Physical Electrical Actuation & Sensing Timings
// ============================================================================

/// Duration of physical door strike relay pulse in milliseconds (PhotoMOS).
pub const UNLOCK_PULSE_MS: u64 = 5000;

/// Debounce time for ring optocoupler input in milliseconds.
pub const RING_DEBOUNCE_MS: u64 = 30;

/// Cooldown between ring optocoupler events in milliseconds.
pub const RING_COOLDOWN_MS: u64 = 2500;

/// How long the BOOT button must be held to trigger ecosystem reset (unpair Matter, keep Wi-Fi).
pub const ECOSYSTEM_RESET_HOLD_MS: u64 = 5000;

/// How long the BOOT button must be held to trigger full factory reset (erase Wi-Fi + Matter).
pub const FACTORY_RESET_HOLD_MS: u64 = 10000;

// ============================================================================
// LED Blink Patterns — Asymmetric Duty Cycles (on_ms / off_ms)
//
// Duty cycle increases with system readiness:
//   Booting (17%) → Starting (30%) → Commissioning (2 Hz) / WifiConnecting (1 Hz) → EcosystemConnecting (90%) → Operational (solid)
// Error and FactoryReset break the pattern intentionally to stand out.
// ============================================================================

/// Booting: slow heartbeat — brief flash, long dark (~17% duty)
pub const BOOTING_ON_MS: u64 = 200;
pub const BOOTING_OFF_MS: u64 = 1000;

/// Starting: faster heartbeat — hardware ready, starting stack (30% duty)
pub const STARTING_ON_MS: u64 = 300;
pub const STARTING_OFF_MS: u64 = 700;

/// Commissioning: rapid 2 Hz blink — awaiting initial pairing (50% duty)
pub const COMMISSIONING_ON_MS: u64 = 250;
pub const COMMISSIONING_OFF_MS: u64 = 250;

/// WifiConnecting: even medium 1 Hz blink — associating with Wi-Fi (50% duty)
pub const WIFI_CONNECTING_ON_MS: u64 = 500;
pub const WIFI_CONNECTING_OFF_MS: u64 = 500;

/// EcosystemConnecting: mostly on, brief wink — LAN up, awaiting hub subscription (90% duty)
pub const ECOSYSTEM_CONNECTING_ON_MS: u64 = 900;
pub const ECOSYSTEM_CONNECTING_OFF_MS: u64 = 100;

/// Error: burst pattern — 2 quick flashes then a long pause (irregular, unmistakable)
pub const ERROR_ON_MS: u64 = 100;
pub const ERROR_OFF_MS: u64 = 100;
pub const ERROR_BURST_COUNT: u8 = 2;
pub const ERROR_PAUSE_MS: u64 = 700;

/// Ecosystem reset: medium 2 Hz blink — unpairing active ecosystem (50% duty)
pub const ECOSYSTEM_RESET_ON_MS: u64 = 250;
pub const ECOSYSTEM_RESET_OFF_MS: u64 = 250;

/// Factory reset: ultra-rapid strobe — critical action in progress (50% duty, very fast)
pub const FACTORY_RESET_ON_MS: u64 = 50;
pub const FACTORY_RESET_OFF_MS: u64 = 50;

/// LED rapid strobe period during OTA update (50ms ON, 50ms OFF = 10 Hz)
pub const UPDATING_STROBE_MS: u64 = 50;

/// Duration of each half of the ring indication blink cycle (ms)
pub const RING_BLINK_MS: u64 = 100;

/// Number of blinks for ring indication
pub const RING_BLINK_COUNT: u8 = 2;

/// Duration of each half of the unlock indication blink cycle (ms)
pub const UNLOCK_BLINK_MS: u64 = 200;

/// Number of blinks for unlock indication
pub const UNLOCK_BLINK_COUNT: u8 = 2;
