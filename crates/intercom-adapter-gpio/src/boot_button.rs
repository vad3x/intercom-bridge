//! Intercom BOOT button wrapper around the shared foundation reset driver.

use intercom_core::ports::{ResetScope, ResetTriggerDriver};

use crate::led::GpioLedDriver;

/// GPIO-based BOOT button driver for dual-stage system reset.
pub struct GpioBootButtonDriver {
    inner: edf_gpio::boot_button::GpioBootButtonDriver,
}

impl GpioBootButtonDriver {
    /// Create a new reset-button driver from the Intercom configuration.
    pub fn new(config: GpioBootButtonConfig) -> Self {
        Self {
            inner: edf_gpio::boot_button::GpioBootButtonDriver::new(config.inner),
        }
    }
}

/// Configuration for the BOOT button reset driver.
pub struct GpioBootButtonConfig {
    inner: edf_gpio::boot_button::GpioBootButtonConfig,
}

impl GpioBootButtonConfig {
    /// Create a config with the given pin and the foundation default hold times.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            inner: edf_gpio::boot_button::GpioBootButtonConfig::new(pin),
        }
    }

    /// Attach the LED indicator used for live hold feedback.
    pub fn with_indicator(mut self, indicator: &'static GpioLedDriver) -> Self {
        self.inner = self.inner.with_indicator(indicator.inner());
        self
    }
}

impl ResetTriggerDriver for GpioBootButtonDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        self.inner.wait_for_reset().await
    }
}
