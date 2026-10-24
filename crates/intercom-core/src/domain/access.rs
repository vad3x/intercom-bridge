//! Smart Intercom - Access Control Domain State Machine
//!
//! Pure, deterministic state machine orchestrating door unlocks,
//! auto-relock deadlines, and visitor chime holds.
//! Completely independent of Embassy, timers, or hardware ports.

use crate::config::{clamp_door_unlock_duration, DEFAULT_DOOR_UNLOCK_DURATION_SEC, RING_HOLD_MS};
use crate::ports::inbound::AccessCommand;
use crate::ports::reporter::LockState;

/// Side effects emitted by state transitions for the service coordinator to execute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AccessEffects {
    /// Whether the physical door unlock mechanism should trigger.
    pub trigger_unlock: bool,
    /// Updated lock state to report to outbound observers (smart home networks, feedback adapters).
    pub report_lock: Option<LockState>,
    /// Updated chime/ring active state to report to outbound observers.
    pub report_ring: Option<bool>,
    /// Updated configured unlock duration in seconds to report to outbound observers.
    pub report_duration: Option<u32>,
}

/// Pure domain state machine for intercom access control.
#[derive(Clone, Copy, Debug)]
pub struct AccessStateMachine {
    ring_active: bool,
    ring_deadline_ms: Option<u64>,
    relock_deadline_ms: Option<u64>,
    unlock_duration_sec: u32,
}

impl AccessStateMachine {
    /// Create a new access state machine in the default (idle, locked) state.
    pub const fn new() -> Self {
        Self {
            ring_active: false,
            ring_deadline_ms: None,
            relock_deadline_ms: None,
            unlock_duration_sec: DEFAULT_DOOR_UNLOCK_DURATION_SEC,
        }
    }

    /// Create a new access state machine with a specific unlock duration in seconds.
    pub const fn with_duration(mut self, duration_sec: u32) -> Self {
        self.unlock_duration_sec = clamp_door_unlock_duration(duration_sec);
        self
    }

    /// Returns the configured unlock duration in seconds.
    pub const fn unlock_duration_sec(&self) -> u32 {
        self.unlock_duration_sec
    }

    /// Process an inbound command and return the required effects.
    pub fn handle_command(&mut self, cmd: AccessCommand, now_ms: u64) -> AccessEffects {
        match cmd {
            AccessCommand::Unlock => self.handle_unlock(now_ms),
            AccessCommand::Ring => self.handle_ring(now_ms),
            AccessCommand::SetUnlockDuration(sec) => {
                let clamped = clamp_door_unlock_duration(sec);
                self.unlock_duration_sec = clamped;
                AccessEffects {
                    report_duration: Some(clamped),
                    ..Default::default()
                }
            }
        }
    }

    /// Process an unlock request: triggers unlock actuation and schedules auto-relock.
    pub fn handle_unlock(&mut self, now_ms: u64) -> AccessEffects {
        self.relock_deadline_ms = Some(now_ms + (self.unlock_duration_sec as u64 * 1000));

        AccessEffects {
            trigger_unlock: true,
            report_lock: Some(LockState::Unlocked),
            report_ring: None,
            report_duration: None,
        }
    }

    /// Process a ring trigger: marks ring active and schedules hold expiration.
    pub fn handle_ring(&mut self, now_ms: u64) -> AccessEffects {
        self.ring_active = true;
        self.ring_deadline_ms = Some(now_ms + RING_HOLD_MS);

        AccessEffects {
            trigger_unlock: false,
            report_lock: None,
            report_ring: Some(true),
            report_duration: None,
        }
    }

    /// Poll active timer deadlines against the current time in milliseconds.
    pub fn poll_deadlines(&mut self, now_ms: u64) -> AccessEffects {
        let mut effects = AccessEffects::default();

        if let Some(deadline) = self.ring_deadline_ms {
            if now_ms >= deadline {
                self.ring_active = false;
                self.ring_deadline_ms = None;
                effects.report_ring = Some(false);
            }
        }

        if let Some(deadline) = self.relock_deadline_ms {
            if now_ms >= deadline {
                self.relock_deadline_ms = None;
                effects.report_lock = Some(LockState::Locked);
            }
        }

        effects
    }

    /// Returns the nearest active deadline in milliseconds, or `None` if idle.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.ring_deadline_ms, self.relock_deadline_ms) {
            (Some(r), Some(l)) => Some(r.min(l)),
            (Some(r), None) => Some(r),
            (None, Some(l)) => Some(l),
            (None, None) => None,
        }
    }

    /// Returns true if visitor chime/ring hold is currently active.
    pub fn is_ring_active(&self) -> bool {
        self.ring_active
    }

    /// Returns true if an auto-relock timer is currently active.
    pub fn is_unlock_pending(&self) -> bool {
        self.relock_deadline_ms.is_some()
    }
}

impl Default for AccessStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Unit Tests (Arrange -> Act -> Assert)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_triggers_actuation_and_reports_unlocked() {
        // Arrange
        let mut sm = AccessStateMachine::new();

        // Act
        let effects = sm.handle_unlock(1000);

        // Assert
        assert!(effects.trigger_unlock);
        assert_eq!(effects.report_lock, Some(LockState::Unlocked));
        assert_eq!(sm.next_deadline(), Some(1000 + sm.unlock_duration_sec() as u64 * 1000));
    }

    #[test]
    fn auto_relock_expires_at_deadline() {
        // Arrange
        let mut sm = AccessStateMachine::new();
        sm.handle_unlock(1000);
        let relock_time = 1000 + (sm.unlock_duration_sec() as u64 * 1000);

        // Act - before deadline
        let pre_effects = sm.poll_deadlines(relock_time - 1);

        // Assert - still unlocked
        assert_eq!(pre_effects.report_lock, None);
        assert!(sm.is_unlock_pending());

        // Act - at deadline
        let post_effects = sm.poll_deadlines(relock_time);

        // Assert - relocked
        assert_eq!(post_effects.report_lock, Some(LockState::Locked));
        assert!(!sm.is_unlock_pending());
        assert_eq!(sm.next_deadline(), None);
    }

    #[test]
    fn ring_activates_chime_and_holds_for_duration() {
        // Arrange
        let mut sm = AccessStateMachine::new();

        // Act
        let effects = sm.handle_ring(2000);

        // Assert
        assert_eq!(effects.report_ring, Some(true));
        assert!(sm.is_ring_active());
        assert_eq!(sm.next_deadline(), Some(2000 + RING_HOLD_MS));

        // Act - fast forward to expiration
        let expiry = sm.poll_deadlines(2000 + RING_HOLD_MS);

        // Assert
        assert_eq!(expiry.report_ring, Some(false));
        assert!(!sm.is_ring_active());
        assert_eq!(sm.next_deadline(), None);
    }

    #[test]
    fn concurrent_ring_and_unlock_deadlines_expire_independently() {
        // Arrange
        let mut sm = AccessStateMachine::new();

        // Act - ring at t=1000, unlock at t=2000
        sm.handle_ring(1000); // Ring deadline = 1000 + 15000 = 16000
        sm.handle_unlock(2000); // Relock deadline = 2000 + 5000 = 7000

        // Assert - earliest deadline is the relock at t=7000
        assert_eq!(sm.next_deadline(), Some(7000));

        // Act - advance time to relock deadline (t=7000)
        let relock_effects = sm.poll_deadlines(7000);

        // Assert - door relocked, ring still active, next deadline is ring hold expiry (t=16000)
        assert_eq!(relock_effects.report_lock, Some(LockState::Locked));
        assert_eq!(relock_effects.report_ring, None);
        assert!(sm.is_ring_active());
        assert!(!sm.is_unlock_pending());
        assert_eq!(sm.next_deadline(), Some(16000));

        // Act - advance time to ring hold expiration (t=16000)
        let ring_effects = sm.poll_deadlines(16000);

        // Assert - ring cleared, no further deadlines
        assert_eq!(ring_effects.report_ring, Some(false));
        assert_eq!(ring_effects.report_lock, None);
        assert!(!sm.is_ring_active());
        assert_eq!(sm.next_deadline(), None);
    }

    #[test]
    fn repeated_unlock_command_extends_relock_deadline() {
        // Arrange
        let mut sm = AccessStateMachine::new();
        sm.handle_unlock(1000); // Initial deadline = 1000 + 5000 = 6000

        // Act - user taps unlock again at t=3000
        sm.handle_unlock(3000); // Extended deadline = 3000 + 5000 = 8000

        // Assert - deadline was pushed back
        assert_eq!(sm.next_deadline(), Some(8000));

        // Act - check at original expiration time (t=6000)
        let mid_effects = sm.poll_deadlines(6000);

        // Assert - door remains unlocked
        assert_eq!(mid_effects.report_lock, None);
        assert!(sm.is_unlock_pending());

        // Act - check at new extended expiration time (t=8000)
        let final_effects = sm.poll_deadlines(8000);

        // Assert - door locks at the extended deadline
        assert_eq!(final_effects.report_lock, Some(LockState::Locked));
        assert!(!sm.is_unlock_pending());
    }

    #[test]
    fn access_state_machine_with_duration_clamps_properly() {
        let sm1 = AccessStateMachine::new().with_duration(15);
        assert_eq!(sm1.unlock_duration_sec(), 15);

        let sm2 = AccessStateMachine::new().with_duration(0);
        assert_eq!(sm2.unlock_duration_sec(), 1);

        let sm3 = AccessStateMachine::new().with_duration(100);
        assert_eq!(sm3.unlock_duration_sec(), 30);
    }

    #[test]
    fn repeated_ring_command_extends_chime_hold() {
        // Arrange
        let mut sm = AccessStateMachine::new();
        sm.handle_ring(1000); // Initial deadline = 1000 + 15000 = 16000

        // Act - visitor rings again at t=5000
        sm.handle_ring(5000); // Extended deadline = 5000 + 15000 = 20000

        // Assert - deadline was pushed back
        assert_eq!(sm.next_deadline(), Some(20000));

        // Act - check at original expiration time (t=16000)
        let mid_effects = sm.poll_deadlines(16000);

        // Assert - chime remains active
        assert_eq!(mid_effects.report_ring, None);
        assert!(sm.is_ring_active());

        // Act - check at extended expiration time (t=20000)
        let final_effects = sm.poll_deadlines(20000);

        // Assert - chime hold clears at extended deadline
        assert_eq!(final_effects.report_ring, Some(false));
        assert!(!sm.is_ring_active());
    }
}
