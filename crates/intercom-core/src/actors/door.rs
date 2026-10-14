//! Smart Intercom - Door Actor
//!
//! Subscribes to `Event::UnlockDispatched` and triggers the door
//! unlock relay via the `DoorDriver` port.

use log::info;

use crate::events::{Event, EventBus, EventBusSubscriber};
use crate::ports::DoorDriver;

/// Door control actor.
///
/// Generic over any `DoorDriver` port implementation.
/// Listens for unlock events and triggers the physical door relay.
pub struct DoorActor<D: DoorDriver> {
    driver: D,
}

impl<D: DoorDriver> DoorActor<D> {
    pub fn new(driver: D) -> Self {
        Self { driver }
    }

    /// Run the door control loop (never returns).
    ///
    /// Subscribes to all events, acts only on `UnlockDispatched`.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        loop {
            self.next(&mut subscriber).await;
        }
    }

    /// Await the next unlock event and trigger the hardware relay.
    ///
    /// Drains any unlock events that queued up during the pulse to
    /// deduplicate rapid-fire commands (e.g. double-tap).
    pub async fn next(&mut self, subscriber: &mut EventBusSubscriber<'_>) {
        if let Event::UnlockDispatched = subscriber.next_message_pure().await {
            info!("unlock event → triggering hardware");
            self.driver.trigger_unlock().await;
            info!("unlock pulse complete");

            let mut drained = 0;
            while let Some(Event::UnlockDispatched) = subscriber.try_next_message_pure() {
                drained += 1;
            }
            if drained > 0 {
                info!("drained {} queued unlock events", drained);
            }
        }
    }
}
