//! Smart Intercom - Matter Access Control Driver & Ecosystem Reporter
//!
//! Composite driver wrapping `IntercomDoorLock` and `IntercomDoorbellSwitch`,
//! implementing the `IntercomReporter` domain port.

use intercom_core::ports::{IntercomReporter, LockState};

use super::door_lock::IntercomDoorLock;
use super::doorbell_switch::IntercomDoorbellSwitch;

/// Composite Matter access control driver and ecosystem reporter.
pub struct MatterAccessControlDriver<'a> {
    door_lock: &'a IntercomDoorLock,
    doorbell_switch: &'a IntercomDoorbellSwitch,
}

impl<'a> MatterAccessControlDriver<'a> {
    /// Create a new Matter access control driver wrapping the door lock and doorbell switch clusters.
    pub fn new(
        door_lock: &'a IntercomDoorLock,
        doorbell_switch: &'a IntercomDoorbellSwitch,
    ) -> Self {
        Self {
            door_lock,
            doorbell_switch,
        }
    }
}

impl IntercomReporter for MatterAccessControlDriver<'_> {
    fn report_ring(&self, ringing: bool) {
        self.doorbell_switch.set_ringing(ringing);
    }

    fn report_lock_state(&self, state: LockState) {
        match state {
            LockState::Locked => self.door_lock.lock(),
            LockState::Unlocked => self.door_lock.unlock(),
        }
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        self.door_lock.set_unlock_duration(duration_sec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doorbell_switch::IntercomDoorbellSwitch;
    use crate::{door_lock::IntercomDoorLockConfig, ClusterReadinessBarrier};
    use intercom_core::events::{SystemStateSender, SystemStateWatch};
    use intercom_core::ports::{AccessControllerHandle, CommandChannel};

    #[test]
    fn reporter_latches_press_and_release_on_doorbell_switch() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        static BARRIER: ClusterReadinessBarrier =
            ClusterReadinessBarrier::new(SystemStateSender::new(&WATCH));
        static COMMANDS: CommandChannel = CommandChannel::new();
        let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
            access: AccessControllerHandle::new(COMMANDS.sender()),
            readiness: &BARRIER,
        });
        let switch = IntercomDoorbellSwitch::new(&BARRIER);
        let reporter = MatterAccessControlDriver::new(&door_lock, &switch);

        // Act
        reporter.report_ring(true);
        reporter.report_ring(false);
        let events = switch.take_pending_events();

        // Assert
        assert_eq!(events, 0b11);
    }
}
