//! Smart Intercom - Indicator Actor
//!
//! Subscribes to lifecycle state changes via `SystemStateReceiver` and drives
//! visual feedback via `IndicatorDriver`. Pure lifecycle observer.

use embassy_futures::join::join;
use log::info;

use crate::config::{
    CONNECTED_OFF_MS, CONNECTED_ON_MS, CONNECTING_OFF_MS, CONNECTING_ON_MS, ERROR_BURST_COUNT,
    ERROR_OFF_MS, ERROR_ON_MS, ERROR_PAUSE_MS, FACTORY_RESET_OFF_MS, FACTORY_RESET_ON_MS,
    INITIALIZED_OFF_MS, INITIALIZED_ON_MS, INITIALIZING_OFF_MS, INITIALIZING_ON_MS,
};
use crate::events::{SystemState, SystemStateReceiver};
use crate::ports::IndicatorDriver;

/// Visual feedback actor.
///
/// Generic over any `IndicatorDriver` port implementation.
/// Observes lifecycle state transitions and orchestrates the indicator patterns.
pub struct IndicatorActor<I: IndicatorDriver> {
    driver: I,
}

impl<I: IndicatorDriver> IndicatorActor<I> {
    /// Create a new indicator actor wrapping an indicator driver port.
    pub fn new(driver: I) -> Self {
        Self { driver }
    }

    /// Run the indicator loop (never returns).
    ///
    /// Joins the driver's `run` loop with the actor's lifecycle state observer loop.
    pub async fn run(&self, mut receiver: SystemStateReceiver<'_>) {
        join(self.driver.run(), self.state_loop(&mut receiver)).await;
    }

    /// State loop: observe lifecycle changes and signal the driver.
    async fn state_loop(&self, receiver: &mut SystemStateReceiver<'_>) {
        if let Some(state) = receiver.try_get() {
            self.handle_state(state);
        }

        loop {
            let state = receiver.changed().await;
            self.handle_state(state);
        }
    }

    /// Update indicator pattern based on system lifecycle state.
    fn handle_state(&self, state: SystemState) {
        info!("system state → {:?}", state);
        match state {
            SystemState::Initializing => {
                self.driver
                    .blink(INITIALIZING_ON_MS, INITIALIZING_OFF_MS, None);
            }
            SystemState::Initialized => {
                self.driver
                    .blink(INITIALIZED_ON_MS, INITIALIZED_OFF_MS, None);
            }
            SystemState::Connecting => {
                self.driver
                    .blink(CONNECTING_ON_MS, CONNECTING_OFF_MS, None);
            }
            SystemState::Connected => {
                self.driver.blink(CONNECTED_ON_MS, CONNECTED_OFF_MS, None);
            }
            SystemState::Ready => {
                self.driver.solid();
            }
            SystemState::FactoryReset => {
                self.driver
                    .blink(FACTORY_RESET_ON_MS, FACTORY_RESET_OFF_MS, None);
            }
            SystemState::Error => {
                self.driver.burst(
                    ERROR_ON_MS,
                    ERROR_OFF_MS,
                    ERROR_BURST_COUNT,
                    ERROR_PAUSE_MS,
                );
            }
        }
    }
}
