//! Smart Intercom - Outbound Domain Reporting Port
//!
//! Defines the protocol-agnostic reporting interface for smart home ecosystems.
//! Outbound adapters (e.g. smart home protocols, network broadcasters) implement this port.

/// Domain-level lock state of the entrance door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    /// Entrance door is secured and locked.
    Locked,
    /// Entrance door is unlocked or unlatched.
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

    /// Report current configured door unlock duration in seconds.
    fn report_unlock_duration(&self, duration_sec: u32);
}

impl<T: IntercomReporter> IntercomReporter for &T {
    fn report_ring(&self, ringing: bool) {
        (*self).report_ring(ringing);
    }

    fn report_lock_state(&self, state: LockState) {
        (*self).report_lock_state(state);
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        (*self).report_unlock_duration(duration_sec);
    }
}

impl<A: IntercomReporter, B: IntercomReporter> IntercomReporter for (A, B) {
    fn report_ring(&self, ringing: bool) {
        self.0.report_ring(ringing);
        self.1.report_ring(ringing);
    }

    fn report_lock_state(&self, state: LockState) {
        self.0.report_lock_state(state);
        self.1.report_lock_state(state);
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        self.0.report_unlock_duration(duration_sec);
        self.1.report_unlock_duration(duration_sec);
    }
}

impl<A: IntercomReporter, B: IntercomReporter, C: IntercomReporter> IntercomReporter for (A, B, C) {
    fn report_ring(&self, ringing: bool) {
        self.0.report_ring(ringing);
        self.1.report_ring(ringing);
        self.2.report_ring(ringing);
    }

    fn report_lock_state(&self, state: LockState) {
        self.0.report_lock_state(state);
        self.1.report_lock_state(state);
        self.2.report_lock_state(state);
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        self.0.report_unlock_duration(duration_sec);
        self.1.report_unlock_duration(duration_sec);
        self.2.report_unlock_duration(duration_sec);
    }
}
