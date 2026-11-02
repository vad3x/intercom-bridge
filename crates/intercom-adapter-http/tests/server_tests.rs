//! Integration tests for async `HttpServer` with injected core services.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

#![recursion_limit = "256"]

use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use flate2::read::GzDecoder;
use futures::executor::block_on;
use intercom_adapter_http::{HttpServer, HttpServerConfig, DASHBOARD_HTML_GZ};
use intercom_core::events::{SystemState, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle, CommandChannel};
use intercom_core::service::DiagnosticsService;
use serde_json::Value;
use std::io::Read;
use std::string::String;
use std::vec::Vec;

// ============================================================================
// Mock Stream Implementation
// ============================================================================

struct MockStream {
    read_data: Vec<u8>,
    read_pos: usize,
    written_data: Vec<u8>,
}

impl MockStream {
    fn new(input: &[u8]) -> Self {
        Self {
            read_data: input.to_vec(),
            read_pos: 0,
            written_data: Vec::new(),
        }
    }

    fn written_bytes(&self) -> &[u8] {
        &self.written_data
    }

    fn written_str(&self) -> &str {
        core::str::from_utf8(&self.written_data).expect("invalid utf-8 in written response")
    }
}

impl OtaStream for MockStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        if self.read_pos >= self.read_data.len() {
            return Ok(0);
        }
        let available = self.read_data.len() - self.read_pos;
        let to_copy = core::cmp::min(available, buf.len());
        buf[..to_copy].copy_from_slice(&self.read_data[self.read_pos..self.read_pos + to_copy]);
        self.read_pos += to_copy;
        Ok(to_copy)
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.written_data.extend_from_slice(buf);
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

// ============================================================================
// Mock Core Port Drivers
// ============================================================================

struct MockHardwareInfo;

impl HardwareInfoDriver for MockHardwareInfo {
    fn device_type(&self) -> &'static str {
        "intercom_bridge"
    }

    fn uptime_secs(&self) -> u64 {
        1234
    }

    fn free_heap_bytes(&self) -> usize {
        120_000
    }

    fn total_heap_bytes(&self) -> usize {
        320_000
    }

    fn wifi_ssid(&self) -> Option<intercom_core::domain::diagnostics::WifiSsid> {
        intercom_core::domain::diagnostics::WifiSsid::new("TestNet")
    }

    fn wifi_rssi(&self) -> Option<i8> {
        Some(-55)
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        Some([192, 168, 1, 142])
    }

    fn mac_address(&self) -> [u8; 6] {
        [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
    }

    fn active_slot(&self) -> &'static str {
        "ota_0"
    }

    fn is_slot_valid(&self) -> bool {
        true
    }

    fn firmware_version(&self) -> &'static str {
        "1.2.3"
    }

    fn git_commit(&self) -> &'static str {
        "deadbeef"
    }

    fn build_date(&self) -> &'static str {
        "2026-10-15"
    }
}

struct MockOtaFlashDriver {
    should_fail: bool,
    received_payload: Vec<u8>,
    call_count: usize,
}

impl MockOtaFlashDriver {
    fn new(should_fail: bool) -> Self {
        Self {
            should_fail,
            received_payload: Vec::new(),
            call_count: 0,
        }
    }
}

impl OtaFlashDriver for MockOtaFlashDriver {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        content_length: usize,
    ) -> Result<(), OtaDriverError> {
        self.call_count += 1;
        let mut buf = [0u8; 512];
        let mut total = 0;
        while total < content_length {
            let to_read = core::cmp::min(buf.len(), content_length - total);
            let n = stream
                .read(&mut buf[..to_read])
                .await
                .map_err(|_| OtaDriverError::TruncatedStream)?;
            if n == 0 {
                break;
            }
            self.received_payload.extend_from_slice(&buf[..n]);
            total += n;
        }

        if self.should_fail {
            Err(OtaDriverError::VerificationFailed)
        } else {
            Ok(())
        }
    }
}

fn test_log_provider(buf: &mut [u8]) -> usize {
    let msg = b"00:01:23.456 INFO  test - system running smoothly\n";
    let len = msg.len().min(buf.len());
    buf[..len].copy_from_slice(&msg[..len]);
    len
}

// ============================================================================
// Test Suite
// ============================================================================

#[test]
fn test_get_dashboard_returns_gzipped_html() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"GET / HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    let response = stream.written_bytes();
    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("missing header separator");
    let header_str = core::str::from_utf8(&response[..header_end]).unwrap();
    assert!(header_str.starts_with("HTTP/1.1 200"));
    assert!(header_str.contains("Content-Type: text/html; charset=utf-8"));
    assert!(header_str.contains("Content-Encoding: gzip"));
    assert!(header_str.contains(&format!("Content-Length: {}", DASHBOARD_HTML_GZ.len())));

    let body = &response[header_end + 4..];
    assert_eq!(body, DASHBOARD_HTML_GZ);

    let mut decoder = GzDecoder::new(body);
    let mut html = String::new();
    decoder
        .read_to_string(&mut html)
        .expect("decompression failed");
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Intercom Bridge"));
}

#[test]
fn test_get_status_returns_valid_telemetry_json() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"GET /api/status HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    let response = stream.written_bytes();
    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("missing header separator");
    let header_str = core::str::from_utf8(&response[..header_end]).unwrap();
    assert!(header_str.starts_with("HTTP/1.1 200"));
    assert!(header_str.contains("Content-Type: application/json"));

    let body_str = core::str::from_utf8(&response[header_end + 4..]).unwrap();
    let v: Value = serde_json::from_str(body_str).expect("invalid JSON");
    assert_eq!(v["device_type"], "intercom_bridge");
    assert_eq!(v["state"], "Operational");
    assert_eq!(v["door_locked"], true);
    assert_eq!(v["door_unlock_duration_sec"], 5);
    assert_eq!(v["uptime_sec"], 1234);
    assert_eq!(v["wifi_ssid"], "TestNet");
    assert_eq!(v["wifi_rssi_dbm"], -55);
    assert_eq!(v["ip"], "192.168.1.142");
    assert_eq!(v["mac"], "AA:BB:CC:DD:EE:FF");
    assert_eq!(v["active_slot"], "ota_0");
    assert_eq!(v["slot_valid"], true);
    assert_eq!(v["free_heap_bytes"], 120_000);
    assert_eq!(v["total_heap_bytes"], 320_000);
    assert_eq!(v["available_ecosystems"], serde_json::json!([]));
    assert!(v["active_ecosystem"].is_null());
    assert_eq!(v["version"], "1.2.3");
    assert_eq!(v["git_commit"], "deadbeef");
    assert_eq!(v["build_date"], "2026-10-15");
}

#[test]
fn test_get_logs_returns_text_plain_logs() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"GET /api/logs HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    let response = stream.written_bytes();
    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("missing header separator");
    let header_str = core::str::from_utf8(&response[..header_end]).unwrap();
    assert!(header_str.starts_with("HTTP/1.1 200"));
    assert!(header_str.contains("Content-Type: text/plain; charset=utf-8"));

    let body_str = core::str::from_utf8(&response[header_end + 4..]).unwrap();
    assert_eq!(
        body_str,
        "00:01:23.456 INFO  test - system running smoothly\n"
    );
}

#[test]
fn test_post_action_ring_dispatches_command() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream =
        MockStream::new(b"POST /api/action/ring HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 200 "));
    assert_eq!(cmd_channel.try_receive(), Ok(AccessCommand::Ring));
}

#[test]
fn test_post_action_unlock_dispatches_command() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream =
        MockStream::new(b"POST /api/action/unlock HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 200 "));
    assert_eq!(cmd_channel.try_receive(), Ok(AccessCommand::Unlock));
}

#[test]
fn test_post_action_door_duration_valid() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(
        b"POST /api/action/door-duration?seconds=7 HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n",
    );

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 200 "));
    assert_eq!(
        cmd_channel.try_receive(),
        Ok(AccessCommand::SetUnlockDuration(7))
    );
}

#[test]
fn test_post_action_door_duration_invalid_range() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(
        b"POST /api/action/door-duration?seconds=0 HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n",
    );

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 400 "));
    assert!(cmd_channel.try_receive().is_err());
}

#[test]
fn test_post_ota_flash_success() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let payload = b"0123456789ABCDEF";
    let req = format!(
        "POST /api/v1/ota/flash HTTP/1.1\r\nHost: 192.168.1.142\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    let mut req_bytes = req.into_bytes();
    req_bytes.extend_from_slice(payload);
    let mut stream = MockStream::new(&req_bytes);

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 200 "));
    assert_eq!(ota_driver.call_count, 1);
    assert_eq!(ota_driver.received_payload, payload);
}

#[test]
fn test_post_ota_flash_driver_error_returns_400() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(true); // Should fail
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let payload = b"corrupted payload";
    let req = format!(
        "POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    let mut req_bytes = req.into_bytes();
    req_bytes.extend_from_slice(payload);
    let mut stream = MockStream::new(&req_bytes);

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_err());
    assert!(stream.written_str().contains("HTTP/1.1 400 "));
}

#[test]
fn test_post_ota_flash_missing_content_length_returns_400() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream =
        MockStream::new(b"POST /api/v1/ota/flash HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_err());
    assert!(stream.written_str().contains("HTTP/1.1 400 "));
}

#[test]
fn test_get_invalid_route_returns_404_not_found() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"GET /unknown/path HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 404 "));
}

struct MockResetDriver;

impl intercom_core::ports::SystemResetDriver for MockResetDriver {
    fn erase_credentials(&self, _scope: intercom_core::ports::ResetScope) {}
    fn reboot(&self) -> ! {
        panic!("MOCK_REBOOT_TRIGGERED");
    }
}

#[test]
#[should_panic(expected = "MOCK_REBOOT_TRIGGERED")]
fn test_post_ota_flash_triggers_reboot() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let reset_driver = MockResetDriver;
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_reset_driver(&reset_driver);

    let payload = b"new valid binary payload";
    let req = format!(
        "POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    let mut req_bytes = req.into_bytes();
    req_bytes.extend_from_slice(payload);
    let mut stream = MockStream::new(&req_bytes);

    // Act
    let _ = block_on(server.handle_request(&mut stream));

    // Assert: should_panic triggered on reset_driver.reboot()
}

#[test]
fn test_post_ota_flash_sets_updating_state() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_state_sender(state_sender);

    let payload = b"new valid binary payload";
    let req = format!(
        "POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    );
    let mut req_bytes = req.into_bytes();
    req_bytes.extend_from_slice(payload);
    let mut stream = MockStream::new(&req_bytes);

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert_eq!(state_sender.current_state(), Some(SystemState::Updating));
}

struct RecordingResetDriver {
    erased_scope: std::sync::atomic::AtomicU8,
}

impl intercom_core::ports::SystemResetDriver for RecordingResetDriver {
    fn erase_credentials(&self, scope: intercom_core::ports::ResetScope) {
        let val = match scope {
            intercom_core::ports::ResetScope::Ecosystem => 1,
            intercom_core::ports::ResetScope::Factory => 2,
        };
        self.erased_scope
            .store(val, std::sync::atomic::Ordering::SeqCst);
    }
    fn reboot(&self) -> ! {
        assert_eq!(
            self.erased_scope.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "expected ResetScope::Ecosystem to be erased before reboot"
        );
        panic!("MOCK_ECOSYSTEM_RESET_REBOOT_TRIGGERED");
    }
}

#[test]
#[should_panic(expected = "MOCK_ECOSYSTEM_RESET_REBOOT_TRIGGERED")]
fn test_post_action_reset_matter_erases_ecosystem_and_reboots() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let reset_driver = RecordingResetDriver {
        erased_scope: std::sync::atomic::AtomicU8::new(0),
    };
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_reset_driver(&reset_driver);

    let mut stream =
        MockStream::new(b"POST /api/action/reset/matter HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let _ = block_on(server.handle_request(&mut stream));

    // Assert: should_panic triggered on reset_driver.reboot()
}

#[test]
fn test_post_action_reset_matter_sets_ecosystem_reset_state() {
    // Arrange
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    state_watch.sender().send(SystemState::Operational);
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_state_sender(state_sender);

    let mut stream =
        MockStream::new(b"POST /api/action/reset/matter HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));

    // Assert
    assert!(result.is_ok());
    assert!(stream.written_str().contains("HTTP/1.1 200 "));
    assert_eq!(
        state_sender.current_state(),
        Some(SystemState::EcosystemReset)
    );
}

#[test]
#[should_panic(expected = "MOCK_REBOOT_TRIGGERED")]
fn test_post_action_reboot_invokes_reboot() {
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let reset_driver = MockResetDriver;
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_reset_driver(&reset_driver);

    let mut stream =
        MockStream::new(b"POST /api/action/reboot HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    let _ = block_on(server.handle_request(&mut stream));
}

struct RecordingFactoryResetDriver {
    erased_scope: std::sync::atomic::AtomicU8,
}

impl intercom_core::ports::SystemResetDriver for RecordingFactoryResetDriver {
    fn erase_credentials(&self, scope: intercom_core::ports::ResetScope) {
        let val = match scope {
            intercom_core::ports::ResetScope::Ecosystem => 1,
            intercom_core::ports::ResetScope::Factory => 2,
        };
        self.erased_scope
            .store(val, std::sync::atomic::Ordering::SeqCst);
    }
    fn reboot(&self) -> ! {
        assert_eq!(
            self.erased_scope.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "expected ResetScope::Factory to be erased before reboot"
        );
        panic!("MOCK_FACTORY_RESET_REBOOT_TRIGGERED");
    }
}

#[test]
#[should_panic(expected = "MOCK_FACTORY_RESET_REBOOT_TRIGGERED")]
fn test_post_action_reset_factory_erases_and_reboots() {
    let cmd_channel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(cmd_channel.sender());
    let hardware = MockHardwareInfo;
    let state_watch = SystemStateWatch::new();
    let diagnostics = DiagnosticsService::new(&hardware, &state_watch);
    let mut ota_driver = MockOtaFlashDriver::new(false);
    let reset_driver = RecordingFactoryResetDriver {
        erased_scope: std::sync::atomic::AtomicU8::new(0),
    };
    let mut server = HttpServer::new_unbound(
        access_handle,
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_reset_driver(&reset_driver);

    let mut stream =
        MockStream::new(b"POST /api/action/reset/factory HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

    let _ = block_on(server.handle_request(&mut stream));
}

#[test]
fn bus_debug_routes_dispatch_http_owner_commands() {
    use intercom_core::domain::intercom::SessionOwner;
    // Arrange
    let cases = [
        (
            "listen",
            AccessCommand::StartListen(SessionOwner::HttpDebug),
        ),
        (
            "talk/start",
            AccessCommand::StartTalk(SessionOwner::HttpDebug),
        ),
        (
            "talk/stop",
            AccessCommand::StopTalk(SessionOwner::HttpDebug),
        ),
        (
            "heartbeat",
            AccessCommand::SessionHeartbeat(SessionOwner::HttpDebug),
        ),
        ("hangup", AccessCommand::EndSession(SessionOwner::HttpDebug)),
    ];
    for (path, expected) in cases {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        let request = format!("POST /api/action/{path} HTTP/1.1\r\nHost: device\r\n\r\n");
        let mut stream = MockStream::new(request.as_bytes());

        // Act
        let result = block_on(server.handle_request(&mut stream));

        // Assert
        assert!(result.is_ok());
        assert!(stream.written_str().starts_with("HTTP/1.1 200 "));
        assert!(stream.written_str().contains("application/json"));
        assert_eq!(channel.try_receive(), Ok(expected));
        assert!(channel.try_receive().is_err());
    }
}

#[test]
fn maintenance_routes_request_emergency_stop() {
    // Arrange
    for path in ["reboot", "reset/matter", "reset/factory"] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        let request = format!("POST /api/action/{path} HTTP/1.1\r\nHost: device\r\n\r\n");
        let mut stream = MockStream::new(request.as_bytes());

        // Act
        let result = block_on(server.handle_request(&mut stream));

        // Assert
        assert!(result.is_ok());
        assert_eq!(channel.try_receive(), Ok(AccessCommand::EmergencyStop));
    }
}

// Existing protocol tests do not model physical outputs. Safety tests below use
// the production interlock around an observable bus driver instead.
struct BusSafetyDriverMock;

impl intercom_core::ports::BusSafetyDriver for BusSafetyDriverMock {
    fn inhibit(&self) {}
}

struct BusSwitchDriverMock<'a>(&'a std::cell::Cell<intercom_core::domain::intercom::BusLineState>);

impl intercom_core::ports::BusSwitchDriver for BusSwitchDriverMock<'_> {
    fn set_bus_state(&mut self, state: intercom_core::domain::intercom::BusLineState) {
        self.0.set(state);
    }
}

struct OtaSafetyDriverMock<'a> {
    interlock: &'a intercom_core::service::BusSafetyInterlock<BusSwitchDriverMock<'a>>,
    physical_state: &'a std::cell::Cell<intercom_core::domain::intercom::BusLineState>,
    channel: &'a CommandChannel,
    queue_was_full: bool,
    fail: bool,
    entered: bool,
}

impl OtaFlashDriver for OtaSafetyDriverMock<'_> {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        content_length: usize,
    ) -> Result<(), OtaDriverError> {
        use intercom_core::domain::intercom::BusLineState;
        use intercom_core::ports::BusSwitchDriver;
        assert_eq!(
            self.physical_state.get(),
            BusLineState::Idle,
            "relay must be physically LOW before entering flash driver"
        );
        if self.queue_was_full {
            assert_eq!(
                self.channel.len(),
                intercom_core::ports::inbound::CMD_QUEUE_CAP,
                "shutdown must not depend on consuming queued commands"
            );
        } else {
            assert_eq!(self.channel.try_receive(), Ok(AccessCommand::EmergencyStop));
        }
        for requested in [
            BusLineState::Listen,
            BusLineState::Talk,
            BusLineState::DoorUnlock,
        ] {
            let mut bus = self.interlock;
            bus.set_bus_state(requested);
            assert_eq!(
                self.physical_state.get(),
                BusLineState::Idle,
                "relay must not re-energize during flash work"
            );
        }
        self.entered = true;
        let mut body = [0; 4];
        assert_eq!(stream.read(&mut body).await.unwrap(), content_length);
        if self.fail {
            Err(OtaDriverError::VerificationFailed)
        } else {
            Ok(())
        }
    }
}

#[test]
fn ota_inhibits_relays_before_flash_with_empty_or_saturated_command_queue() {
    use intercom_core::domain::intercom::{BusLineState, SessionOwner};
    use intercom_core::ports::BusSwitchDriver;
    // Arrange
    for queue_was_full in [false, true] {
        for fail in [false, true] {
            let channel = CommandChannel::new();
            let handle = AccessControllerHandle::new(channel.sender());
            if queue_was_full {
                for _ in 0..intercom_core::ports::inbound::CMD_QUEUE_CAP {
                    handle.start_talk(SessionOwner::HttpDebug);
                }
            }
            let physical_state = std::cell::Cell::new(BusLineState::DoorUnlock);
            let interlock = intercom_core::service::BusSafetyInterlock::new(BusSwitchDriverMock(
                &physical_state,
            ));
            let hardware = MockHardwareInfo;
            let watch = SystemStateWatch::new();
            let diagnostics = DiagnosticsService::new(&hardware, &watch);
            let mut ota = OtaSafetyDriverMock {
                interlock: &interlock,
                physical_state: &physical_state,
                channel: &channel,
                queue_was_full,
                fail,
                entered: false,
            };
            let mut server = HttpServer::new_unbound(
                handle,
                &interlock,
                &diagnostics,
                &mut ota,
                test_log_provider,
                HttpServerConfig::default(),
            );
            let mut stream = MockStream::new(
                b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 4\r\n\r\nDATA",
            );

            // Act
            let result = block_on(server.handle_request(&mut stream));
            drop(server);
            (&interlock).set_bus_state(BusLineState::Talk);

            // Assert
            assert_eq!(result.is_err(), fail);
            assert!(ota.entered);
            assert_eq!(
                physical_state.get(),
                BusLineState::Idle,
                "failed OTA stays inhibited until reboot"
            );
        }
    }
}

#[test]
fn reset_and_reboot_inhibit_relays_before_queueing_emergency_stop() {
    use intercom_core::domain::intercom::BusLineState;
    use intercom_core::ports::BusSafetyDriver;
    // Arrange
    struct OrderedSafetyDriverMock<'a> {
        interlock: &'a intercom_core::service::BusSafetyInterlock<BusSwitchDriverMock<'a>>,
        channel: &'a CommandChannel,
    }
    impl BusSafetyDriver for OrderedSafetyDriverMock<'_> {
        fn inhibit(&self) {
            assert!(
                self.channel.is_empty(),
                "inhibit must precede queued emergency stop"
            );
            self.interlock.inhibit();
        }
    }
    for path in ["reboot", "reset/matter", "reset/factory"] {
        let channel = CommandChannel::new();
        let physical_state = std::cell::Cell::new(BusLineState::Listen);
        let interlock =
            intercom_core::service::BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));
        let safety = OrderedSafetyDriverMock {
            interlock: &interlock,
            channel: &channel,
        };
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &safety,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        let request = format!("POST /api/action/{path} HTTP/1.1\r\nHost: device\r\n\r\n");
        let mut stream = MockStream::new(request.as_bytes());

        // Act
        let result = block_on(server.handle_request(&mut stream));

        // Assert
        assert!(result.is_ok());
        assert_eq!(physical_state.get(), BusLineState::Idle);
        assert_eq!(channel.try_receive(), Ok(AccessCommand::EmergencyStop));
    }
}

#[test]
fn malformed_ota_inhibits_relays_without_starting_flash() {
    use intercom_core::domain::intercom::BusLineState;
    // Arrange
    let channel = CommandChannel::new();
    let physical_state = std::cell::Cell::new(BusLineState::Talk);
    let interlock =
        intercom_core::service::BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));
    let hardware = MockHardwareInfo;
    let watch = SystemStateWatch::new();
    let diagnostics = DiagnosticsService::new(&hardware, &watch);
    let mut ota = MockOtaFlashDriver::new(false);
    let mut server = HttpServer::new_unbound(
        AccessControllerHandle::new(channel.sender()),
        &interlock,
        &diagnostics,
        &mut ota,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"POST /api/v1/ota/flash HTTP/1.1\r\nHost: device\r\n\r\n");

    // Act
    let result = block_on(server.handle_request(&mut stream));
    drop(server);

    // Assert
    assert!(result.is_err());
    assert_eq!(ota.call_count, 0);
    assert_eq!(physical_state.get(), BusLineState::Idle);
    assert_eq!(channel.try_receive(), Ok(AccessCommand::EmergencyStop));
}

struct PendingOtaDriverMock<'a>(&'a std::cell::Cell<bool>);

impl OtaFlashDriver for PendingOtaDriverMock<'_> {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        _stream: &mut S,
        _content_length: usize,
    ) -> Result<(), OtaDriverError> {
        self.0.set(true);
        futures::future::pending().await
    }
}

#[test]
fn cancelled_ota_keeps_relays_inhibited_until_reboot() {
    use futures::Future;
    use intercom_core::domain::intercom::BusLineState;
    use intercom_core::ports::BusSwitchDriver;
    // Arrange
    let channel = CommandChannel::new();
    let physical_state = std::cell::Cell::new(BusLineState::Listen);
    let interlock =
        intercom_core::service::BusSafetyInterlock::new(BusSwitchDriverMock(&physical_state));
    let hardware = MockHardwareInfo;
    let watch = SystemStateWatch::new();
    let diagnostics = DiagnosticsService::new(&hardware, &watch);
    let entered = std::cell::Cell::new(false);
    let mut ota = PendingOtaDriverMock(&entered);
    let mut server = HttpServer::new_unbound(
        AccessControllerHandle::new(channel.sender()),
        &interlock,
        &diagnostics,
        &mut ota,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream =
        MockStream::new(b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 4\r\n\r\nDATA");
    let mut request = Box::pin(server.handle_request(&mut stream));
    let waker = futures::task::noop_waker();
    let mut context = std::task::Context::from_waker(&waker);

    // Act
    let result = request.as_mut().poll(&mut context);
    drop(request);
    (&interlock).set_bus_state(BusLineState::DoorUnlock);

    // Assert
    assert!(result.is_pending());
    assert!(
        entered.get(),
        "request reached the flash driver before cancellation"
    );
    assert_eq!(physical_state.get(), BusLineState::Idle);
    assert_eq!(channel.try_receive(), Ok(AccessCommand::EmergencyStop));
}

struct RelayObservingResetDriverMock<'a> {
    interlock: &'a intercom_core::service::BusSafetyInterlock<BusSwitchDriverMock<'a>>,
    physical_state: &'a std::cell::Cell<intercom_core::domain::intercom::BusLineState>,
    erased_scope: std::cell::Cell<Option<intercom_core::ports::ResetScope>>,
    reboot_entered: std::cell::Cell<bool>,
}

impl RelayObservingResetDriverMock<'_> {
    fn assert_relays_are_idle_and_inhibited(&self, entry_point: &str) {
        use intercom_core::domain::intercom::BusLineState;
        use intercom_core::ports::BusSwitchDriver;
        assert_eq!(
            self.physical_state.get(),
            BusLineState::Idle,
            "physical relays must be LOW on entry to {entry_point}"
        );
        for requested in [
            BusLineState::Listen,
            BusLineState::Talk,
            BusLineState::DoorUnlock,
        ] {
            let mut bus = self.interlock;
            bus.set_bus_state(requested);
            assert_eq!(
                self.physical_state.get(),
                BusLineState::Idle,
                "physical relays must stay inhibited inside {entry_point}"
            );
        }
    }
}

impl intercom_core::ports::SystemResetDriver for RelayObservingResetDriverMock<'_> {
    fn erase_credentials(&self, scope: intercom_core::ports::ResetScope) {
        self.assert_relays_are_idle_and_inhibited("erase_credentials");
        self.erased_scope.set(Some(scope));
    }

    fn reboot(&self) -> ! {
        self.assert_relays_are_idle_and_inhibited("reboot");
        self.reboot_entered.set(true);
        panic!("RELAY_OBSERVING_REBOOT_REACHED");
    }
}

#[test]
fn maintenance_reset_driver_entry_points_observe_idle_and_inhibited_relays() {
    use intercom_core::domain::intercom::{BusLineState, SessionOwner};
    use intercom_core::ports::ResetScope;
    // Arrange
    for (path, expected_scope) in [
        ("reboot", None),
        ("reset/matter", Some(ResetScope::Ecosystem)),
        ("reset/factory", Some(ResetScope::Factory)),
    ] {
        for saturated in [false, true] {
            let channel = CommandChannel::new();
            let handle = AccessControllerHandle::new(channel.sender());
            if saturated {
                for _ in 0..intercom_core::ports::inbound::CMD_QUEUE_CAP {
                    handle.start_talk(SessionOwner::HttpDebug);
                }
            }
            let physical_state = std::cell::Cell::new(BusLineState::DoorUnlock);
            let interlock = intercom_core::service::BusSafetyInterlock::new(BusSwitchDriverMock(
                &physical_state,
            ));
            let reset = RelayObservingResetDriverMock {
                interlock: &interlock,
                physical_state: &physical_state,
                erased_scope: std::cell::Cell::new(None),
                reboot_entered: std::cell::Cell::new(false),
            };
            let hardware = MockHardwareInfo;
            let watch = SystemStateWatch::new();
            let diagnostics = DiagnosticsService::new(&hardware, &watch);
            let mut ota = MockOtaFlashDriver::new(false);
            let mut server = HttpServer::new_unbound(
                handle,
                &interlock,
                &diagnostics,
                &mut ota,
                test_log_provider,
                HttpServerConfig::default(),
            )
            .with_reset_driver(&reset);
            let request = format!("POST /api/action/{path} HTTP/1.1\r\nHost: device\r\n\r\n");
            let mut stream = MockStream::new(request.as_bytes());

            // Act
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                block_on(server.handle_request(&mut stream))
            }));

            // Assert
            let panic = outcome.expect_err("the reset driver must reach its diverging reboot");
            assert_eq!(
                panic.downcast_ref::<&str>().copied(),
                Some("RELAY_OBSERVING_REBOOT_REACHED"),
                "{path} must reach reboot after passing the physical safety assertions"
            );
            assert_eq!(reset.erased_scope.get(), expected_scope);
            assert!(reset.reboot_entered.get());
            assert_eq!(physical_state.get(), BusLineState::Idle);
        }
    }
}

#[test]
fn audio_gain_route_validates_boundaries_before_dispatch() {
    use intercom_core::domain::policy::{RxPgaGain, TxDacGain};
    // Arrange
    for (query, valid) in [
        ("rx_half_db=0&tx_half_db=-127", true),
        ("rx_half_db=95&tx_half_db=48", true),
        ("rx_half_db=40&tx_half_db=0", true),
        ("rx_half_db=96&tx_half_db=0", false),
        ("rx_half_db=40&tx_half_db=49", false),
        ("rx_half_db=40&tx_half_db=-128", false),
        ("rx_half_db=-1&tx_half_db=0", false),
        ("rx_half_db=256&tx_half_db=0", false),
        ("rx_half_db=40", false),
    ] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        let request =
            format!("POST /api/action/audio/gain?{query} HTTP/1.1\r\nHost: device\r\n\r\n");
        let mut stream = MockStream::new(request.as_bytes());
        // Act
        let result = block_on(server.handle_request(&mut stream));
        // Assert
        assert!(result.is_ok());
        let code = if valid { "200" } else { "400" };
        assert!(
            stream
                .written_str()
                .starts_with(&format!("HTTP/1.1 {code} ")),
            "{query}: {}",
            stream.written_str()
        );
        if valid {
            let (rx, tx) = match query {
                "rx_half_db=0&tx_half_db=-127" => (0, -127),
                "rx_half_db=95&tx_half_db=48" => (95, 48),
                _ => (40, 0),
            };
            assert_eq!(
                channel.try_receive(),
                Ok(AccessCommand::SetAudioGain {
                    rx: RxPgaGain::try_new(rx).unwrap(),
                    tx: TxDacGain::try_new(tx).unwrap(),
                })
            );
        }
        assert!(channel.try_receive().is_err());
    }
}

#[test]
fn bench_tone_route_dispatches_bench_owner_and_rejects_invalid_query() {
    use intercom_core::domain::intercom::SessionOwner;
    // Arrange
    for (query, expected) in [
        (
            "enable=true",
            Some(AccessCommand::StartTalk(SessionOwner::BenchTone)),
        ),
        (
            "enable=false",
            Some(AccessCommand::EndSession(SessionOwner::BenchTone)),
        ),
        ("enable=invalid", None),
        ("", None),
    ] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        let request =
            format!("POST /api/debug/audio/tone?{query} HTTP/1.1\r\nHost: device\r\n\r\n");
        let mut stream = MockStream::new(request.as_bytes());
        // Act
        let result = block_on(server.handle_request(&mut stream));
        // Assert
        assert!(result.is_ok());
        let code = if expected.is_some() { "200" } else { "400" };
        assert!(stream
            .written_str()
            .starts_with(&format!("HTTP/1.1 {code} ")));
        assert_eq!(channel.try_receive().ok(), expected);
        assert!(channel.try_receive().is_err());
    }
}

const WS_AUDIO_REQUEST: &[u8] = b"GET /api/ws/audio HTTP/1.1\r\nHost: device\r\nConnection: upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n";

fn masked_ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x80 | opcode];
    if payload.len() < 126 {
        bytes.push(0x80 | payload.len() as u8);
    } else {
        bytes.push(0x80 | 126);
        bytes.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    let mask = [1, 2, 3, 4];
    bytes.extend_from_slice(&mask);
    bytes.extend(payload.iter().enumerate().map(|(i, v)| v ^ mask[i % 4]));
    bytes
}

#[test]
fn websocket_dispatches_controls_and_audio_and_releases_session_on_disconnect() {
    use core::sync::atomic::{AtomicBool, Ordering};
    use intercom_adapter_http::HttpAudioMedia;
    use intercom_core::domain::intercom::SessionOwner;
    use intercom_core::events::CallStateWatch;
    // Arrange
    let channel = CommandChannel::new();
    let hardware = MockHardwareInfo;
    let watch = SystemStateWatch::new();
    let calls = CallStateWatch::new();
    let diagnostics = DiagnosticsService::new(&hardware, &watch);
    let mut ota = MockOtaFlashDriver::new(false);
    let rx_audio = intercom_media::RxAudioChannel::new();
    let tx_audio = intercom_media::TxAudioChannel::new();
    let active = AtomicBool::new(false);
    let mut server = HttpServer::new_unbound(
        AccessControllerHandle::new(channel.sender()),
        &BusSafetyDriverMock,
        &diagnostics,
        &mut ota,
        test_log_provider,
        HttpServerConfig::default(),
    )
    .with_audio_media(HttpAudioMedia {
        rx_audio: &rx_audio,
        tx_audio: &tx_audio,
        call_watch: &calls,
        state_watch: &watch,
        ws_active: &active,
    });
    let mut request = WS_AUDIO_REQUEST.to_vec();
    for ctrl in [1, 2, 0] {
        request.extend(masked_ws_frame(2, &[ctrl]));
    }
    request.extend(masked_ws_frame(2, &[0x80; 320]));
    request.extend(masked_ws_frame(8, &[]));
    let mut stream = MockStream::new(&request);
    // Act
    let result = block_on(server.handle_request(&mut stream));
    // Assert
    assert!(result.is_ok());
    assert!(stream.written_bytes().starts_with(b"HTTP/1.1 101 "));
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::StartListen(SessionOwner::HttpDebug))
    );
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::StartTalk(SessionOwner::HttpDebug))
    );
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::StopTalk(SessionOwner::HttpDebug))
    );
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::SessionHeartbeat(SessionOwner::HttpDebug))
    );
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::SessionHeartbeat(SessionOwner::HttpDebug))
    );
    assert_eq!(
        channel.try_receive(),
        Ok(AccessCommand::EndSession(SessionOwner::HttpDebug))
    );
    assert_eq!(tx_audio.try_receive().unwrap().samples, [32124; 320]);
    assert!(!active.load(Ordering::Acquire));
}

#[test]
fn websocket_rejects_busy_matter_and_updating_without_acquiring_or_dispatching() {
    use core::sync::atomic::{AtomicBool, Ordering};
    use intercom_adapter_http::HttpAudioMedia;
    use intercom_core::domain::intercom::SessionOwner;
    use intercom_core::events::{CallStateWatch, SessionSnapshot};
    // Arrange
    for reason in ["unbound", "busy", "matter", "updating"] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let calls = CallStateWatch::new();
        if reason == "matter" {
            calls.sender().send(SessionSnapshot {
                active_owner: Some(SessionOwner::MatterCall),
                ..Default::default()
            });
        }
        if reason == "updating" {
            watch.sender().send(SystemState::Updating);
        }
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let rx_audio = intercom_media::RxAudioChannel::new();
        let tx_audio = intercom_media::TxAudioChannel::new();
        let active = AtomicBool::new(reason == "busy");
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        );
        if reason != "unbound" {
            server = server.with_audio_media(HttpAudioMedia {
                rx_audio: &rx_audio,
                tx_audio: &tx_audio,
                call_watch: &calls,
                state_watch: &watch,
                ws_active: &active,
            });
        }
        let mut stream = MockStream::new(WS_AUDIO_REQUEST);
        // Act
        let result = block_on(server.handle_request(&mut stream));
        // Assert
        assert!(result.is_ok());
        assert!(
            stream.written_bytes().starts_with(b"HTTP/1.1 409 "),
            "{reason}"
        );
        assert_eq!(active.load(Ordering::Acquire), reason == "busy");
        assert!(channel.try_receive().is_err());
    }
}

struct AudioFaultStream<'a> {
    inner: MockStream,
    watch: &'a SystemStateWatch,
    calls: &'a intercom_core::events::CallStateWatch,
    fault: &'static str,
    triggered: bool,
}
impl OtaStream for AudioFaultStream<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        if self.inner.read_pos < self.inner.read_data.len() {
            return self.inner.read(buf).await;
        }
        if self.fault == "read_error" {
            return Err(());
        }
        if self.fault == "partial_frame" && !self.triggered {
            self.triggered = true;
            self.watch.sender().send(SystemState::Updating);
        }
        futures::future::pending().await
    }
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
        if self.fault == "upgrade_error" {
            return Err(());
        }
        if self.fault == "pong_error" && buf.first() == Some(&0x8a) {
            return Err(());
        }
        if buf.first() == Some(&0x82) {
            if self.fault == "audio_error" {
                return Err(());
            }
            if !self.triggered
                && ["blocked_write_ota", "blocked_write_matter"].contains(&self.fault)
            {
                self.triggered = true;
                if self.fault == "blocked_write_ota" {
                    self.watch.sender().send(SystemState::Updating);
                } else {
                    self.calls
                        .sender()
                        .send(intercom_core::events::SessionSnapshot {
                            active_owner: Some(
                                intercom_core::domain::intercom::SessionOwner::MatterCall,
                            ),
                            ..Default::default()
                        });
                }
                return futures::future::pending().await;
            }
        }
        self.inner.write_all(buf).await
    }
    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

#[test]
fn websocket_preemption_interrupts_partial_frames_and_blocked_downlink_writes() {
    use core::sync::atomic::{AtomicBool, Ordering};
    use futures::Future;
    use intercom_adapter_http::HttpAudioMedia;
    use intercom_core::domain::intercom::SessionOwner;
    use intercom_core::events::CallStateWatch;
    // Arrange
    for fault in ["partial_frame", "blocked_write_ota", "blocked_write_matter"] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let calls = CallStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let rx_audio = intercom_media::RxAudioChannel::new();
        let tx_audio = intercom_media::TxAudioChannel::new();
        let active = AtomicBool::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        )
        .with_audio_media(HttpAudioMedia {
            rx_audio: &rx_audio,
            tx_audio: &tx_audio,
            call_watch: &calls,
            state_watch: &watch,
            ws_active: &active,
        });
        let mut input = WS_AUDIO_REQUEST.to_vec();
        if fault == "partial_frame" {
            input.extend([0x82, 0xfe]);
        } else {
            rx_audio
                .try_send(intercom_media::PcmFrame::SILENCE)
                .unwrap();
        }
        let mut stream = AudioFaultStream {
            inner: MockStream::new(&input),
            watch: &watch,
            calls: &calls,
            fault,
            triggered: false,
        };
        let mut future = Box::pin(server.handle_request(&mut stream));
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        // Act
        let mut result = future.as_mut().poll(&mut cx);
        for _ in 0..2 {
            if result.is_pending() {
                result = future.as_mut().poll(&mut cx);
            }
        }
        drop(future);
        // Assert
        assert!(
            result.is_ready(),
            "{fault} must interrupt the outstanding I/O"
        );
        assert!(
            stream
                .inner
                .written_bytes()
                .windows(4)
                .any(|w| w == [0x88, 11, 0x03, 0xf5]),
            "{fault}: missing 1013 close"
        );
        assert!(!active.load(Ordering::Acquire));
        assert_eq!(
            channel.try_receive(),
            Ok(AccessCommand::StartListen(SessionOwner::HttpDebug))
        );
        assert_eq!(
            channel.try_receive(),
            Ok(AccessCommand::EndSession(SessionOwner::HttpDebug))
        );
    }
}

#[test]
fn websocket_releases_reservation_and_session_on_io_errors_and_cancellation() {
    use core::sync::atomic::{AtomicBool, Ordering};
    use futures::Future;
    use intercom_adapter_http::HttpAudioMedia;
    use intercom_core::domain::intercom::SessionOwner;
    use intercom_core::events::CallStateWatch;
    // Arrange
    for fault in [
        "upgrade_error",
        "read_error",
        "pong_error",
        "audio_error",
        "cancelled",
    ] {
        let channel = CommandChannel::new();
        let hardware = MockHardwareInfo;
        let watch = SystemStateWatch::new();
        let calls = CallStateWatch::new();
        let diagnostics = DiagnosticsService::new(&hardware, &watch);
        let mut ota = MockOtaFlashDriver::new(false);
        let rx_audio = intercom_media::RxAudioChannel::new();
        let tx_audio = intercom_media::TxAudioChannel::new();
        let active = AtomicBool::new(false);
        let mut server = HttpServer::new_unbound(
            AccessControllerHandle::new(channel.sender()),
            &BusSafetyDriverMock,
            &diagnostics,
            &mut ota,
            test_log_provider,
            HttpServerConfig::default(),
        )
        .with_audio_media(HttpAudioMedia {
            rx_audio: &rx_audio,
            tx_audio: &tx_audio,
            call_watch: &calls,
            state_watch: &watch,
            ws_active: &active,
        });
        let mut input = WS_AUDIO_REQUEST.to_vec();
        if fault == "pong_error" {
            input.extend(masked_ws_frame(9, b"ping"));
        }
        if fault == "audio_error" {
            rx_audio
                .try_send(intercom_media::PcmFrame::SILENCE)
                .unwrap();
        }
        let mut stream = AudioFaultStream {
            inner: MockStream::new(&input),
            watch: &watch,
            calls: &calls,
            fault,
            triggered: false,
        };
        let mut future = Box::pin(server.handle_request(&mut stream));
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        // Act
        let result = future.as_mut().poll(&mut cx);
        drop(future);
        // Assert
        assert_eq!(result.is_pending(), fault == "cancelled", "{fault}");
        assert!(!active.load(Ordering::Acquire), "{fault}");
        if fault != "upgrade_error" {
            assert_eq!(
                channel.try_receive(),
                Ok(AccessCommand::StartListen(SessionOwner::HttpDebug))
            );
            assert_eq!(
                channel.try_receive(),
                Ok(AccessCommand::EndSession(SessionOwner::HttpDebug))
            );
        }
        assert!(channel.try_receive().is_err(), "{fault}");
    }
}
