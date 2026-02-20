//! Smart Intercom - GPIO Door Unlock Driver
//!
//! Async door unlock via PhotoMOS relay (AQY212EHA).
//! Uses Embassy async `Timer::after()` for the pulse duration.

use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output, OutputConfig};
use log::{info, warn};

use crate::actors::door::DoorDriver;
use crate::config;

/// Configuration for GPIO door driver
pub struct GpioDoorConfig {
    /// Duration of unlock pulse in milliseconds
    pub unlock_pulse_ms: u64,
}

impl Default for GpioDoorConfig {
    fn default() -> Self {
        Self {
            unlock_pulse_ms: config::UNLOCK_PULSE_MS,
        }
    }
}

/// GPIO-based door unlock driver.
///
/// Controls a PhotoMOS relay to release the door latch.
/// Produces a timed HIGH pulse, then returns to LOW.
pub struct GpioDoorDriver {
    pin: Output<'static>,
    config: GpioDoorConfig,
    unlocking: bool,
}

impl GpioDoorDriver {
    /// Create a new door driver with the given output pin and configuration.
    pub fn new(pin: impl esp_hal::gpio::OutputPin + 'static, config: GpioDoorConfig) -> Self {
        info!("pulse={}ms", config.unlock_pulse_ms);

        Self {
            pin: Output::new(pin, Level::Low, OutputConfig::default()),
            config,
            unlocking: false,
        }
    }
}

impl DoorDriver for GpioDoorDriver {
    async fn trigger_unlock(&mut self) {
        if self.unlocking {
            warn!("Unlock already in progress, ignoring");
            return;
        }

        info!("Unlock pulse start ({}ms)", self.config.unlock_pulse_ms);
        self.unlocking = true;

        self.pin.set_high();
        Timer::after(Duration::from_millis(self.config.unlock_pulse_ms)).await;
        self.pin.set_low();

        self.unlocking = false;
        info!("Unlock pulse end");
    }
}
