//! Unit tests for Intercom Core application services and ports.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::app::App;
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle, CommandChannel};
use intercom_core::ports::reporter::{IntercomReporter, LockState};
use intercom_core::ports::{
    DoorDriver, FactoryResetDriver, RingDriver, SystemResetDriver,
};
use intercom_core::service::access::AccessController;
use intercom_core::service::ring::RingWatcher;
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
    let mut controller = AccessController::new(&door, &reporter);

    // Act
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 1);
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // 2 = Unlocked
}

#[test]
fn access_controller_processes_ring_command_from_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let reporter = MockReporter::new();
    let mut controller = AccessController::new(&door, &reporter);

    // Act
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert!(reporter.ring.load(Ordering::SeqCst));
}

#[test]
fn ring_watcher_forwards_to_access_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let ring_driver = MockRingDriver::new(true);
    let mut watcher = RingWatcher::new(&ring_driver);

    // Act
    futures::executor::block_on(async {
        use embassy_futures::select::{select, Either};
        match select(watcher.run(handle), channel.receiver().receive()).await {
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
    let reporter = MockReporter::new();
    let factory_reset = MockFactoryResetDriver::new(false);
    let reset = MockResetDriver::new();

    // Act
    let _app = App::new(
        &ring,
        &door,
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
    let mut controller = AccessController::new(&door, &reporter);

    // Act - trigger ring first
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - ring active
    assert!(reporter.ring.load(Ordering::SeqCst));
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 0);

    // Act - dispatch unlock while ring is active
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - door unlocked, ring remains active
    assert_eq!(door.trigger_count.load(Ordering::SeqCst), 1);
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // Unlocked
    assert!(reporter.ring.load(Ordering::SeqCst));
}

#[test]
fn access_controller_dispatches_to_composite_reporters() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let rep1 = MockReporter::new();
    let rep2 = MockReporter::new();
    let composite = (&rep1, &rep2);
    let mut controller = AccessController::new(&door, composite);

    // Act - trigger ring
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - both reporters in the composite received ring event
    assert!(rep1.ring.load(Ordering::SeqCst));
    assert!(rep2.ring.load(Ordering::SeqCst));

    // Act - request unlock
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - both reporters in the composite received lock state update
    assert_eq!(rep1.lock_state.load(Ordering::SeqCst), 2);
    assert_eq!(rep2.lock_state.load(Ordering::SeqCst), 2);
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

#[test]
fn system_state_watch_notifies_updating_transition() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut receiver = watch.receiver().unwrap();

    // Act
    sender.send(SystemState::Updating);

    // Assert
    assert_eq!(receiver.try_get(), Some(SystemState::Updating));

    // Act - via helper method
    sender.ready();
    sender.updating();

    // Assert
    assert_eq!(receiver.try_get(), Some(SystemState::Updating));
}

#[test]
fn system_state_sender_queries_current_state() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);

    // Act
    sender.connected();

    // Assert
    assert_eq!(sender.current_state(), Some(SystemState::Connected));
}
