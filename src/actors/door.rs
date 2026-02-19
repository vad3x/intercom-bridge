//! Smart Intercom - Door Actor
//!
//! Subscribes to `Event::UnlockDispatched` and triggers the door
//! unlock relay via `DoorDriver` trait.
//!
//! C++ equivalent: `DoorActor<Driver>`

use log::info;

use crate::events::{Event, EventBus};

// ============================================================================
// DoorDriver trait (like C++ concept)
// ============================================================================

/// Door unlock driver.
///
/// Implementors control the physical door unlock relay.
pub trait DoorDriver {
    /// Trigger a timed unlock pulse. Returns when the pulse completes.
    async fn trigger_unlock(&mut self);
}

/// Door control actor.
///
/// Generic over any `DoorDriver` implementation.
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
        info!("DoorActor: started");

        let mut subscriber = bus.subscriber().unwrap();

        loop {
            if let Event::UnlockDispatched = subscriber.next_message_pure().await {
                info!("DoorActor: unlock event → triggering hardware");
                self.driver.trigger_unlock().await;
                info!("DoorActor: unlock pulse complete");
            }
        }
    }
}
