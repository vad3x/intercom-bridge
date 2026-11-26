# Proposal

## Why

`/api/status` currently builds JSON by manual `core::fmt::Write` string concatenation in `edf-http-diagnostics` and fragile trailing-`}` string splicing in `intercom-adapter-http` to append `memory` and `cpu` telemetry. Moving to typed `serde` DTOs serialized with `serde-json-core` in `edf-http-diagnostics` and `intercom-adapter-http` eliminates hand-rolled JSON formatting, removes string splicing, and shares `memory` and `cpu` telemetry uniformly across all EDF-based bridges.

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
- **Intercom HTTP Adapter (`intercom-adapter-http`)**:
  - Replace `IntercomPayload` (`DiagnosticPayload`) and `ByteWriter` trailing-`}` splicing with a typed `#[derive(Serialize)]` `IntercomStatusDto` mapped from `IntercomState` at the HTTP adapter boundary (keeping `intercom-core` free of `serde`).
  - Pass required `MemorySnapshot` and `CpuSnapshot` directly to `edf-http-diagnostics`.
- **Frontend & Scripts (`intercom-matter`)**:
  - **BREAKING**: Update the Solid.js dashboard (`frontend/src/types.ts`, `frontend/src/api.ts`, `frontend/src/store/`, `frontend/src/components/`, and `frontend/tests/`) to read `status.system.*`, `status.network.*`, `status.firmware.*`, `status.ecosystems.*`, and `status.device.*`, and rebuild `assets/dashboard.html`.
  - Update `scripts/ota.py` (`status["device"]["type"]`, `status["firmware"]["version"]`, etc.) and `scripts/test_matter_tcp.py` (`status["system"]["status"]`).

## Capabilities

### New Capabilities
- `http-diagnostics-status`: Typed `serde` + `serde-json-core` serialization for `/api/status` with shared `edf-http-diagnostics` DTOs, required `memory` and `cpu` telemetry, and nested `"device"` payloads.

### Modified Capabilities
<!-- None: no existing specs in openspec/specs/ cover /api/status JSON serialization. -->

## Impact

- `../embedded-device-foundation/crates/edf-http-diagnostics`: `Cargo.toml`, `src/lib.rs`, `src/json.rs`, `src/handler.rs`, tests, and `packages/dashboard-core` TypeScript types if applicable.
- `crates/intercom-adapter-http`: `Cargo.toml`, `src/router.rs`, `src/server.rs`, `src/lib.rs`, `tests/router_tests.rs`, `tests/server_tests.rs`.
- `crates/intercom-adapter-http/frontend`: `src/types.ts`, `src/api.ts`, `src/store/*`, `src/components/*`, `tests/*`, and compiled `assets/dashboard.html`.
- `scripts/ota.py`, `scripts/test_matter_tcp.py`.
