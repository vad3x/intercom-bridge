//! Integration tests for async `HttpServer` with injected core services.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use flate2::read::GzDecoder;
use futures::executor::block_on;
use intercom_adapter_http::{HttpServer, HttpServerConfig, DASHBOARD_HTML_GZ};
use intercom_core::events::{SystemState, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle, CommandChannel};
use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
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
            let n = stream.read(&mut buf[..to_read]).await.map_err(|_| OtaDriverError::TruncatedStream)?;
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
    decoder.read_to_string(&mut html).expect("decompression failed");
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
    assert_eq!(body_str, "00:01:23.456 INFO  test - system running smoothly\n");
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
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"POST /api/action/ring HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

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
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"POST /api/action/unlock HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

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
        &diagnostics,
        &mut ota_driver,
        test_log_provider,
        HttpServerConfig::default(),
    );
    let mut stream = MockStream::new(b"POST /api/v1/ota/flash HTTP/1.1\r\nHost: 192.168.1.142\r\n\r\n");

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
        self.erased_scope.store(val, std::sync::atomic::Ordering::SeqCst);
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
        self.erased_scope.store(val, std::sync::atomic::Ordering::SeqCst);
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


