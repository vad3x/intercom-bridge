//! Smart Intercom - Outbound Domain Reporting Port
//!
//! Defines the protocol-agnostic reporting interface for smart home ecosystems.
//! Outbound adapters (e.g. smart home protocols, network broadcasters) implement this port.

/// Domain-level lock state of the entrance door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    /// Door strike relay is inactive, door is secured.
    Locked,
    /// Door strike relay is active or unlocked by user request.
    Unlocked,
}

/// Outbound capability: notifies external smart home ecosystems of intercom events.
///
/// Decoupled from protocol details (e.g. device clusters, attributes, network messages).
pub trait IntercomReporter {
    /// Report whether the intercom chime/ring is actively ringing.
    fn report_ring(&self, ringing: bool);

    /// Report current lock latch state.
    fn report_lock_state(&self, state: LockState);
}
