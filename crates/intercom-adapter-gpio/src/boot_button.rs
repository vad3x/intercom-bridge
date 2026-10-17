//! Smart Intercom - BOOT Button Reset Driver
//!
//! Monitors GPIO9 (BOOT button) for dual-stage long press:
//! - Held 5s to 10s and released: Ecosystem Reset (unpairs active ecosystem, preserves Wi-Fi).
//! - Held >= 10s: Full Factory Reset (erases both Wi-Fi and ecosystem credentials).

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use crate::config;
use crate::led::GpioLedDriver;
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
            "BOOT button reset driver: ecosystem_hold={}ms, factory_hold={}ms, feedback={}",
            config.ecosystem_hold_ms,
            config.factory_hold_ms,
            config.indicator.is_some()
        );
        Self { config }
    }
}

/// Configuration for the BOOT button reset driver.
pub struct GpioBootButtonConfig {
    /// Input pin (BOOT button, active LOW with pull-up)
    pub pin: Input<'static>,
    /// Optional LED indicator for live hold feedback
    pub indicator: Option<&'static GpioLedDriver>,
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
            indicator: None,
            ecosystem_hold_ms: config::ECOSYSTEM_RESET_HOLD_MS,
            factory_hold_ms: config::FACTORY_RESET_HOLD_MS,
        }
    }

    /// Attach an LED indicator for live visual feedback during long press.
    pub fn with_indicator(mut self, indicator: &'static GpioLedDriver) -> Self {
        self.indicator = Some(indicator);
        self
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

            if let Some(led) = self.config.indicator {
                led.hold_feedback_press();
            }

            let mut stage1_notified = false;

            let scope = loop {
                Timer::after(Duration::from_millis(100)).await;

                let elapsed = Instant::now().duration_since(press_start);
                let pin_high = self.config.pin.is_high();
                let released = if pin_high {
                    // Debounce release to filter out mechanical contact bounce during long holds
                    Timer::after(Duration::from_millis(30)).await;
                    self.config.pin.is_high()
                } else {
                    false
                };

                // Visual feedback when Stage 1 (Ecosystem reset, 5s) is armed
                if !stage1_notified && elapsed >= ecosystem_duration {
                    stage1_notified = true;
                    info!("BOOT button hold 5s reached (Ecosystem Reset armed)");
                    if let Some(led) = self.config.indicator {
                        led.hold_feedback_stage1();
                    }
                }

                if let Some(scope) = classify_reset_hold(elapsed, released, ecosystem_duration, factory_duration) {
                    if scope == ResetScope::Factory {
                        info!("BOOT button hold 10s reached (Factory Reset triggered)");
                        if let Some(led) = self.config.indicator {
                            led.hold_feedback_stage2();
                        }
                    }
                    break Some(scope);
                }

                if released {
                    info!("BOOT button released early (ignored)");
                    if let Some(led) = self.config.indicator {
                        led.restore_state();
                    }
                    break None;
                }
            };

            if let Some(triggered_scope) = scope {
                return triggered_scope;
            }
        }
    }
}
