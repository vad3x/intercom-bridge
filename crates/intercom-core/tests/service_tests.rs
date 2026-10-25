//! Unit tests for Intercom Core application services and ports.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::app::App;
use intercom_core::domain::access::AccessStateMachine;
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle, CommandChannel};
use intercom_core::ports::reporter::{IntercomReporter, LockState};
use intercom_core::ports::{
    DoorDriver, ResetScope, ResetTriggerDriver, RingDriver, SystemResetDriver,
};
use intercom_core::service::access::AccessController;
use intercom_core::service::ring::RingWatcher;
use portable_atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

// ============================================================================
// Mock Drivers
// ============================================================================

struct MockDoorDriver {
    trigger_count: AtomicU32,
    duration_ms: AtomicU32,
}

impl MockDoorDriver {
    fn new() -> Self {
        Self {
            trigger_count: AtomicU32::new(0),
            duration_ms: AtomicU32::new(5000),
        }
    }
}

impl DoorDriver for &MockDoorDriver {
    async fn trigger_unlock(&mut self) {
        self.trigger_count.fetch_add(1, Ordering::SeqCst);
    }

    fn set_unlock_duration_ms(&mut self, duration_ms: u32) {
        self.duration_ms.store(duration_ms, Ordering::SeqCst);
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
    fn erase_credentials(&self, _scope: ResetScope) {
        self.erased.store(true, Ordering::SeqCst);
    }

    fn reboot(&self) -> ! {
        self.rebooted.store(true, Ordering::SeqCst);
        panic!("simulated reboot");
    }
}

struct MockResetTriggerDriver {
    scope: ResetScope,
}

impl MockResetTriggerDriver {
    fn new(scope: ResetScope) -> Self {
        Self { scope }
    }
}

impl ResetTriggerDriver for &MockResetTriggerDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        self.scope
    }
}

struct MockReporter {
    ring: AtomicBool,
    lock_state: AtomicU8,
    duration_sec: AtomicU32,
}

impl MockReporter {
    fn new() -> Self {
        Self {
            ring: AtomicBool::new(false),
            lock_state: AtomicU8::new(0),
            duration_sec: AtomicU32::new(5),
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

    fn report_unlock_duration(&self, duration_sec: u32) {
        self.duration_sec.store(duration_sec, Ordering::SeqCst);
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
    let reset_trigger = MockResetTriggerDriver::new(ResetScope::Factory);
    let reset = MockResetDriver::new();

    // Act
    let _app = App::new(
        &ring,
        &door,
        &reporter,
        &reset_trigger,
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
    sender.operational();
    let state = futures::executor::block_on(receiver.changed());

    // Assert
    assert_eq!(state, SystemState::Operational);
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
    sender.wifi_connecting();
    let (s1, s2) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert - both independent receivers observed the state transition
    assert_eq!(s1, SystemState::WifiConnecting);
    assert_eq!(s2, SystemState::WifiConnecting);

    // Act - second transition
    sender.operational();
    let (s3, s4) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert
    assert_eq!(s3, SystemState::Operational);
    assert_eq!(s4, SystemState::Operational);
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
    sender.operational();
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
    sender.booting();
    assert_eq!(sender.current_state(), Some(SystemState::Booting));
    sender.starting();
    assert_eq!(sender.current_state(), Some(SystemState::Starting));
    sender.commissioning();
    assert_eq!(sender.current_state(), Some(SystemState::Commissioning));
    sender.ecosystem_connecting();

    // Assert
    assert_eq!(sender.current_state(), Some(SystemState::EcosystemConnecting));
}

#[test]
fn test_reset_service_erases_ecosystem_scope_and_reboots() {
    // Arrange
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;
    use intercom_core::events::{SystemState, SystemStateWatch};
    use core::sync::atomic::{AtomicBool, Ordering};

    struct MockTrigger { scope: ResetScope }
    impl ResetTriggerDriver for MockTrigger {
        async fn wait_for_reset(&mut self) -> ResetScope { self.scope }
    }

    struct MockReset {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
        reboot_called: AtomicBool,
    }
    impl SystemResetDriver for MockReset {
        fn erase_credentials(&self, scope: ResetScope) { self.erased_scope.set(Some(scope)); }
        fn reboot(&self) -> ! {
            self.reboot_called.store(true, Ordering::SeqCst);
            panic!("Mock reboot");
        }
    }

    let trigger = MockTrigger { scope: ResetScope::Ecosystem };
    let reset = MockReset {
        erased_scope: core::cell::Cell::new(None),
        reboot_called: AtomicBool::new(false),
    };
    let mut service = ResetService::new(trigger, reset);
    let state_watch = SystemStateWatch::new();
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);

    // Act
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        futures::executor::block_on(service.run(state_sender));
    }));

    // Assert
    assert_eq!(service.reset.erased_scope.get(), Some(ResetScope::Ecosystem));
    assert_eq!(state_watch.try_get(), Some(SystemState::EcosystemReset));
}

#[test]
fn test_reset_service_erases_factory_scope_and_reboots() {
    // Arrange
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;
    use intercom_core::events::{SystemState, SystemStateWatch};

    struct MockTrigger { scope: ResetScope }
    impl ResetTriggerDriver for MockTrigger {
        async fn wait_for_reset(&mut self) -> ResetScope { self.scope }
    }

    struct MockReset {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
    }
    impl SystemResetDriver for MockReset {
        fn erase_credentials(&self, scope: ResetScope) { self.erased_scope.set(Some(scope)); }
        fn reboot(&self) -> ! { panic!("Mock reboot"); }
    }

    let trigger = MockTrigger { scope: ResetScope::Factory };
    let reset = MockReset { erased_scope: core::cell::Cell::new(None) };
    let mut service = ResetService::new(trigger, reset);
    let state_watch = SystemStateWatch::new();
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);

    // Act
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        futures::executor::block_on(service.run(state_sender));
    }));

    // Assert
    assert_eq!(service.reset.erased_scope.get(), Some(ResetScope::Factory));
    assert_eq!(state_watch.try_get(), Some(SystemState::FactoryReset));
}

#[test]
fn access_state_machine_default_duration_is_5_seconds() {
    let sm = AccessStateMachine::new();
    assert_eq!(sm.unlock_duration_sec(), 5);
}

#[test]
fn access_state_machine_set_duration_updates_deadline() {
    let mut sm = AccessStateMachine::new();
    let effects = sm.handle_command(AccessCommand::SetUnlockDuration(10), 1000);
    assert_eq!(sm.unlock_duration_sec(), 10);
    assert_eq!(effects.report_duration, Some(10));

    let unlock_effects = sm.handle_command(AccessCommand::Unlock, 2000);
    assert!(unlock_effects.trigger_unlock);
    assert_eq!(sm.next_deadline(), Some(2000 + 10_000));
}

#[test]
fn access_state_machine_clamps_duration_to_bounds() {
    let mut sm = AccessStateMachine::new();
    sm.handle_command(AccessCommand::SetUnlockDuration(0), 1000);
    assert_eq!(sm.unlock_duration_sec(), 1);

    sm.handle_command(AccessCommand::SetUnlockDuration(45), 1000);
    assert_eq!(sm.unlock_duration_sec(), 30);
}

#[test]
fn access_controller_processes_set_unlock_duration_command() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let reporter = MockReporter::new();
    let mut controller = AccessController::new(&door, &reporter);

    // Act
    assert!(handle.set_unlock_duration(8).is_ok());
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(door.duration_ms.load(Ordering::SeqCst), 8000);
    assert_eq!(reporter.duration_sec.load(Ordering::SeqCst), 8);
}

#[test]
fn access_controller_dispatches_duration_to_composite_reporters() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let door = MockDoorDriver::new();
    let rep1 = MockReporter::new();
    let rep2 = MockReporter::new();
    let composite = (&rep1, &rep2);
    let mut controller = AccessController::new(&door, composite);

    // Act
    handle.send(AccessCommand::SetUnlockDuration(12));
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(door.duration_ms.load(Ordering::SeqCst), 12_000);
    assert_eq!(rep1.duration_sec.load(Ordering::SeqCst), 12);
    assert_eq!(rep2.duration_sec.load(Ordering::SeqCst), 12);
}



