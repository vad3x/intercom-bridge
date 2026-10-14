//! Smart Intercom - Indicator Actor
//!
//! Subscribes to ALL events and drives visual feedback via `IndicatorDriver`.
//! Pure observer — other actors have no knowledge of indicator behavior.

use embassy_futures::join::join;
use log::info;

use crate::config::{
    CONNECTED_OFF_MS, CONNECTED_ON_MS, CONNECTING_OFF_MS, CONNECTING_ON_MS, ERROR_BURST_COUNT,
    ERROR_OFF_MS, ERROR_ON_MS, ERROR_PAUSE_MS, FACTORY_RESET_OFF_MS, FACTORY_RESET_ON_MS,
    INITIALIZED_OFF_MS, INITIALIZED_ON_MS, INITIALIZING_OFF_MS, INITIALIZING_ON_MS,
    RING_BLINK_COUNT, RING_BLINK_MS, UNLOCK_BLINK_COUNT, UNLOCK_BLINK_MS,
};
use crate::events::{Event, EventBus, EventBusSubscriber, SystemState};
use crate::ports::IndicatorDriver;

/// Visual feedback actor.
///
/// Generic over any `IndicatorDriver` port implementation.
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
        join(self.driver.run(), self.event_loop(bus)).await;
    }

    /// Event loop: subscribe to bus events and signal the driver.
    async fn event_loop(&self, bus: &EventBus) {
        let mut subscriber = bus.subscriber().unwrap();
        let mut current = false;

        loop {
            current = self.next(&mut subscriber, current).await;
        }
    }

    /// Await the next event and handle it. Returns updated `current` state.
    ///
    /// Driver methods are sync (signal-based), so no `.await` on them.
    pub async fn next(&self, subscriber: &mut EventBusSubscriber<'_>, current: bool) -> bool {
        let event = subscriber.next_message_pure().await;

        match event {
            Event::SystemState(state) => {
                info!("system state → {:?}", state);
                match state {
                    SystemState::Initializing => {
                        self.driver
                            .blink(INITIALIZING_ON_MS, INITIALIZING_OFF_MS, None);
                        true
                    }
                    SystemState::Initialized => {
                        self.driver
                            .blink(INITIALIZED_ON_MS, INITIALIZED_OFF_MS, None);
                        true
                    }
                    SystemState::Connecting => {
                        self.driver.blink(CONNECTING_ON_MS, CONNECTING_OFF_MS, None);
                        true
                    }
                    SystemState::Connected => {
                        self.driver.blink(CONNECTED_ON_MS, CONNECTED_OFF_MS, None);
                        true
                    }
                    SystemState::Ready => {
                        self.driver.solid();
                        false
                    }
                    SystemState::FactoryReset => {
                        self.driver
                            .blink(FACTORY_RESET_ON_MS, FACTORY_RESET_OFF_MS, None);
                        false
                    }
                    SystemState::Error => {
                        self.driver.burst(
                            ERROR_ON_MS,
                            ERROR_OFF_MS,
                            ERROR_BURST_COUNT,
                            ERROR_PAUSE_MS,
                        );
                        false
                    }
                }
            }
            Event::Ringing => {
                info!("ring → blink");
                self.driver
                    .blink(RING_BLINK_MS, RING_BLINK_MS, Some(RING_BLINK_COUNT));
                current
            }
            Event::UnlockDispatched => {
                info!("unlock → blink");
                self.driver
                    .blink(UNLOCK_BLINK_MS, UNLOCK_BLINK_MS, Some(UNLOCK_BLINK_COUNT));
                current
            }
            // Internal readiness signals — no visual feedback
            Event::DoorLockReady | Event::RingReady => current,
        }
    }
}
