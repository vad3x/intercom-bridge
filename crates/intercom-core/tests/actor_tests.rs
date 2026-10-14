//! Unit tests for Intercom Core actors and ports.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::actors::access_controller::AccessController;
use intercom_core::actors::indicator::IndicatorActor;
use intercom_core::actors::ring::RingActor;
use intercom_core::app::App;
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle, CommandChannel};
use intercom_core::ports::reporter::{IntercomReporter, LockState};
use intercom_core::ports::{
    DoorDriver, FactoryResetDriver, IndicatorDriver, RingDriver, SystemResetDriver,
};
use portable_atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

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

struct MockIndicatorDriver {
    solid_count: AtomicU32,
    blink_calls: AtomicU32,
    burst_calls: AtomicU32,
    ring_blinks: AtomicU32,
    unlock_blinks: AtomicU32,
}

impl MockIndicatorDriver {
    fn new() -> Self {
        Self {
            solid_count: AtomicU32::new(0),
            blink_calls: AtomicU32::new(0),
            burst_calls: AtomicU32::new(0),
            ring_blinks: AtomicU32::new(0),
            unlock_blinks: AtomicU32::new(0),
        }
    }
}

impl IndicatorDriver for &MockIndicatorDriver {
    fn solid(&self) {
        self.solid_count.fetch_add(1, Ordering::SeqCst);
    }

    fn blink(&self, _on_ms: u64, _off_ms: u64, _count: Option<u8>) {
        self.blink_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn burst(&self, _on_ms: u64, _off_ms: u64, _burst_count: u8, _pause_ms: u64) {
        self.burst_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn indicate_ring(&self) {
        self.ring_blinks.fetch_add(1, Ordering::SeqCst);
    }

    fn indicate_unlock(&self) {
        self.unlock_blinks.fetch_add(1, Ordering::SeqCst);
    }

    async fn run(&self) {}
}

struct MockResetDriver {
    erased: AtomicBool,
    rebooted: AtomicBool,
}

impl MockResetDriver {
    fn new() -> Self {
        Self {
            erased: AtomicBool::new(false),
            rebooted: AtomicBool::new(false),
        }
    }
}

impl SystemResetDriver for &MockResetDriver {
    fn erase_credentials(&self) {
        self.erased.store(true, Ordering::SeqCst);
    }

    fn reboot(&self) -> ! {
        self.rebooted.store(true, Ordering::SeqCst);
        panic!("simulated reboot");
    }
}

struct MockFactoryResetDriver {
    triggered: AtomicBool,
}

impl MockFactoryResetDriver {
    fn new(triggered: bool) -> Self {
        Self {
            triggered: AtomicBool::new(triggered),
        }
    }
}

impl FactoryResetDriver for &MockFactoryResetDriver {
    async fn wait_for_factory_reset(&mut self) {
        while !self.triggered.load(Ordering::SeqCst) {
            embassy_futures::yield_now().await;
        }
        self.triggered.store(false, Ordering::SeqCst);
    }
}

struct MockReporter {
    ring: AtomicBool,
    lock_state: AtomicU8,
}

impl MockReporter {
    fn new() -> Self {
        Self {
            ring: AtomicBool::new(false),
            lock_state: AtomicU8::new(0),
        }
    }
}

impl IntercomReporter for &MockReporter {
    fn report_ring(&self, ringing: bool) {
        self.ring.store(ringing, Ordering::SeqCst);
    }

    fn report_lock_state(&self, state: LockState) {
        let val = match state {
            LockState::Locked => 1,
            LockState::Unlocked => 2,
        };
        self.lock_state.store(val, Ordering::SeqCst);
    }
}

// ============================================================================
// Tests (Arrange -> Act -> Assert)
// ============================================================================

#[test]
fn access_controller_processes_unlock_command_from_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let reporter = MockReporter::new();
    let indicator = MockIndicatorDriver::new();
    let mut controller = AccessController::new(&door, &reporter, &indicator);

    // Act
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 1);
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // 2 = Unlocked
    assert_eq!(indicator.unlock_blinks.load(Ordering::SeqCst), 1);
}

#[test]
fn access_controller_processes_ring_command_from_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let reporter = MockReporter::new();
    let indicator = MockIndicatorDriver::new();
    let mut controller = AccessController::new(&door, &reporter, &indicator);

    // Act
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert!(reporter.ring.load(Ordering::SeqCst));
    assert_eq!(indicator.ring_blinks.load(Ordering::SeqCst), 1);
}

#[test]
fn ring_actor_forwards_to_access_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let ring_driver = MockRingDriver::new(true);
    let mut actor = RingActor::new(&ring_driver);

    // Act
    futures::executor::block_on(async {
        use embassy_futures::select::{select, Either};
        match select(actor.run(handle), channel.receiver().receive()).await {
            Either::First(_) => unreachable!(),
            Either::Second(cmd) => {
                // Assert
                assert_eq!(cmd, AccessCommand::Ring);
            }
        }
    });
}

#[test]
fn app_compilation_and_mock_instantiation() {
    // Arrange
    let ring = MockRingDriver::new(false);
    let door = MockDoorDriver::new();
    let indicator = MockIndicatorDriver::new();
    let reporter = MockReporter::new();
    let factory_reset = MockFactoryResetDriver::new(false);
    let reset = MockResetDriver::new();

    // Act
    let _app = App::new(
        &ring,
        &door,
        &indicator,
        &reporter,
        &factory_reset,
        &reset,
    );

    // Assert - Successfully constructed App generic over references to mock drivers
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 0);
}

#[test]
fn system_state_watch_sender_and_receiver() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut receiver = watch.receiver().unwrap();

    // Act
    sender.ready();
    let state = futures::executor::block_on(receiver.changed());

    // Assert
    assert_eq!(state, SystemState::Ready);
}

#[test]
fn access_controller_handles_concurrent_ring_and_unlock() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let reporter = MockReporter::new();
    let indicator = MockIndicatorDriver::new();
    let mut controller = AccessController::new(&door, &reporter, &indicator);

    // Act - trigger ring first
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - ring active, chime indicated
    assert!(reporter.ring.load(Ordering::SeqCst));
    assert_eq!(indicator.ring_blinks.load(Ordering::SeqCst), 1);
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 0);

    // Act - dispatch unlock while ring is active
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - door unlocked, unlock indicated, ring remains active
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 1);
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // Unlocked
    assert_eq!(indicator.unlock_blinks.load(Ordering::SeqCst), 1);
    assert!(reporter.ring.load(Ordering::SeqCst));
}

#[test]
fn indicator_actor_routes_lifecycle_states_to_driver() {
    // Arrange
    let watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&watch);
    let state_receiver = watch.receiver().unwrap();
    let indicator = MockIndicatorDriver::new();
    let actor = IndicatorActor::new(&indicator);

    // Act - drive lifecycle states and verify driver commands
    futures::executor::block_on(async {
        use embassy_futures::select::{select, Either};
        use embassy_futures::yield_now;

        let runner = actor.run(state_receiver);

        let driver_checks = async {
            // Initializing / Connecting should blink
            state_sender.connecting();
            yield_now().await;
            assert_eq!(indicator.blink_calls.load(Ordering::SeqCst), 1);

            // Ready state should go solid
            state_sender.ready();
            yield_now().await;
            assert_eq!(indicator.solid_count.load(Ordering::SeqCst), 1);

            // Error state should burst
            state_sender.error();
            yield_now().await;
            assert_eq!(indicator.burst_calls.load(Ordering::SeqCst), 1);
        };

        match select(runner, driver_checks).await {
            Either::First(_) => unreachable!(),
            Either::Second(()) => {}
        }
    });

    // Assert
    assert_eq!(indicator.blink_calls.load(Ordering::SeqCst), 1);
    assert_eq!(indicator.solid_count.load(Ordering::SeqCst), 1);
    assert_eq!(indicator.burst_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn system_state_watch_broadcasts_to_multiple_receivers() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut rx1 = watch.receiver().unwrap();
    let mut rx2 = watch.receiver().unwrap();

    // Act
    sender.connecting();
    let (s1, s2) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert - both independent receivers observed the state transition
    assert_eq!(s1, SystemState::Connecting);
    assert_eq!(s2, SystemState::Connecting);

    // Act - second transition
    sender.ready();
    let (s3, s4) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert
    assert_eq!(s3, SystemState::Ready);
    assert_eq!(s4, SystemState::Ready);
}
