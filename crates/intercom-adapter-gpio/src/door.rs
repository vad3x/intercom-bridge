//! Smart Intercom - GPIO Door Unlock Driver
//!
//! Async door unlock via PhotoMOS relay (AQY212EHA).
//! Uses Embassy async `Timer::after()` for the pulse duration.

use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output, OutputConfig};
use crate::config;
use intercom_core::ports::DoorDriver;
use log::{info, warn};

/// GPIO-based door unlock driver.
///
/// Controls a PhotoMOS relay to release the door latch.
/// Produces a timed HIGH pulse, then returns to LOW.
pub struct GpioDoorDriver {
    config: GpioDoorConfig,
    unlocking: bool,
}

impl GpioDoorDriver {
    /// Create a new door driver with the given configuration.
    pub fn new(config: GpioDoorConfig) -> Self {
        info!("pulse={}ms", config.unlock_pulse_ms);

        Self {
            config,
            unlocking: false,
        }
    }
}

/// Configuration for GPIO door driver
pub struct GpioDoorConfig {
    /// Output pin (PhotoMOS relay control, active HIGH)
    pub pin: Output<'static>,
    /// Duration of unlock pulse in milliseconds
    pub unlock_pulse_ms: u64,
}

impl GpioDoorConfig {
    /// Create a config with the given output pin and default timing values.
    pub fn new(pin: impl esp_hal::gpio::OutputPin + 'static) -> Self {
        Self {
            pin: Output::new(pin, Level::Low, OutputConfig::default()),
            unlock_pulse_ms: config::UNLOCK_PULSE_MS,
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

        self.config.pin.set_high();
        Timer::after(Duration::from_millis(self.config.unlock_pulse_ms)).await;
        self.config.pin.set_low();

        self.unlocking = false;
        info!("Unlock pulse end");
    }

    fn set_unlock_duration_ms(&mut self, duration_ms: u32) {
        info!("Updating unlock pulse duration: {}ms", duration_ms);
        self.config.unlock_pulse_ms = duration_ms as u64;
    }
}
