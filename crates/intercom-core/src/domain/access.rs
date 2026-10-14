//! Smart Intercom - Access Control Domain State Machine
//!
//! Pure, deterministic state machine orchestrating door unlocks,
//! auto-relock deadlines, and visitor chime holds.
//! Completely independent of Embassy, timers, or hardware ports.

use crate::config::{AUTO_RELOCK_SEC, RING_HOLD_MS};
use crate::ports::inbound::AccessCommand;
use crate::ports::reporter::LockState;

/// Side effects emitted by state transitions for the actor pump to execute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AccessEffects {
    /// Whether the physical door strike relay should trigger.
    pub trigger_relay: bool,
    /// Updated lock state to report to external smart home ecosystems.
    pub report_lock: Option<LockState>,
    /// Updated chime/ring active state to report to external ecosystems.
    pub report_ring: Option<bool>,
    /// Whether to show the unlock visual indicator pattern.
    pub indicate_unlock: bool,
    /// Whether to show the ring visual indicator pattern.
    pub indicate_ring: bool,
}

/// Pure domain state machine for intercom access control.
#[derive(Clone, Copy, Debug)]
pub struct AccessStateMachine {
    ring_active: bool,
    ring_deadline_ms: Option<u64>,
    relock_deadline_ms: Option<u64>,
}

impl AccessStateMachine {
    /// Create a new access state machine in the default (idle, locked) state.
    pub const fn new() -> Self {
        Self {
            ring_active: false,
            ring_deadline_ms: None,
            relock_deadline_ms: None,
        }
    }

    /// Process an inbound command and return the required effects.
    pub fn handle_command(&mut self, cmd: AccessCommand, now_ms: u64) -> AccessEffects {
        match cmd {
            AccessCommand::Unlock => self.handle_unlock(now_ms),
            AccessCommand::Ring => self.handle_ring(now_ms),
        }
    }

    /// Process an unlock request: triggers relay pulse, starts auto-relock, and indicates unlock.
    pub fn handle_unlock(&mut self, now_ms: u64) -> AccessEffects {
        self.relock_deadline_ms = Some(now_ms + (AUTO_RELOCK_SEC * 1000));

        AccessEffects {
            trigger_relay: true,
            report_lock: Some(LockState::Unlocked),
            report_ring: None,
            indicate_unlock: true,
            indicate_ring: false,
        }
    }

    /// Process a ring trigger: marks ring active, schedules hold expiration, and indicates ring.
    pub fn handle_ring(&mut self, now_ms: u64) -> AccessEffects {
        self.ring_active = true;
        self.ring_deadline_ms = Some(now_ms + RING_HOLD_MS);

        AccessEffects {
            trigger_relay: false,
            report_lock: None,
            report_ring: Some(true),
            indicate_unlock: false,
            indicate_ring: true,
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

// ============================================================================
// Unit Tests (Arrange -> Act -> Assert)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_triggers_relay_and_reports_unlocked() {
        // Arrange
        let mut sm = AccessStateMachine::new();

        // Act
        let effects = sm.handle_unlock(1000);

        // Assert
        assert!(effects.trigger_relay);
        assert_eq!(effects.report_lock, Some(LockState::Unlocked));
        assert!(effects.indicate_unlock);
        assert_eq!(sm.next_deadline(), Some(1000 + AUTO_RELOCK_SEC * 1000));
    }

    #[test]
    fn auto_relock_expires_at_deadline() {
        // Arrange
        let mut sm = AccessStateMachine::new();
        sm.handle_unlock(1000);
        let relock_time = 1000 + (AUTO_RELOCK_SEC * 1000);

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
        assert!(effects.indicate_ring);
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
        sm.handle_unlock(2000); // Relock deadline = 2000 + 4000 = 6000

        // Assert - earliest deadline is the relock at t=6000
        assert_eq!(sm.next_deadline(), Some(6000));

        // Act - advance time to relock deadline (t=6000)
        let relock_effects = sm.poll_deadlines(6000);

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
        sm.handle_unlock(1000); // Initial deadline = 1000 + 4000 = 5000

        // Act - user taps unlock again at t=3000
        sm.handle_unlock(3000); // Extended deadline = 3000 + 4000 = 7000

        // Assert - deadline was pushed back
        assert_eq!(sm.next_deadline(), Some(7000));

        // Act - check at original expiration time (t=5000)
        let mid_effects = sm.poll_deadlines(5000);

        // Assert - door remains unlocked
        assert_eq!(mid_effects.report_lock, None);
        assert!(sm.is_unlock_pending());

        // Act - check at new extended expiration time (t=7000)
        let final_effects = sm.poll_deadlines(7000);

        // Assert - door locks at the extended deadline
        assert_eq!(final_effects.report_lock, Some(LockState::Locked));
        assert!(!sm.is_unlock_pending());
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
