# Tasks: ESP32-S31 Hardware & Audio Platform Migration

## 1. Hardware Schematic & Module Footprint (EasyEDA)

- [ ] 1.1 Create ESP32-S31-WROOM module footprint and schematic symbol in EasyEDA and verify pin numbering against Espressif S31 datasheet
- [ ] 1.2 Update intercom board schematic to wire S31 GPIOs to optocoupler ring input, PhotoMOS strike relay, status LED, boot button, and I2S codec lines, verifying ERC checks pass
- [ ] 1.3 Design power decoupling network (minimum 1.0A 3.3V LDO regulator and 22uF + 10uF + 0.1uF low-ESR ceramic capacitor array) and verify power tree analysis
- [ ] 1.4 Generate updated manufacturing package (Gerbers, BOM, Pick-and-Place) and verify DRC clearance rules pass

## 2. Multi-Core Architecture & Embassy IPC Abstractions

- [ ] 2.1 Define typed audio frame structs (`OpusPacket`, `PcmFrame`) and configure bounded `embassy_sync::channel::Channel<CriticalSectionRawMutex, _, _>` in `crates/intercom-core`, verifying with host unit tests
- [ ] 2.2 Create `AudioDspPort` and abstract multi-core orchestration trait, verifying host tests pass on `x86_64-apple-darwin`
- [ ] 2.3 Implement Core 1 (`AppCpu`) Embassy executor and async audio DSP worker task, verifying clean compilation under RISC-V target

## 3. Tiered PSRAM Memory Allocation

- [ ] 3.1 Configure `esp-alloc` external heap allocator for DDR Octal PSRAM in `crates/intercom-firmware`, verifying heap initialization telemetry
- [ ] 3.2 Update WebRTC jitter buffers and DTLS scratchpads to allocate dynamically from the PSRAM heap, verifying internal SRAM remains >32 KiB
- [ ] 3.3 Implement memory telemetry reporting in `/api/status` distinguishing internal SRAM vs PSRAM free heap, verifying output via HTTP curl test

## 4. Half-Duplex Audio Engine & Opus Offload

- [ ] 4.1 Implement half-duplex direction interlock between I2S DMA receive (terminal 3) and transmit (terminal 2) with click-free mute/unmute ramps, verifying clean audio switching
- [ ] 4.2 Benchmark Opus SILK encoding execution duration on Core 1 at 320 MHz, verifying frame completion latency under 5 ms and zero stack overflow
- [ ] 4.3 Verify continuous WebRTC listen stream stability (>180 seconds) without heap or stack degradation

## 5. Dual-Bank OTA Partitioning & Build Integration

- [ ] 5.1 Configure 8 MB / 16 MB partition table with symmetric `ota_0` / `ota_1` application slots for S31 flash, verifying `partitions.csv` geometry
- [ ] 5.2 Validate Ed25519 image signing and A/B bootloader rollback watchdog on the S31 partition scheme using `scripts/ota.py`
- [ ] 5.3 Run end-to-end host test suite (`cargo test -p intercom-core -p intercom-adapter-http -p intercom-firmware --lib --target x86_64-apple-darwin`) and verify all tests pass

## Workflow follow-up

- Validate OpenSpec change with `openspec validate esp32-s31-migration`
- Proceed to implementation via `/opsx-apply` when hardware prototypes and upstream HAL targets are ready
