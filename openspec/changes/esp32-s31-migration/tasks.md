# Tasks: ESP32-S31 Hardware & Audio Platform Migration

## 1. Dual-Target Feature Configuration & Platform Encapsulation

- [ ] 1.1 Configure mutually exclusive `esp32c6` (default) and `esp32s31` features in `crates/intercom-firmware/Cargo.toml` and verify feature resolution
- [ ] 1.2 Refactor platform initialization into modular submodules (`platform/c6.rs` and `platform/s31.rs`) behind a common `PlatformServices` trait, verifying clean compilation for both targets
- [ ] 1.3 Update edge adapter crates (`intercom-adapter-gpio`, `intercom-adapter-webrtc`) with target feature gates, verifying host unit tests pass on macOS

## 2. Hardware Schematic & Module Footprint (EasyEDA)

- [ ] 2.1 Create ESP32-S31-WROOM module footprint and schematic symbol in EasyEDA and verify pin numbering against Espressif S31 datasheet
- [ ] 2.2 Update intercom board schematic to wire S31 GPIOs to optocoupler ring input, PhotoMOS strike relay, status LED, boot button, and I2S codec lines, verifying ERC checks pass
- [ ] 2.3 Design power decoupling network (minimum 1.0A 3.3V LDO regulator and 22uF + 10uF + 0.1uF low-ESR ceramic capacitor array) and verify power tree analysis
- [ ] 2.4 Generate updated manufacturing package (Gerbers, BOM, Pick-and-Place) and verify DRC clearance rules pass

## 3. Multi-Core Architecture & Embassy IPC Abstractions (S31)

- [ ] 3.1 Define typed audio frame structs (`OpusPacket`, `PcmFrame`) and configure bounded `embassy_sync::channel::Channel<CriticalSectionRawMutex, _, _>` in `crates/intercom-core`, verifying with host unit tests
- [ ] 3.2 Implement Core 1 (`AppCpu`) Embassy executor and async audio worker task gated under `cfg(feature = "esp32s31")`, verifying compile check on RISC-V target
- [ ] 3.3 Implement half-duplex direction switching interlock between terminal 3 (listen) and terminal 2 (talk) on Core 1, verifying click-free mute/unmute ramps

## 4. Tiered PSRAM Memory Allocation (S31)

- [ ] 4.1 Configure `esp-alloc` external heap allocator for DDR Octal PSRAM under `cfg(feature = "esp32s31")`, verifying heap initialization telemetry
- [ ] 4.2 Update WebRTC jitter buffers and DTLS scratchpads to allocate dynamically from the PSRAM heap on S31, verifying internal SRAM remains >32 KiB
- [ ] 4.3 Implement memory telemetry reporting in `/api/status` distinguishing internal SRAM vs PSRAM free heap, verifying output via HTTP curl test

## 5. Dual-Target Build & OTA Validation

- [ ] 5.1 Configure symmetric 3.4 MB dual-bank partition table (`partitions.csv`) compatible with 8 MB/16 MB flash across both C6 and S31 builds
- [ ] 5.2 Validate Ed25519 firmware image signing and bootloader rollback watchdog on both target configurations using `scripts/ota.py`
- [ ] 5.3 Run automated CI test suite for both feature sets (`cargo test --features esp32c6` and `cargo test --features esp32s31 --lib --target x86_64-apple-darwin`), verifying all tests pass

## Workflow follow-up

- Validate OpenSpec change with `openspec validate esp32-s31-migration`
- Proceed to implementation via `/opsx-apply` when hardware prototypes and upstream HAL targets are ready
