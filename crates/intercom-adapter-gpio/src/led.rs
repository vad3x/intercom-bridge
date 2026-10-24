//! Intercom access-event wrappers around the shared foundation LED driver.

use esp_hal::gpio::OutputPin;
use intercom_core::events::SystemStateReceiver;
use intercom_core::ports::reporter::{IntercomReporter, LockState};

use crate::config::{self, IntercomLedProfile};

/// GPIO-based active-low LED indicator driver.
///
/// Lifecycle patterns come from [`IntercomLedProfile`]; Intercom ring and
/// unlock feedback is layered on through the shared generic blink command.
pub struct GpioLedDriver {
    inner: edf_gpio::led::GpioLedDriver<IntercomLedProfile>,
}

impl GpioLedDriver {
    /// Create a new LED driver with the given output pin.
    pub fn new(pin: impl OutputPin + 'static) -> Self {
        Self {
            inner: edf_gpio::led::GpioLedDriver::<IntercomLedProfile>::with_profile(pin),
        }
    }

    /// Driver event loop — observes lifecycle states and drives the hardware pin.
    pub async fn run(&self, state_rx: SystemStateReceiver<'_>) {
        self.inner.run(state_rx).await;
    }

    /// Indicate that the reset/BOOT button is currently being pressed.
    pub fn hold_feedback_press(&self) {
        self.inner.hold_feedback_press();
    }

    /// Indicate that the ecosystem-reset hold threshold has been reached.
    pub fn hold_feedback_stage1(&self) {
        self.inner.hold_feedback_stage1();
    }

    /// Indicate that the factory-reset hold threshold has been reached.
    pub fn hold_feedback_stage2(&self) {
        self.inner.hold_feedback_stage2();
    }

    /// Re-apply the current lifecycle state after a button is released early.
    pub fn restore_state(&self) {
        self.inner.restore_state();
    }

    pub(crate) fn inner(&self) -> &edf_gpio::led::GpioLedDriver<IntercomLedProfile> {
        &self.inner
    }
}

impl IntercomReporter for GpioLedDriver {
    fn report_ring(&self, ringing: bool) {
        if ringing {
            self.inner.blink(
                config::RING_BLINK_MS,
                config::RING_BLINK_MS,
                Some(config::RING_BLINK_COUNT),
            );
        }
    }

    fn report_lock_state(&self, state: LockState) {
        if matches!(state, LockState::Unlocked) {
            self.inner.blink(
                config::UNLOCK_BLINK_MS,
                config::UNLOCK_BLINK_MS,
                Some(config::UNLOCK_BLINK_COUNT),
            );
        }
    }

    fn report_unlock_duration(&self, _duration_sec: u32) {
        // LED indicator does not change based on duration setting
    }
}
