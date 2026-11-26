//! Integration and unit tests for the HTTP request router and JSON formatter.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_adapter_http::config::JSON_BUFFER_SIZE;
use intercom_adapter_http::router::{
    format_status_json, format_status_json_with_telemetry, parse_route, CpuCoreLoad, CpuSnapshot,
    MemoryRegion, MemorySnapshot, Route, HTTP_200_JSON_OK, HTTP_400_BAD_REQUEST,
    HTTP_404_NOT_FOUND,
};
use intercom_core::domain::diagnostics::{IntercomState, SystemDiagnostics, WifiSsid};
use intercom_core::domain::ecosystem::{
    ClusterInfo, EcosystemId, EcosystemStatus, EcosystemValue, EndpointInfo, FabricInfo,
    FabricLabel, FabricSubscription,
};
use intercom_core::events::SystemState;
use serde_json::{json, Value};

#[test]
fn test_parse_route_dashboard_root() {
    // Arrange
    let request_line = "GET / HTTP/1.1\r\n";

    // Act
    let route = parse_route(request_line);

    // Assert
    assert_eq!(route, Route::Dashboard);
}

#[test]
fn test_parse_route_status() {
    // Arrange
    let request_line = "GET /api/status HTTP/1.1\r\nHost: 192.168.1.100\r\n";

    // Act
    let route = parse_route(request_line);

    // Assert
    assert_eq!(route, Route::Status);
}

#[test]
fn test_parse_route_logs() {
    // Arrange
    let request_line = "GET /api/logs HTTP/1.1\r\n";

    // Act
    let route = parse_route(request_line);

    // Assert
    assert_eq!(route, Route::Logs);
}

#[test]
fn test_parse_route_actions() {
    // Arrange
    let ring_req = "POST /api/action/ring HTTP/1.1\r\n";
    let unlock_req = "POST /api/action/unlock HTTP/1.1\r\n";

    // Act
    let route_ring = parse_route(ring_req);
    let route_unlock = parse_route(unlock_req);

    // Assert
    assert_eq!(route_ring, Route::ActionRing);
    assert_eq!(route_unlock, Route::ActionUnlock);
}

#[test]
fn parse_route_action_door_duration() {
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=7 HTTP/1.1"),
        Route::ActionSetDoorDuration(7)
    );
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=0 HTTP/1.1"),
        Route::NotFound
    );
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=35 HTTP/1.1"),
        Route::NotFound
    );
}

#[test]
fn format_status_json_includes_door_unlock_duration() {
    let mut diag = test_diagnostics();
    diag.domain.door_unlock_duration_sec = 8;
    let mut buf = [0u8; JSON_BUFFER_SIZE];
    let len = format_status_json(&diag, &mut buf).unwrap();
    let val: Value = serde_json::from_slice(&buf[..len]).unwrap();
    assert_eq!(val["device"]["state"]["door_unlock_duration_sec"], 8);
}

#[test]
fn test_parse_route_action_reset_matter() {
    // Arrange
    let req = "POST /api/action/reset/matter HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n";

    // Act
    let route = parse_route(req);

    // Assert
    assert_eq!(route, Route::ActionResetMatter);
}

#[test]
fn test_parse_route_action_reboot() {
    let req = "POST /api/action/reboot HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n";
    let route = parse_route(req);
    assert_eq!(route, Route::ActionReboot);
}

#[test]
fn test_parse_route_action_reset_factory() {
    let req = "POST /api/action/reset/factory HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n";
    let route = parse_route(req);
    assert_eq!(route, Route::ActionResetFactory);
}

#[test]
fn test_parse_route_ota_flash() {
    // Arrange
    let req = "POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 1024\r\n";

    // Act
    let route = parse_route(req);

    // Assert
    assert_eq!(route, Route::OtaFlash);
}

#[test]
fn test_parse_route_with_query_params() {
    // Arrange
    let req = "GET /api/status?timestamp=123456 HTTP/1.1\r\n";

    // Act
    let route = parse_route(req);

    // Assert
    assert_eq!(route, Route::Status);
}

#[test]
fn test_parse_route_not_found_on_unknown_or_wrong_method() {
    // Arrange
    let wrong_method = "GET /api/action/ring HTTP/1.1";
    let wrong_method_status = "POST /api/status HTTP/1.1";
    let unknown_path = "GET /unknown/path HTTP/1.1";
    let empty_req = "";
    let malformed = "NOT_A_VALID_HTTP_REQUEST";

    // Act
    let r1 = parse_route(wrong_method);
    let r2 = parse_route(wrong_method_status);
    let r3 = parse_route(unknown_path);
    let r4 = parse_route(empty_req);
    let r5 = parse_route(malformed);

    // Assert
    assert_eq!(r1, Route::NotFound);
    assert_eq!(r2, Route::NotFound);
    assert_eq!(r3, Route::NotFound);
    assert_eq!(r4, Route::NotFound);
    assert_eq!(r5, Route::NotFound);
}

fn test_diagnostics() -> SystemDiagnostics<IntercomState, intercom_core::domain::EcosystemId> {
    SystemDiagnostics {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC,
        },
        uptime_sec: 14250,
        wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("MyHomeNetwork"),
        wifi_rssi_dbm: Some(-58),
        ip: Some([192, 168, 1, 142]),
        mac: [0x68, 0xB6, 0xB3, 0x21, 0x40, 0x8C],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 151552,
        total_heap_bytes: 215040,
        available_ecosystems: &[],
        active_ecosystem: None,
        version: "0.1.0",
        git_commit: "a3f89b",
        build_date: "2026-10-15",
    }
}

#[test]
fn test_format_status_json_success() {
    // Arrange
    const AVAILABLE: &[EcosystemId] = &[
        EcosystemId::new("Matter"),
        EcosystemId::new("HomeAssistant"),
    ];
    let active = EcosystemStatus::new(EcosystemId::new("Matter"), true)
        .with_property("door_lock_subscribed", EcosystemValue::Bool(true))
        .with_property("door_lock_endpoint", EcosystemValue::Int(1))
        .with_property("ring_sensor_subscribed", EcosystemValue::Bool(false))
        .with_property("ring_sensor_endpoint", EcosystemValue::Int(2))
        .with_endpoint(
            EndpointInfo::new(1, "Door Lock", 0x000A)
                .with_cluster(ClusterInfo::new(0x0101, "DoorLock")),
        )
        .with_endpoint(
            EndpointInfo::new(2, "Chime / Ring", 0x0107)
                .with_cluster(ClusterInfo::new(0x0406, "OccupancySensing")),
        );
    let diag = SystemDiagnostics::<IntercomState, EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
        },
        uptime_sec: 14250,
        wifi_ssid: WifiSsid::new("MyHomeNetwork"),
        wifi_rssi_dbm: Some(-58),
        ip: Some([192, 168, 1, 142]),
        mac: [0x68, 0xB6, 0xB3, 0x21, 0x40, 0x8C],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 151552,
        total_heap_bytes: 215040,
        available_ecosystems: AVAILABLE,
        active_ecosystem: Some(active),
        version: "0.1.0",
        git_commit: "a3f89b",
        build_date: "2026-10-15",
    };
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    let root = val.as_object().expect("root must be JSON object");
    assert_eq!(root.len(), 5);
    for legacy_key in [
        "device_type",
        "state",
        "door_locked",
        "door_unlock_duration_sec",
        "uptime_sec",
        "wifi_ssid",
        "wifi_rssi_dbm",
        "ip",
        "mac",
        "active_slot",
        "slot_valid",
        "free_heap_bytes",
        "total_heap_bytes",
        "available_ecosystems",
        "active_ecosystem",
        "version",
        "git_commit",
        "build_date",
    ] {
        assert!(
            !root.contains_key(legacy_key),
            "legacy root key present: {legacy_key}"
        );
    }

    assert_eq!(val["system"]["status"], "Operational");
    assert_eq!(val["system"]["uptime_sec"], 14250);
    assert_eq!(
        val["system"]["memory"],
        json!({"internal": {"free_bytes": 151552, "total_bytes": 215040}, "psram": null})
    );
    assert_eq!(val["system"]["cpu"], json!({}));

    assert_eq!(val["network"]["ssid"], "MyHomeNetwork");
    assert_eq!(val["network"]["rssi_dbm"], -58);
    assert_eq!(val["network"]["ip"], "192.168.1.142");
    assert_eq!(val["network"]["mac"], "68:B6:B3:21:40:8C");

    assert_eq!(val["firmware"]["version"], "0.1.0");
    assert_eq!(val["firmware"]["git_commit"], "a3f89b");
    assert_eq!(val["firmware"]["build_date"], "2026-10-15");
    assert_eq!(val["firmware"]["active_slot"], "ota_0");
    assert_eq!(val["firmware"]["slot_valid"], true);

    assert_eq!(
        val["ecosystems"]["available"],
        json!(["Matter", "HomeAssistant"])
    );
    assert_eq!(val["ecosystems"]["active"]["type"], "Matter");
    assert_eq!(val["ecosystems"]["active"]["paired"], true);
    assert_eq!(
        val["ecosystems"]["active"]["properties"]["door_lock_subscribed"],
        true
    );
    assert_eq!(
        val["ecosystems"]["active"]["properties"]["door_lock_endpoint"],
        1
    );
    assert_eq!(
        val["ecosystems"]["active"]["properties"]["ring_sensor_subscribed"],
        false
    );
    assert_eq!(
        val["ecosystems"]["active"]["properties"]["ring_sensor_endpoint"],
        2
    );
    assert_eq!(val["ecosystems"]["active"]["fabrics"], json!([]));
    assert_eq!(
        val["ecosystems"]["active"]["endpoints"],
        json!([
            {"id": 1, "name": "Door Lock", "device_type": "0x000A", "clusters": [{"id": "0x0101", "name": "DoorLock"}]},
            {"id": 2, "name": "Chime / Ring", "device_type": "0x0107", "clusters": [{"id": "0x0406", "name": "OccupancySensing"}]}
        ])
    );

    assert_eq!(val["device"]["type"], "intercom_bridge");
    assert_eq!(
        val["device"]["state"],
        json!({"door_locked": true, "door_unlock_duration_sec": 5})
    );
    assert_eq!(val["device"]["state"].as_object().unwrap().len(), 2);
}

#[test]
fn test_format_status_json_with_multiple_fabrics() {
    // Arrange
    const AVAILABLE: &[EcosystemId] = &[EcosystemId::new("Matter")];
    let active = EcosystemStatus::new(EcosystemId::new("Matter"), true)
        .with_property("door_lock_subscribed", EcosystemValue::Bool(true))
        .with_fabric(
            FabricInfo::new(
                1,
                4996,
                0x1B8246E1309A4D12,
                0x0000000000000002,
                FabricLabel::from_str("Living Room HomePod"),
            )
            .with_subscription(FabricSubscription::new(1, 0x0101)),
        )
        .with_fabric(FabricInfo::new(
            2,
            4877,
            0x4F92A0019C21EE08,
            0x0000000000000005,
            FabricLabel::from_str("Home Assistant Core"),
        ));

    let diag = SystemDiagnostics::<IntercomState, EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
        },
        uptime_sec: 100,
        wifi_ssid: None,
        wifi_rssi_dbm: None,
        ip: None,
        mac: [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 100000,
        total_heap_bytes: 200000,
        available_ecosystems: AVAILABLE,
        active_ecosystem: Some(active),
        version: "1.0.0",
        git_commit: "abcdef",
        build_date: "2026-10-17",
    };
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["device"]["type"], "intercom_bridge");
    let fabrics = &val["ecosystems"]["active"]["fabrics"];
    assert_eq!(fabrics.as_array().unwrap().len(), 2);

    assert_eq!(fabrics[0]["fab_idx"], 1);
    assert_eq!(fabrics[0]["vendor_id"], 4996);
    assert_eq!(fabrics[0]["fabric_id"], "0x1B8246E1309A4D12");
    assert_eq!(fabrics[0]["node_id"], "0x0000000000000002");
    assert_eq!(fabrics[0]["label"], "Living Room HomePod");
    assert_eq!(
        fabrics[0]["subscriptions"],
        json!([{"endpoint": 1, "cluster": "0x0101"}])
    );

    assert_eq!(fabrics[1]["fab_idx"], 2);
    assert_eq!(fabrics[1]["vendor_id"], 4877);
    assert_eq!(fabrics[1]["fabric_id"], "0x4F92A0019C21EE08");
    assert_eq!(fabrics[1]["node_id"], "0x0000000000000005");
    assert_eq!(fabrics[1]["label"], "Home Assistant Core");
}

#[test]
fn test_format_status_json_with_none_options() {
    // Arrange
    let diag = SystemDiagnostics::<IntercomState, EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::WifiConnecting,
        domain: IntercomState {
            door_locked: false,
            door_unlock_duration_sec: 5,
        },
        uptime_sec: 42,
        wifi_ssid: None,
        wifi_rssi_dbm: None,
        ip: None,
        mac: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        active_slot: "ota_1",
        slot_valid: false,
        free_heap_bytes: 100000,
        total_heap_bytes: 200000,
        available_ecosystems: &[],
        active_ecosystem: None,
        version: "0.2.0",
        git_commit: "deadbeef",
        build_date: "2026-10-16",
    };
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["device"]["type"], "intercom_bridge");
    assert_eq!(val["system"]["status"], "WifiConnecting");
    assert_eq!(val["device"]["state"]["door_locked"], false);
    assert_eq!(val["system"]["uptime_sec"], 42);
    assert!(val["network"]["ssid"].is_null());
    assert!(val["network"]["rssi_dbm"].is_null());
    assert!(val["network"]["ip"].is_null());
    assert_eq!(val["network"]["mac"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(val["firmware"]["active_slot"], "ota_1");
    assert_eq!(val["firmware"]["slot_valid"], false);
    assert_eq!(val["system"]["memory"]["internal"]["free_bytes"], 100000);
    assert_eq!(val["system"]["memory"]["internal"]["total_bytes"], 200000);
    assert!(val["system"]["memory"]["psram"].is_null());
    assert_eq!(val["system"]["cpu"], json!({}));
    assert_eq!(val["ecosystems"]["available"], json!([]));
    assert!(val["ecosystems"]["active"].is_null());
    assert_eq!(val["firmware"]["version"], "0.2.0");
    assert_eq!(val["firmware"]["git_commit"], "deadbeef");
    assert_eq!(val["firmware"]["build_date"], "2026-10-16");
}

#[test]
fn test_format_status_json_with_telemetry_includes_psram_and_cpu_cores() {
    // Arrange
    let diag = test_diagnostics();
    let memory = MemorySnapshot {
        internal: MemoryRegion {
            free_bytes: 120_000,
            total_bytes: 320_000,
        },
        psram: Some(MemoryRegion {
            free_bytes: 2_097_152,
            total_bytes: 4_194_304,
        }),
    };
    let cpu = CpuSnapshot::from_cores(&[
        CpuCoreLoad {
            id: 0,
            load_pct: 24,
        },
        CpuCoreLoad {
            id: 1,
            load_pct: 61,
        },
    ]);
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json_with_telemetry(&diag, memory, cpu, &mut buf)
        .expect("serialization with telemetry should succeed");
    let val: Value = serde_json::from_slice(&buf[..written]).expect("valid json");

    // Assert
    assert_eq!(
        val["system"]["memory"],
        json!({
            "internal": { "free_bytes": 120_000, "total_bytes": 320_000 },
            "psram": { "free_bytes": 2_097_152, "total_bytes": 4_194_304 }
        })
    );
    assert_eq!(
        val["system"]["cpu"],
        json!({
            "core_0": { "load_pct": 24 },
            "core_1": { "load_pct": 61 }
        })
    );
}

#[test]
fn test_format_status_json_escapes_control_chars_and_utf8() {
    // Arrange
    let special = "a\"b\\c\n\t雪";
    let mut diag = test_diagnostics();
    diag.wifi_ssid = WifiSsid::new(special);
    diag.active_ecosystem = Some(
        EcosystemStatus::new(EcosystemId::new("Matter"), true)
            .with_fabric(FabricInfo::new(
                1,
                4996,
                1,
                2,
                FabricLabel::from_str(special),
            ))
            .with_endpoint(
                EndpointInfo::new(1, special, 0x000A)
                    .with_cluster(ClusterInfo::new(0x0101, special)),
            ),
    );
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("escaping should succeed");
    let val: Value = serde_json::from_slice(&buf[..written]).expect("valid escaped json");

    // Assert
    assert_eq!(val["network"]["ssid"], special);
    assert_eq!(val["ecosystems"]["active"]["fabrics"][0]["label"], special);
    assert_eq!(val["ecosystems"]["active"]["endpoints"][0]["name"], special);
    assert_eq!(
        val["ecosystems"]["active"]["endpoints"][0]["clusters"][0]["name"],
        special
    );
}

#[test]
fn test_format_status_json_buffer_overflow_protection() {
    // Arrange
    let diag = test_diagnostics();
    let mut small_buf = [0u8; 64];

    // Act
    let result = format_status_json(&diag, &mut small_buf);

    // Assert
    assert_eq!(result, Err(()));
}

#[test]
fn test_format_status_json_max_capacity_fits_json_buffer_size() {
    // Arrange
    const AVAILABLE: &[EcosystemId] = &[EcosystemId::new("Matter")];
    let mut active = EcosystemStatus::new(EcosystemId::new("Matter"), true);
    for key in ["prop_1", "prop_2", "prop_3", "prop_4", "prop_5", "prop_6"] {
        active = active.with_property(key, EcosystemValue::Text("max_capacity_value"));
    }
    for fab_idx in 1..=5u8 {
        let mut fabric = FabricInfo::new(
            fab_idx,
            0x1349,
            0xFFFF_FFFF_FFFF_0000 + u64::from(fab_idx),
            0x0000_0000_0000_0010 + u64::from(fab_idx),
            FabricLabel::from_str("ThirtyTwoByteMaxFabricLabel12345"),
        );
        for sub_idx in 0..4u16 {
            fabric = fabric.with_subscription(FabricSubscription::new(
                sub_idx + 1,
                0x0000_0100 + u32::from(sub_idx),
            ));
        }
        active = active.with_fabric(fabric);
    }
    for ep_idx in 1..=4u16 {
        let mut ep = EndpointInfo::new(ep_idx, "Max Capacity Endpoint Name", 0x0141);
        for cl_idx in 0..6u32 {
            ep = ep.with_cluster(ClusterInfo::new(0x0500 + cl_idx, "MaxClusterName"));
        }
        active = active.with_endpoint(ep);
    }

    let mut diag = test_diagnostics();
    diag.available_ecosystems = AVAILABLE;
    diag.active_ecosystem = Some(active);
    let mut buf = [0u8; JSON_BUFFER_SIZE];

    // Act
    let written = format_status_json(&diag, &mut buf)
        .expect("maximal 5-fabric/4-endpoint payload must fit JSON_BUFFER_SIZE");
    let val: Value = serde_json::from_slice(&buf[..written]).expect("valid maximal json");

    // Assert
    assert!(written <= JSON_BUFFER_SIZE);
    assert_eq!(
        val["ecosystems"]["active"]["properties"]
            .as_object()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        val["ecosystems"]["active"]["fabrics"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        val["ecosystems"]["active"]["endpoints"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn test_http_constants_format() {
    // Arrange & Act & Assert
    assert!(HTTP_200_JSON_OK.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(HTTP_404_NOT_FOUND.starts_with(b"HTTP/1.1 404 Not Found\r\n"));
    assert!(HTTP_400_BAD_REQUEST.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
}
