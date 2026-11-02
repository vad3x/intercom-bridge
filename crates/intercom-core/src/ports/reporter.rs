//! Smart Intercom - Outbound Domain Reporting Port

use crate::domain::intercom::{BusLineState, CallState, SessionOwner};

/// Outbound capability: notifies external smart home ecosystems and telemetry of intercom events.
pub trait IntercomReporter {
    fn report_ring(&self, ringing: bool);
    fn report_lock_state(&self, state: LockState);
    fn report_unlock_duration(&self, duration_sec: u32);
    fn report_bus_state(&self, _state: BusLineState) {}
    fn report_call_state(&self, _state: CallState) {}
    fn report_session_preempted(&self, _owner: SessionOwner) {}
}

/// Domain-level lock state of the entrance door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    Locked,
    Unlocked,
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

    fn report_bus_state(&self, state: BusLineState) {
        (*self).report_bus_state(state);
    }

    fn report_call_state(&self, state: CallState) {
        (*self).report_call_state(state);
    }

    fn report_session_preempted(&self, owner: SessionOwner) {
        (*self).report_session_preempted(owner);
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

    fn report_bus_state(&self, state: BusLineState) {
        self.0.report_bus_state(state);
        self.1.report_bus_state(state);
    }

    fn report_call_state(&self, state: CallState) {
        self.0.report_call_state(state);
        self.1.report_call_state(state);
    }

    fn report_session_preempted(&self, owner: SessionOwner) {
        self.0.report_session_preempted(owner);
        self.1.report_session_preempted(owner);
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

    fn report_bus_state(&self, state: BusLineState) {
        self.0.report_bus_state(state);
        self.1.report_bus_state(state);
        self.2.report_bus_state(state);
    }

    fn report_call_state(&self, state: CallState) {
        self.0.report_call_state(state);
        self.1.report_call_state(state);
        self.2.report_call_state(state);
    }

    fn report_session_preempted(&self, owner: SessionOwner) {
        self.0.report_session_preempted(owner);
        self.1.report_session_preempted(owner);
        self.2.report_session_preempted(owner);
    }
}
