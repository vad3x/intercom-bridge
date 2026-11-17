# Tasks: ESP32-S31 Firmware & Multi-Core Platform Migration

## 1. Dual-Target PlatformDriver Trait & Feature Architecture

- [ ] 1.1 Configure feature bubbling across `Cargo.toml`, `intercom-adapter-gpio`, `intercom-adapter-matter`, and `intercom-firmware` (`esp32c6` default vs `esp32s31`), verifying clean dependency resolution
- [ ] 1.2 Define formal `PlatformDriver` trait and `PlatformDrivers` bundle in `crates/intercom-firmware/src/platform/mod.rs`, implementing `C6Platform`, `S31Platform`, and `MockPlatform`
- [ ] 1.3 Centralize S31 GPIO pin declarations in `src/platform/s31.rs` for easy future PCB trace alignment, verifying no scattered pin numbers across adapter crates
- [ ] 1.4 Refactor `crates/intercom-firmware/src/main.rs` to interact strictly via `ActivePlatform: PlatformDriver` with zero target-specific `#[cfg]` checks in `main()`

## 2. Media Types & Multi-Core IPC Abstractions

- [ ] 2.1 Define `OpusPacket` and `OpusChannel` in `crates/intercom-media` with bounded capacity, verifying host unit tests pass
- [ ] 2.2 Implement 10 ms linear ramp helpers (`apply_ramp_in`, `apply_ramp_out`) and 30 ms relay settling in `crates/intercom-media`, verifying sample fade accuracy in tests
- [ ] 2.3 Implement Core 1 (`AppCpu`) Embassy executor and async audio worker task in `S31Platform`, verifying clean compilation under RISC-V target
- [ ] 2.4 Connect Core 1 audio worker to shared `CALL_STATE_WATCH` and `SYSTEM_STATE_WATCH` receivers directly, verifying cross-core state propagation

## 3. PSRAM Jitter Buffer & Tiered Memory Allocation

- [ ] 3.1 Implement sequence-ordered `JitterBuffer<N>` with Opus Packet Loss Concealment (PLC) in `crates/intercom-adapter-webrtc`, verifying out-of-order packet reordering in host unit tests
- [ ] 3.2 Configure `esp-alloc` external heap allocator for DDR Octal PSRAM under `S31Platform` with fail-fast boot-time panic check, verifying initialization telemetry
- [ ] 3.3 Update `/api/status` schema and `HardwareInfoDriver` to report structured `"memory"` object with internal and PSRAM sub-maps, verifying output via HTTP curl and integration tests

## 4. Audio Engine & Sustained Streaming Benchmarking

- [ ] 4.1 Benchmark Opus SILK encoding execution duration on Core 1 at 320 MHz, verifying frame completion latency under 5 ms and zero stack overflow
- [ ] 4.2 Verify continuous WebRTC listen stream stability (>180 seconds) without heap or stack degradation
- [ ] 4.3 Verify 100 ms PSRAM jitter buffer absorption during simulated packet delay spikes without audio underrun

## 5. Dual-Target Build & OTA Validation

- [ ] 5.1 Configure symmetric 3.4 MB dual-bank partition table (`partitions.csv`) compatible with 8 MB/16 MB flash across both C6 and S31 builds
- [ ] 5.2 Validate Ed25519 firmware image signing and bootloader rollback watchdog on both target configurations using `scripts/ota.py`
- [ ] 5.3 Run automated CI test suite for both feature sets (`cargo test --features esp32c6` and `cargo test --features esp32s31 --lib --target x86_64-apple-darwin`), verifying all tests pass

## Workflow follow-up

- Validate OpenSpec change with `openspec validate esp32-s31-migration`
- Proceed to implementation via `/opsx-apply` when hardware prototypes and upstream HAL targets are ready
