//! Smart Intercom - Indicator Actor
//!
//! Subscribes to ALL events and drives visual feedback via `IndicatorDriver`.
//! Pure observer — other actors have no knowledge of indicator behavior.
//!
//! C++ equivalent: `IndicatorActor<Driver>`

use log::info;

use crate::events::{Event, EventBus, SystemState};

// ============================================================================
// IndicatorDriver trait (like C++ concept)
// ============================================================================

/// Visual/audio indicator driver.
///
/// Implementors provide status feedback using domain-level concepts.
/// The implementation decides how to visually represent each state
/// (e.g., steady LED, blink pattern, RGB color).
pub trait IndicatorDriver {
    /// Set steady-state indication based on system lifecycle.
    async fn set_state(&mut self, state: SystemState);

    /// Visual feedback: someone is ringing the intercom.
    async fn indicate_ring(&mut self);

    /// Visual feedback: unlock command was dispatched.
    async fn indicate_unlock(&mut self);

    /// Turn the indicator off.
    async fn off(&mut self);
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
    /// Sets `Initializing` state immediately, then reacts to every event.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("IndicatorActor: started");

        let mut subscriber = bus.subscriber().unwrap();

        // Set initial state before entering event loop
        self.driver.set_state(SystemState::Initializing).await;

        loop {
            match subscriber.next_message_pure().await {
                Event::SystemState(state) => {
                    info!("IndicatorActor: system state → {:?}", state);
                    self.driver.set_state(state).await;
                }
                Event::Ringing => {
                    info!("IndicatorActor: ring → blink");
                    self.driver.indicate_ring().await;
                }
                Event::UnlockDispatched => {
                    info!("IndicatorActor: unlock → blink");
                    self.driver.indicate_unlock().await;
                }
            }
        }
    }
}
