//! Outbound port traits for Smart Intercom core.
//!
//! In Hexagonal Architecture, ports define the contracts required by the core
//! application. Adapters (e.g. GPIO, Matter) implement these traits.



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
}

/// Access control port.
///
/// Bridges core access control rules (auto-relock, occupancy hold)
/// to the external ecosystem or cluster handlers.
pub trait AccessControlDriver {
    /// Update occupancy state (ring detected / hold expired).
    fn set_occupancy(&self, occupied: bool);

    /// Lock the door (auto-relock timer expired).
    fn lock(&self);
}

/// Factory reset detection port.
///
/// Implementors detect factory reset trigger conditions (e.g. button hold).
pub trait FactoryResetDriver {
    /// Block until the factory reset trigger condition is met.
    async fn wait_for_factory_reset(&mut self);
}
