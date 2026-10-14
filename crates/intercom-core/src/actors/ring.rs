//! Smart Intercom - Ring Actor
//!
//! Listens to hardware ring detection via the `RingDriver` port
//! and publishes `Event::Ringing` to the event bus.

use log::info;

use crate::events::{Event, EventBus, EventBusPublisher};
use crate::ports::RingDriver;

/// Ring detection actor.
///
/// Generic over any `RingDriver` port implementation.
/// Publishes `Event::Ringing` whenever the driver reports a valid ring.
pub struct RingActor<R: RingDriver> {
    driver: R,
}

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
    pub async fn next(&mut self, publisher: &EventBusPublisher<'_>) {
        self.driver.wait_for_ring().await;
        info!("ring detected → publishing");
        publisher.publish(Event::Ringing).await;
    }
}
