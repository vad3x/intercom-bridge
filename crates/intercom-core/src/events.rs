//! Smart Intercom - Lifecycle Event System
//!
//! Embassy `Watch` channel for continuous system lifecycle state.
//! Replaces legacy broadcast event buses with typed, bounded state observation.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};

/// System lifecycle states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemState {
    /// Starting up, hardware init in progress
    Initializing,
    /// Core services running, hardware initialized
    Initialized,
    /// Network stack is establishing connection to ecosystem controller
    Connecting,
    /// Connected to network and controller session active
    Connected,
    /// All endpoints and services fully online and operational
    Ready,
    /// Firmware OTA update in progress (flashing flash memory)
    Updating,
    /// Factory reset triggered (erasing persistent credentials, rebooting)
    FactoryReset,
    /// Something went wrong
    Error,
}

// ============================================================================
// Lifecycle Watch type aliases & sender
// ============================================================================

/// Maximum concurrent receivers for system lifecycle state.
pub const STATE_WATCH_RECEIVERS: usize = 3;

/// Continuous lifecycle state watch signal (zero-allocation, zero queue overflow).
pub type SystemStateWatch = Watch<CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Watch receiver for observing system lifecycle transitions.
pub type SystemStateReceiver<'a> =
    WatchReceiver<'a, CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Lightweight, Copy handle for sending system lifecycle state updates.
#[derive(Clone, Copy)]
pub struct SystemStateSender<'a>(&'a SystemStateWatch);

impl<'a> SystemStateSender<'a> {
    /// Create a new sender handle wrapping a `SystemStateWatch`.
    pub const fn new(watch: &'a SystemStateWatch) -> Self {
        Self(watch)
    }

    /// Query the current system lifecycle state, if available.
    pub fn current_state(&self) -> Option<SystemState> {
        self.0.try_get()
    }

    /// Send a new system lifecycle state.
    pub fn send(&self, state: SystemState) {
        self.0.sender().send(state);
    }

    /// Convenience helper to transition to `SystemState::Initialized`.
    pub fn initialized(&self) {
        self.send(SystemState::Initialized);
    }

    /// Convenience helper to transition to `SystemState::Connecting`.
    pub fn connecting(&self) {
        self.send(SystemState::Connecting);
    }

    /// Convenience helper to transition to `SystemState::Connected`.
    pub fn connected(&self) {
        self.send(SystemState::Connected);
    }

    /// Convenience helper to transition to `SystemState::Ready`.
    pub fn ready(&self) {
        self.send(SystemState::Ready);
    }

    /// Convenience helper to transition to `SystemState::Updating`.
    pub fn updating(&self) {
        self.send(SystemState::Updating);
    }

    /// Convenience helper to transition to `SystemState::FactoryReset`.
    pub fn factory_reset(&self) {
        self.send(SystemState::FactoryReset);
    }

    /// Convenience helper to transition to `SystemState::Error`.
    pub fn error(&self) {
        self.send(SystemState::Error);
    }
}
