//! Smart Intercom - Matter Access Control Driver
//!
//! Composite driver wrapping `IntercomDoorLock` and `IntercomOccupancy`,
//! implementing the `AccessControlDriver` trait defined by the actor layer.
//!
//! C++ equivalent: `MatterAccessControlDriver`

use intercom_core::ports::AccessControlDriver;

use super::door_lock::IntercomDoorLock;
use super::occupancy::IntercomOccupancy;

/// Composite Matter access control driver.
///
/// Wraps references to both Matter cluster handlers and delegates
/// `AccessControlDriver` trait methods to their respective methods.
pub struct MatterAccessControlDriver<'a, OnUnlock: Fn(), OnDoorReady: Fn(), OnOccReady: Fn()> {
    door_lock: &'a IntercomDoorLock<OnUnlock, OnDoorReady>,
    occupancy: &'a IntercomOccupancy<OnOccReady>,
}

impl<'a, OnUnlock: Fn(), OnDoorReady: Fn(), OnOccReady: Fn()>
    MatterAccessControlDriver<'a, OnUnlock, OnDoorReady, OnOccReady>
{
    pub fn new(
        door_lock: &'a IntercomDoorLock<OnUnlock, OnDoorReady>,
        occupancy: &'a IntercomOccupancy<OnOccReady>,
    ) -> Self {
        Self {
            door_lock,
            occupancy,
        }
    }
}

impl<OnUnlock: Fn(), OnDoorReady: Fn(), OnOccReady: Fn()> AccessControlDriver
    for MatterAccessControlDriver<'_, OnUnlock, OnDoorReady, OnOccReady>
{
    fn set_occupancy(&self, occupied: bool) {
        self.occupancy.set_occupancy(occupied);
    }

    fn lock(&self) {
        self.door_lock.lock();
    }
}
