//! Smart Intercom - GPIO Ring Detection Driver
//!
//! Async ring detection via optocoupler (HCPL-817).
//! Uses Embassy async `wait_for_falling_edge()` + `Timer::after()` for
//! debounce and cooldown.

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use crate::config;
use intercom_core::ports::RingDriver;
use log::info;

/// GPIO-based ring detection driver.
///
/// Detects ring signals from optocoupler input (active LOW, falling edge).
/// Includes debouncing and cooldown logic, all via async Embassy timers.
pub struct GpioRingDriver {
    config: GpioRingConfig,
    last_ring_time: Instant,
}

impl GpioRingDriver {
    /// Create a new ring driver with the given configuration.
    pub fn new(config: GpioRingConfig) -> Self {
        info!(
            "debounce={}ms cooldown={}ms",
            config.debounce_ms, config.cooldown_ms
        );

        Self {
            config,
            last_ring_time: Instant::from_millis(0),
        }
    }
}

/// Configuration for GPIO ring driver
pub struct GpioRingConfig {
    /// Input pin (optocoupler output, active LOW)
    pub pin: Input<'static>,
    /// Debounce time in milliseconds
    pub debounce_ms: u64,
    /// Cooldown between ring events in milliseconds
    pub cooldown_ms: u64,
}

impl GpioRingConfig {
    /// Create a config with the given input pin and default timing values.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            debounce_ms: config::RING_DEBOUNCE_MS,
            cooldown_ms: config::RING_COOLDOWN_MS,
        }
    }
}

impl RingDriver for GpioRingDriver {
    async fn wait_for_ring(&mut self) {
        loop {
            self.config.pin.wait_for_falling_edge().await;

            // Debounce: wait then re-check
            Timer::after(Duration::from_millis(self.config.debounce_ms)).await;

            if self.config.pin.is_high() {
                continue;
            }

            // Cooldown check
            let now = Instant::now();
            if now.duration_since(self.last_ring_time)
                < Duration::from_millis(self.config.cooldown_ms)
            {
                continue;
            }

            self.last_ring_time = now;
            info!("=== RING DETECTED ===");
            return;
        }
    }
}
