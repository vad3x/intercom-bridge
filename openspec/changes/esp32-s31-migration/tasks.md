# Tasks: ESP32-S31 Firmware & Multi-Core Platform Migration

## 1. Dual-Target Feature Configuration & Platform Encapsulation

- [ ] 1.1 Configure mutually exclusive `esp32c6` (default) and `esp32s31` features in `crates/intercom-firmware/Cargo.toml` and verify default resolution
- [ ] 1.2 Refactor platform initialization into modular submodules (`platform/c6.rs` and `platform/s31.rs`) behind a common `PlatformServices` trait, verifying clean compilation for both targets
- [ ] 1.3 Centralize S31 GPIO pin declarations in `src/platform/s31.rs` for easy future PCB trace alignment, verifying no scattered pin numbers across adapter crates

## 2. Multi-Core Architecture & Embassy IPC Abstractions (S31)

- [ ] 2.1 Define typed audio frame structs (`OpusPacket`, `PcmFrame`) and configure bounded `embassy_sync::channel::Channel<CriticalSectionRawMutex, _, _>` in `crates/intercom-core`, verifying with host unit tests
- [ ] 2.2 Implement Core 1 (`AppCpu`) Embassy executor and async audio worker task gated under `cfg(feature = "esp32s31")`, verifying compile check on RISC-V target
- [ ] 2.3 Implement half-duplex direction switching interlock between terminal 3 (listen) and terminal 2 (talk) on Core 1, verifying click-free mute/unmute ramps

## 3. Tiered PSRAM Memory Allocation & Fail-Fast Safety (S31)

- [ ] 3.1 Configure `esp-alloc` external heap allocator for DDR Octal PSRAM under `cfg(feature = "esp32s31")` with fail-fast boot-time panic check, verifying initialization telemetry
- [ ] 3.2 Update WebRTC jitter buffers and DTLS scratchpads to allocate dynamically from the PSRAM heap on S31, verifying internal SRAM remains >32 KiB
- [ ] 3.3 Implement memory telemetry reporting in `/api/status` distinguishing internal SRAM vs PSRAM free heap, verifying output via HTTP curl test

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
