//! HTTP request router and zero-allocation JSON serialization.
//!
//! Provides static route parsing and diagnostic telemetry formatting
//! directly into pre-allocated byte slices without heap allocation.

use edf_http_diagnostics::{parse_diagnostic_route, DiagnosticPayload, DiagnosticRoute};
use intercom_core::domain::diagnostics::{IntercomState, SystemDiagnostics};
use intercom_core::domain::ecosystem::EcosystemType;

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
    ActionResetFactory,
    ActionReboot,
    ActionSetDoorDuration(u32),
    OtaFlash,
    NotFound,
}

impl From<DiagnosticRoute> for Route {
    fn from(route: DiagnosticRoute) -> Self {
        match route {
            DiagnosticRoute::Dashboard => Route::Dashboard,
            DiagnosticRoute::Status => Route::Status,
            DiagnosticRoute::Logs => Route::Logs,
            DiagnosticRoute::Reboot => Route::ActionReboot,
            DiagnosticRoute::Reset(edf_core::domain::reset::ResetScope::Ecosystem) => {
                Route::ActionResetMatter
            }
            DiagnosticRoute::Reset(edf_core::domain::reset::ResetScope::Factory) => {
                Route::ActionResetFactory
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
    let full_path = match parts.next() {
        Some(p) => p,
        None => return Route::NotFound,
    };
    let (path, query) = match full_path.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (full_path, None),
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
        ("POST", "/api/action/door-duration") => {
            if let Some(q) = query {
                for param in q.split('&') {
                    if let Some(val_str) = param.strip_prefix("seconds=") {
                        if let Ok(sec) = val_str.parse::<u32>() {
                            if intercom_core::config::is_valid_door_unlock_duration(sec) {
                                return Route::ActionSetDoorDuration(sec);
                            }
                        }
                    }
                }
            }
            Route::NotFound
        }
        ("POST", "/api/v1/ota/flash") => Route::OtaFlash,
        _ => Route::NotFound,
    }
}

/// Formats `SystemDiagnostics` into caller-provided buffer as compact JSON.
///
/// Returns the number of bytes written, or `Err(())` if the buffer is too small.
pub fn format_status_json<T: EcosystemType>(
    diag: &SystemDiagnostics<IntercomState, T>,
    out: &mut [u8],
) -> Result<usize, ()> {
    let shared = shared_snapshot(diag);
    edf_http_diagnostics::format_status_json(&shared, out).map_err(|_| ())
}

pub(crate) fn shared_snapshot<'a, T: EcosystemType>(
    diag: &'a SystemDiagnostics<IntercomState, T>,
) -> SystemDiagnostics<IntercomPayload<'a>, T> {
    SystemDiagnostics {
        device_type: diag.device_type,
        state: diag.state,
        domain: IntercomPayload(&diag.domain),
        uptime_sec: diag.uptime_sec,
        wifi_ssid: diag.wifi_ssid,
        wifi_rssi_dbm: diag.wifi_rssi_dbm,
        ip: diag.ip,
        mac: diag.mac,
        active_slot: diag.active_slot,
        slot_valid: diag.slot_valid,
        free_heap_bytes: diag.free_heap_bytes,
        total_heap_bytes: diag.total_heap_bytes,
        available_ecosystems: diag.available_ecosystems,
        active_ecosystem: diag.active_ecosystem.clone(),
        version: diag.version,
        git_commit: diag.git_commit,
        build_date: diag.build_date,
    }
}

pub(crate) struct IntercomPayload<'a>(&'a IntercomState);

impl DiagnosticPayload for IntercomPayload<'_> {
    fn write_fields<W: core::fmt::Write>(&self, out: &mut W) -> core::fmt::Result {
        write!(
            out,
            ",\"door_locked\":{},\"door_unlock_duration_sec\":{}",
            if self.0.door_locked { "true" } else { "false" },
            self.0.door_unlock_duration_sec
        )
    }
}

// ============================================================================
// Private Helpers & Writers
// ============================================================================


