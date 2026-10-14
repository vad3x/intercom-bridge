//! Outbound port traits for Smart Intercom core.
//!
//! In Hexagonal Architecture, ports define abstract capability contracts
//! required by the core domain application. Edge adapters implement these traits.

pub mod inbound;
pub mod reporter;
pub mod reset;

pub use inbound::{
    AccessCommand, AccessControllerHandle, CommandChannel, CommandReceiver, CommandSender,
};
pub use reporter::{IntercomReporter, LockState};
pub use reset::SystemResetDriver;

/// Door unlock port.
///
/// Implementors control the physical entrance door unlock mechanism
/// (e.g. electric strike, magnetic lock, motorized latch).
pub trait DoorDriver {
    /// Trigger an unlock pulse. Returns when the actuation completes.
    async fn trigger_unlock(&mut self);
}

/// Ring detection port.
///
/// Implementors detect incoming visitor call signals from the intercom system.
pub trait RingDriver {
    /// Block until a valid, debounced ring event occurs.
    async fn wait_for_ring(&mut self);
}

/// Factory reset detection port.
///
/// Implementors detect factory reset trigger conditions (e.g. maintenance input hold).
pub trait FactoryResetDriver {
    /// Block until the factory reset trigger condition is met.
    async fn wait_for_factory_reset(&mut self);
}
