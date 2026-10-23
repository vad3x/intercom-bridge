//! HTTP request router and zero-allocation JSON serialization.
//!
//! Provides static route parsing and diagnostic telemetry formatting
//! directly into pre-allocated byte slices without heap allocation.

use core::fmt::Write;
use edf_core::domain::diagnostics as edf_diagnostics;
use edf_http_diagnostics::{parse_diagnostic_route, DiagnosticRoute};
use intercom_core::domain::diagnostics::{SystemDiagnostics, WifiSsid as CoreWifiSsid};
use intercom_core::domain::ecosystem::{EcosystemType, EcosystemValue};
use intercom_core::events::SystemState;

// ============================================================================
// Public Constants
// ============================================================================

/// Static HTTP 200 OK JSON response.
pub const HTTP_200_JSON_OK: &[u8] = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}";

/// Static HTTP 404 Not Found JSON response.
pub const HTTP_404_NOT_FOUND: &[u8] = b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 22\r\n\r\n{\"error\":\"not_found\"}";

/// Static HTTP 400 Bad Request JSON response.
pub const HTTP_400_BAD_REQUEST: &[u8] = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 24\r\n\r\n{\"error\":\"bad_request\"}";

// ============================================================================
// Public Route Definitions & Parsing
// ============================================================================

/// Identified HTTP routes supported by the Intercom Bridge HTTP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Dashboard,
    Status,
    Logs,
    ActionRing,
    ActionUnlock,
    ActionResetMatter,
    OtaFlash,
    NotFound,
}

impl From<DiagnosticRoute> for Route {
    fn from(route: DiagnosticRoute) -> Self {
        match route {
            DiagnosticRoute::Dashboard => Route::Dashboard,
            DiagnosticRoute::Status => Route::Status,
            DiagnosticRoute::Logs => Route::Logs,
            DiagnosticRoute::Reboot => Route::NotFound,
            DiagnosticRoute::Reset(edf_core::domain::reset::ResetScope::Ecosystem) => {
                Route::ActionResetMatter
            }
            DiagnosticRoute::Reset(edf_core::domain::reset::ResetScope::Factory) => {
                Route::NotFound
            }
        }
    }
}

/// Parses an HTTP request line (e.g., `GET /api/status HTTP/1.1`) into a `Route`.
pub fn parse_route(request_line: &str) -> Route {
    let mut parts = request_line.split_whitespace();
    let method = match parts.next() {
        Some(m) => m,
        None => return Route::NotFound,
    };
    let path = match parts.next() {
        Some(p) => match p.split_once('?') {
            Some((base, _query)) => base,
            None => p,
        },
        None => return Route::NotFound,
    };

    if let Some(diagnostic) = parse_diagnostic_route(method, path) {
        let route = Route::from(diagnostic);
        if !matches!(route, Route::NotFound) {
            return route;
        }
    }

    match (method, path) {
        ("POST", "/api/action/ring") => Route::ActionRing,
        ("POST", "/api/action/unlock") => Route::ActionUnlock,
        ("POST", "/api/v1/ota/flash") => Route::OtaFlash,
        _ => Route::NotFound,
    }
}

/// Formats `SystemDiagnostics` into caller-provided buffer as compact JSON.
///
/// Returns the number of bytes written, or `Err(())` if the buffer is too small.
pub fn format_status_json<T: EcosystemType>(
    diag: &SystemDiagnostics<T>,
    out: &mut [u8],
) -> Result<usize, ()> {
    use edf_http_diagnostics::{format_status_json as edf_format_status_json, DiagnosticPayload};

    struct IntercomPayload(bool);
    impl IntercomPayload {
        fn new(locked: bool) -> Self {
            Self(locked)
        }
    }
    impl DiagnosticPayload for IntercomPayload {
        fn write_fields<W: core::fmt::Write>(&self, out: &mut W) -> core::fmt::Result {
            write!(out, ",\"door_locked\":{}", if self.0 { "true" } else { "false" })
        }
    }

    let wifi_ssid = if let Some(ref ssid) = diag.wifi_ssid {
        edf_core::domain::diagnostics::WifiSsid::new(ssid.as_str())
    } else {
        None
    };

    let edf_diag: edf_diagnostics::SystemDiagnostics<IntercomPayload, T> =
        edf_diagnostics::SystemDiagnostics {
            device_type: diag.device_type,
            state: diag.state,
            uptime_sec: diag.uptime_sec,
            wifi_ssid,
            wifi_rssi_dbm: diag.wifi_rssi_dbm,
            ip: diag.ip,
            mac: diag.mac,
            active_slot: diag.active_slot,
            slot_valid: diag.slot_valid,
            free_heap_bytes: diag.free_heap_bytes,
            total_heap_bytes: diag.total_heap_bytes,
            available_ecosystems: diag.available_ecosystems,
            active_ecosystem: diag.active_ecosystem.clone(),
            domain: IntercomPayload::new(diag.door_locked),
            version: diag.version,
            git_commit: diag.git_commit,
            build_date: diag.build_date,
        };

    edf_format_status_json(&edf_diag, out).map_err(|_| ())
}

// ============================================================================
// Private Helpers & Writers
// ============================================================================

fn write_escaped_str(writer: &mut BufferWriter<'_>, s: &str) -> core::fmt::Result {
    writer.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => writer.write_str("\\\"")?,
            '\\' => writer.write_str("\\\\")?,
            '\n' => writer.write_str("\\n")?,
            '\r' => writer.write_str("\\r")?,
            '\t' => writer.write_str("\\t")?,
            _ => write!(writer, "{}", c)?,
        }
    }
    writer.write_str("\"")?;
    Ok(())
}

fn state_to_str(state: SystemState) -> &'static str {
    match state {
        SystemState::Booting => "Booting",
        SystemState::Starting => "Starting",
        SystemState::Commissioning => "Commissioning",
        SystemState::WifiConnecting => "WifiConnecting",
        SystemState::EcosystemConnecting => "EcosystemConnecting",
        SystemState::Operational => "Operational",
        SystemState::Updating => "Updating",
        SystemState::EcosystemReset => "EcosystemReset",
        SystemState::FactoryReset => "FactoryReset",
        SystemState::Error => "Error",
    }
}

struct BufferWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BufferWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn written(&self) -> usize {
        self.pos
    }
}

impl<'a> Write for BufferWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len().saturating_sub(self.pos);
        if bytes.len() > remaining {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}
