//! HTTP request router and zero-allocation JSON serialization.
//!
//! Provides static route parsing and diagnostic telemetry formatting
//! directly into pre-allocated byte slices without heap allocation.

use core::fmt::Write as _;
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
    LastFailure,
    ActionRing,
    ActionUnlock,
    ActionListen,
    ActionTalkStart,
    ActionTalkStop,
    ActionHeartbeat,
    ActionHangup,
    ActionResetMatter,
    ActionResetFactory,
    ActionReboot,
    ActionClearFailure,
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
        ("GET", "/api/last-failure") => Route::LastFailure,
        ("POST", "/api/action/clear-failure") => Route::ActionClearFailure,
        ("POST", "/api/action/ring") => Route::ActionRing,
        ("POST", "/api/action/unlock") => Route::ActionUnlock,
        ("POST", "/api/action/listen") => Route::ActionListen,
        ("POST", "/api/action/talk/start") => Route::ActionTalkStart,
        ("POST", "/api/action/talk/stop") => Route::ActionTalkStop,
        ("POST", "/api/action/heartbeat") => Route::ActionHeartbeat,
        ("POST", "/api/action/hangup") => Route::ActionHangup,
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
    format_status_json_with_memory(
        diag,
        MemorySnapshot {
            internal: MemoryRegion {
                free_bytes: diag.free_heap_bytes,
                total_bytes: diag.total_heap_bytes,
            },
            psram: None,
        },
        out,
    )
}

/// Memory statistics for one allocator region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryRegion {
    pub free_bytes: usize,
    pub total_bytes: usize,
}

/// Internal RAM and optional external PSRAM allocator statistics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemorySnapshot {
    pub internal: MemoryRegion,
    pub psram: Option<MemoryRegion>,
}

/// Maximum number of CPU cores reported in a `CpuSnapshot`.
pub const MAX_CPU_CORES: usize = 4;

/// Utilization telemetry for a single CPU core.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuCoreLoad {
    pub id: u8,
    pub load_pct: u8,
}

/// Fixed-capacity snapshot of per-core CPU load percentages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuSnapshot {
    core_count: u8,
    cores: [CpuCoreLoad; MAX_CPU_CORES],
}

impl CpuSnapshot {
    /// Construct a `CpuSnapshot` from up to `MAX_CPU_CORES` core readings.
    pub fn from_cores(cores: &[CpuCoreLoad]) -> Self {
        let mut buf = [CpuCoreLoad::default(); MAX_CPU_CORES];
        let count = cores.len().min(MAX_CPU_CORES);
        for (idx, core) in cores.iter().take(count).enumerate() {
            buf[idx] = CpuCoreLoad {
                id: core.id,
                load_pct: core.load_pct.min(100),
            };
        }
        Self {
            core_count: count as u8,
            cores: buf,
        }
    }

    /// Return the active core load entries.
    pub fn cores(&self) -> &[CpuCoreLoad] {
        &self.cores[..usize::from(self.core_count)]
    }
}

/// Formats status JSON with allocator-specific memory region statistics.
pub fn format_status_json_with_memory<T: EcosystemType>(
    diag: &SystemDiagnostics<IntercomState, T>,
    memory: MemorySnapshot,
    out: &mut [u8],
) -> Result<usize, ()> {
    format_status_json_with_telemetry(diag, memory, None, out)
}

/// Formats status JSON with allocator-specific memory region and optional CPU core telemetry.
pub fn format_status_json_with_telemetry<T: EcosystemType>(
    diag: &SystemDiagnostics<IntercomState, T>,
    memory: MemorySnapshot,
    cpu: Option<CpuSnapshot>,
    out: &mut [u8],
) -> Result<usize, ()> {
    let shared = shared_snapshot(diag);
    let base_len = edf_http_diagnostics::format_status_json(&shared, out).map_err(|_| ())?;
    if base_len == 0 || out[base_len - 1] != b'}' {
        return Err(());
    }

    let mut suffix = [0u8; 256];
    let mut writer = ByteWriter::new(&mut suffix);
    core::write!(
        writer,
        ",\"memory\":{{\"internal\":{{\"free_bytes\":{},\"total_bytes\":{}}},\"psram\":",
        memory.internal.free_bytes,
        memory.internal.total_bytes
    )
    .map_err(|_| ())?;
    match memory.psram {
        Some(psram) => core::write!(
            writer,
            "{{\"free_bytes\":{},\"total_bytes\":{}}}}}",
            psram.free_bytes,
            psram.total_bytes
        ),
        None => core::write!(writer, "null}}"),
    }
    .map_err(|_| ())?;

    if let Some(cpu) = cpu {
        let cores = cpu.cores();
        if !cores.is_empty() {
            core::write!(writer, ",\"cpu\":{{").map_err(|_| ())?;
            for (idx, core) in cores.iter().enumerate() {
                if idx != 0 {
                    core::write!(writer, ",").map_err(|_| ())?;
                }
                core::write!(
                    writer,
                    "\"core_{}\":{{\"load_pct\":{}}}",
                    core.id,
                    core.load_pct.min(100)
                )
                .map_err(|_| ())?;
            }
            core::write!(writer, "}}").map_err(|_| ())?;
        }
    }

    core::write!(writer, "}}").map_err(|_| ())?;

    let suffix_len = writer.len();
    let final_len = base_len - 1 + suffix_len;
    if final_len > out.len() {
        return Err(());
    }
    out[base_len - 1..final_len].copy_from_slice(&suffix[..suffix_len]);
    Ok(final_len)
}

struct ByteWriter<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl<'a> ByteWriter<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, len: 0 }
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl core::fmt::Write for ByteWriter<'_> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(core::fmt::Error)?;
        let target = self.out.get_mut(self.len..end).ok_or(core::fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
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
            ",\"door_locked\":{},\"door_unlock_duration_sec\":{},\"bus_state\":\"{}\",\"call_state\":\"{}\",\"rx_rms_dbfs\":{},\"rx_peak_dbfs\":{},\"rx_gain_half_db\":{},\"tx_gain_half_db\":{},\"tone_active\":{}",
            if self.0.door_locked { "true" } else { "false" },
            self.0.door_unlock_duration_sec,
            self.0.bus_state.as_str(),
            self.0.call_state.as_str(),
            self.0.rx_rms_dbfs,
            self.0.rx_peak_dbfs,
            self.0.rx_gain_half_db,
            self.0.tx_gain_half_db,
            self.0.tone_active,
        )
    }
}

// ============================================================================
// Private Helpers & Writers
// ============================================================================
