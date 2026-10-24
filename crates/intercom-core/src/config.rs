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

/// Minimum configurable door unlock duration in seconds.
pub const MIN_DOOR_UNLOCK_DURATION_SEC: u32 = 1;

/// Maximum configurable door unlock duration in seconds.
pub const MAX_DOOR_UNLOCK_DURATION_SEC: u32 = 30;

/// Default door unlock duration in seconds.
pub const DEFAULT_DOOR_UNLOCK_DURATION_SEC: u32 = 5;

/// Returns true if the given door unlock duration is within valid bounds [1, 30].
pub const fn is_valid_door_unlock_duration(duration_sec: u32) -> bool {
    duration_sec >= MIN_DOOR_UNLOCK_DURATION_SEC && duration_sec <= MAX_DOOR_UNLOCK_DURATION_SEC
}

/// Clamps a door unlock duration to valid domain bounds [1, 30].
pub const fn clamp_door_unlock_duration(duration_sec: u32) -> u32 {
    if duration_sec < MIN_DOOR_UNLOCK_DURATION_SEC {
        MIN_DOOR_UNLOCK_DURATION_SEC
    } else if duration_sec > MAX_DOOR_UNLOCK_DURATION_SEC {
        MAX_DOOR_UNLOCK_DURATION_SEC
    } else {
        duration_sec
    }
}

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

    #[test]
    fn door_unlock_duration_validation_and_clamping() {
        assert!(!is_valid_door_unlock_duration(0));
        assert!(is_valid_door_unlock_duration(1));
        assert!(is_valid_door_unlock_duration(5));
        assert!(is_valid_door_unlock_duration(30));
        assert!(!is_valid_door_unlock_duration(31));

        assert_eq!(clamp_door_unlock_duration(0), 1);
        assert_eq!(clamp_door_unlock_duration(1), 1);
        assert_eq!(clamp_door_unlock_duration(15), 15);
        assert_eq!(clamp_door_unlock_duration(30), 30);
        assert_eq!(clamp_door_unlock_duration(100), 30);
    }
}
