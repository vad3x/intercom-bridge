//! Unit tests for DiagnosticsService, SystemDiagnostics, EcosystemStatus, HardwareInfoDriver, and EcosystemInfoDriver.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::domain::diagnostics::{IntercomState, SystemDiagnostics, WifiSsid};
use intercom_core::domain::ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue,
};
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::service::diagnostics::DiagnosticsService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestEcosystem {
    ProtoA,
    ProtoB,
}

impl EcosystemType for TestEcosystem {
    fn id(&self) -> EcosystemId {
        match self {
            Self::ProtoA => EcosystemId::new("ProtoA"),
            Self::ProtoB => EcosystemId::new("ProtoB"),
        }
    }
}

const TEST_AVAILABLE_ECOSYSTEMS: &[TestEcosystem] = &[TestEcosystem::ProtoA, TestEcosystem::ProtoB];

pub struct MockHardwareInfoDriver {
    pub device_type: &'static str,
    pub uptime_secs: u64,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub wifi_ssid: Option<WifiSsid>,
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
            device_type: "intercom_bridge",
            uptime_secs: 12345,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
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

impl Default for MockHardwareInfoDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareInfoDriver for MockHardwareInfoDriver {
    fn device_type(&self) -> &'static str {
        self.device_type
    }

    fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }

    fn free_heap_bytes(&self) -> usize {
        self.free_heap_bytes
    }

    fn total_heap_bytes(&self) -> usize {
        self.total_heap_bytes
    }

    fn wifi_ssid(&self) -> Option<WifiSsid> {
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
    pub status: Option<EcosystemStatus>,
}

impl MockEcosystemInfoDriver {
    pub fn new(status: Option<EcosystemStatus>) -> Self {
        Self { status }
    }
}

impl EcosystemInfoDriver for MockEcosystemInfoDriver {
    fn status(&self) -> Option<EcosystemStatus> {
        self.status
    }
}

#[test]
fn diagnostics_service_snapshot_with_default_ecosystem() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.operational();
    let service = DiagnosticsService::new(&hardware, &state_watch);

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            device_type: "intercom_bridge",
            state: SystemState::Operational,
            domain: IntercomState {
                door_locked: true,
                door_unlock_duration_sec: 5,
            },
            uptime_sec: 12345,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            available_ecosystems: &[],
            active_ecosystem: None,
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}

#[test]
fn diagnostics_service_snapshot_with_explicit_ecosystem_and_properties() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let expected_status = EcosystemStatus::new(TestEcosystem::ProtoA.id(), true)
        .with_property("door_lock_subscribed", EcosystemValue::Bool(true))
        .with_property("ring_sensor_subscribed", EcosystemValue::Bool(false));
    let ecosystem = MockEcosystemInfoDriver::new(Some(expected_status));
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.ecosystem_connecting();
    let service = DiagnosticsService::with_ecosystem(
        &hardware,
        &ecosystem,
        TEST_AVAILABLE_ECOSYSTEMS,
        &state_watch,
    );

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            device_type: "intercom_bridge",
            state: SystemState::EcosystemConnecting,
            domain: IntercomState {
                door_locked: true,
                door_unlock_duration_sec: 5,
            },
            uptime_sec: 12345,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            available_ecosystems: TEST_AVAILABLE_ECOSYSTEMS,
            active_ecosystem: Some(expected_status),
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}

#[test]
fn ecosystem_status_iterates_populated_properties() {
    // Arrange
    let status = EcosystemStatus::new(EcosystemId::new("Custom"), false)
        .with_property("key_bool", EcosystemValue::Bool(true))
        .with_property("key_int", EcosystemValue::Int(42))
        .with_property("key_text", EcosystemValue::Text("online"));

    // Act
    let props: std::vec::Vec<_> = status.iter_properties().collect();

    // Assert
    assert_eq!(
        props,
        std::vec![
            ("key_bool", EcosystemValue::Bool(true)),
            ("key_int", EcosystemValue::Int(42)),
            ("key_text", EcosystemValue::Text("online")),
        ]
    );
}

#[test]
fn ecosystem_status_property_lookup_and_overwrite() {
    // Arrange
    let status = EcosystemStatus::new(EcosystemId::new("Matter"), true)
        .with_property("door_lock_subscribed", EcosystemValue::Bool(false))
        .with_property("ring_sensor_subscribed", EcosystemValue::Bool(true));

    // Act
    let lock_before = status.property("door_lock_subscribed");
    let ring_val = status.property("ring_sensor_subscribed");
    let missing_val = status.property("nonexistent_key");

    let updated_status = status.with_property("door_lock_subscribed", EcosystemValue::Bool(true));
    let lock_after = updated_status.property("door_lock_subscribed");

    // Assert
    assert_eq!(lock_before, Some(&EcosystemValue::Bool(false)));
    assert_eq!(ring_val, Some(&EcosystemValue::Bool(true)));
    assert_eq!(missing_val, None);
    assert_eq!(lock_after, Some(&EcosystemValue::Bool(true)));

    let prop_count = updated_status.iter_properties().count();
    assert_eq!(prop_count, 2);
}

#[test]
fn ecosystem_status_fabric_addition_and_iteration() {
    // Arrange
    let fab1 = intercom_core::domain::ecosystem::FabricInfo {
        fab_idx: 1,
        vendor_id: 4996,
        fabric_id: 0x1122334455667788,
        node_id: 0x0000000000000002,
        label: intercom_core::domain::ecosystem::FabricLabel::from_str("Apple HomePod"),
    };

    let fab2 = intercom_core::domain::ecosystem::FabricInfo {
        fab_idx: 2,
        vendor_id: 4877,
        fabric_id: 0xAABBCCDDEEFF0011,
        node_id: 0x0000000000000005,
        label: intercom_core::domain::ecosystem::FabricLabel::from_str("Home Assistant"),
    };

    // Act
    let status = EcosystemStatus::new(EcosystemId::new("Matter"), true)
        .with_fabric(fab1)
        .with_fabric(fab2);

    let fabrics: std::vec::Vec<_> = status.iter_fabrics().copied().collect();

    // Assert
    assert_eq!(fabrics.len(), 2);
    assert_eq!(fabrics[0], fab1);
    assert_eq!(fabrics[1], fab2);
    assert_eq!(fabrics[0].label.as_str(), "Apple HomePod");
    assert_eq!(fabrics[1].label.as_str(), "Home Assistant");
}

#[test]
fn ecosystem_status_fabric_capacity_limit() {
    // Arrange
    let mut status = EcosystemStatus::new(EcosystemId::new("Matter"), true);

    // Act - add MAX_ECOSYSTEM_FABRICS + 2
    for i in 1..=7 {
        status = status.with_fabric(intercom_core::domain::ecosystem::FabricInfo {
            fab_idx: i as u8,
            vendor_id: 0x1000 + i,
            fabric_id: i as u64,
            node_id: i as u64,
            label: intercom_core::domain::ecosystem::FabricLabel::from_str("Hub"),
        });
    }

    // Assert - only MAX_ECOSYSTEM_FABRICS (5) retained without crashing
    assert_eq!(
        status.iter_fabrics().count(),
        intercom_core::domain::ecosystem::MAX_ECOSYSTEM_FABRICS
    );
}
