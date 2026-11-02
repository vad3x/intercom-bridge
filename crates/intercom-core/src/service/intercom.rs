//! Non-blocking intercom bus, session, and strike orchestrator.

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use crate::domain::intercom::{AudioDirection, IntercomEffects, IntercomStateMachine};
use crate::ports::inbound::CommandReceiver;
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
        let timeout = match self.state.next_deadline() {
            Some(deadline) => {
                let now = Instant::now().as_millis();
                Duration::from_millis(deadline.saturating_sub(now))
            }
            None => Duration::from_secs(86_400),
        };

        let effects = match select(receiver.receive(), Timer::after(timeout)).await {
            Either::First(cmd) => self.state.handle_action(cmd, Instant::now().as_millis()),
            Either::Second(()) => self.state.poll_deadlines(Instant::now().as_millis()),
        };

        self.dispatch_effects(effects);
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
