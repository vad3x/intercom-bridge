//! Outbound port traits for Smart Intercom core.
//!
//! In Hexagonal Architecture, ports define the contracts required by the core
//! application. Adapters (e.g. GPIO, Matter) implement these traits.

pub mod inbound;
pub mod reporter;
pub mod reset;

pub use inbound::{
    AccessCommand, AccessControllerHandle, CommandChannel, CommandReceiver, CommandSender,
};
pub use reporter::{IntercomReporter, LockState};
pub use reset::SystemResetDriver;

use crate::config::{
    RING_BLINK_COUNT, RING_BLINK_MS, UNLOCK_BLINK_COUNT, UNLOCK_BLINK_MS,
};

/// Door unlock port.
///
/// Implementors control the physical door unlock mechanism (e.g. relay, strike).
pub trait DoorDriver {
    /// Trigger a timed unlock pulse. Returns when the pulse completes.
    async fn trigger_unlock(&mut self);
}

/// Ring detection port.
///
/// Implementors detect ring signals from the intercom hardware (e.g. optocoupler).
pub trait RingDriver {
    /// Block until a valid, debounced ring event occurs.
    async fn wait_for_ring(&mut self);
}

/// Visual/audio indicator feedback port.
///
/// Implementors control feedback hardware (e.g. status LED, buzzer).
pub trait IndicatorDriver {
    /// Turn the indicator on (steady state).
    fn solid(&self);

    /// Start blinking with the given on/off durations.
    ///
    /// - `count: None` — blink continuously until the next command.
    /// - `count: Some(n)` — blink `n` times, then return to steady on.
    fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>);

    /// Start a burst pattern: `burst_count` quick blinks, then a long pause.
    fn burst(&self, on_ms: u64, off_ms: u64, burst_count: u8, pause_ms: u64);

    /// Driver event loop — processes commands and drives hardware.
    /// Must run concurrently with the actor.
    async fn run(&self);

    /// Indicate ring detection with a transient blink pattern.
    fn indicate_ring(&self) {
        self.blink(RING_BLINK_MS, RING_BLINK_MS, Some(RING_BLINK_COUNT));
    }

    /// Indicate unlock dispatch with a transient blink pattern.
    fn indicate_unlock(&self) {
        self.blink(UNLOCK_BLINK_MS, UNLOCK_BLINK_MS, Some(UNLOCK_BLINK_COUNT));
    }
}

impl<T: IndicatorDriver> IndicatorDriver for &T {
    fn solid(&self) {
        (*self).solid();
    }

    fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>) {
        (*self).blink(on_ms, off_ms, count);
    }

    fn burst(&self, on_ms: u64, off_ms: u64, burst_count: u8, pause_ms: u64) {
        (*self).burst(on_ms, off_ms, burst_count, pause_ms);
    }

    fn indicate_ring(&self) {
        (*self).indicate_ring();
    }

    fn indicate_unlock(&self) {
        (*self).indicate_unlock();
    }

    async fn run(&self) {
        (*self).run().await;
    }
}

/// Factory reset detection port.
///
/// Implementors detect factory reset trigger conditions (e.g. button hold).
pub trait FactoryResetDriver {
    /// Block until the factory reset trigger condition is met.
    async fn wait_for_factory_reset(&mut self);
}
