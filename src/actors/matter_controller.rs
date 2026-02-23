//! Smart Intercom - Matter Controller Actor
//!
//! Bridges system events to Matter controller actions.
//! Subscribes to the event bus and handles factory reset:
//! erases commissioning data, waits for the indicator to blink, then reboots.

use embassy_time::{Duration, Timer};
use log::info;

use crate::events::{Event, EventBus, SystemState};

// ============================================================================
// MatterControllerActor
// ============================================================================

/// Matter controller actor — handles factory reset for the Matter stack.
///
/// When `SystemState::FactoryReset` is received, calls `on_factory_reset`
/// to erase commissioning data, waits for the indicator LED to show the
/// reset pattern, then reboots.
pub struct MatterControllerActor {
    on_factory_reset: fn(),
}

impl MatterControllerActor {
    pub fn new(on_factory_reset: fn()) -> Self {
        Self { on_factory_reset }
    }

    /// Run the event bridge loop (never returns).
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        loop {
            if let Event::SystemState(SystemState::FactoryReset) =
                subscriber.next_message_pure().await
            {
                info!("factory reset event → erasing Matter KVS");
                (self.on_factory_reset)();

                Timer::after(Duration::from_millis(
                    crate::config::FACTORY_RESET_REBOOT_DELAY_MS,
                ))
                .await;

                info!("Rebooting...");
                esp_hal::system::software_reset();
            }
        }
    }
}
