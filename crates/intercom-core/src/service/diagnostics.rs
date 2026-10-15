//! Diagnostics application service aggregating telemetry into system snapshots.

use crate::domain::diagnostics::SystemDiagnostics;
use crate::events::{SystemState, SystemStateWatch};
use crate::ports::diagnostics::HardwareInfoDriver;
use crate::ports::ecosystem::EcosystemInfoDriver;

/// Service responsible for producing point-in-time system diagnostics snapshots.
pub struct DiagnosticsService<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver = ()> {
    hardware: &'a H,
    ecosystem: &'a E,
    state_watch: &'a SystemStateWatch,
}

impl<'a, H: HardwareInfoDriver> DiagnosticsService<'a, H, ()> {
    /// Create a new DiagnosticsService instance without a dedicated smart home ecosystem driver.
    pub fn new(hardware: &'a H, state_watch: &'a SystemStateWatch) -> Self {
        Self {
            hardware,
            ecosystem: &(),
            state_watch,
        }
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver> DiagnosticsService<'a, H, E> {
    /// Create a new DiagnosticsService instance with a dedicated smart home ecosystem driver.
    pub fn with_ecosystem(
        hardware: &'a H,
        ecosystem: &'a E,
        state_watch: &'a SystemStateWatch,
    ) -> Self {
        Self {
            hardware,
            ecosystem,
            state_watch,
        }
    }

    /// Aggregate current domain state, ecosystem status, and hardware telemetry into a snapshot.
    pub fn snapshot(&self) -> SystemDiagnostics {
        let state = self.state_watch.try_get().unwrap_or(SystemState::Initializing);
        let door_locked = true;

        SystemDiagnostics {
            state,
            door_locked,
            uptime_sec: self.hardware.uptime_secs(),
            wifi_rssi_dbm: self.hardware.wifi_rssi(),
            ip: self.hardware.ip_address(),
            mac: self.hardware.mac_address(),
            active_slot: self.hardware.active_slot(),
            slot_valid: self.hardware.is_slot_valid(),
            free_heap_bytes: self.hardware.free_heap_bytes(),
            total_heap_bytes: self.hardware.total_heap_bytes(),
            ecosystem_paired: self.ecosystem.is_paired(),
            ecosystem_protocol: self.ecosystem.protocol_name(),
            version: self.hardware.firmware_version(),
            git_commit: self.hardware.git_commit(),
            build_date: self.hardware.build_date(),
        }
    }
}
