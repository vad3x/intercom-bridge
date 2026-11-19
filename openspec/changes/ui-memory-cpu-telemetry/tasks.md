# Tasks

## 1. Shared Dashboard Core (`../embedded-device-foundation/packages/dashboard-core`)

- [x] 1.1 Replace `free_heap_bytes`/`total_heap_bytes` on `SystemDiagnosticsBase` in `src/types.ts` with `MemoryPoolStats`, `CpuCoreStats`, `memory`, and `cpu`
- [x] 1.2 Add helper(s) in `src/presentation.ts` to format `MemoryPoolStats` (used/total KB, percentage, pool label mapping)
- [x] 1.3 Update `src/DiagnosticsTab.tsx` to dynamically render one memory bar per non-null pool in `status.memory` (removing the legacy `Heap Memory` fallback)
- [x] 1.4 Update `src/DiagnosticsTab.tsx` to dynamically render one CPU load bar per non-null entry in `status.cpu`

## 2. Firmware Per-Core CPU Load (`crates/intercom-firmware`)

- [x] 2.1 Implement a zero-alloc per-core CPU load sampler (`AtomicU8` per core over a 1s window) compatible with single-core ESP32-C6 and dual-core ESP32-S3/S31 Embassy executors
- [x] 2.2 Wire the CPU snapshot provider into `HttpServer` alongside `memory_info`

## 3. HTTP Serialization & Frontend Types (`crates/intercom-adapter-http`)

- [x] 3.1 Extend `router.rs` to serialize `cpu: { core_0: { load_pct }, ... }` into `/api/status` JSON and verify `JSON_BUFFER_SIZE` headroom
- [x] 3.2 Add host unit tests in `router.rs` covering `memory` (with/without PSRAM) and `cpu` (1 core and 2 cores)
- [x] 3.3 Rebuild frontend (`cargo fe-check && cargo fe-build`) and verify `DiagnosticsTab` renders dynamic memory and CPU bars
