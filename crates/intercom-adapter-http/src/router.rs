//! HTTP request router and zero-allocation JSON serialization.
//!
//! Provides static route parsing and diagnostic telemetry formatting
//! directly into pre-allocated byte slices without heap allocation.

use core::fmt::Write;
use intercom_core::domain::diagnostics::SystemDiagnostics;
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
    OtaFlash,
    NotFound,
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

    match (method, path) {
        ("GET", "/") => Route::Dashboard,
        ("GET", "/api/status") => Route::Status,
        ("GET", "/api/logs") => Route::Logs,
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
    let mut writer = BufferWriter::new(out);

    write!(
        writer,
        "{{\"state\":\"{}\",\"door_locked\":{},\"uptime_sec\":{},\"wifi_ssid\":",
        state_to_str(diag.state),
        diag.door_locked,
        diag.uptime_sec
    )
    .map_err(|_| ())?;

    match &diag.wifi_ssid {
        Some(ssid) => write!(writer, "\"{}\"", ssid.as_str()).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(writer, ",\"wifi_rssi_dbm\":").map_err(|_| ())?;
    match diag.wifi_rssi_dbm {
        Some(rssi) => write!(writer, "{}", rssi).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(writer, ",\"ip\":").map_err(|_| ())?;
    match diag.ip {
        Some([a, b, c, d]) => write!(writer, "\"{}.{}.{}.{}\"", a, b, c, d).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    let [m0, m1, m2, m3, m4, m5] = diag.mac;
    write!(
        writer,
        ",\"mac\":\"{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}\",\"active_slot\":\"{}\",\"slot_valid\":{},\"free_heap_bytes\":{},\"total_heap_bytes\":{},\"available_ecosystems\":[",
        m0,
        m1,
        m2,
        m3,
        m4,
        m5,
        diag.active_slot,
        diag.slot_valid,
        diag.free_heap_bytes,
        diag.total_heap_bytes,
    )
    .map_err(|_| ())?;

    for (idx, eco) in diag.available_ecosystems.iter().enumerate() {
        if idx > 0 {
            write!(writer, ",").map_err(|_| ())?;
        }
        write!(writer, "\"{}\"", eco.id().as_str()).map_err(|_| ())?;
    }

    write!(writer, "],\"active_ecosystem\":").map_err(|_| ())?;
    match &diag.active_ecosystem {
        Some(active) => {
            write!(
                writer,
                "{{\"type\":\"{}\",\"paired\":{},\"properties\":{{",
                active.ecosystem_type.as_str(),
                active.paired
            )
            .map_err(|_| ())?;

            for (idx, (key, val)) in active.iter_properties().enumerate() {
                if idx > 0 {
                    write!(writer, ",").map_err(|_| ())?;
                }
                match val {
                    EcosystemValue::Bool(b) => write!(writer, "\"{}\":{}", key, b).map_err(|_| ())?,
                    EcosystemValue::Int(n) => write!(writer, "\"{}\":{}", key, n).map_err(|_| ())?,
                    EcosystemValue::Text(s) => {
                        write!(writer, "\"{}\":\"{}\"", key, s).map_err(|_| ())?
                    }
                }
            }
            write!(writer, "}}}}").map_err(|_| ())?;
        }
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(
        writer,
        ",\"version\":\"{}\",\"git_commit\":\"{}\",\"build_date\":\"{}\"}}",
        diag.version, diag.git_commit, diag.build_date
    )
    .map_err(|_| ())?;

    Ok(writer.written())
}

// ============================================================================
// Private Helpers & Writers
// ============================================================================

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
