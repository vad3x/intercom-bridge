//! Smart Intercom - Factory Reset Actor
//!
//! Monitors a factory reset trigger via `FactoryResetDriver`,
//! updates system lifecycle state to `FactoryReset`, erases persistent credentials
//! via `SystemResetDriver`, and reboots the hardware.

use embassy_time::{Duration, Timer};
use log::info;

use crate::config::FACTORY_RESET_REBOOT_DELAY_MS;
use crate::events::SystemStateSender;
use crate::ports::{FactoryResetDriver, SystemResetDriver};

/// Factory reset actor.
///
/// Generic over `FactoryResetDriver` (inbound trigger) and `SystemResetDriver`
/// (outbound reset effect).
pub struct FactoryResetActor<F: FactoryResetDriver, R: SystemResetDriver> {
    driver: F,
    reset: R,
}

impl<F: FactoryResetDriver, R: SystemResetDriver> FactoryResetActor<F, R> {
    /// Create a new factory reset actor.
    pub fn new(driver: F, reset: R) -> Self {
        Self { driver, reset }
    }

    /// Run the factory reset monitor (never returns — controller reboots).
    ///
    /// Waits for the driver to detect a factory reset trigger, notifies the
    /// lifecycle watch sender, erases credentials, waits for reboot delay,
    /// and triggers reboot.
    pub async fn run(&mut self, state_sender: SystemStateSender<'_>) {
        info!("monitoring for factory reset trigger");

        self.driver.wait_for_factory_reset().await;

        info!("=== FACTORY RESET TRIGGERED ===");

        state_sender.factory_reset();
        self.reset.erase_credentials();

        Timer::after(Duration::from_millis(FACTORY_RESET_REBOOT_DELAY_MS)).await;

        info!("Rebooting...");
        self.reset.reboot();
    }
}
