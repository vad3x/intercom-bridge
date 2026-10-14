//! Unit tests for Intercom Core actors and ports.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::actors::access_control::AccessControlActor;
use intercom_core::actors::door::DoorActor;
use intercom_core::actors::ring::RingActor;
use intercom_core::events::{Event, EventBus};
use intercom_core::ports::{AccessControlDriver, DoorDriver, RingDriver};
use portable_atomic::{AtomicBool, AtomicU32, Ordering};

// ============================================================================
// Mock Drivers
// ============================================================================

struct MockDoorDriver {
    trigger_count: AtomicU32,
}

impl MockDoorDriver {
    fn new() -> Self {
        Self {
            trigger_count: AtomicU32::new(0),
        }
    }
}

impl DoorDriver for &MockDoorDriver {
    async fn trigger_unlock(&mut self) {
        self.trigger_count.fetch_add(1, Ordering::SeqCst);
    }
}

struct MockRingDriver {
    should_ring: AtomicBool,
}

impl MockRingDriver {
    fn new(should_ring: bool) -> Self {
        Self {
            should_ring: AtomicBool::new(should_ring),
        }
    }
}

impl RingDriver for &MockRingDriver {
    async fn wait_for_ring(&mut self) {
        while !self.should_ring.load(Ordering::SeqCst) {
            embassy_futures::yield_now().await;
        }
        self.should_ring.store(false, Ordering::SeqCst);
    }
}

struct MockAccessControlDriver {
    occupancy: AtomicBool,
    locked: AtomicBool,
}

impl MockAccessControlDriver {
    fn new() -> Self {
        Self {
            occupancy: AtomicBool::new(false),
            locked: AtomicBool::new(false),
        }
    }
}

impl AccessControlDriver for &MockAccessControlDriver {
    fn set_occupancy(&self, occupied: bool) {
        self.occupancy.store(occupied, Ordering::SeqCst);
    }

    fn lock(&self) {
        self.locked.store(true, Ordering::SeqCst);
    }
}

// ============================================================================
// Tests (Arrange -> Act -> Assert)
// ============================================================================

#[test]
fn door_actor_triggers_driver_on_unlock_event() {
    // Arrange
    let bus = EventBus::new();
    let driver = MockDoorDriver::new();
    let mut actor = DoorActor::new(&driver);
    let mut subscriber = bus.subscriber().unwrap();
    bus.immediate_publisher()
        .publish_immediate(Event::UnlockDispatched);

    // Act
    futures::executor::block_on(actor.next(&mut subscriber));

    // Assert
    assert_eq!(driver.trigger_count.load(Ordering::SeqCst), 1);
}

#[test]
fn ring_actor_publishes_ringing_event() {
    // Arrange
    let bus = EventBus::new();
    let driver = MockRingDriver::new(true);
    let mut actor = RingActor::new(&driver);
    let publisher = bus.publisher().unwrap();
    let mut subscriber = bus.subscriber().unwrap();

    // Act
    futures::executor::block_on(actor.next(&publisher));

    // Assert
    let received = subscriber.try_next_message_pure();
    assert!(matches!(received, Some(Event::Ringing)));
}

#[test]
fn access_control_actor_updates_occupancy_on_ring() {
    // Arrange
    let bus = EventBus::new();
    let driver = MockAccessControlDriver::new();
    let mut actor = AccessControlActor::new(&driver);
    let mut subscriber = bus.subscriber().unwrap();
    bus.immediate_publisher()
        .publish_immediate(Event::Ringing);

    // Act
    futures::executor::block_on(actor.next(&mut subscriber));

    // Assert
    assert!(driver.occupancy.load(Ordering::SeqCst));
}
