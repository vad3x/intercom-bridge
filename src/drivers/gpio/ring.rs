//! Smart Intercom - GPIO Ring Detection Driver
//!
//! Async ring detection via optocoupler (HCPL-817).
//! Uses Embassy async `wait_for_falling_edge()` + `Timer::after()` for
//! debounce and cooldown.

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use log::info;

use crate::actors::ring::RingDriver;
use crate::config;

/// Configuration for GPIO ring driver
pub struct GpioRingConfig {
    /// Debounce time in milliseconds
    pub debounce_ms: u64,
    /// Cooldown between ring events in milliseconds
    pub cooldown_ms: u64,
}

impl Default for GpioRingConfig {
    fn default() -> Self {
        Self {
            debounce_ms: config::RING_DEBOUNCE_MS,
            cooldown_ms: config::RING_COOLDOWN_MS,
        }
    }
}

/// GPIO-based ring detection driver.
///
/// Detects ring signals from optocoupler input (active LOW, falling edge).
/// Includes debouncing and cooldown logic, all via async Embassy timers.
pub struct GpioRingDriver {
    //TODO: pass pin into config
    pin: Input<'static>,
    config: GpioRingConfig,
    last_ring_time: Instant,
}

impl GpioRingDriver {
    /// Create a new ring driver with the given input pin and configuration.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static, config: GpioRingConfig) -> Self {
        info!(
            "debounce={}ms cooldown={}ms",
            config.debounce_ms, config.cooldown_ms
        );

        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            config,
            last_ring_time: Instant::from_millis(0),
        }
    }
}

impl RingDriver for GpioRingDriver {
    async fn wait_for_ring(&mut self) {
        loop {
            self.pin.wait_for_falling_edge().await;

            // Debounce: wait then re-check
            Timer::after(Duration::from_millis(self.config.debounce_ms)).await;

            if self.pin.is_high() {
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
