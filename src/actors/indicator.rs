//! Smart Intercom - Indicator Actor
//!
//! Subscribes to ALL events and drives visual feedback via `IndicatorDriver`.
//! Pure observer — other actors have no knowledge of indicator behavior.
//!
//! C++ equivalent: `IndicatorActor<Driver>`

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Timer};
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

/// Duration of each half of the connecting blink cycle (500ms on, 500ms off = 1s total).
const CONNECTING_BLINK_MS: u64 = 500;

impl<I: IndicatorDriver> IndicatorActor<I> {
    pub fn new(driver: I) -> Self {
        Self { driver }
    }

    /// Run the indicator loop (never returns).
    ///
    /// Sets `Initializing` state immediately, then reacts to every event.
    /// When in `Connecting` state, blinks the LED (500ms on / 500ms off)
    /// until another event arrives.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        // Set initial state before entering event loop
        self.driver.set_state(SystemState::Initializing).await;

        let mut connecting = false;

        loop {
            let event = if connecting {
                // Blink loop: LED on → 500ms → LED off → 500ms → repeat
                // Interrupted immediately when a new event arrives.
                loop {
                    // ON phase
                    self.driver.set_state(SystemState::Connecting).await;
                    match select(
                        Timer::after(Duration::from_millis(CONNECTING_BLINK_MS)),
                        subscriber.next_message_pure(),
                    )
                    .await
                    {
                        Either::First(()) => {}
                        Either::Second(event) => break event,
                    }

                    // OFF phase
                    self.driver.off().await;
                    match select(
                        Timer::after(Duration::from_millis(CONNECTING_BLINK_MS)),
                        subscriber.next_message_pure(),
                    )
                    .await
                    {
                        Either::First(()) => continue, // loop back to ON phase
                        Either::Second(event) => break event,
                    }
                }
            } else {
                subscriber.next_message_pure().await
            };

            match event {
                Event::SystemState(state) => {
                    info!("system state → {:?}", state);
                    connecting = state == SystemState::Connecting;
                    self.driver.set_state(state).await;
                }
                Event::Ringing => {
                    info!("ring → blink");
                    self.driver.indicate_ring().await;
                }
                Event::UnlockDispatched => {
                    info!("unlock → blink");
                    self.driver.indicate_unlock().await;
                }
            }
        }
    }
}
