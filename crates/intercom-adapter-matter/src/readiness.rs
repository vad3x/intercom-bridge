//! Smart Intercom - Matter Cluster Readiness Barrier
//!
//! Coordinates Apple Home subscription priming across endpoints and implements
//! `EcosystemInfoDriver` to report Matter pairing and cluster subscription status.

use core::sync::atomic::{AtomicBool, Ordering};
use intercom_core::domain::{
    ClusterInfo, EcosystemStatus, EcosystemValue, EndpointInfo, FabricInfo, FabricLabel,
    FabricSubscription,
};
use intercom_core::events::SystemStateSender;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use log::info;
use portable_atomic::{AtomicPtr, AtomicU32};

use crate::config::{AUDIO_DOORBELL_ENDPOINT_ID, DOOR_LOCK_ENDPOINT_ID, MATTER_ECOSYSTEM};

const DOOR_LOCK_READY_BIT: u32 = 1 << 0;
const RING_READY_BIT: u32 = 1 << 1;
const REQUIRED_READY_BITS: u32 = DOOR_LOCK_READY_BIT | RING_READY_BIT;

/// Internal synchronization barrier coordinating Matter pairing and cluster readiness.
pub struct ClusterReadinessBarrier {
    inner: edf_matter::readiness::ClusterReadinessBarrier<'static>,
    door_lock_ready: AtomicBool,
    ring_ready: AtomicBool,
    door_lock_fabrics: AtomicU32,
    ring_fabrics: AtomicU32,
    matter: AtomicPtr<rs_matter::Matter<'static>>,
}

impl ClusterReadinessBarrier {
    /// Create a new cluster readiness barrier.
    pub const fn new(state_sender: SystemStateSender<'static>) -> Self {
        Self {
            inner: edf_matter::readiness::ClusterReadinessBarrier::new(
                REQUIRED_READY_BITS,
                state_sender,
            ),
            door_lock_ready: AtomicBool::new(false),
            ring_ready: AtomicBool::new(false),
            door_lock_fabrics: AtomicU32::new(0),
            ring_fabrics: AtomicU32::new(0),
            matter: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Attach the running Matter protocol stack instance.
    pub fn attach_matter(&self, matter: &'static rs_matter::Matter<'static>) {
        self.matter
            .store(matter as *const _ as *mut _, Ordering::Release);
    }

    /// Update whether the Matter node is commissioned with an active controller fabric.
    pub fn set_paired(&self, paired: bool) {
        self.inner.set_paired(paired);
    }

    /// Returns true if the Matter node is commissioned with an active controller fabric.
    pub fn is_paired(&self) -> bool {
        self.inner.is_paired()
    }

    /// Mark the DoorLock cluster as subscription-primed, optionally associating a fabric index.
    pub fn mark_door_lock_ready(&self, fab_idx: Option<u8>) {
        if let Some(idx) = fab_idx {
            if (1..=32).contains(&idx) {
                self.door_lock_fabrics
                    .fetch_or(1 << (idx - 1), Ordering::Release);
            }
        }
        let was_ready = self.is_ready();
        self.door_lock_ready.store(true, Ordering::Release);
        self.inner.mark_ready(DOOR_LOCK_READY_BIT);
        if !was_ready && self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
        }
    }

    /// Mark the Occupancy/Ring cluster as subscription-primed, optionally associating a fabric index.
    pub fn mark_ring_ready(&self, fab_idx: Option<u8>) {
        if let Some(idx) = fab_idx {
            if (1..=32).contains(&idx) {
                self.ring_fabrics
                    .fetch_or(1 << (idx - 1), Ordering::Release);
            }
        }
        let was_ready = self.is_ready();
        self.ring_ready.store(true, Ordering::Release);
        self.inner.mark_ready(RING_READY_BIT);
        if !was_ready && self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
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
        self.door_lock_fabrics.store(0, Ordering::Release);
        self.ring_fabrics.store(0, Ordering::Release);
        self.inner.reset();
    }

    /// Returns true if all clusters have been subscription-primed by the controller.
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }
}

impl EcosystemInfoDriver for ClusterReadinessBarrier {
    fn status(&self) -> Option<EcosystemStatus> {
        let ep1 = EndpointInfo::new(AUDIO_DOORBELL_ENDPOINT_ID, "Audio Doorbell", 0x0141)
            .with_cluster(ClusterInfo::new(0x003B, "Switch"))
            .with_cluster(ClusterInfo::new(0x0551, "Camera AV Stream"))
            .with_cluster(ClusterInfo::new(0x0553, "WebRTC Provider"))
            .with_cluster(ClusterInfo::new(0x0556, "Chime"));

        let ep2 = EndpointInfo::new(DOOR_LOCK_ENDPOINT_ID, "Door Lock", 0x000A)
            .with_cluster(ClusterInfo::new(0x000A, "Door Lock"));

        let mut status = EcosystemStatus::new(MATTER_ECOSYSTEM, self.is_paired())
            .with_endpoint(ep1)
            .with_endpoint(ep2)
            .with_property(
                "door_lock_subscribed",
                EcosystemValue::Bool(self.is_door_lock_ready()),
            )
            .with_property(
                "door_lock_endpoint",
                EcosystemValue::Int(DOOR_LOCK_ENDPOINT_ID as i64),
            )
            .with_property(
                "ring_sensor_subscribed",
                EcosystemValue::Bool(self.is_ring_ready()),
            )
            .with_property(
                "ring_sensor_endpoint",
                EcosystemValue::Int(AUDIO_DOORBELL_ENDPOINT_ID as i64),
            );

        let matter_ptr = self.matter.load(Ordering::Acquire);
        if !matter_ptr.is_null() {
            let matter = unsafe { &*matter_ptr };
            let ring_mask = self.ring_fabrics.load(Ordering::Acquire);
            let door_mask = self.door_lock_fabrics.load(Ordering::Acquire);

            matter.with_state(|state| {
                for fab in state.fabrics.iter() {
                    let fab_idx = fab.fab_idx().get();
                    let bit = if (1..=32).contains(&fab_idx) {
                        1 << (fab_idx - 1)
                    } else {
                        0
                    };

                    let mut fabric_info = FabricInfo::new(
                        fab_idx,
                        fab.vendor_id(),
                        fab.fabric_id(),
                        fab.node_id(),
                        FabricLabel::from_str(fab.label()),
                    );

                    if (ring_mask & bit) != 0 {
                        fabric_info = fabric_info.with_subscription(FabricSubscription::new(
                            AUDIO_DOORBELL_ENDPOINT_ID,
                            0x003B,
                        ));
                    }
                    if (door_mask & bit) != 0 {
                        fabric_info = fabric_info.with_subscription(FabricSubscription::new(
                            DOOR_LOCK_ENDPOINT_ID,
                            0x000A,
                        ));
                    }

                    status = status.with_fabric(fabric_info);
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

        // Act
        let barrier = ClusterReadinessBarrier::new(sender);

        // Assert
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
        barrier.mark_door_lock_ready(Some(1));

        // Assert
        assert!(barrier.is_door_lock_ready());
        assert!(!barrier.is_ring_ready());
        assert!(!barrier.is_ready());
        assert_eq!(sender.current_state(), None);

        // Act
        barrier.mark_ring_ready(Some(1));

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
        barrier.mark_door_lock_ready(Some(1));
        barrier.mark_ring_ready(Some(1));

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
        barrier.mark_door_lock_ready(Some(1));
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
        assert_eq!(status.iter_endpoints().count(), 2);
        let endpoints: heapless::Vec<_, 2> = status.iter_endpoints().cloned().collect();
        assert_eq!(endpoints[0].id, AUDIO_DOORBELL_ENDPOINT_ID);
        assert_eq!(endpoints[0].name, "Audio Doorbell");
        assert_eq!(endpoints[0].device_type, 0x0141);
        assert_eq!(endpoints[0].iter_clusters().count(), 4);
        assert_eq!(endpoints[1].id, DOOR_LOCK_ENDPOINT_ID);
        assert_eq!(endpoints[1].name, "Door Lock");
        assert_eq!(endpoints[1].device_type, 0x000A);
        assert_eq!(endpoints[1].iter_clusters().count(), 1);
    }

    #[test]
    fn readiness_barrier_deduplicates_repeated_marks() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        let sender = SystemStateSender::new(&WATCH);
        let barrier = ClusterReadinessBarrier::new(sender);
        barrier.mark_door_lock_ready(Some(1));
        barrier.mark_ring_ready(Some(1));
        assert_eq!(sender.current_state(), Some(SystemState::Operational));

        // Act - repeated calls when already ready
        barrier.mark_door_lock_ready(Some(1));
        barrier.mark_ring_ready(Some(1));

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
        barrier.mark_door_lock_ready(Some(1));
        barrier.mark_ring_ready(Some(1));
        assert_eq!(sender.current_state(), Some(SystemState::Operational));

        // Act - reset clears readiness
        barrier.reset();
        sender.ecosystem_connecting();
        assert_eq!(
            sender.current_state(),
            Some(SystemState::EcosystemConnecting)
        );
        assert!(!barrier.is_ready());

        // Marking door lock only does not transition yet
        barrier.mark_door_lock_ready(Some(1));
        assert_eq!(
            sender.current_state(),
            Some(SystemState::EcosystemConnecting)
        );

        // Marking ring completes readiness and re-arms Operational
        barrier.mark_ring_ready(Some(1));

        // Assert
        assert!(barrier.is_ready());
        assert_eq!(sender.current_state(), Some(SystemState::Operational));
    }
}
