//! Integration and unit tests for the HTTP request router and JSON formatter.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_adapter_http::router::{
    format_status_json, parse_route, Route, HTTP_200_JSON_OK, HTTP_400_BAD_REQUEST,
    HTTP_404_NOT_FOUND,
};
use intercom_core::domain::diagnostics::SystemDiagnostics;
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

#[test]
fn test_format_status_json_success() {
    // Arrange
    let diag = SystemDiagnostics {
        state: SystemState::Ready,
        door_locked: true,
        uptime_sec: 14250,
        wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("MyHomeNetwork"),
        wifi_rssi_dbm: Some(-58),
        ip: Some([192, 168, 1, 142]),
        mac: [0x68, 0xB6, 0xB3, 0x21, 0x40, 0x8C],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 151552,
        total_heap_bytes: 215040,
        ecosystem_paired: true,
        ecosystem_protocol: "Matter",
        version: "0.1.0",
        git_commit: "a3f89b",
        build_date: "2026-10-15",
    };
    let mut buf = [0u8; 512];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["state"], "Ready");
    assert_eq!(val["door_locked"], true);
    assert_eq!(val["uptime_sec"], 14250);
    assert_eq!(val["wifi_ssid"], "MyHomeNetwork");
    assert_eq!(val["wifi_rssi_dbm"], -58);
    assert_eq!(val["ip"], "192.168.1.142");
    assert_eq!(val["mac"], "68:B6:B3:21:40:8C");
    assert_eq!(val["active_slot"], "ota_0");
    assert_eq!(val["slot_valid"], true);
    assert_eq!(val["free_heap_bytes"], 151552);
    assert_eq!(val["total_heap_bytes"], 215040);
    assert_eq!(val["ecosystem_paired"], true);
    assert_eq!(val["ecosystem_protocol"], "Matter");
    assert_eq!(val["version"], "0.1.0");
    assert_eq!(val["git_commit"], "a3f89b");
    assert_eq!(val["build_date"], "2026-10-15");
}

#[test]
fn test_format_status_json_with_none_options() {
    // Arrange
    let diag = SystemDiagnostics {
        state: SystemState::Connecting,
        door_locked: false,
        uptime_sec: 42,
        wifi_ssid: None,
        wifi_rssi_dbm: None,
        ip: None,
        mac: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        active_slot: "ota_1",
        slot_valid: false,
        free_heap_bytes: 100000,
        total_heap_bytes: 200000,
        ecosystem_paired: false,
        ecosystem_protocol: "None",
        version: "0.2.0",
        git_commit: "deadbeef",
        build_date: "2026-10-16",
    };
    let mut buf = [0u8; 512];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["state"], "Connecting");
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
    assert_eq!(val["ecosystem_paired"], false);
    assert_eq!(val["ecosystem_protocol"], "None");
    assert_eq!(val["version"], "0.2.0");
    assert_eq!(val["git_commit"], "deadbeef");
    assert_eq!(val["build_date"], "2026-10-16");
}

#[test]
fn test_format_status_json_buffer_overflow_protection() {
    // Arrange
    let diag = SystemDiagnostics {
        state: SystemState::Ready,
        door_locked: true,
        uptime_sec: 14250,
        wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("MyHomeNetwork"),
        wifi_rssi_dbm: Some(-58),
        ip: Some([192, 168, 1, 142]),
        mac: [0x68, 0xB6, 0xB3, 0x21, 0x40, 0x8C],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 151552,
        total_heap_bytes: 215040,
        ecosystem_paired: true,
        ecosystem_protocol: "Matter",
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
