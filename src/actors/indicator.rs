//! Smart Intercom - Indicator Actor
//!
//! Subscribes to ALL events and drives visual feedback via `IndicatorDriver`.
//! Pure observer — other actors have no knowledge of indicator behavior.
//!
//! C++ equivalent: `IndicatorActor<Driver>`

use embassy_futures::join::join;

use log::info;

use crate::config::{
    CONNECTING_BLINK_MS, ERROR_BLINK_MS, FACTORY_RESET_BLINK_MS, INITIALIZING_BLINK_MS,
    RING_BLINK_COUNT, RING_BLINK_MS, UNLOCK_BLINK_COUNT, UNLOCK_BLINK_MS,
};
use crate::events::{Event, EventBus, EventBusSubscriber, SystemState};

// ============================================================================
// IndicatorDriver trait
// ============================================================================

/// Visual/audio indicator driver.
///
/// Methods are sync — they signal the desired state to the driver's `run` loop.
/// The `run` loop handles all GPIO timing internally.
pub trait IndicatorDriver {
    /// Turn the indicator on (steady state).
    fn solid(&self);

    /// Start blinking with the given on/off durations.
    ///
    /// - `count: None` — blink continuously until the next command.
    /// - `count: Some(n)` — blink `n` times, then return to steady on.
    fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>);

    /// Driver event loop — processes commands and drives hardware.
    ///
    /// Must be run concurrently with the actor's event loop via `join`.
    async fn run(&self);
}

/// Visual feedback actor.
///
/// Generic over any `IndicatorDriver` implementation.
/// Observes all events and orchestrates the indicator accordingly.
pub struct IndicatorActor<I: IndicatorDriver> {
    driver: I,
}

impl<I: IndicatorDriver> IndicatorActor<I> {
    pub fn new(driver: I) -> Self {
        Self { driver }
    }

    /// Run the indicator loop (never returns).
    ///
    /// Joins the driver's `run` loop with the actor's event loop.
    /// The driver handles all GPIO timing; the actor translates
    /// bus events into driver commands.
    pub async fn run(&self, bus: &EventBus) {
        // Turn on before entering event loop
        // self.driver.solid();

        join(self.driver.run(), self.event_loop(bus)).await;
    }

    /// Event loop: subscribe to bus events and signal the driver.
    async fn event_loop(&self, bus: &EventBus) {
        let mut subscriber = bus.subscriber().unwrap();
        let mut connecting = false;

        loop {
            connecting = self.next(&mut subscriber, connecting).await;
        }
    }

    /// Await the next event and handle it. Returns updated `connecting` state.
    ///
    /// Driver methods are sync (signal-based), so no `.await` on them.
    async fn next(&self, subscriber: &mut EventBusSubscriber<'_>, connecting: bool) -> bool {
        let event = subscriber.next_message_pure().await;

        match event {
            Event::SystemState(state) => {
                info!("system state → {:?}", state);
                match state {
                    SystemState::Initializing => {
                        self.driver
                            .blink(INITIALIZING_BLINK_MS, INITIALIZING_BLINK_MS, None);
                        true
                    }
                    SystemState::Connected => {
                        self.driver.solid();
                        false
                    }
                    SystemState::Connecting => {
                        self.driver
                            .blink(CONNECTING_BLINK_MS, CONNECTING_BLINK_MS, None);
                        true
                    }
                    SystemState::Ready => {
                        self.driver.solid();
                        false
                    }
                    SystemState::FactoryReset => {
                        self.driver
                            .blink(FACTORY_RESET_BLINK_MS, FACTORY_RESET_BLINK_MS, None);
                        false
                    }
                    SystemState::Error => {
                        self.driver.blink(ERROR_BLINK_MS, ERROR_BLINK_MS, None);
                        false
                    }
                }
            }
            Event::Ringing => {
                info!("ring → blink");
                self.driver
                    .blink(RING_BLINK_MS, RING_BLINK_MS, Some(RING_BLINK_COUNT));
                connecting
            }
            Event::UnlockDispatched => {
                info!("unlock → blink");
                self.driver
                    .blink(UNLOCK_BLINK_MS, UNLOCK_BLINK_MS, Some(UNLOCK_BLINK_COUNT));
                connecting
            }
        }
    }
}
