//! Smart Intercom - Domain Configuration Constants
//!
//! Protocol-agnostic and hardware-agnostic timing policies for the core domain.

// ============================================================================
// Access & Lifecycle Policies
// ============================================================================

/// Duration that visitor chime event remains asserted before returning to idle (ms).
pub const RING_HOLD_MS: u64 = 15000;

/// Automatic relock delay in seconds (logical access state returns to locked).
pub const AUTO_RELOCK_SEC: u64 = 4;

/// Delay before reboot after factory credential erasure (ms) — allows feedback propagation.
pub const FACTORY_RESET_REBOOT_DELAY_MS: u64 = 2000;
