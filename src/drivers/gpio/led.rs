//! Smart Intercom - GPIO LED Indicator Driver
//!
//! Async LED control for status indication (active-low red LED).
//! Replaces the C++ GpioLedDriver (output + FreeRTOS blink timer)
//! with Embassy async blink loops.

use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output, OutputConfig};
use log::info;

use crate::actors::indicator::IndicatorDriver;
use crate::events::SystemState;

/// Configuration for GPIO LED driver
pub struct GpioLedConfig {
    /// Blink ON duration for ring indication (ms)
    pub ring_blink_on_ms: u64,
    /// Blink OFF duration for ring indication (ms)
    pub ring_blink_off_ms: u64,
    /// Number of blinks for ring indication
    pub ring_blink_count: u8,
    /// Blink ON duration for unlock indication (ms)
    pub unlock_blink_on_ms: u64,
    /// Blink OFF duration for unlock indication (ms)
    pub unlock_blink_off_ms: u64,
    /// Number of blinks for unlock indication
    pub unlock_blink_count: u8,
}

impl Default for GpioLedConfig {
    fn default() -> Self {
        Self {
            ring_blink_on_ms: 100,
            ring_blink_off_ms: 100,
            ring_blink_count: 2,
            unlock_blink_on_ms: 200,
            unlock_blink_off_ms: 200,
            unlock_blink_count: 2,
        }
    }
}

/// GPIO-based LED indicator driver (active-low red LED).
///
/// LED is ON when GPIO is LOW, OFF when GPIO is HIGH.
pub struct GpioLedDriver {
    pin: Output<'static>,
    config: GpioLedConfig,
}

impl GpioLedDriver {
    /// Create a new LED driver with the given output pin and configuration.
    pub fn new(pin: impl esp_hal::gpio::OutputPin + 'static, config: GpioLedConfig) -> Self {
        info!("GpioLedDriver created (active-low)");

        Self {
            pin: Output::new(pin, Level::High, OutputConfig::default()), // Start OFF (active-low)
            config,
        }
    }

    /// Turn LED on (active-low: GPIO LOW)
    fn led_on(&mut self) {
        self.pin.set_low();
    }

    /// Turn LED off (active-low: GPIO HIGH)
    fn led_off(&mut self) {
        self.pin.set_high();
    }

    /// Blink the LED `count` times with given ON/OFF durations.
    async fn blink_pattern(&mut self, count: u8, on_ms: u64, off_ms: u64) {
        for _ in 0..count {
            self.led_on();
            Timer::after(Duration::from_millis(on_ms)).await;
            self.led_off();
            Timer::after(Duration::from_millis(off_ms)).await;
        }
        // Return to steady on after blink sequence
        self.led_on();
    }
}

impl IndicatorDriver for GpioLedDriver {
    async fn set_state(&mut self, state: SystemState) {
        match state {
            SystemState::Initializing => {
                info!("LED: initializing");
                self.led_on();
            }
            SystemState::Ready => {
                info!("LED: ready");
                self.led_on();
            }
            SystemState::Error => {
                info!("LED: error");
                self.led_on();
            }
        }
    }

    async fn indicate_ring(&mut self) {
        info!("LED: ring indication");
        self.blink_pattern(
            self.config.ring_blink_count,
            self.config.ring_blink_on_ms,
            self.config.ring_blink_off_ms,
        )
        .await;
    }

    async fn indicate_unlock(&mut self) {
        info!("LED: unlock indication");
        self.blink_pattern(
            self.config.unlock_blink_count,
            self.config.unlock_blink_on_ms,
            self.config.unlock_blink_off_ms,
        )
        .await;
    }

    async fn off(&mut self) {
        self.led_off();
    }
}

/// Stub indicator driver for boards without LEDs.
///
/// C++ equivalent: `StubLedDriver`
pub struct StubIndicatorDriver;

impl IndicatorDriver for StubIndicatorDriver {
    async fn set_state(&mut self, _state: SystemState) {}
    async fn indicate_ring(&mut self) {}
    async fn indicate_unlock(&mut self) {}
    async fn off(&mut self) {}
}
