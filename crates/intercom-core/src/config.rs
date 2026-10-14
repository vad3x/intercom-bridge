//! Smart Intercom - Domain Configuration Constants
//!
//! Timing constants, debounce durations, and indicator patterns for the core domain.

// ============================================================================
// Timing
// ============================================================================

/// Duration of unlock pulse in milliseconds
pub const UNLOCK_PULSE_MS: u64 = 5000;

/// Debounce time for ring detection in milliseconds
pub const RING_DEBOUNCE_MS: u64 = 30;

/// Cooldown between ring events in milliseconds
pub const RING_COOLDOWN_MS: u64 = 2500;

/// Duration that visitor chime event remains asserted before returning to idle (ms)
pub const RING_HOLD_MS: u64 = 15000;

/// Automatic relock delay in seconds (logical access state returns to locked)
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
