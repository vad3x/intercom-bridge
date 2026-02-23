//! Smart Intercom - Factory Reset Actor
//!
//! Monitors a factory reset trigger (e.g. BOOT button held for 5 seconds)
//! and publishes `SystemState::FactoryReset` to the event bus.
//!
//! The `MatterControllerActor` listens for this event and signals the
//! MatterController to erase the KVS partition and reboot.

use embassy_time::{Duration, Timer};
use log::info;

use crate::events::{Event, EventBus, SystemState};

// ============================================================================
// FactoryResetDriver trait
// ============================================================================

/// Factory reset trigger driver.
///
/// Implementors detect the reset condition (e.g. button held for N seconds).
/// The actual erase + reboot is handled by the MatterController.
pub trait FactoryResetDriver {
    /// Block until the factory reset trigger condition is met.
    async fn wait_for_factory_reset(&mut self);
}

// ============================================================================
// FactoryResetActor
// ============================================================================

/// Factory reset actor.
///
/// Generic over any `FactoryResetDriver` implementation.
/// Waits for the trigger, then publishes `SystemState::FactoryReset`
/// to the event bus. The `MatterControllerActor` bridges this event
/// to the MatterController driver for the actual erase + reboot.
pub struct FactoryResetActor<F: FactoryResetDriver> {
    driver: F,
}

impl<F: FactoryResetDriver> FactoryResetActor<F> {
    pub fn new(driver: F) -> Self {
        Self { driver }
    }

    /// Run the factory reset monitor (never returns — controller reboots).
    ///
    /// Waits for the driver to detect a factory reset trigger, then
    /// publishes `SystemState::FactoryReset` for LED feedback and
    /// the MatterControllerActor to forward to the controller.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("monitoring for factory reset trigger");

        self.driver.wait_for_factory_reset().await;

        info!("=== FACTORY RESET TRIGGERED ===");

        // Publish factory reset event (indicator shows pattern,
        // MatterControllerActor signals the controller to erase + reboot)
        bus.immediate_publisher()
            .publish_immediate(Event::SystemState(SystemState::FactoryReset));

        // Wait for the controller to reboot us
        loop {
            Timer::after(Duration::from_secs(60)).await;
        }
    }
}
