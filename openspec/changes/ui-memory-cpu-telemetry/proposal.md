# Proposal

## Why

Multi-target ESP32 deployments (single-core ESP32-C6 without PSRAM vs. dual-core ESP32-S3/S31 with external PSRAM) expose distinct memory pools and CPU core counts. Currently, the web dashboard's Diagnostics tab only renders a single flat `Heap Memory` bar from `free_heap_bytes`/`total_heap_bytes` and displays no CPU load telemetry, hiding PSRAM utilization and per-core DSP/network saturation.

## What Changes

- **BREAKING:** Replace `free_heap_bytes`/`total_heap_bytes` in the diagnostics schema and `@embedded-device-foundation/dashboard-core` (`DiagnosticsTab` and `SystemDiagnosticsBase`) with a structured `memory` map of named pools (e.g., `internal` and optional `psram`), rendering one bar per non-null pool without legacy fallback.
- Add per-core CPU load reporting (`cpu: { core_0: { load_pct }, ... }`) to `/api/status` in `intercom-adapter-http`, backed by lightweight Embassy per-core idle/busy window instrumentation in `intercom-firmware` (1 core on ESP32-C6, 2 cores on ESP32-S3/S31).
- Render dynamic per-core CPU load bars in `DiagnosticsTab` when `status.cpu` is present.

## Capabilities

### New Capabilities
- `telemetry/dynamic-memory-cpu`: Dynamic per-pool memory reporting/rendering (`internal`, `psram`, or named pools) and per-core CPU load measurement and visualization across single-core and multi-core targets.

### Modified Capabilities
<!-- No existing specs in openspec/specs/ are modified. -->

## Impact

- `../embedded-device-foundation/packages/dashboard-core`: `src/types.ts` (`SystemDiagnosticsBase`), `src/DiagnosticsTab.tsx`, `src/presentation.ts`.
- `crates/intercom-adapter-http`: `src/router.rs` (`format_status_json_with_memory` / CPU telemetry serialization), `frontend/src/types.ts`.
- `crates/intercom-firmware`: Per-core idle/busy window sampler for Embassy executors (single-core C6, dual-core S3/S31) and diagnostics provider wiring.
