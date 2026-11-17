# Tasks: ESP32-S31 Firmware & Multi-Core Platform Migration

## 1. Dual-Target PlatformDriver Trait & Feature Architecture

- [ ] 1.1 Configure feature bubbling in Cargo manifests (`esp32c6` default vs `esp32s31`), verifying clean dependency resolution with: `mise exec -- cargo check -p intercom-firmware --features esp32c6 && mise exec -- cargo check -p intercom-firmware --no-default-features --features esp32s31`
- [ ] 1.2 [RED] Create failing host integration test for `PlatformDriver` in `crates/intercom-firmware/tests/platform_driver_tests.rs` asserting `MockPlatform` lifecycle and task execution, verifying failure with: `mise exec -- cargo test -p intercom-firmware --test platform_driver_tests --target x86_64-apple-darwin`
- [ ] 1.3 [GREEN] Define formal `PlatformDriver` trait and implement `C6Platform`, `S31Platform`, and `MockPlatform` in `crates/intercom-firmware/src/platform/`, verifying tests pass with: `mise exec -- cargo test -p intercom-firmware --test platform_driver_tests --target x86_64-apple-darwin`
- [ ] 1.4 Centralize S31 GPIO pin declarations in `src/platform/s31.rs` for future PCB trace alignment, verifying clean compilation with: `mise exec -- cargo check -p intercom-firmware --features esp32s31`
- [ ] 1.5 Refactor `crates/intercom-firmware/src/main.rs` to interact strictly via `ActivePlatform: PlatformDriver` with zero target-specific `#[cfg]` checks in `main()`, verifying host build passes with: `mise exec -- cargo check -p intercom-firmware --target x86_64-apple-darwin`

## 2. Media Types & Multi-Core IPC Abstractions

- [ ] 2.1 [RED] Write unit test for `OpusPacket` and `OpusChannel` in `crates/intercom-media/tests/opus_channel_tests.rs` asserting capacity 4, Full on 5th frame, and payload integrity, verifying failure with: `mise exec -- cargo test -p intercom-media --test opus_channel_tests --target x86_64-apple-darwin`
- [ ] 2.2 [GREEN] Define `OpusPacket` and `OpusChannel` in `crates/intercom-media/src/lib.rs`, verifying test passes with: `mise exec -- cargo test -p intercom-media --test opus_channel_tests --target x86_64-apple-darwin`
- [ ] 2.3 [RED] Write unit test for 10 ms linear ramp helpers in `crates/intercom-media/tests/ramp_tests.rs` asserting exact 160-sample fade-in and fade-out multipliers, verifying failure with: `mise exec -- cargo test -p intercom-media --test ramp_tests --target x86_64-apple-darwin`
- [ ] 2.4 [GREEN] Implement `apply_ramp_in` and `apply_ramp_out` in `crates/intercom-media`, verifying test passes with: `mise exec -- cargo test -p intercom-media --test ramp_tests --target x86_64-apple-darwin`
- [ ] 2.5 Implement Core 1 (`AppCpu`) Embassy executor and async audio worker task in `S31Platform`, verifying compile check on RISC-V target with: `mise exec -- cargo check -p intercom-firmware --features esp32s31`
- [ ] 2.6 Connect Core 1 audio worker to shared `CALL_STATE_WATCH` and `SYSTEM_STATE_WATCH` receivers, verifying state propagation with: `mise exec -- cargo test -p intercom-firmware --test platform_driver_tests --target x86_64-apple-darwin`

## 3. PSRAM Jitter Buffer & Tiered Memory Allocation

- [ ] 3.1 [RED] Write unit test for `JitterBuffer<5>` in `crates/intercom-adapter-webrtc/tests/jitter_buffer_tests.rs` asserting in-order playout, sequence reordering, and `None` emission on loss for PLC, verifying failure with: `mise exec -- cargo test -p intercom-adapter-webrtc --test jitter_buffer_tests --target x86_64-apple-darwin`
- [ ] 3.2 [GREEN] Implement `JitterBuffer<N>` in `crates/intercom-adapter-webrtc`, verifying test passes with: `mise exec -- cargo test -p intercom-adapter-webrtc --test jitter_buffer_tests --target x86_64-apple-darwin`
- [ ] 3.3 Configure `esp-alloc` external heap allocator for DDR Octal PSRAM under `S31Platform` with fail-fast boot-time panic check, verifying compile check with: `mise exec -- cargo check -p intercom-firmware --features esp32s31`
- [ ] 3.4 [RED] Add test case `test_status_memory_schema` in `crates/intercom-adapter-http/tests/server_tests.rs` asserting structured `"memory"` object in `/api/status`, verifying failure with: `mise exec -- cargo test -p intercom-adapter-http --test server_tests test_status_memory_schema --target x86_64-apple-darwin`
- [ ] 3.5 [GREEN] Update `/api/status` serializer and `HardwareInfoDriver` to emit structured `"memory"` JSON with internal and PSRAM maps, verifying test passes with: `mise exec -- cargo test -p intercom-adapter-http --test server_tests test_status_memory_schema --target x86_64-apple-darwin`

## 4. Audio Engine & Sustained Streaming Benchmarking

- [ ] 4.1 Benchmark Opus SILK encoding execution duration on Core 1 at 320 MHz, verifying frame completion latency under 5 ms and zero stack overflow with: `python3 scripts/monitor.py --no-reset`
- [ ] 4.2 Verify continuous WebRTC listen stream stability (>180 seconds) without heap or stack degradation using: `python3 scripts/test_matter_tcp.py`
- [ ] 4.3 Verify 100 ms PSRAM jitter buffer absorption during simulated packet delay spikes without audio underrun using: `cargo test -p intercom-adapter-webrtc --test jitter_buffer_tests --target x86_64-apple-darwin`

## 5. Dual-Target Build & OTA Validation

- [ ] 5.1 Configure symmetric 3.4 MB dual-bank partition table (`partitions.csv`) compatible with 8 MB/16 MB flash across both C6 and S31 builds, verifying partition table with: `espflash partition-table partitions.csv`
- [ ] 5.2 Validate Ed25519 firmware image signing and bootloader rollback watchdog on both target configurations using: `python3 scripts/ota.py --help`
- [ ] 5.3 Run automated CI test suite for both feature sets, verifying all tests pass with: `mise exec -- cargo test -p intercom-core -p intercom-media -p intercom-adapter-http -p intercom-firmware --lib --target x86_64-apple-darwin`

## Workflow follow-up

- Validate OpenSpec change with `openspec validate esp32-s31-migration --strict`
- Proceed to implementation via `/opsx-apply esp32-s31-migration`
