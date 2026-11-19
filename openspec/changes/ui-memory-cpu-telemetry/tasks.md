# Tasks

## 1. Shared Dashboard Core (`../embedded-device-foundation/packages/dashboard-core`)

- [ ] 1.1 Replace `free_heap_bytes`/`total_heap_bytes` on `SystemDiagnosticsBase` in `src/types.ts` with `MemoryPoolStats`, `CpuCoreLoad`, `memory`, and `cpu`
- [ ] 1.2 Add helper(s) in `src/presentation.ts` to format `MemoryPoolStats` (used/total KB, percentage, pool label mapping)
- [ ] 1.3 Update `src/DiagnosticsTab.tsx` to dynamically render one memory bar per non-null pool in `status.memory` (removing the legacy `Heap Memory` fallback)
- [ ] 1.4 Update `src/DiagnosticsTab.tsx` to dynamically render one CPU load bar per entry in `status.cpu?.cores`

## 2. Firmware Per-Core CPU Load (`crates/intercom-firmware`)

- [ ] 2.1 Implement a zero-alloc per-core CPU load sampler (`AtomicU8` per core over a 1s window) compatible with single-core ESP32-C6 and dual-core ESP32-S3/S31 Embassy executors
- [ ] 2.2 Wire the CPU snapshot provider into `HttpServer` alongside `memory_info`

## 3. HTTP Serialization & Frontend Types (`crates/intercom-adapter-http`)

- [ ] 3.1 Extend `router.rs` to serialize `cpu: { cores: [{ id, load_pct }, ...] }` into `/api/status` JSON and verify `JSON_BUFFER_SIZE` headroom
- [ ] 3.2 Add host unit tests in `router.rs` covering `memory` (with/without PSRAM) and `cpu.cores` (1 core and 2 cores)
- [ ] 3.3 Rebuild frontend (`cargo fe-check && cargo fe-build`) and verify `DiagnosticsTab` renders dynamic memory and CPU bars
