//! Non-blocking intercom bus, session, and strike orchestrator.

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use crate::domain::intercom::{AudioDirection, IntercomEffects, IntercomStateMachine};
use crate::ports::inbound::{CommandReceiver, IntercomCommand};
use crate::ports::reporter::IntercomReporter;
use crate::ports::{AudioControlDriver, BusSwitchDriver};

/// Coordinates intercom commands and deadlines through synchronous outbound ports.
pub struct IntercomController<B: BusSwitchDriver, A: AudioControlDriver, R: IntercomReporter> {
    bus: B,
    audio: A,
    reporter: R,
    state: IntercomStateMachine,
}

impl<B: BusSwitchDriver, A: AudioControlDriver, R: IntercomReporter> IntercomController<B, A, R> {
    /// Create an idle controller with the default timing policy.
    pub fn new(bus: B, audio: A, reporter: R) -> Self {
        Self {
            bus,
            audio,
            reporter,
            state: IntercomStateMachine::new(),
        }
    }

    /// Create a controller with a clamped initial strike duration in seconds.
    pub fn with_duration(bus: B, audio: A, reporter: R, unlock_duration_sec: u32) -> Self {
        Self {
            bus,
            audio,
            reporter,
            state: IntercomStateMachine::new().with_duration(unlock_duration_sec),
        }
    }

    /// Run command and deadline processing indefinitely.
    pub async fn run(&mut self, receiver: &CommandReceiver<'_>) {
        info!("intercom controller started");
        loop {
            self.next(receiver).await;
        }
    }

    /// Process the next queued command or elapsed domain deadline.
    pub async fn next(&mut self, receiver: &CommandReceiver<'_>) {
        if self.dispatch_due_deadlines(Instant::now().as_millis()) {
            return;
        }

        let timeout = match self.state.next_deadline() {
            Some(deadline) => {
                let now = Instant::now().as_millis();
                Duration::from_millis(deadline.saturating_sub(now))
            }
            None => Duration::from_secs(86_400),
        };

        match select(receiver.receive(), Timer::after(timeout)).await {
            Either::First(cmd) => self.dispatch_command(cmd, Instant::now().as_millis()),
            Either::Second(()) => {
                let effects = self.state.poll_deadlines(Instant::now().as_millis());
                self.dispatch_effects(effects);
            }
        }
    }

    fn dispatch_command(&mut self, command: IntercomCommand, now_ms: u64) {
        // A deadline can expire while waiting even when the ready command wins select.
        self.dispatch_due_deadlines(now_ms);
        let effects = self.state.handle_action(command, now_ms);
        self.dispatch_effects(effects);
    }

    fn dispatch_due_deadlines(&mut self, now_ms: u64) -> bool {
        if !self
            .state
            .next_deadline()
            .is_some_and(|deadline| deadline <= now_ms)
        {
            return false;
        }

        let effects = self.state.poll_deadlines(now_ms);
        self.dispatch_effects(effects);
        true
    }

    fn dispatch_effects(&mut self, effects: IntercomEffects) {
        // Apply hardware effects before reporting: mute, switch the bus, then restore audio.
        if effects.audio_direction == Some(AudioDirection::Muted) {
            self.audio.set_audio_direction(AudioDirection::Muted);
        }

        if let Some(bus_state) = effects.bus_state {
            info!("bus line -> {:?}", bus_state);
            self.bus.set_bus_state(bus_state);
        }

        if let Some(dir) = effects.audio_direction {
            if dir != AudioDirection::Muted {
                self.audio.set_audio_direction(dir);
            }
        }

        if let Some(bus_state) = effects.bus_state {
            self.reporter.report_bus_state(bus_state);
        }

        if let Some(preempted) = effects.preempted_owner {
            self.reporter.report_session_preempted(preempted);
        }

        if let Some(lock) = effects.report_lock {
            self.reporter.report_lock_state(lock);
        }

        if let Some(ring) = effects.report_ring {
            self.reporter.report_ring(ring);
        }

        if let Some(duration_sec) = effects.report_duration {
            self.reporter.report_unlock_duration(duration_sec);
        }

        if let Some(call_state) = effects.report_call_state {
            self.reporter.report_call_state(call_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::intercom::{BusLineState, CallState, IntercomAction, SessionOwner};
    use crate::domain::policy::{IntercomTimingPolicy, SessionHeartbeatTimeout};
    use crate::ports::inbound::CommandChannel;
    use crate::ports::LockState;
    use std::cell::RefCell;
    use std::vec::Vec;

    #[test]
    fn received_heartbeat_cannot_revive_an_expired_session() {
        // Arrange
        let trace = RefCell::new(Vec::new());
        let mut controller = IntercomController::new(
            BusSwitchDriverMock(&trace),
            AudioControlDriverMock(&trace),
            IntercomReporterMock(&trace),
        );
        let owner = SessionOwner::HttpDebug;
        controller.dispatch_command(IntercomAction::StartListen(owner), 0);
        trace.borrow_mut().clear();

        // Act
        controller.dispatch_command(IntercomAction::SessionHeartbeat(owner), 5_000);

        // Assert
        assert_eq!(controller.state.call_state(), CallState::Idle);
        assert_eq!(
            *trace.borrow(),
            std::vec![
                TraceEvent::Audio(AudioDirection::Muted),
                TraceEvent::Bus(BusLineState::Idle),
                TraceEvent::BusReport(BusLineState::Idle),
                TraceEvent::Call(CallState::Idle),
            ]
        );
    }

    #[test]
    fn received_command_cannot_postpone_talk_thermal_cutoff() {
        // Arrange
        let trace = RefCell::new(Vec::new());
        let mut controller = IntercomController::new(
            BusSwitchDriverMock(&trace),
            AudioControlDriverMock(&trace),
            IntercomReporterMock(&trace),
        );
        let owner = SessionOwner::MatterCall;
        controller.dispatch_command(IntercomAction::StartTalk(owner), 0);
        for now_ms in (4_000..60_000).step_by(4_000) {
            controller.dispatch_command(IntercomAction::SessionHeartbeat(owner), now_ms);
        }
        trace.borrow_mut().clear();

        // Act
        controller.dispatch_command(IntercomAction::SessionHeartbeat(owner), 60_000);

        // Assert
        assert_eq!(controller.state.call_state(), CallState::Idle);
        assert_eq!(
            *trace.borrow(),
            std::vec![
                TraceEvent::Audio(AudioDirection::Muted),
                TraceEvent::Bus(BusLineState::Idle),
                TraceEvent::BusReport(BusLineState::Idle),
                TraceEvent::Call(CallState::Idle),
            ]
        );
    }

    #[test]
    fn expired_strike_relocks_before_a_received_ring_command() {
        // Arrange
        let trace = RefCell::new(Vec::new());
        let mut controller = IntercomController::with_duration(
            BusSwitchDriverMock(&trace),
            AudioControlDriverMock(&trace),
            IntercomReporterMock(&trace),
            1,
        );
        controller.dispatch_command(IntercomAction::Unlock, 0);
        trace.borrow_mut().clear();

        // Act
        controller.dispatch_command(IntercomAction::Ring, 1_000);

        // Assert
        assert_eq!(controller.state.bus_state(), BusLineState::Idle);
        assert_eq!(
            *trace.borrow(),
            std::vec![
                TraceEvent::Bus(BusLineState::Idle),
                TraceEvent::BusReport(BusLineState::Idle),
                TraceEvent::Lock(LockState::Locked),
                TraceEvent::Call(CallState::Idle),
                TraceEvent::Ring(true),
            ]
        );
    }

    #[test]
    fn next_handles_expired_deadline_before_dequeuing_a_ready_command() {
        // Arrange
        let trace = RefCell::new(Vec::new());
        let channel = CommandChannel::new();
        let mut controller = IntercomController::new(
            BusSwitchDriverMock(&trace),
            AudioControlDriverMock(&trace),
            IntercomReporterMock(&trace),
        );
        controller.state = IntercomStateMachine::with_policy(IntercomTimingPolicy {
            heartbeat_timeout: SessionHeartbeatTimeout::new(0),
            ..IntercomTimingPolicy::new()
        });
        controller.dispatch_command(
            IntercomAction::StartListen(SessionOwner::HttpDebug),
            Instant::now().as_millis(),
        );
        channel.sender().try_send(IntercomAction::Ring).unwrap();
        trace.borrow_mut().clear();

        // Act
        futures::executor::block_on(controller.next(&channel.receiver()));

        // Assert
        assert_eq!(controller.state.call_state(), CallState::Idle);
        assert_eq!(channel.receiver().try_receive(), Ok(IntercomAction::Ring));
        assert_eq!(
            *trace.borrow(),
            std::vec![
                TraceEvent::Audio(AudioDirection::Muted),
                TraceEvent::Bus(BusLineState::Idle),
                TraceEvent::BusReport(BusLineState::Idle),
                TraceEvent::Call(CallState::Idle),
            ]
        );
    }

    #[derive(Debug, PartialEq, Eq)]
    enum TraceEvent {
        Audio(AudioDirection),
        Bus(BusLineState),
        BusReport(BusLineState),
        Lock(LockState),
        Call(CallState),
        Ring(bool),
    }

    struct BusSwitchDriverMock<'a>(&'a RefCell<Vec<TraceEvent>>);
    struct AudioControlDriverMock<'a>(&'a RefCell<Vec<TraceEvent>>);
    struct IntercomReporterMock<'a>(&'a RefCell<Vec<TraceEvent>>);

    impl BusSwitchDriver for BusSwitchDriverMock<'_> {
        fn set_bus_state(&mut self, state: BusLineState) {
            self.0.borrow_mut().push(TraceEvent::Bus(state));
        }
    }

    impl AudioControlDriver for AudioControlDriverMock<'_> {
        fn set_audio_direction(&mut self, direction: AudioDirection) {
            self.0.borrow_mut().push(TraceEvent::Audio(direction));
        }
    }

    impl IntercomReporter for IntercomReporterMock<'_> {
        fn report_ring(&self, ringing: bool) {
            self.0.borrow_mut().push(TraceEvent::Ring(ringing));
        }

        fn report_lock_state(&self, state: LockState) {
            self.0.borrow_mut().push(TraceEvent::Lock(state));
        }

        fn report_unlock_duration(&self, _duration_sec: u32) {}

        fn report_bus_state(&self, state: BusLineState) {
            self.0.borrow_mut().push(TraceEvent::BusReport(state));
        }

        fn report_call_state(&self, state: CallState) {
            self.0.borrow_mut().push(TraceEvent::Call(state));
        }
    }
}
