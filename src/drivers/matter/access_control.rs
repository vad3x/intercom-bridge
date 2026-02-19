//! Smart Intercom - Matter Access Control Driver
//!
//! Composite driver wrapping `IntercomDoorLock` and `IntercomOccupancy`,
//! implementing the `AccessControlDriver` trait defined by the actor layer.
//!
//! C++ equivalent: `MatterAccessControlDriver`

use crate::actors::access_control::AccessControlDriver;

use super::door_lock::IntercomDoorLock;
use super::occupancy::IntercomOccupancy;

/// Composite Matter access control driver.
///
/// Wraps references to both Matter cluster handlers and delegates
/// `AccessControlDriver` trait methods to their respective methods.
pub struct MatterAccessControlDriver<'a> {
    door_lock: &'a IntercomDoorLock<'a>,
    occupancy: &'a IntercomOccupancy,
}

impl<'a> MatterAccessControlDriver<'a> {
    pub fn new(door_lock: &'a IntercomDoorLock<'a>, occupancy: &'a IntercomOccupancy) -> Self {
        Self {
            door_lock,
            occupancy,
        }
    }
}

impl AccessControlDriver for MatterAccessControlDriver<'_> {
    fn set_occupancy(&self, occupied: bool) {
        self.occupancy.set_occupancy(occupied);
    }

    fn lock(&self) {
        self.door_lock.lock();
    }
}
