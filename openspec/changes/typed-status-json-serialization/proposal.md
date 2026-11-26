# Proposal

## Why

`/api/status` currently builds JSON by manual `core::fmt::Write` string concatenation in `edf-http-diagnostics` and `IntercomPayload` (`DiagnosticPayload`) in `intercom-adapter-http`. Moving to typed `serde` DTOs serialized with `serde-json-core` in `edf-http-diagnostics` and `intercom-adapter-http` eliminates hand-rolled JSON formatting, organizes `/api/status` into five symmetrical buckets (`system`, `network`, `firmware`, `ecosystems`, `device`), and includes shared `memory` and `cpu` telemetry uniformly across all EDF-based bridges.

## What Changes

- **Common `edf-http-diagnostics` (`embedded-device-foundation`)**:
  - Replace manual `write_status_json` / `write_json_string` / `DiagnosticPayload` string concatenation with typed `#[derive(Serialize)]` DTOs serialized via `serde-json-core` (`no_std`, zero heap allocation).
  - Move `MemorySnapshot`, `MemoryRegion`, `CpuSnapshot`, and `CpuCoreLoad` into `edf-http-diagnostics` and make both `memory` and `cpu` required fields on the shared status DTO.
  - **BREAKING**: Reorganize `/api/status` into five symmetrical top-level buckets (`system`, `network`, `firmware`, `ecosystems`, `device`):
    - `system`: `{ status, uptime_sec, memory, cpu }` (renaming `state` to `status` and removing duplicate `free_heap_bytes`/`total_heap_bytes`)
    - `network`: `{ ssid, rssi_dbm, ip, mac }`
    - `firmware`: `{ version, git_commit, build_date, active_slot, slot_valid }`
    - `ecosystems`: `{ available, active }`
    - `device`: `{ type, state }` (`DeviceDto<'a, D>` in `edf-http-diagnostics`)
- **Intercom HTTP Adapter (`intercom-adapter-http` on `v1`)**:
  - Replace `IntercomPayload` (`DiagnosticPayload`) and `shared_snapshot` in `crates/intercom-adapter-http/src/router.rs` with a typed 2-field `#[derive(Serialize)]` `IntercomStatusDto { door_locked, door_unlock_duration_sec }` mapped from `IntercomState` at the HTTP adapter boundary (keeping `intercom-core` free of `serde`).
  - Re-export `MemoryRegion`, `MemorySnapshot`, `CpuCoreLoad`, `CpuSnapshot`, and `MAX_CPU_CORES` from `intercom_adapter_http::router` and `lib.rs`.
  - Bump `JSON_BUFFER_SIZE` from `2560` to `4096` in `crates/intercom-adapter-http/src/config.rs` so maximal 5-fabric / 4-endpoint payloads never truncate.
  - Supply `MemorySnapshot` (`internal: { free_bytes, total_bytes }`, `psram: None`) and `CpuSnapshot::from_cores(&[])` (`{}`) in `crates/intercom-adapter-http/src/server.rs` and `format_status_json` / `format_status_json_with_telemetry`.
- **Frontend & Scripts (`intercom-matter` `v1`)**:
  - **BREAKING**: Update the Solid.js dashboard (`frontend/src/types.ts`, `frontend/src/api.ts`, `frontend/src/store/intercomStore.ts`, `frontend/src/App.tsx`, and `frontend/tests/{status-api.test.ts,types-contract.ts}`) to read `status.system.*`, `status.network.*`, `status.firmware.*`, `status.ecosystems.*`, and `status.device.state.{door_locked,door_unlock_duration_sec}`, and rebuild `assets/dashboard.html` (verified by `dashboard-asset.test.ts`).
  - Update `scripts/ota.py` (`status["device"]["type"]`, `status["network"]["mac"]`, `status["firmware"]["version"]`, `status["firmware"]["active_slot"]`).

## Capabilities

### New Capabilities
- `http-diagnostics-status`: Typed `serde` + `serde-json-core` serialization for `/api/status` with shared `edf-http-diagnostics` DTOs, required `memory` and `cpu` telemetry, and nested `"device"` payloads.

### Modified Capabilities
<!-- None: no existing specs in openspec/specs/ cover /api/status JSON serialization. -->

## Impact

- `../embedded-device-foundation/crates/edf-http-diagnostics`: Wave 1 prerequisite (`StatusDto`, `MemorySnapshot`, `CpuSnapshot`, `format_status_json`, `packages/dashboard-core`).
- `crates/intercom-adapter-http`: `src/config.rs`, `src/lib.rs`, `src/router.rs`, `src/server.rs`, `tests/router_tests.rs`, `tests/server_tests.rs`.
- `crates/intercom-adapter-http/frontend`: `src/types.ts`, `src/api.ts`, `src/store/intercomStore.ts`, `src/App.tsx`, `tests/status-api.test.ts`, `tests/types-contract.ts`, `tests/dashboard-asset.test.ts`, and compiled `assets/dashboard.html`.
- `scripts/ota.py`.
