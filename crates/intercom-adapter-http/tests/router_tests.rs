//! Integration and unit tests for the HTTP request router and JSON formatter.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_adapter_http::router::{
    format_status_json, parse_route, Route, HTTP_200_JSON_OK, HTTP_400_BAD_REQUEST,
    HTTP_404_NOT_FOUND,
};
use intercom_core::domain::diagnostics::{IntercomState, SystemDiagnostics};
use intercom_core::events::SystemState;
use serde_json::Value;

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
    let mut buf = [0u8; 1024];
    let len = format_status_json(&diag, &mut buf).unwrap();
    let json = core::str::from_utf8(&buf[..len]).unwrap();
    assert!(json.contains("\"door_unlock_duration_sec\":8"));
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
            ..IntercomState::default()
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
    const AVAILABLE: &[intercom_core::domain::EcosystemId] = &[
        intercom_core::domain::EcosystemId::new("Matter"),
        intercom_core::domain::EcosystemId::new("HomeAssistant"),
    ];
    let active = intercom_core::domain::EcosystemStatus::new(
        intercom_core::domain::EcosystemId::new("Matter"),
        true,
    )
    .with_property(
        "door_lock_subscribed",
        intercom_core::domain::EcosystemValue::Bool(true),
    )
    .with_property(
        "door_lock_endpoint",
        intercom_core::domain::EcosystemValue::Int(1),
    )
    .with_property(
        "ring_sensor_subscribed",
        intercom_core::domain::EcosystemValue::Bool(false),
    )
    .with_property(
        "ring_sensor_endpoint",
        intercom_core::domain::EcosystemValue::Int(2),
    )
    .with_endpoint(
        intercom_core::domain::EndpointInfo::new(1, "Door Lock", 0x000A)
            .with_cluster(intercom_core::domain::ClusterInfo::new(0x000A, "Door Lock")),
    )
    .with_endpoint(
        intercom_core::domain::EndpointInfo::new(2, "Chime / Ring", 0x0141)
            .with_cluster(intercom_core::domain::ClusterInfo::new(0x003B, "Switch")),
    );
    let diag = SystemDiagnostics::<IntercomState, intercom_core::domain::EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
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
        available_ecosystems: AVAILABLE,
        active_ecosystem: Some(active),
        version: "0.1.0",
        git_commit: "a3f89b",
        build_date: "2026-10-15",
    };
    let mut buf = [0u8; 1024];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["device_type"], "intercom_bridge");
    assert_eq!(val["state"], "Operational");
    assert_eq!(val["door_locked"], true);
    assert_eq!(val["door_unlock_duration_sec"], 5);
    assert_eq!(val["uptime_sec"], 14250);
    assert_eq!(val["wifi_ssid"], "MyHomeNetwork");
    assert_eq!(val["wifi_rssi_dbm"], -58);
    assert_eq!(val["ip"], "192.168.1.142");
    assert_eq!(val["mac"], "68:B6:B3:21:40:8C");
    assert_eq!(val["active_slot"], "ota_0");
    assert_eq!(val["slot_valid"], true);
    assert_eq!(val["free_heap_bytes"], 151552);
    assert_eq!(val["total_heap_bytes"], 215040);
    assert_eq!(
        val["available_ecosystems"],
        serde_json::json!(["Matter", "HomeAssistant"])
    );
    assert_eq!(val["active_ecosystem"]["type"], "Matter");
    assert_eq!(val["active_ecosystem"]["paired"], true);
    assert_eq!(
        val["active_ecosystem"]["properties"]["door_lock_subscribed"],
        true
    );
    assert_eq!(
        val["active_ecosystem"]["properties"]["door_lock_endpoint"],
        1
    );
    assert_eq!(
        val["active_ecosystem"]["properties"]["ring_sensor_subscribed"],
        false
    );
    assert_eq!(
        val["active_ecosystem"]["properties"]["ring_sensor_endpoint"],
        2
    );
    assert_eq!(val["active_ecosystem"]["fabrics"], serde_json::json!([]));
    assert_eq!(
        val["active_ecosystem"]["endpoints"],
        serde_json::json!([
            {
                "id": 1,
                "name": "Door Lock",
                "device_type": "0x000A",
                "clusters": [
                    {"id": "0x000A", "name": "Door Lock"}
                ]
            },
            {
                "id": 2,
                "name": "Chime / Ring",
                "device_type": "0x0141",
                "clusters": [
                    {"id": "0x003B", "name": "Switch"}
                ]
            }
        ])
    );
    assert_eq!(val["version"], "0.1.0");
    assert_eq!(val["git_commit"], "a3f89b");
    assert_eq!(val["build_date"], "2026-10-15");
}

#[test]
fn test_format_status_json_with_multiple_fabrics() {
    // Arrange
    const AVAILABLE: &[intercom_core::domain::EcosystemId] =
        &[intercom_core::domain::EcosystemId::new("Matter")];
    let active = intercom_core::domain::EcosystemStatus::new(
        intercom_core::domain::EcosystemId::new("Matter"),
        true,
    )
    .with_property(
        "door_lock_subscribed",
        intercom_core::domain::EcosystemValue::Bool(true),
    )
    .with_fabric(
        intercom_core::domain::FabricInfo::new(
            1,
            4996,
            0x1B8246E1309A4D12,
            0x0000000000000002,
            intercom_core::domain::FabricLabel::from_str("Living Room HomePod"),
        )
        .with_subscription(intercom_core::domain::FabricSubscription::new(1, 0x000A)),
    )
    .with_fabric(intercom_core::domain::FabricInfo::new(
        2,
        4877,
        0x4F92A0019C21EE08,
        0x0000000000000005,
        intercom_core::domain::FabricLabel::from_str("Home Assistant Core"),
    ));

    let diag = SystemDiagnostics::<IntercomState, intercom_core::domain::EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
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
    let mut buf = [0u8; 1024];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["device_type"], "intercom_bridge");
    let fabrics = &val["active_ecosystem"]["fabrics"];
    assert_eq!(fabrics.as_array().unwrap().len(), 2);

    assert_eq!(fabrics[0]["fab_idx"], 1);
    assert_eq!(fabrics[0]["vendor_id"], 4996);
    assert_eq!(fabrics[0]["fabric_id"], "0x1B8246E1309A4D12");
    assert_eq!(fabrics[0]["node_id"], "0x0000000000000002");
    assert_eq!(fabrics[0]["label"], "Living Room HomePod");
    assert_eq!(
        fabrics[0]["subscriptions"],
        serde_json::json!([
            {"endpoint": 1, "cluster": "0x000A"}
        ])
    );

    assert_eq!(fabrics[1]["fab_idx"], 2);
    assert_eq!(fabrics[1]["vendor_id"], 4877);
    assert_eq!(fabrics[1]["fabric_id"], "0x4F92A0019C21EE08");
    assert_eq!(fabrics[1]["node_id"], "0x0000000000000005");
    assert_eq!(fabrics[1]["label"], "Home Assistant Core");
    assert_eq!(fabrics[1]["subscriptions"], serde_json::json!([]));
}

#[test]
fn test_format_status_json_with_none_options() {
    // Arrange
    let diag = SystemDiagnostics::<IntercomState, intercom_core::domain::EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::WifiConnecting,
        domain: IntercomState {
            door_locked: false,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
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
    let mut buf = [0u8; 1024];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["device_type"], "intercom_bridge");
    assert_eq!(val["state"], "WifiConnecting");
    assert_eq!(val["door_locked"], false);
    assert_eq!(val["uptime_sec"], 42);
    assert!(val["wifi_ssid"].is_null());
    assert!(val["wifi_rssi_dbm"].is_null());
    assert!(val["ip"].is_null());
    assert_eq!(val["mac"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(val["active_slot"], "ota_1");
    assert_eq!(val["slot_valid"], false);
    assert_eq!(val["free_heap_bytes"], 100000);
    assert_eq!(val["total_heap_bytes"], 200000);
    assert_eq!(val["available_ecosystems"], serde_json::json!([]));
    assert!(val["active_ecosystem"].is_null());
    assert_eq!(val["version"], "0.2.0");
    assert_eq!(val["git_commit"], "deadbeef");
    assert_eq!(val["build_date"], "2026-10-16");
}

#[test]
fn test_format_status_json_buffer_overflow_protection() {
    // Arrange
    let diag = SystemDiagnostics::<IntercomState, intercom_core::domain::EcosystemId> {
        device_type: "intercom_bridge",
        state: SystemState::Operational,
        domain: IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
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
    };
    let mut small_buf = [0u8; 32];

    // Act
    let result = format_status_json(&diag, &mut small_buf);

    // Assert
    assert_eq!(result, Err(()));
}

#[test]
fn test_http_constants_format() {
    // Arrange & Act & Assert
    assert!(HTTP_200_JSON_OK.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(HTTP_404_NOT_FOUND.starts_with(b"HTTP/1.1 404 Not Found\r\n"));
    assert!(HTTP_400_BAD_REQUEST.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
}

#[test]
fn parses_bus_debug_action_routes() {
    // Arrange
    let requests = [
        "POST /api/action/listen HTTP/1.1",
        "POST /api/action/talk/start HTTP/1.1",
        "POST /api/action/talk/stop HTTP/1.1",
        "POST /api/action/heartbeat HTTP/1.1",
        "POST /api/action/hangup HTTP/1.1",
    ];

    // Act
    let parsed = requests.map(parse_route);

    // Assert
    assert_eq!(
        parsed,
        [
            Route::ActionListen,
            Route::ActionTalkStart,
            Route::ActionTalkStop,
            Route::ActionHeartbeat,
            Route::ActionHangup
        ]
    );
    for request in requests {
        assert_eq!(
            parse_route(&request.replacen("POST", "GET", 1)),
            Route::NotFound
        );
    }
}

#[test]
fn status_json_includes_all_bus_and_call_states() {
    use intercom_core::domain::intercom::{BusLineState, CallState};
    // Arrange
    let states = [
        (BusLineState::Idle, CallState::Idle, "idle", "idle"),
        (
            BusLineState::Listen,
            CallState::Listening,
            "listen",
            "listening",
        ),
        (BusLineState::Talk, CallState::Talking, "talk", "talking"),
        (BusLineState::Idle, CallState::Settling, "idle", "settling"),
        (
            BusLineState::DoorUnlock,
            CallState::Unlocking,
            "door_unlock",
            "unlocking",
        ),
    ];
    for (bus, call, bus_name, call_name) in states {
        let mut diag = test_diagnostics();
        diag.domain.bus_state = bus;
        diag.domain.call_state = call;
        let mut buf = [0u8; 2048];

        // Act
        let len = format_status_json(&diag, &mut buf).expect("json fits buffer");
        let json: Value = serde_json::from_slice(&buf[..len]).unwrap();

        // Assert
        assert_eq!(json["bus_state"], bus_name);
        assert_eq!(json["call_state"], call_name);
    }
}

#[test]
fn status_reports_live_audio_levels_and_calibration() {
    // Arrange
    let mut diag = test_diagnostics();
    diag.domain.rx_rms_dbfs = -68;
    diag.domain.rx_peak_dbfs = -62;
    diag.domain.rx_gain_half_db = 95;
    diag.domain.tx_gain_half_db = -127;
    diag.domain.tone_active = true;
    let mut out = [0; 1536];
    // Act
    let len = format_status_json(&diag, &mut out).unwrap();
    let json: Value = serde_json::from_slice(&out[..len]).unwrap();
    // Assert
    assert_eq!(json["rx_rms_dbfs"], -68);
    assert_eq!(json["rx_peak_dbfs"], -62);
    assert_eq!(json["rx_gain_half_db"], 95);
    assert_eq!(json["tx_gain_half_db"], -127);
    assert_eq!(json["tone_active"], true);
}
