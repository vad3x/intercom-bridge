//! Smart Intercom - Ring Actor
//!
//! Listens to hardware ring detection via `RingDriver` trait
//! and publishes `Event::Ringing` to the event bus.
//!
//! C++ equivalent: `RingActor<Driver>`

use log::info;

use crate::events::{Event, EventBus, EventBusPublisher};

// ============================================================================
// RingDriver trait (like C++ concept)
// ============================================================================

/// Ring detection driver.
///
/// Implementors wait for a valid, debounced ring event from hardware.
pub trait RingDriver {
    /// Block until a valid ring event occurs (debounced + cooldown applied).
    async fn wait_for_ring(&mut self);
}

/// Ring detection actor.
///
/// Generic over any `RingDriver` implementation.
/// Publishes `Event::Ringing` whenever the driver reports a valid ring.
pub struct RingActor<R: RingDriver> {
    driver: R,
}

/// Publisher type alias matching `EventBus` parameters.

impl<R: RingDriver> RingActor<R> {
    pub fn new(driver: R) -> Self {
        Self { driver }
    }

    /// Run the ring detection loop (never returns).
    ///
    /// Waits for the driver to report a ring, then publishes to the event bus.
    pub async fn run(&mut self, bus: &EventBus) {
        let publisher = bus.publisher().unwrap();

        loop {
            self.next(&publisher).await;
        }
    }

    /// Wait for a ring event and publish it.
    async fn next(&mut self, publisher: &EventBusPublisher<'_>) {
        self.driver.wait_for_ring().await;
        info!("ring detected → publishing");
        publisher.publish(Event::Ringing).await;
    }
}
