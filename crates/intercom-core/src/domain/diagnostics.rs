//! Compatibility re-exports and lock-free intercom telemetry snapshots.

pub use edf_core::domain::diagnostics::{SystemDiagnostics, WifiSsid};

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use crate::domain::intercom::{BusLineState, CallState};

/// Intercom-specific telemetry domain payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntercomState {
    pub door_locked: bool,
    pub door_unlock_duration_sec: u32,
    pub bus_state: BusLineState,
    pub call_state: CallState,
}

/// Update the active door lock latch state reflected in telemetry snapshots.
pub fn update_door_locked(locked: bool) {
    DOOR_LOCKED.store(locked, Ordering::Relaxed);
}

/// Update the active door unlock duration reflected in telemetry snapshots.
pub fn update_door_unlock_duration(duration_sec: u32) {
    DOOR_UNLOCK_DURATION.store(duration_sec, Ordering::Relaxed);
}

/// Update the active 1-hot analog bus relay state reflected in telemetry snapshots.
pub fn update_bus_state(state: BusLineState) {
    BUS_STATE.store(state as u8, Ordering::Relaxed);
}

/// Update the active call/session phase reflected in telemetry snapshots.
pub fn update_call_state(state: CallState) {
    CALL_STATE.store(state as u8, Ordering::Relaxed);
}

/// Retrieve the active intercom domain state for telemetry snapshots.
pub fn current_intercom_state() -> IntercomState {
    IntercomState {
        door_locked: DOOR_LOCKED.load(Ordering::Relaxed),
        door_unlock_duration_sec: DOOR_UNLOCK_DURATION.load(Ordering::Relaxed),
        bus_state: BusLineState::from_u8(BUS_STATE.load(Ordering::Relaxed)),
        call_state: CallState::from_u8(CALL_STATE.load(Ordering::Relaxed)),
    }
}

impl Default for IntercomState {
    fn default() -> Self {
        Self {
            door_locked: true,
            door_unlock_duration_sec: crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC,
            bus_state: BusLineState::Idle,
            call_state: CallState::Idle,
        }
    }
}

static DOOR_LOCKED: AtomicBool = AtomicBool::new(true);
static DOOR_UNLOCK_DURATION: AtomicU32 =
    AtomicU32::new(crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC);
static BUS_STATE: AtomicU8 = AtomicU8::new(BusLineState::Idle as u8);
static CALL_STATE: AtomicU8 = AtomicU8::new(CallState::Idle as u8);

#[cfg(test)]
mod tests {
    use super::{IntercomState, SystemDiagnostics};
    use crate::events::SystemState;

    #[test]
    fn diagnostics_contract_keeps_intercom_as_generic_domain_payload() {
        // Arrange
        let intercom = IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
        };

        // Act
        let diagnostics = SystemDiagnostics::<IntercomState> {
            device_type: "intercom_bridge",
            state: SystemState::Operational,
            domain: intercom,
            uptime_sec: 0,
            wifi_ssid: None,
            wifi_rssi_dbm: None,
            ip: None,
            mac: [0; 6],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 0,
            total_heap_bytes: 0,
            available_ecosystems: &[],
            active_ecosystem: None,
            version: "0.1.0",
            git_commit: "test",
            build_date: "test",
        };

        // Assert
        assert_eq!(diagnostics.domain, intercom);
        assert!(diagnostics.domain.door_locked);
        assert_eq!(diagnostics.domain.door_unlock_duration_sec, 5);
    }

    #[test]
    fn telemetry_snapshot_tracks_bus_and_call_state() {
        // Arrange
        let expected_bus = super::BusLineState::Talk;
        let expected_call = super::CallState::Talking;

        // Act
        super::update_bus_state(expected_bus);
        super::update_call_state(expected_call);
        let snapshot = super::current_intercom_state();
        super::update_bus_state(super::BusLineState::Idle);
        super::update_call_state(super::CallState::Idle);

        // Assert
        assert_eq!(snapshot.bus_state, expected_bus);
        assert_eq!(snapshot.call_state, expected_call);
    }
}
