//! Unit tests for Intercom Core application services and ports.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;
use intercom_core::app::App;
use intercom_core::domain::intercom::{
    AudioDirection, BusLineState, CallState, IntercomStateMachine, SessionOwner,
};
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::inbound::{
    AccessCommand, AccessControllerHandle, CommandChannel, IntercomCommand,
    IntercomControllerHandle, CMD_QUEUE_CAP,
};
use intercom_core::ports::reporter::{IntercomReporter, LockState};
use intercom_core::ports::{
    AudioControlDriver, BusSwitchDriver, NoopAudioControlDriver, ResetScope, ResetTriggerDriver,
    RingDriver, SystemResetDriver,
};
use intercom_core::service::intercom::IntercomController;
use intercom_core::service::ring::RingWatcher;
use portable_atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

// ============================================================================
// Tests (Arrange -> Act -> Assert)
// ============================================================================

#[test]
fn intercom_controller_processes_unlock_command_from_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, &reporter);

    // Act
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(
        *trace.borrow(),
        alloc::vec![TraceEvent::Bus(BusLineState::DoorUnlock)]
    );
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // 2 = Unlocked
}

#[test]
fn intercom_controller_processes_ring_command_from_handle() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, &reporter);

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
    let ring_driver = RingDriverMock::new(true);
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
    let ring = RingDriverMock::new(false);
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let reporter = ReporterMock::new();
    let reset_trigger = ResetTriggerDriverMock::new(ResetScope::Factory);
    let reset = SystemResetDriverMock::new();

    // Act
    let _app = App::new(
        &ring,
        bus,
        NoopAudioControlDriver,
        &reporter,
        &reset_trigger,
        &reset,
    );

    // Assert - Successfully constructed App generic over references to mock drivers
    assert!(trace.borrow().is_empty());
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
fn intercom_controller_handles_concurrent_ring_and_unlock() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, &reporter);

    // Act - trigger ring first
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - ring active
    assert!(reporter.ring.load(Ordering::SeqCst));
    assert!(trace.borrow().is_empty());

    // Act - dispatch unlock while ring is active
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert - door unlocked, ring remains active
    assert_eq!(
        *trace.borrow(),
        alloc::vec![TraceEvent::Bus(BusLineState::DoorUnlock)]
    );
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 2); // Unlocked
    assert!(reporter.ring.load(Ordering::SeqCst));
}

#[test]
fn intercom_controller_dispatches_to_composite_reporters() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let rep1 = ReporterMock::new();
    let rep2 = ReporterMock::new();
    let composite = (&rep1, &rep2);
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, composite);

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
    let booting = sender.current_state();
    sender.starting();
    let starting = sender.current_state();
    sender.commissioning();
    let commissioning = sender.current_state();
    sender.ecosystem_connecting();
    let ecosystem_connecting = sender.current_state();

    // Assert
    assert_eq!(booting, Some(SystemState::Booting));
    assert_eq!(starting, Some(SystemState::Starting));
    assert_eq!(commissioning, Some(SystemState::Commissioning));
    assert_eq!(ecosystem_connecting, Some(SystemState::EcosystemConnecting));
}

#[test]
fn test_reset_service_erases_ecosystem_scope_and_reboots() {
    // Arrange
    use core::sync::atomic::{AtomicBool, Ordering};
    use intercom_core::events::{SystemState, SystemStateWatch};
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;

    struct TriggerMock {
        scope: ResetScope,
    }
    impl ResetTriggerDriver for TriggerMock {
        async fn wait_for_reset(&mut self) -> ResetScope {
            self.scope
        }
    }

    struct ResetMock {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
        reboot_called: AtomicBool,
    }
    impl SystemResetDriver for ResetMock {
        fn erase_credentials(&self, scope: ResetScope) {
            self.erased_scope.set(Some(scope));
        }
        fn reboot(&self) -> ! {
            self.reboot_called.store(true, Ordering::SeqCst);
            panic!("Mock reboot");
        }
    }

    let trigger = TriggerMock {
        scope: ResetScope::Ecosystem,
    };
    let reset = ResetMock {
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
    assert_eq!(
        service.reset.erased_scope.get(),
        Some(ResetScope::Ecosystem)
    );
    assert_eq!(state_watch.try_get(), Some(SystemState::EcosystemReset));
}

#[test]
fn test_reset_service_erases_factory_scope_and_reboots() {
    // Arrange
    use intercom_core::events::{SystemState, SystemStateWatch};
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;

    struct TriggerMock {
        scope: ResetScope,
    }
    impl ResetTriggerDriver for TriggerMock {
        async fn wait_for_reset(&mut self) -> ResetScope {
            self.scope
        }
    }

    struct ResetMock {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
    }
    impl SystemResetDriver for ResetMock {
        fn erase_credentials(&self, scope: ResetScope) {
            self.erased_scope.set(Some(scope));
        }
        fn reboot(&self) -> ! {
            panic!("Mock reboot");
        }
    }

    let trigger = TriggerMock {
        scope: ResetScope::Factory,
    };
    let reset = ResetMock {
        erased_scope: core::cell::Cell::new(None),
    };
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
fn intercom_state_machine_default_duration_is_5_seconds() {
    // Arrange
    let sm = IntercomStateMachine::new();

    // Act
    let duration = sm.unlock_duration_sec();

    // Assert
    assert_eq!(duration, 5);
}

#[test]
fn intercom_state_machine_set_duration_updates_deadline() {
    // Arrange
    let mut sm = IntercomStateMachine::new();

    // Act
    let effects = sm.handle_action(AccessCommand::SetUnlockDuration(10), 1000);

    // Assert
    assert_eq!(sm.unlock_duration_sec(), 10);
    assert_eq!(effects.report_duration, Some(10));

    // Act
    let unlock_effects = sm.handle_action(AccessCommand::Unlock, 2000);

    // Assert
    assert_eq!(unlock_effects.bus_state, Some(BusLineState::DoorUnlock));
    assert_eq!(sm.next_deadline(), Some(2000 + 10_000));
}

#[test]
fn intercom_state_machine_clamps_duration_to_bounds() {
    // Arrange
    let mut sm = IntercomStateMachine::new();

    // Act
    sm.handle_action(AccessCommand::SetUnlockDuration(0), 1000);
    let minimum = sm.unlock_duration_sec();
    sm.handle_action(AccessCommand::SetUnlockDuration(45), 1000);
    let maximum = sm.unlock_duration_sec();

    // Assert
    assert_eq!(minimum, 1);
    assert_eq!(maximum, 30);
}

#[test]
fn intercom_controller_processes_set_unlock_duration_command() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, &reporter);

    // Act
    assert!(handle.set_unlock_duration(8).is_ok());
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(reporter.duration_sec.load(Ordering::SeqCst), 8);
}

#[test]
fn intercom_controller_dispatches_duration_to_composite_reporters() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = AccessControllerHandle::new(channel.sender());
    let trace = RefCell::new(Vec::new());
    let bus = BusSwitchDriverMock { trace: &trace };
    let rep1 = ReporterMock::new();
    let rep2 = ReporterMock::new();
    let composite = (&rep1, &rep2);
    let mut controller = IntercomController::new(bus, NoopAudioControlDriver, composite);

    // Act
    handle.send(AccessCommand::SetUnlockDuration(12));
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(rep1.duration_sec.load(Ordering::SeqCst), 12);
    assert_eq!(rep2.duration_sec.load(Ordering::SeqCst), 12);
}

#[test]
fn controller_mutes_audio_before_opening_relay_and_unmutes_after_closing() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let receiver = channel.receiver();
    let mut controller = IntercomController::new(
        BusSwitchDriverMock { trace: &trace },
        AudioControlDriverMock { trace: &trace },
        IntercomReporterMock { trace: &trace },
    );

    // Act
    handle.start_listen(SessionOwner::HttpDebug);
    futures::executor::block_on(controller.next(&receiver));
    handle.start_talk(SessionOwner::HttpDebug);
    futures::executor::block_on(controller.next(&receiver));

    // Assert
    assert_eq!(
        *trace.borrow(),
        alloc::vec![
            TraceEvent::Bus(BusLineState::Listen),
            TraceEvent::Audio(AudioDirection::CaptureRx),
            TraceEvent::BusReport(BusLineState::Listen),
            TraceEvent::Call(CallState::Listening),
            TraceEvent::Audio(AudioDirection::Muted),
            TraceEvent::Bus(BusLineState::Idle),
            TraceEvent::BusReport(BusLineState::Idle),
            TraceEvent::Call(CallState::Settling),
        ]
    );
}

#[test]
fn controller_selects_talk_bus_before_enabling_playback_after_settling_deadline() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let receiver = channel.receiver();
    let mut controller = IntercomController::new(
        BusSwitchDriverMock { trace: &trace },
        AudioControlDriverMock { trace: &trace },
        IntercomReporterMock { trace: &trace },
    );
    handle.start_listen(SessionOwner::HttpDebug);
    futures::executor::block_on(controller.next(&receiver));
    handle.start_talk(SessionOwner::HttpDebug);
    futures::executor::block_on(controller.next(&receiver));
    trace.borrow_mut().clear();

    // Act
    futures::executor::block_on(controller.next(&receiver));

    // Assert
    assert_eq!(
        *trace.borrow(),
        alloc::vec![
            TraceEvent::Bus(BusLineState::Talk),
            TraceEvent::Audio(AudioDirection::PlaybackTx),
            TraceEvent::BusReport(BusLineState::Talk),
            TraceEvent::Call(CallState::Talking),
        ]
    );
}

#[test]
fn controller_processes_ring_and_emergency_stop_while_strike_is_active() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(
        BusSwitchDriverMock { trace: &trace },
        NoopAudioControlDriver,
        &reporter,
    );
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Act
    handle.trigger_ring();
    futures::executor::block_on(controller.next(&channel.receiver()));
    let ring_during_strike = reporter.ring.load(Ordering::SeqCst);
    handle.emergency_stop();
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert!(ring_during_strike);
    assert_eq!(reporter.lock_state.load(Ordering::SeqCst), 1);
    assert_eq!(
        reporter.call_state.load(Ordering::SeqCst),
        CallState::Idle as u8
    );
    assert_eq!(
        *trace.borrow(),
        alloc::vec![
            TraceEvent::Bus(BusLineState::DoorUnlock),
            TraceEvent::Bus(BusLineState::Idle),
        ]
    );
}

#[test]
fn controller_relocks_on_deadline_without_another_command() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let mut controller = IntercomController::with_duration(
        BusSwitchDriverMock { trace: &trace },
        AudioControlDriverMock { trace: &trace },
        IntercomReporterMock { trace: &trace },
        1,
    );
    handle.request_unlock();
    futures::executor::block_on(controller.next(&channel.receiver()));
    trace.borrow_mut().clear();

    // Act
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(
        *trace.borrow(),
        alloc::vec![
            TraceEvent::Bus(BusLineState::Idle),
            TraceEvent::BusReport(BusLineState::Idle),
            TraceEvent::Lock(LockState::Locked),
            TraceEvent::Call(CallState::Idle),
        ]
    );
}

#[test]
fn controller_reports_preempted_session_and_new_bus_and_call_states() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let reporter = ReporterMock::new();
    let mut controller = IntercomController::new(
        BusSwitchDriverMock { trace: &trace },
        NoopAudioControlDriver,
        &reporter,
    );
    handle.start_listen(SessionOwner::HttpDebug);
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Act
    handle.start_listen(SessionOwner::MatterCall);
    futures::executor::block_on(controller.next(&channel.receiver()));

    // Assert
    assert_eq!(
        reporter.preempted_owner.load(Ordering::SeqCst),
        SessionOwner::HttpDebug as u8
    );
    assert_eq!(
        reporter.bus_state.load(Ordering::SeqCst),
        BusLineState::Listen as u8
    );
    assert_eq!(
        reporter.call_state.load(Ordering::SeqCst),
        CallState::Listening as u8
    );
}

#[test]
fn reporter_reference_and_tuple_compositions_forward_new_events() {
    // Arrange
    let rep1 = ReporterMock::new();
    let rep2 = ReporterMock::new();
    let rep3 = ReporterMock::new();
    let rep4 = ReporterMock::new();
    let composite = ((&rep1, &rep2), &rep3, &rep4);

    // Act
    composite.report_bus_state(BusLineState::Listen);
    composite.report_call_state(CallState::Listening);
    composite.report_session_preempted(SessionOwner::HttpDebug);

    // Assert
    for reporter in [&rep1, &rep2, &rep3, &rep4] {
        assert_eq!(
            reporter.bus_state.load(Ordering::SeqCst),
            BusLineState::Listen as u8
        );
        assert_eq!(
            reporter.call_state.load(Ordering::SeqCst),
            CallState::Listening as u8
        );
        assert_eq!(
            reporter.preempted_owner.load(Ordering::SeqCst),
            SessionOwner::HttpDebug as u8
        );
    }
}

#[test]
fn intercom_handle_queues_all_control_plane_commands_in_order() {
    // Arrange
    let channel = CommandChannel::new();
    let handle = IntercomControllerHandle::new(channel.sender());
    let owner = SessionOwner::HttpDebug;

    // Act
    handle.start_listen(owner);
    handle.start_talk(owner);
    handle.stop_talk(owner);
    handle.session_heartbeat(owner);
    handle.end_session(owner);
    handle.emergency_stop();
    handle.request_unlock();
    handle.trigger_ring();

    // Assert
    assert_eq!(CMD_QUEUE_CAP, 8);
    for command in [
        IntercomCommand::StartListen(owner),
        IntercomCommand::StartTalk(owner),
        IntercomCommand::StopTalk(owner),
        IntercomCommand::SessionHeartbeat(owner),
        IntercomCommand::EndSession(owner),
        IntercomCommand::EmergencyStop,
        IntercomCommand::Unlock,
        IntercomCommand::Ring,
    ] {
        assert_eq!(channel.receiver().try_receive(), Ok(command));
    }
}

#[test]
fn legacy_handle_and_command_aliases_share_the_intercom_queue_and_report_full_capacity() {
    // Arrange
    let channel = CommandChannel::new();
    let legacy = AccessControllerHandle::new(channel.sender());
    let current: IntercomControllerHandle<'_> = legacy;

    // Act
    for _ in 0..CMD_QUEUE_CAP {
        current.send(AccessCommand::Ring);
    }
    let result = legacy.set_unlock_duration(9);
    legacy.request_unlock();

    // Assert
    assert!(result.is_err());
    for _ in 0..CMD_QUEUE_CAP {
        assert_eq!(channel.receiver().try_receive(), Ok(IntercomCommand::Ring));
    }
    assert!(channel.receiver().try_receive().is_err());
}

#[test]
fn app_with_duration_constructs_the_six_port_coordinator() {
    // Arrange
    let trace = RefCell::new(Vec::new());
    let ring = RingDriverMock::new(false);
    let reporter = ReporterMock::new();
    let reset_trigger = ResetTriggerDriverMock::new(ResetScope::Factory);
    let reset = SystemResetDriverMock::new();

    // Act
    let _app = App::with_duration(
        &ring,
        BusSwitchDriverMock { trace: &trace },
        NoopAudioControlDriver,
        &reporter,
        &reset_trigger,
        &reset,
        8,
    );

    // Assert
    assert!(trace.borrow().is_empty());
}

// ============================================================================
// Mock Drivers
// ============================================================================

struct RingDriverMock {
    should_ring: AtomicBool,
}

impl RingDriverMock {
    fn new(should_ring: bool) -> Self {
        Self {
            should_ring: AtomicBool::new(should_ring),
        }
    }
}

impl RingDriver for &RingDriverMock {
    async fn wait_for_ring(&mut self) {
        while !self.should_ring.load(Ordering::SeqCst) {
            embassy_futures::yield_now().await;
        }
        self.should_ring.store(false, Ordering::SeqCst);
    }
}

struct SystemResetDriverMock {
    erased: AtomicBool,
    rebooted: AtomicBool,
}

impl SystemResetDriverMock {
    fn new() -> Self {
        Self {
            erased: AtomicBool::new(false),
            rebooted: AtomicBool::new(false),
        }
    }
}

impl SystemResetDriver for &SystemResetDriverMock {
    fn erase_credentials(&self, _scope: ResetScope) {
        self.erased.store(true, Ordering::SeqCst);
    }

    fn reboot(&self) -> ! {
        self.rebooted.store(true, Ordering::SeqCst);
        panic!("simulated reboot");
    }
}

struct ResetTriggerDriverMock {
    scope: ResetScope,
}

impl ResetTriggerDriverMock {
    fn new(scope: ResetScope) -> Self {
        Self { scope }
    }
}

impl ResetTriggerDriver for &ResetTriggerDriverMock {
    async fn wait_for_reset(&mut self) -> ResetScope {
        self.scope
    }
}

struct ReporterMock {
    ring: AtomicBool,
    lock_state: AtomicU8,
    duration_sec: AtomicU32,
    bus_state: AtomicU8,
    call_state: AtomicU8,
    preempted_owner: AtomicU8,
}

impl ReporterMock {
    fn new() -> Self {
        Self {
            ring: AtomicBool::new(false),
            lock_state: AtomicU8::new(0),
            duration_sec: AtomicU32::new(5),
            bus_state: AtomicU8::new(0),
            call_state: AtomicU8::new(0),
            preempted_owner: AtomicU8::new(0),
        }
    }
}

impl IntercomReporter for ReporterMock {
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

    fn report_bus_state(&self, state: BusLineState) {
        self.bus_state.store(state as u8, Ordering::SeqCst);
    }

    fn report_call_state(&self, state: CallState) {
        self.call_state.store(state as u8, Ordering::SeqCst);
    }

    fn report_session_preempted(&self, owner: SessionOwner) {
        self.preempted_owner.store(owner as u8, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TraceEvent {
    Audio(AudioDirection),
    Bus(BusLineState),
    BusReport(BusLineState),
    Lock(LockState),
    Call(CallState),
}

struct BusSwitchDriverMock<'a> {
    trace: &'a RefCell<Vec<TraceEvent>>,
}

impl BusSwitchDriver for BusSwitchDriverMock<'_> {
    fn set_bus_state(&mut self, state: BusLineState) {
        self.trace.borrow_mut().push(TraceEvent::Bus(state));
    }
}

struct AudioControlDriverMock<'a> {
    trace: &'a RefCell<Vec<TraceEvent>>,
}

impl AudioControlDriver for AudioControlDriverMock<'_> {
    fn set_audio_direction(&mut self, direction: AudioDirection) {
        self.trace.borrow_mut().push(TraceEvent::Audio(direction));
    }
}

struct IntercomReporterMock<'a> {
    trace: &'a RefCell<Vec<TraceEvent>>,
}

impl IntercomReporter for IntercomReporterMock<'_> {
    fn report_ring(&self, _ringing: bool) {}

    fn report_lock_state(&self, state: LockState) {
        self.trace.borrow_mut().push(TraceEvent::Lock(state));
    }

    fn report_unlock_duration(&self, _duration_sec: u32) {}

    fn report_bus_state(&self, state: BusLineState) {
        self.trace.borrow_mut().push(TraceEvent::BusReport(state));
    }

    fn report_call_state(&self, state: CallState) {
        self.trace.borrow_mut().push(TraceEvent::Call(state));
    }
}
