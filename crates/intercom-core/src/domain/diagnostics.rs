//! Compatibility re-exports for shared diagnostics contracts.

pub use edf_core::domain::diagnostics::{SystemDiagnostics, WifiSsid};

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static DOOR_LOCKED: AtomicBool = AtomicBool::new(true);
static DOOR_UNLOCK_DURATION: AtomicU32 =
    AtomicU32::new(crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC);

/// Update the active door lock latch state reflected in telemetry snapshots.
pub fn update_door_locked(locked: bool) {
    DOOR_LOCKED.store(locked, Ordering::Relaxed);
}

/// Update the active door unlock duration reflected in telemetry snapshots.
pub fn update_door_unlock_duration(duration_sec: u32) {
    DOOR_UNLOCK_DURATION.store(duration_sec, Ordering::Relaxed);
}

/// Retrieve the active intercom domain state for telemetry snapshots.
pub fn current_intercom_state() -> IntercomState {
    IntercomState {
        door_locked: DOOR_LOCKED.load(Ordering::Relaxed),
        door_unlock_duration_sec: DOOR_UNLOCK_DURATION.load(Ordering::Relaxed),
    }
}

/// Intercom-specific telemetry domain payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntercomState {
    pub door_locked: bool,
    pub door_unlock_duration_sec: u32,
}

impl Default for IntercomState {
    fn default() -> Self {
        Self {
            door_locked: true,
            door_unlock_duration_sec: crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IntercomState, SystemDiagnostics};
    use crate::events::SystemState;

    #[test]
    fn diagnostics_contract_keeps_intercom_as_generic_domain_payload() {
        let intercom = IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
        };
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

        assert_eq!(diagnostics.domain, intercom);
        assert!(diagnostics.domain.door_locked);
        assert_eq!(diagnostics.domain.door_unlock_duration_sec, 5);
    }
}
