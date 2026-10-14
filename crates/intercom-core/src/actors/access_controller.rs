//! Smart Intercom - Access Controller
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
use crate::ports::{DoorDriver, IndicatorDriver};

/// Access Controller application orchestrator.
///
/// Owns the pure `AccessStateMachine` and coordinates the door strike relay,
/// ecosystem status reporting, and visual feedback events.
pub struct AccessController<D: DoorDriver, R: IntercomReporter, I: IndicatorDriver> {
    door: D,
    reporter: R,
    indicator: I,
    state: AccessStateMachine,
}

impl<D: DoorDriver, R: IntercomReporter, I: IndicatorDriver> AccessController<D, R, I> {
    /// Create a new Access Controller with the required outbound port drivers.
    pub fn new(door: D, reporter: R, indicator: I) -> Self {
        Self {
            door,
            reporter,
            indicator,
            state: AccessStateMachine::new(),
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

        if effects.indicate_unlock {
            info!("indicating unlock");
            self.indicator.indicate_unlock();
        }

        if effects.indicate_ring {
            info!("indicating ring");
            self.indicator.indicate_ring();
        }

        if effects.trigger_relay {
            info!("triggering door unlock relay");
            self.door.trigger_unlock().await;
            info!("door unlock relay complete");
        }
    }
}
