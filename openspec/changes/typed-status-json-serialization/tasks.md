# Tasks (Wave 2 — `intercom-matter` `v1`)

> **Prerequisite (Wave 1 Blocker):** Complete and verify Wave 1 (`embedded-device-foundation`) before executing this checklist.

## 1. Intercom HTTP Adapter (`crates/intercom-adapter-http`)

- [x] 1.1 [RED] Update and add unit tests in `crates/intercom-adapter-http/tests/router_tests.rs` covering:
  - **CASE:** formats intercom status JSON with five buckets, zero legacy root keys, and 2-field `device.state`
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** operational `SystemDiagnostics<IntercomState>` with fabric and endpoint metadata
    - **WHEN:** status JSON is formatted into `JSON_BUFFER_SIZE` (`4096`)
    - **THEN:**
      - root keys are strictly `system`, `network`, `firmware`, `ecosystems`, `device` (no legacy root keys)
      - `device.type == "intercom_bridge"` and `device.state` contains only `door_locked` and `door_unlock_duration_sec`
      - `system.status`, `system.uptime_sec`, `system.memory`, `system.cpu` (`{}` when empty), `network.*`, `firmware.*`, and `ecosystems.*` match input
  - **CASE:** includes explicit memory (including PSRAM) and multi-core CPU telemetry
    - **UNIT:** `intercom_adapter_http::router::format_status_json_with_telemetry`
    - **GIVEN:** `MemorySnapshot` (with `psram: Some(...)`) and `CpuSnapshot` (dual-core `core_0`, `core_1`)
    - **WHEN:** status JSON is formatted into `JSON_BUFFER_SIZE`
    - **THEN:**
      - `system.memory.internal` and `system.memory.psram` match input
      - `system.cpu.core_0.load_pct` and `system.cpu.core_1.load_pct` match input
  - **CASE:** escapes control characters, quotes, backslashes, and UTF-8 strings (e.g. `"雪"`)
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** `SystemDiagnostics` containing quotes, backslashes, control characters, and `"雪"`
    - **WHEN:** status JSON is formatted and parsed back
    - **THEN:** decoded strings match original UTF-8 strings verbatim
  - **CASE:** rejects undersized buffer (`64` bytes) without panicking and fits maximal ecosystem capacity within `JSON_BUFFER_SIZE` (`4096`)
    - **UNIT:** `intercom_adapter_http::router::format_status_json`
    - **GIVEN:** a 64-byte buffer, and separately a maximal `EcosystemStatus` (6 properties, 5 fabrics x 4 subscriptions, 4 endpoints x 6 clusters)
    - **WHEN:** status JSON is formatted
    - **THEN:** 64-byte buffer returns `Err(())`, and maximal payload succeeds with `len <= JSON_BUFFER_SIZE`
  Verify failure with: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
- [x] 1.2 [GREEN] Update `crates/intercom-adapter-http/src/{config.rs,lib.rs,router.rs}` (`JSON_BUFFER_SIZE = 4096`, `#[derive(serde::Serialize)] IntercomStatusDto`, `From<&IntercomState>`, `format_status_json`, `format_status_json_with_telemetry`, re-exports of `MemoryRegion`, `MemorySnapshot`, `CpuCoreLoad`, `CpuSnapshot`, `MAX_CPU_CORES`, `parse_route`, `Route`), verifying all tests pass with: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
- [x] 1.3 [RED] Update `crates/intercom-adapter-http/tests/server_tests.rs` (`test_get_status_returns_valid_telemetry_json`) covering:
  - **CASE:** `GET /api/status` matches golden five-bucket `serde_json::Value` snapshot (`assert_eq!(actual, expected)`)
    - **UNIT:** `GET /api/status`
    - **GIVEN:** running `HttpServer` with `MockHardwareInfo` and default `IntercomState`
    - **WHEN:** client sends `GET /api/status` and compares parsed JSON against expected five-bucket snapshot
    - **THEN:**
      - HTTP status == 200 and `Content-Type == "application/json"`
      - `actual == expected` across all five buckets (`system`, `network`, `firmware`, `ecosystems`, `device`) with zero stray legacy keys
  Verify failure with: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
- [x] 1.4 [GREEN] Update `/api/status` handler in `crates/intercom-adapter-http/src/server.rs` to construct `MemorySnapshot` (`internal` from `diag.free_heap_bytes`/`diag.total_heap_bytes`, `psram: None`) and `CpuSnapshot::from_cores(&[])` and call `crate::router::format_status_json_with_telemetry(&diag, memory, cpu, &mut json_buf)`, verifying tests pass with: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`

## 2. Intercom Solid.js Dashboard, Scripts & Firmware Verification

- [x] 2.1 [RED] Add `crates/intercom-adapter-http/frontend/tests/status-api.test.ts` and `crates/intercom-adapter-http/frontend/tests/types-contract.ts` covering:
  - **CASE:** `fetchStatus` and `createIntercomStore().poll()` read five-bucket `/api/status` and `device.state.{door_locked,door_unlock_duration_sec}`, and `tsc` rejects legacy flat keys via `@ts-expect-error`
    - **UNIT:** `fetchStatus`, `createIntercomStore`, `IntercomDiagnostics`
    - **GIVEN:** mocked `/api/status` returning the five-bucket schema
    - **WHEN:** `fetchStatus()` and `store.poll()` run and `tsc` checks `tests/types-contract.ts`
    - **THEN:** `store.doorUnlockDurationSec()` and `store.isUnlocking()` react to `device.state.*` and `tsc` verifies `IntercomDiagnostics`
  Verify failure with: `(cd crates/intercom-adapter-http/frontend && bun test tests/status-api.test.ts)`
- [x] 2.2 [GREEN] Update `crates/intercom-adapter-http/frontend/src/{types.ts,api.ts,store/intercomStore.ts,App.tsx}` to consume `system.*`, `network.*`, `firmware.*`, `ecosystems.*`, and `device.state.{door_locked,door_unlock_duration_sec}`, then rebuild `assets/dashboard.html`, verifying with: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo fe-check && CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo fe-build && (cd crates/intercom-adapter-http/frontend && bun test)`
- [x] 2.3 Update `scripts/ota.py` (`device.type`, `network.mac`, `firmware.version`, `firmware.active_slot`), ensure `crates/intercom-firmware/ota_public_key.bin` exists (`python3 scripts/generate_keys.py`), and verify full host tests + lint/fmt + frontend + ESP32-C6 release and 4 MB `flash-4mb` firmware builds:
  - `python3 -m py_compile scripts/ota.py`
  - `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`
  - `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo clippy --no-deps -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`
  - `mise exec -- rustfmt --edition 2021 --check crates/intercom-adapter-http/src/*.rs crates/intercom-adapter-http/tests/*.rs`
  - `(cd crates/intercom-adapter-http/frontend && bun test)`
  - `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo fe-check && CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo fe-build`
  - `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo build -p intercom-firmware --release`
  - `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target-v1 mise exec -- cargo build -p intercom-firmware --release --features flash-4mb`
