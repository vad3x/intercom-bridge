# Tasks (Wave 2 — `intercom-matter`)

> **Prerequisite (Wave 1 Blocker):** Complete and verify [wave1-edf-tasks.md](wave1-edf-tasks.md) in `../embedded-device-foundation` before running this Wave 2 checklist in `/swarm`.

## 1. Intercom HTTP Adapter (`crates/intercom-adapter-http`)

- [x] 1.1 [RED] Update and add unit tests in `crates/intercom-adapter-http/tests/router_tests.rs` covering:
  - **CASE:** formats intercom status JSON with five buckets and device state
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** operational `SystemDiagnostics<IntercomState>` with fabric and endpoint metadata
    - **WHEN:** status JSON is formatted into `JSON_BUFFER_SIZE`
    - **THEN:**
      - `device.type == "intercom_bridge"`
      - `device.state.door_locked` and `device.state.door_unlock_duration_sec` match input
      - `system.status` and `system.uptime_sec` match input
      - `network.mac` and `firmware.active_slot` match input
      - `ecosystems.active.fabrics` preserves fabric entries
  - **CASE:** includes required memory and CPU telemetry within buffer limit
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** `MemorySnapshot` (with and without `psram`) and `CpuSnapshot` (single-core and dual-core)
    - **WHEN:** status JSON is formatted into `JSON_BUFFER_SIZE`
    - **THEN:**
      - `system.memory.internal.free_bytes` and `system.memory.internal.total_bytes` match input
      - `system.memory.psram` is `null` when absent and `{ free_bytes, total_bytes }` when present
      - `system.cpu.core_0.load_pct` (and `core_1.load_pct` for dual-core) match input
      - serialized length <= `JSON_BUFFER_SIZE`
  - **CASE:** includes bus, call, and live audio calibration fields under device.state
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** `IntercomState` across `BusLineState` and `CallState` variants with live RMS/peak/gain values
    - **WHEN:** status JSON is formatted
    - **THEN:**
      - `device.state.bus_state` and `device.state.call_state` match all enum variants
      - `device.state.rx_rms_dbfs`, `rx_peak_dbfs`, `rx_gain_half_db`, `tx_gain_half_db`, `tone_active` match input
  - **CASE:** rejects undersized buffer without panicking
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** a 64-byte output buffer
    - **WHEN:** status JSON is formatted
    - **THEN:**
      - returns an error without panicking
  Verify failure with: `mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
- [x] 1.2 [GREEN] Replace `IntercomPayload` (`DiagnosticPayload`) and `ByteWriter` trailing-`}` splicing in `crates/intercom-adapter-http/src/router.rs` with `#[derive(serde::Serialize)]` `IntercomStatusDto` (`From<&IntercomState>`) delegating to `edf_http_diagnostics::format_status_json`, and update `crates/intercom-adapter-http/src/lib.rs` and `src/server.rs`, verifying all tests pass with: `mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
- [x] 1.3 [RED] Update `crates/intercom-adapter-http/tests/server_tests.rs` covering:
  - **CASE:** GET /api/status matches golden five-bucket intercom JSON snapshot
    - **UNIT:** `GET /api/status`
    - **GIVEN:** running HTTP server with `SystemDiagnostics`, `MemorySnapshot`, and `CpuSnapshot` providers
    - **WHEN:** client sends `GET /api/status` and compares parsed JSON against the expected five-bucket intercom snapshot
    - **THEN:**
      - HTTP status == 200 and `Content-Type == "application/json"`
      - parsed JSON equals the expected five-bucket snapshot (`system`, `network`, `firmware`, `ecosystems`, `device` with `type == "intercom_bridge"`) with no stray legacy keys
  Verify failure with: `mise exec -- cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
- [x] 1.4 [GREEN] Update `/api/status` handler in `crates/intercom-adapter-http/src/server.rs` to supply required `MemorySnapshot` and `CpuSnapshot` to `format_status_json`, verifying tests pass with: `mise exec -- cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`

## 2. Intercom Solid.js Dashboard, Scripts & Firmware Verification

- [x] 2.1 [RED] Update `crates/intercom-adapter-http/frontend/tests/bus-debug-api.test.ts` covering:
  - **CASE:** dashboard API reads system, network, firmware, ecosystems, and device.state
    - **UNIT:** `fetchStatus`
    - **GIVEN:** mocked `/api/status` response returning the five-bucket schema
    - **WHEN:** dashboard store polls `/api/status`
    - **THEN:**
      - `status.system.status`, `status.system.memory`, `status.system.cpu`, `status.network`, `status.firmware`, and `status.ecosystems` are populated
      - UI/store consumers read `device.state.door_locked`, `rx_gain_half_db`, `tx_gain_half_db`
  Verify failure with: `cd crates/intercom-adapter-http/frontend && bun test`
- [x] 2.2 [GREEN] Update `crates/intercom-adapter-http/frontend/src/types.ts`, `src/api.ts`, `src/store/*`, and `src/components/*` to consume `status.system.*`, `status.network.*`, `status.firmware.*`, `status.ecosystems.*`, and `status.device.*`, then rebuild `assets/dashboard.html`, verifying with: `cd crates/intercom-adapter-http/frontend && bun test && cd ../../.. && mise exec -- cargo fe-check && mise exec -- cargo fe-build`
- [x] 2.3 Update `scripts/ota.py` (`status["device"]["type"]`, `status["firmware"]`, etc.) and `scripts/test_matter_tcp.py` (`status["system"]["status"]`) for the updated `/api/status` schema, and verify full host test suite + ESP32-C6 release firmware build: `mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin && mise exec -- cargo build -p intercom-firmware --release`
