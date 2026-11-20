//! Smart Intercom - Lifecycle Event System
//!
//! Embassy `Watch` channel for continuous system lifecycle state.

pub use edf_core::events::{
    SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch, STATE_WATCH_RECEIVERS,
};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};

use crate::domain::intercom::{CallState, SessionOwner};
use crate::ports::reporter::{IntercomReporter, LockState};

pub const CALL_STATE_WATCH_RECEIVERS: usize = 3;

/// Current call phase and owner, published together for media consumers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub call_state: CallState,
    pub active_owner: Option<SessionOwner>,
}

pub type CallStateWatch =
    Watch<CriticalSectionRawMutex, SessionSnapshot, CALL_STATE_WATCH_RECEIVERS>;
pub type CallStateReceiver<'a> =
    WatchReceiver<'a, CriticalSectionRawMutex, SessionSnapshot, CALL_STATE_WATCH_RECEIVERS>;

#[derive(Clone, Copy)]
pub struct CallStateBroadcastReporter<'a> {
    watch: &'a CallStateWatch,
}

impl<'a> CallStateBroadcastReporter<'a> {
    pub const fn new(watch: &'a CallStateWatch) -> Self {
        Self { watch }
    }
}

impl IntercomReporter for CallStateBroadcastReporter<'_> {
    fn report_ring(&self, _ringing: bool) {}
    fn report_lock_state(&self, _state: LockState) {}
    fn report_unlock_duration(&self, _duration_sec: u32) {}
    fn report_session_state(&self, call_state: CallState, active_owner: Option<SessionOwner>) {
        self.watch.sender().send(SessionSnapshot {
            call_state,
            active_owner,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_sender_updates_consumer_watch() {
        // Arrange
        let watch = SystemStateWatch::new();

        // Act
        edf_core::SystemStateSender::new(&watch).operational();

        // Assert
        assert_eq!(watch.try_get(), Some(SystemState::Operational));
    }

    #[test]
    fn foundation_watch_capacity_matches_intercom_consumers() {
        // Arrange
        const EXPECTED_RECEIVERS: usize = 5;

        // Act
        let capacity = STATE_WATCH_RECEIVERS;

        // Assert
        assert_eq!(capacity, EXPECTED_RECEIVERS);
    }
}
