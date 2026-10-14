//! Smart Intercom - BOOT Button Factory Reset Driver
//!
//! Monitors GPIO9 (BOOT button on DFRobot Beetle ESP32-C6) for a long press.
//! When held for the configured duration, signals the factory reset trigger.
//! The actual credential erasure and reboot is coordinated by the domain FactoryResetActor.

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use crate::config;
use intercom_core::ports::FactoryResetDriver;
use log::info;

/// GPIO-based BOOT button driver for factory reset.
///
/// Waits for the BOOT button (active LOW) to be held continuously for the
/// configured duration. Uses async edge detection + periodic polling.
pub struct GpioBootButtonDriver {
    config: GpioBootButtonConfig,
}

impl GpioBootButtonDriver {
    pub fn new(config: GpioBootButtonConfig) -> Self {
        info!("BOOT button factory reset: hold={}ms", config.hold_ms);
        Self { config }
    }
}

/// Configuration for the BOOT button factory reset driver.
pub struct GpioBootButtonConfig {
    /// Input pin (BOOT button, active LOW with pull-up)
    pub pin: Input<'static>,
    /// How long the button must be held to trigger factory reset (ms)
    pub hold_ms: u64,
}

impl GpioBootButtonConfig {
    /// Create a config with the given pin and default hold duration.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            hold_ms: config::FACTORY_RESET_HOLD_MS,
        }
    }
}

impl FactoryResetDriver for GpioBootButtonDriver {
    async fn wait_for_factory_reset(&mut self) {
        let hold_duration = Duration::from_millis(self.config.hold_ms);

        loop {
            // Wait for button press (falling edge — BOOT is active LOW)
            self.config.pin.wait_for_falling_edge().await;

            let press_start = Instant::now();
            info!(
                "BOOT button pressed — hold {}s to factory reset",
                self.config.hold_ms / 1000
            );

            // Poll until released or hold duration reached
            let triggered = loop {
                Timer::after(Duration::from_millis(100)).await;

                if self.config.pin.is_high() {
                    // Released before hold duration elapsed
                    info!("BOOT button released (ignored)");
                    break false;
                }

                if Instant::now().duration_since(press_start) >= hold_duration {
                    break true;
                }
            };

            if triggered {
                info!(
                    "BOOT button held for {}s — factory reset!",
                    self.config.hold_ms / 1000
                );
                return;
            }
        }
    }
}
