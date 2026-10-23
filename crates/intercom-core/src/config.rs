//! Smart Intercom - Domain Configuration Constants
//!
//! Protocol-agnostic and hardware-agnostic timing policies for the core domain.

// ============================================================================
// Access & Lifecycle Policies
// ============================================================================

pub use edf_core::config::FACTORY_RESET_REBOOT_DELAY_MS;

/// Duration that visitor chime event remains asserted before returning to idle (ms).
pub const RING_HOLD_MS: u64 = 15000;

/// Automatic relock delay in seconds (logical access state returns to locked).
pub const AUTO_RELOCK_SEC: u64 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_timing_policies_are_preserved() {
        // Arrange
        let delays = (FACTORY_RESET_REBOOT_DELAY_MS, RING_HOLD_MS, AUTO_RELOCK_SEC);

        // Act
        let expected = (2000, 15000, 4);

        // Assert
        assert_eq!(delays, expected);
    }
}
