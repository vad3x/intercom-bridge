//! Smart Intercom - Matter Cluster Readiness Barrier
//!
//! Coordinates Apple Home subscription priming across endpoints.
//! When both DoorLock (Endpoint 1) and Occupancy (Endpoint 2) are first read
//! by Apple Home, triggers system readiness.

use core::sync::atomic::{AtomicBool, Ordering};
use intercom_core::events::SystemStateSender;
use log::info;

/// Internal synchronization barrier coordinating Apple Home cluster readiness.
pub struct ClusterReadinessBarrier {
    door_lock_ready: AtomicBool,
    ring_ready: AtomicBool,
    state_sender: SystemStateSender<'static>,
}

impl ClusterReadinessBarrier {
    /// Create a new cluster readiness barrier.
    pub const fn new(state_sender: SystemStateSender<'static>) -> Self {
        Self {
            door_lock_ready: AtomicBool::new(false),
            ring_ready: AtomicBool::new(false),
            state_sender,
        }
    }

    /// Mark the DoorLock cluster as subscription-primed.
    pub fn mark_door_lock_ready(&self) {
        self.door_lock_ready.store(true, Ordering::Release);
        if self.is_ready() {
            info!("all clusters ready → publishing Ready state");
            self.state_sender.ready();
        }
    }

    /// Mark the Occupancy/Ring cluster as subscription-primed.
    pub fn mark_ring_ready(&self) {
        self.ring_ready.store(true, Ordering::Release);
        if self.is_ready() {
            info!("all clusters ready → publishing Ready state");
            self.state_sender.ready();
        }
    }

    /// Reset readiness state on connection loss or reconnect.
    pub fn reset(&self) {
        self.door_lock_ready.store(false, Ordering::Release);
        self.ring_ready.store(false, Ordering::Release);
    }

    /// Returns true if all clusters have been subscription-primed by Apple Home.
    pub fn is_ready(&self) -> bool {
        self.door_lock_ready.load(Ordering::Acquire) && self.ring_ready.load(Ordering::Acquire)
    }
}
