# Design

## Context

- `codex/esp32-s3-target` already extends `intercom-adapter-http/src/router.rs` (`format_status_json_with_memory`) to append `"memory":{"internal":{"free_bytes":...,"total_bytes":...},"psram":...|null}` to `/api/status`.
- `@embedded-device-foundation/dashboard-core` (`../embedded-device-foundation/packages/dashboard-core/src/DiagnosticsTab.tsx`) currently only reads flat `free_heap_bytes` and `total_heap_bytes` from `SystemDiagnosticsBase` and renders a single `Heap Memory` row.
- No per-core CPU utilization telemetry currently exists in `intercom-firmware` or `intercom-adapter-http`.

## Goals / Non-Goals

**Goals:**
- Replace `free_heap_bytes`/`total_heap_bytes` with dynamic `status.memory` pool bars in `dashboard-core`'s `DiagnosticsTab` for any non-null pool (no legacy fallback required).
- Measure per-core CPU load on bare-metal Embassy executors (1 core on ESP32-C6, 2 cores on ESP32-S3/S31) with minimal overhead and zero heap allocation.
- Expose `cpu.cores` in `/api/status` and render per-core progress bars dynamically in `DiagnosticsTab`.

**Non-Goals:**
- Maintaining backward compatibility with flat `free_heap_bytes`/`total_heap_bytes` fields.
- Per-task CPU profiling or historical time-series charts.

## Decisions

1. **Generic `memory` pool rendering in `dashboard-core` (breaking schema update):**
   - Replace `free_heap_bytes` and `total_heap_bytes` on `SystemDiagnosticsBase` in `dashboard-core/src/types.ts` with `memory?: Record<string, MemoryPoolStats | null>` and `cpu?: { cores: CpuCoreLoad[] }`.
   - In `DiagnosticsTab.tsx`, iterate over `Object.entries(status.memory ?? {})` for non-null pools, mapping known keys (`internal` -> `Internal RAM`, `psram` -> `PSRAM`) and title-casing unknown keys.
   - *Alternative considered:* Hardcoding `internal` and `psram` slots in `intercom-matter`'s `App.tsx`, or keeping `free_heap_bytes`/`total_heap_bytes` as a fallback. Rejected because `DiagnosticsTab` is shared across EDF devices and removing the legacy fields keeps the schema clean and smaller on the wire.

2. **Per-core CPU load measurement via Embassy idle/busy window sampling:**
   - On Embassy (`no_std`), there is no FreeRTOS `vTaskGetRunTimeStats`. Instead, each core's executor runs a lowest-priority idle sampler or tracks active poll time vs. `embassy_time::Instant` elapsed over a fixed 1-second window, storing the latest `u8` percentage (`0..=100`) in an `AtomicU8` per core (`[AtomicU8; MAX_CORES]`).
   - On single-core targets (ESP32-C6), `CpuSnapshot` reports 1 core (`id: 0`); on dual-core targets (ESP32-S3/S31), it reports 2 cores (`id: 0`, `id: 1`).
   - *Alternative considered:* Counting `nop` loops in an idle task. Calibration drifts with flash cache misses and frequency scaling; wall-clock `Instant` active-vs-idle accounting is self-calibrating across clock frequencies.

3. **JSON serialization in `intercom-adapter-http`:**
   - Extend `router.rs` (`format_status_json_with_memory` or `format_status_json_with_telemetry`) to serialize both `memory` and `cpu: {"cores":[...]}` using `ByteWriter` on the stack with zero heap allocation, expanding `JSON_BUFFER_SIZE` if needed.

## Risks / Trade-offs

- **[Risk] Cross-repo dependency (`embedded-device-foundation`):** `dashboard-core` lives in `../embedded-device-foundation/packages/dashboard-core`. → **Mitigation:** Keep `memory` and `cpu` optional on `SystemDiagnosticsBase` so existing EDF consumers remain 100% backward-compatible.
- **[Risk] CPU sampler accuracy during tight blocking loops:** If an interrupt or blocking DSP routine (e.g., Opus encoding) hogs a core without yielding, an executor-level idle task only sees the starved window until it next runs. → **Mitigation:** Compute load from elapsed wall-clock time between idle ticks / window boundaries so a starved core naturally reports ~100% load once the window closes.
- **[Risk] `JSON_BUFFER_SIZE` overflow in `intercom-adapter-http`:** Adding `cpu.cores` increases `/api/status` payload size by ~40–60 bytes. → **Mitigation:** Verify `JSON_BUFFER_SIZE` and add a unit test in `router.rs` covering dual-core + PSRAM payloads.
