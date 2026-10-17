//! Smart Intercom - Matter Cluster Readiness Barrier
//!
//! Coordinates Apple Home subscription priming across endpoints and implements
//! `EcosystemInfoDriver` to report Matter pairing and cluster subscription status.

use core::sync::atomic::{AtomicBool, Ordering};
use intercom_core::domain::{EcosystemStatus, EcosystemValue, FabricInfo, FabricLabel};
use intercom_core::events::SystemStateSender;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use log::info;
use portable_atomic::AtomicPtr;

use crate::config::MATTER_ECOSYSTEM;

/// Internal synchronization barrier coordinating Matter pairing and cluster readiness.
pub struct ClusterReadinessBarrier {
    paired: AtomicBool,
    door_lock_ready: AtomicBool,
    ring_ready: AtomicBool,
    state_sender: SystemStateSender<'static>,
    matter: AtomicPtr<rs_matter::Matter<'static>>,
}

impl ClusterReadinessBarrier {
    /// Create a new cluster readiness barrier.
    pub const fn new(state_sender: SystemStateSender<'static>) -> Self {
        Self {
            paired: AtomicBool::new(false),
            door_lock_ready: AtomicBool::new(false),
            ring_ready: AtomicBool::new(false),
            state_sender,
            matter: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Attach the running Matter protocol stack instance.
    pub fn attach_matter(&self, matter: &'static rs_matter::Matter<'static>) {
        self.matter.store(matter as *const _ as *mut _, Ordering::Release);
    }

    /// Update whether the Matter node is commissioned with an active controller fabric.
    pub fn set_paired(&self, paired: bool) {
        self.paired.store(paired, Ordering::Release);
    }

    /// Returns true if the Matter node is commissioned with an active controller fabric.
    pub fn is_paired(&self) -> bool {
        self.paired.load(Ordering::Acquire)
    }

    /// Mark the DoorLock cluster as subscription-primed.
    pub fn mark_door_lock_ready(&self) {
        let was_ready = self.is_ready();
        self.door_lock_ready.store(true, Ordering::Release);
        if !was_ready && self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
            self.state_sender.operational();
        }
    }

    /// Mark the Occupancy/Ring cluster as subscription-primed.
    pub fn mark_ring_ready(&self) {
        let was_ready = self.is_ready();
        self.ring_ready.store(true, Ordering::Release);
        if !was_ready && self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
            self.state_sender.operational();
        }
    }

    /// Returns true if the DoorLock cluster has been subscription-primed.
    pub fn is_door_lock_ready(&self) -> bool {
        self.door_lock_ready.load(Ordering::Acquire)
    }

    /// Returns true if the Occupancy/Ring cluster has been subscription-primed.
    pub fn is_ring_ready(&self) -> bool {
        self.ring_ready.load(Ordering::Acquire)
    }

    /// Reset cluster subscription readiness state on connection loss or reconnect.
    pub fn reset(&self) {
        self.door_lock_ready.store(false, Ordering::Release);
        self.ring_ready.store(false, Ordering::Release);
    }

    /// Returns true if all clusters have been subscription-primed by the controller.
    pub fn is_ready(&self) -> bool {
        self.is_door_lock_ready() && self.is_ring_ready()
    }
}

impl EcosystemInfoDriver for ClusterReadinessBarrier {
    fn status(&self) -> Option<EcosystemStatus> {
        let mut status = EcosystemStatus::new(MATTER_ECOSYSTEM, self.is_paired())
            .with_property(
                "door_lock_subscribed",
                EcosystemValue::Bool(self.is_door_lock_ready()),
            )
            .with_property(
                "ring_sensor_subscribed",
                EcosystemValue::Bool(self.is_ring_ready()),
            );

        let matter_ptr = self.matter.load(Ordering::Acquire);
        if !matter_ptr.is_null() {
            let matter = unsafe { &*matter_ptr };
            matter.with_state(|state| {
                for fab in state.fabrics.iter() {
                    status = status.with_fabric(FabricInfo {
                        fab_idx: fab.fab_idx().get(),
                        vendor_id: fab.vendor_id(),
                        fabric_id: fab.fabric_id(),
                        node_id: fab.node_id(),
                        label: FabricLabel::from_str(fab.label()),
                    });
                }
            });
        }

        Some(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intercom_core::events::{SystemState, SystemStateWatch};

    #[test]
    fn readiness_barrier_initial_state() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);

        // Act & Assert
        assert!(!barrier.is_paired());
        assert!(!barrier.is_door_lock_ready());
        assert!(!barrier.is_ring_ready());
        assert!(!barrier.is_ready());
        assert_eq!(sender.current_state(), None);
    }

    #[test]
    fn readiness_barrier_marks_operational_when_both_clusters_ready() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);

        // Act
        barrier.mark_door_lock_ready();

        // Assert
        assert!(barrier.is_door_lock_ready());
        assert!(!barrier.is_ring_ready());
        assert!(!barrier.is_ready());
        assert_eq!(sender.current_state(), None);

        // Act
        barrier.mark_ring_ready();

        // Assert
        assert!(barrier.is_door_lock_ready());
        assert!(barrier.is_ring_ready());
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }

    #[test]
    fn readiness_barrier_reset_clears_clusters_but_preserves_paired() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);
        barrier.set_paired(true);
        barrier.mark_door_lock_ready();
        barrier.mark_ring_ready();

        // Act
        barrier.reset();

        // Assert
        assert!(barrier.is_paired());
        assert!(!barrier.is_door_lock_ready());
        assert!(!barrier.is_ring_ready());
        assert!(!barrier.is_ready());
    }

    #[test]
    fn readiness_barrier_ecosystem_status_reflects_live_state() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);

        // Act
        barrier.set_paired(true);
        barrier.mark_door_lock_ready();
        let status = barrier.status().unwrap();

        // Assert
        assert_eq!(status.ecosystem_type, MATTER_ECOSYSTEM);
        assert!(status.paired);
        assert_eq!(
            status.property("door_lock_subscribed"),
            Some(&EcosystemValue::Bool(true))
        );
        assert_eq!(
            status.property("ring_sensor_subscribed"),
            Some(&EcosystemValue::Bool(false))
        );
    }

    #[test]
    fn readiness_barrier_deduplicates_repeated_marks() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);
        barrier.mark_door_lock_ready();
        barrier.mark_ring_ready();
        assert_eq!(sender.current_state(), Some(SystemState::Operational));

        // Act - repeated calls when already ready
        barrier.mark_door_lock_ready();
        barrier.mark_ring_ready();

        // Assert
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }

    #[test]
    fn readiness_barrier_rearms_after_reset() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);
        barrier.mark_door_lock_ready();
        barrier.mark_ring_ready();
        assert_eq!(sender.current_state(), Some(SystemState::Operational));

        // Act - reset clears readiness
        barrier.reset();
        sender.ecosystem_connecting();
        assert_eq!(sender.current_state(), Some(SystemState::EcosystemConnecting));
        assert!(!barrier.is_ready());

        // Marking door lock only does not transition yet
        barrier.mark_door_lock_ready();
        assert_eq!(sender.current_state(), Some(SystemState::EcosystemConnecting));

        // Marking ring completes readiness and re-arms Operational
        barrier.mark_ring_ready();

        // Assert
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }
}
