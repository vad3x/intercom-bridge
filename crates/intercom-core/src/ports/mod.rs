//! Outbound port traits for Smart Intercom core.
//!
//! In Hexagonal Architecture, ports define abstract capability contracts
//! required by the core domain application. Edge adapters implement these traits.

pub mod diagnostics;
pub mod ecosystem;
pub mod inbound;
pub mod ota;
pub mod reporter;
pub mod reset;

pub use diagnostics::HardwareInfoDriver;
pub use ecosystem::EcosystemInfoDriver;
pub use inbound::{
    AccessCommand, AccessControllerHandle, CommandChannel, CommandReceiver, CommandSender,
};
pub use ota::{OtaDriverError, OtaFlashDriver, OtaStream};
pub use edf_core::ports::ota as ota_edf;
pub use reporter::{IntercomReporter, LockState};
pub use reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};

/// Door unlock port.
///
/// Implementors control the physical entrance door unlock mechanism
/// (e.g. electric strike, magnetic lock, motorized latch).
pub trait DoorDriver {
    /// Trigger an unlock pulse. Returns when the actuation completes.
    async fn trigger_unlock(&mut self);

    /// Set the physical unlock pulse duration in milliseconds.
    fn set_unlock_duration_ms(&mut self, _duration_ms: u32) {}
}

/// Ring detection port.
///
/// Implementors detect incoming visitor call signals from the intercom system.
pub trait RingDriver {
    /// Block until a valid, debounced ring event occurs.
    async fn wait_for_ring(&mut self);
}
