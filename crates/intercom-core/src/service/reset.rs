//! Smart Intercom - System Reset Service

use embassy_time::{Duration, Timer};
use log::info;

use crate::config::FACTORY_RESET_REBOOT_DELAY_MS;
use crate::events::SystemStateSender;
use crate::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};

/// System reset lifecycle service.
pub struct ResetService<T: ResetTriggerDriver, R: SystemResetDriver> {
    pub driver: T,
    pub reset: R,
}

impl<T: ResetTriggerDriver, R: SystemResetDriver> ResetService<T, R> {
    pub fn new(driver: T, reset: R) -> Self {
        Self { driver, reset }
    }

    pub async fn run(&mut self, state_sender: SystemStateSender<'_>) {
        info!("monitoring for reset trigger");

        let scope = self.driver.wait_for_reset().await;

        match scope {
            ResetScope::Ecosystem => {
                info!("=== ECOSYSTEM RESET TRIGGERED ===");
                state_sender.ecosystem_reset();
            }
            ResetScope::Factory => {
                info!("=== FACTORY RESET TRIGGERED ===");
                state_sender.factory_reset();
            }
        }

        self.reset.erase_credentials(scope);

        Timer::after(Duration::from_millis(FACTORY_RESET_REBOOT_DELAY_MS)).await;

        info!("Rebooting...");
        self.reset.reboot();
    }
}
