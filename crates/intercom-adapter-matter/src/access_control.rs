//! Smart Intercom - Matter Access Control Driver & Ecosystem Reporter
//!
//! Composite driver wrapping `IntercomDoorLock` and `IntercomOccupancy`,
//! implementing the `IntercomReporter` domain port.

use intercom_core::ports::{IntercomReporter, LockState};

use super::door_lock::IntercomDoorLock;
use super::occupancy::IntercomOccupancy;

/// Composite Matter access control driver and ecosystem reporter.
pub struct MatterAccessControlDriver<'a> {
    door_lock: &'a IntercomDoorLock,
    occupancy: &'a IntercomOccupancy,
}

impl<'a> MatterAccessControlDriver<'a> {
    /// Create a new Matter access control driver wrapping the door lock and occupancy clusters.
    pub fn new(door_lock: &'a IntercomDoorLock, occupancy: &'a IntercomOccupancy) -> Self {
        Self {
            door_lock,
            occupancy,
        }
    }
}

impl IntercomReporter for MatterAccessControlDriver<'_> {
    fn report_ring(&self, ringing: bool) {
        self.occupancy.set_occupancy(ringing);
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
