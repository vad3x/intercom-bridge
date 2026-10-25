//! Smart Intercom - Access Controller Service
//!
//! Consolidated use-case orchestrator for entrance door access.
//! Pumps commands from `CommandChannel` into `AccessStateMachine`,
//! tracks active deadlines, and dispatches effects to ports.

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use crate::domain::access::{AccessEffects, AccessStateMachine};
use crate::ports::inbound::CommandReceiver;
use crate::ports::reporter::IntercomReporter;
use crate::ports::DoorDriver;

/// Access Controller application orchestrator.
///
/// Owns the pure `AccessStateMachine` and coordinates door unlock actuation
/// and domain status reporting.
pub struct AccessController<D: DoorDriver, R: IntercomReporter> {
    door: D,
    reporter: R,
    state: AccessStateMachine,
}

impl<D: DoorDriver, R: IntercomReporter> AccessController<D, R> {
    /// Create a new Access Controller with the required outbound port drivers.
    pub fn new(door: D, reporter: R) -> Self {
        Self {
            door,
            reporter,
            state: AccessStateMachine::new(),
        }
    }

    /// Create a new Access Controller with a specific initial unlock duration in seconds.
    pub fn with_duration(door: D, reporter: R, unlock_duration_sec: u32) -> Self {
        Self {
            door,
            reporter,
            state: AccessStateMachine::new().with_duration(unlock_duration_sec),
        }
    }

    /// Run the access controller loop indefinitely.
    pub async fn run(&mut self, receiver: &CommandReceiver<'_>) {
        info!("started");

        loop {
            self.next(receiver).await;
        }
    }

    /// Await the next command or deadline expiration and dispatch resulting effects.
    pub async fn next(&mut self, receiver: &CommandReceiver<'_>) {
        let timeout = match self.state.next_deadline() {
            Some(deadline) => {
                let now = Instant::now().as_millis();
                if deadline > now {
                    Duration::from_millis(deadline - now)
                } else {
                    Duration::from_millis(0)
                }
            }
            None => Duration::from_secs(86400),
        };

        let effects = match select(receiver.receive(), Timer::after(timeout)).await {
            Either::First(cmd) => {
                let now = Instant::now().as_millis();
                self.state.handle_command(cmd, now)
            }
            Either::Second(()) => {
                let now = Instant::now().as_millis();
                self.state.poll_deadlines(now)
            }
        };

        self.dispatch_effects(effects).await;
    }

    /// Dispatch domain effects to outbound ports.
    async fn dispatch_effects(&mut self, effects: AccessEffects) {
        if let Some(lock) = effects.report_lock {
            info!("reporting lock state: {:?}", lock);
            self.reporter.report_lock_state(lock);
        }

        if let Some(ring) = effects.report_ring {
            info!("reporting ring: {}", ring);
            self.reporter.report_ring(ring);
        }

        if effects.trigger_unlock {
            info!("triggering door unlock");
            self.door.trigger_unlock().await;
            info!("door unlock complete");
        }

        if let Some(duration_sec) = effects.report_duration {
            info!("updating unlock duration: {}s", duration_sec);
            self.door.set_unlock_duration_ms(duration_sec * 1000);
            self.reporter.report_unlock_duration(duration_sec);
        }
    }
}
