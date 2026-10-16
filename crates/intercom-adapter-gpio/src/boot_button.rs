//! Smart Intercom - BOOT Button Reset Driver
//!
//! Monitors GPIO9 (BOOT button) for dual-stage long press:
//! - Held 5s to 10s and released: Ecosystem Reset (unpairs active ecosystem, preserves Wi-Fi).
//! - Held >= 10s: Full Factory Reset (erases both Wi-Fi and ecosystem credentials).

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use crate::config;
use intercom_core::domain::reset::classify_reset_hold;
use intercom_core::ports::{ResetScope, ResetTriggerDriver};
use log::info;

/// GPIO-based BOOT button driver for system reset.
pub struct GpioBootButtonDriver {
    config: GpioBootButtonConfig,
}

impl GpioBootButtonDriver {
    pub fn new(config: GpioBootButtonConfig) -> Self {
        info!(
            "BOOT button reset driver: ecosystem_hold={}ms, factory_hold={}ms",
            config.ecosystem_hold_ms, config.factory_hold_ms
        );
        Self { config }
    }
}

/// Configuration for the BOOT button reset driver.
pub struct GpioBootButtonConfig {
    /// Input pin (BOOT button, active LOW with pull-up)
    pub pin: Input<'static>,
    /// How long the button must be held to trigger ecosystem reset (ms)
    pub ecosystem_hold_ms: u64,
    /// How long the button must be held to trigger factory reset (ms)
    pub factory_hold_ms: u64,
}

impl GpioBootButtonConfig {
    /// Create a config with the given pin and default hold durations.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            ecosystem_hold_ms: config::ECOSYSTEM_RESET_HOLD_MS,
            factory_hold_ms: config::FACTORY_RESET_HOLD_MS,
        }
    }
}

impl ResetTriggerDriver for GpioBootButtonDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        let ecosystem_duration = Duration::from_millis(self.config.ecosystem_hold_ms);
        let factory_duration = Duration::from_millis(self.config.factory_hold_ms);

        loop {
            self.config.pin.wait_for_falling_edge().await;

            Timer::after(Duration::from_millis(30)).await;
            if self.config.pin.is_high() {
                continue;
            }

            let press_start = Instant::now();
            info!(
                "BOOT button pressed — hold {}s for ecosystem reset, {}s for full factory reset",
                self.config.ecosystem_hold_ms / 1000,
                self.config.factory_hold_ms / 1000
            );

            let scope = loop {
                Timer::after(Duration::from_millis(100)).await;

                let elapsed = Instant::now().duration_since(press_start);
                let released = self.config.pin.is_high();

                if let Some(scope) = classify_reset_hold(elapsed, released, ecosystem_duration, factory_duration) {
                    break Some(scope);
                }

                if released {
                    info!("BOOT button released early (ignored)");
                    break None;
                }
            };

            if let Some(triggered_scope) = scope {
                return triggered_scope;
            }
        }
    }
}
