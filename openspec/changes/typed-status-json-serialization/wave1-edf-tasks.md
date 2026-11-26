# Wave 1 Blocker Tasks: `embedded-device-foundation`

> **Swarm Gate:** Complete and verify all tasks below in `../embedded-device-foundation` **before** launching Wave 2 parallel workers for `intercom-matter` and `hvac-bridge`.

## 1. Shared `edf-http-diagnostics` Typed DTOs (`../embedded-device-foundation`)

- [x] 1.1 [RED] Write failing unit tests in `../embedded-device-foundation/crates/edf-http-diagnostics/src/json.rs` covering:
  - **CASE:** serializes five root buckets with required memory and cpu
    - **UNIT:** `edf_http_diagnostics::format_status_json`
    - **GIVEN:** operational `SystemDiagnostics` with `MemorySnapshot` and `CpuSnapshot` (`core_0`)
    - **WHEN:** status JSON is serialized into a 2048-byte buffer
    - **THEN:**
      - `system.status == "Operational"` and `system.uptime_sec == 42`
      - `system.memory.internal.free_bytes` and `system.memory.internal.total_bytes` are present
      - `system.memory.psram` is present (`object` or `null`)
      - `system.cpu.core_0.load_pct` is present
      - `network` contains `ssid`, `rssi_dbm`, `ip`, `mac`
      - `firmware` contains `version`, `git_commit`, `build_date`, `active_slot`, `slot_valid`
      - `ecosystems` contains `available` and `active`
      - `device.type` and `device.state` are present
      - legacy root keys (`device_type`, `free_heap_bytes`, `total_heap_bytes`, `wifi_ssid`) are absent
  - **CASE:** escapes control bytes and preserves UTF-8 in all string fields
    - **UNIT:** `edf_http_diagnostics::format_status_json`
    - **GIVEN:** `SystemDiagnostics` containing quotes, backslashes, `\n`, `\0`, `\u{001f}`, and `"雪"` across `network.ssid`, `ecosystems.active`, and `device.state`
    - **WHEN:** status JSON is serialized and parsed back as JSON
    - **THEN:**
      - output is valid JSON
      - decoded strings equal original UTF-8 inputs
  - **CASE:** returns buffer-too-small error without partial success on overflow
    - **UNIT:** `edf_http_diagnostics::format_status_json`
    - **GIVEN:** an 8-byte output buffer
    - **WHEN:** status JSON is serialized
    - **THEN:**
      - returns `BufferTooSmall` error without reporting a written byte count
  - **CASE:** serializes null for absent optional network and ecosystem fields
    - **UNIT:** `edf_http_diagnostics::format_status_json`
    - **GIVEN:** `SystemDiagnostics` with `wifi_ssid == None`, `wifi_rssi_dbm == None`, `ip == None`, `psram == None`, and `active_ecosystem == None`
    - **WHEN:** status JSON is serialized
    - **THEN:**
      - `network.ssid == null`
      - `network.rssi_dbm == null`
      - `network.ip == null`
      - `system.memory.psram == null`
      - `ecosystems.active == null`
  Verify failure with: `cd ../embedded-device-foundation && mise exec -- cargo test -p edf-http-diagnostics --target x86_64-apple-darwin`
- [x] 1.2 [GREEN] Add `serde` (`default-features = false, features = ["derive"]`), `serde-json-core`, and `heapless` to `../embedded-device-foundation/crates/edf-http-diagnostics/Cargo.toml`, move `MemorySnapshot`, `MemoryRegion`, `CpuSnapshot`, and `CpuCoreLoad` into `edf-http-diagnostics`, and implement `StatusDto<'a, D: serde::Serialize, T>` (`SystemDto`, `NetworkDto`, `FirmwareDto`, `EcosystemsDto`, `DeviceDto`) + `serde_json_core::to_slice` in `src/json.rs` and `src/handler.rs` (removing `DiagnosticPayload` and manual `write_status_json`/`write_json_string`), verifying all tests pass with: `cd ../embedded-device-foundation && mise exec -- cargo test -p edf-http-diagnostics --target x86_64-apple-darwin`
- [x] 1.3 [RED] Write failing integration `serde_json` snapshot test in `../embedded-device-foundation/crates/edf-http-diagnostics/tests/status_json_tests.rs` covering:
  - **CASE:** matches golden five-bucket JSON snapshot via serde_json
    - **UNIT:** `edf_http_diagnostics::handle_diagnostic_route`
    - **GIVEN:** `DiagnosticRoute::Status` with `SystemDiagnostics`, `MemorySnapshot` (with PSRAM), and `CpuSnapshot` (dual-core)
    - **WHEN:** response body bytes are parsed with `serde_json` and compared against the expected five-bucket JSON snapshot
    - **THEN:**
      - parsed JSON equals the expected five-bucket snapshot (`system`, `network`, `firmware`, `ecosystems`, `device`) with exact values and no stray legacy keys
  Verify failure with: `cd ../embedded-device-foundation && mise exec -- cargo test -p edf-http-diagnostics --test status_json_tests --target x86_64-apple-darwin`
- [x] 1.4 [GREEN] Wire `MemorySnapshot` and `CpuSnapshot` through `edf_http_diagnostics::handle_diagnostic_route` so `status_json_tests` passes, verifying with: `cd ../embedded-device-foundation && mise exec -- cargo test -p edf-http-diagnostics --target x86_64-apple-darwin`

## 2. Shared `dashboard-core` TypeScript Types (`../embedded-device-foundation`)

- [x] 2.1 [RED] Write/update failing TypeScript test/check in `../embedded-device-foundation/packages/dashboard-core`:
  - **CASE:** parses five-bucket status with system.status and device.state
    - **UNIT:** `SystemDiagnosticsBase`
    - **GIVEN:** a five-bucket `/api/status` JSON payload
    - **WHEN:** type-checked or parsed by `dashboard-core`
    - **THEN:**
      - `system` requires `status`, `uptime_sec`, `memory`, `cpu`
      - `network` requires `ssid`, `rssi_dbm`, `ip`, `mac`
      - `firmware` requires `version`, `git_commit`, `build_date`, `active_slot`, `slot_valid`
      - `ecosystems` requires `available` and `active`
      - `device` requires `type` (string) and `state` (device-specific state object)
  Verify failure with: `cd ../embedded-device-foundation && bun test`
- [x] 2.2 [GREEN] Update `SystemDiagnosticsBase<D>` and shared components in `../embedded-device-foundation/packages/dashboard-core` to use `system`, `network`, `firmware`, `ecosystems`, and `device`, verifying checks pass with: `cd ../embedded-device-foundation && bun test`
