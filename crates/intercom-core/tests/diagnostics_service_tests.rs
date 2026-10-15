//! Unit tests for DiagnosticsService, SystemDiagnostics domain model, HardwareInfoDriver, and EcosystemInfoDriver.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::domain::diagnostics::SystemDiagnostics;
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::service::diagnostics::DiagnosticsService;

pub struct MockHardwareInfoDriver {
    pub uptime_secs: u64,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub wifi_ssid: Option<intercom_core::domain::diagnostics::WifiSsid>,
    pub wifi_rssi: Option<i8>,
    pub ip_address: Option<[u8; 4]>,
    pub mac_address: [u8; 6],
    pub active_slot: &'static str,
    pub is_slot_valid: bool,
    pub firmware_version: &'static str,
    pub git_commit: &'static str,
    pub build_date: &'static str,
}

impl MockHardwareInfoDriver {
    pub fn new() -> Self {
        Self {
            uptime_secs: 12345,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("HomeNet-IoT"),
            wifi_rssi: Some(-65),
            ip_address: Some([192, 168, 1, 100]),
            mac_address: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            is_slot_valid: true,
            firmware_version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    }
}

impl HardwareInfoDriver for MockHardwareInfoDriver {
    fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }

    fn free_heap_bytes(&self) -> usize {
        self.free_heap_bytes
    }

    fn total_heap_bytes(&self) -> usize {
        self.total_heap_bytes
    }

    fn wifi_ssid(&self) -> Option<intercom_core::domain::diagnostics::WifiSsid> {
        self.wifi_ssid
    }

    fn wifi_rssi(&self) -> Option<i8> {
        self.wifi_rssi
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        self.ip_address
    }

    fn mac_address(&self) -> [u8; 6] {
        self.mac_address
    }

    fn active_slot(&self) -> &'static str {
        self.active_slot
    }

    fn is_slot_valid(&self) -> bool {
        self.is_slot_valid
    }

    fn firmware_version(&self) -> &'static str {
        self.firmware_version
    }

    fn git_commit(&self) -> &'static str {
        self.git_commit
    }

    fn build_date(&self) -> &'static str {
        self.build_date
    }
}

pub struct MockEcosystemInfoDriver {
    pub paired: bool,
    pub protocol: &'static str,
}

impl MockEcosystemInfoDriver {
    pub fn new(paired: bool, protocol: &'static str) -> Self {
        Self { paired, protocol }
    }
}

impl EcosystemInfoDriver for MockEcosystemInfoDriver {
    fn is_paired(&self) -> bool {
        self.paired
    }

    fn protocol_name(&self) -> &'static str {
        self.protocol
    }
}

#[test]
fn diagnostics_service_snapshot_with_default_ecosystem() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.ready();
    let service = DiagnosticsService::new(&hardware, &state_watch);

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            state: SystemState::Ready,
            door_locked: true,
            uptime_sec: 12345,
            wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            ecosystem_paired: false,
            ecosystem_protocol: "None",
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}

#[test]
fn diagnostics_service_snapshot_with_explicit_ecosystem() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let ecosystem = MockEcosystemInfoDriver::new(true, "Matter");
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.connected();
    let service = DiagnosticsService::with_ecosystem(&hardware, &ecosystem, &state_watch);

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            state: SystemState::Connected,
            door_locked: true,
            uptime_sec: 12345,
            wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            ecosystem_paired: true,
            ecosystem_protocol: "Matter",
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}
