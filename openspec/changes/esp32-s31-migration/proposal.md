# Proposal: ESP32-S31 Firmware & Multi-Core Platform Migration

## Why

The current ESP32-C6 silicon has reached its architectural limit: its single 160 MHz RISC-V core spends ~48% CPU time on Opus SILK encoding while competing with Wi-Fi 6 MAC interrupts, and its 320 KiB usable internal SRAM (with zero external PSRAM) forces strict 120 KiB heap and 48 KiB stack caps that risk recursive stack overflows during Opus pitch analysis. Migrating the firmware to support the **ESP32-S31**—Espressif's dual-core 32-bit RISC-V SoC (up to 320 MHz) with high-speed DDR Octal PSRAM—preserves our existing `riscv32...` `no_std` Rust toolchain, isolates real-time audio from Wi-Fi networking, and provides ample memory headroom, all while maintaining the half-duplex audio architecture native to the IR204E building intercom and Matter 1.5 Audio Doorbell specification.

This specification focuses strictly on the **firmware platform, multi-core concurrency, and memory allocation architecture**. Physical PCB schematic and hardware redesign in EasyEDA are explicitly **out of scope** and will be addressed in a dedicated hardware revision. The firmware maintains single-codebase dual-target support for both ESP32-C6 (default baseline) and ESP32-S31 via compile-time Cargo features.

## What Changes

- **Dual-Target Feature-Driven Configuration**:
  - Introduce mutually exclusive target features in `crates/intercom-firmware`: `esp32c6` (default baseline) and `esp32s31` (next-gen flagship), bubbling features cleanly across workspace adapter crates (`intercom-adapter-gpio`, `intercom-adapter-matter`).
  - Abstract target initialization behind a formal `PlatformDriver` trait (`platform/c6.rs`, `platform/s31.rs`, `platform/mock.rs`). Centralize S31 GPIO assignments in `src/platform/s31.rs` for easy future PCB routing alignment.
  - Universal domain logic (`intercom-core`) remains 100% domain-pure with zero audio or hardware dependencies.
- **Unified WebRTC Opus Audio Pipeline**:
  - Standardize both Matter Audio Doorbell (0x0141) and the Solid.js Web Dashboard on **WebRTC Opus**, eliminating raw PCM WebSocket streaming on Core 0.
  - Define `OpusPacket` and `OpusChannel` in `crates/intercom-media`.
  - On `esp32s31`: Dedicate `ProCpu` (Core 0) to Wi-Fi 6, Matter-over-TCP, and WebRTC network transport; dedicate `AppCpu` (Core 1) to real-time I2S DMA, 10 ms soft ramps, and Opus encoding/decoding.
  - On `esp32c6`: Retain single-core cooperative execution.
- **Shared Cross-Core Event Bus**:
  - Both cores reuse the existing `CriticalSectionRawMutex`-backed `CallStateWatch` and `SystemStateWatch` directly for call state and indicator synchronization via hardware spinlocks.
- **PSRAM-Backed Sequence-Ordered Jitter Buffer**:
  - Implement `JitterBuffer<5>` (100 ms) backed by PSRAM on S31, absorbing Wi-Fi packet jitter and emitting `None` on missing packets to trigger Opus Packet Loss Concealment (PLC). (Capacity 2 on C6).
- **Tiered Memory Allocation & Structured Diagnostics**:
  - On `esp32s31`: Initialize `esp-alloc` external heap allocator for DDR PSRAM. If PSRAM initialization fails at boot, fail-fast with a diagnostic panic.
  - Structured memory reporting in `/api/status`: `"memory": { "internal": { "free_bytes": ..., "total_bytes": ... }, "psram": { "free_bytes": ..., "total_bytes": ... } }` (with `"psram": null` on C6).
- **Dual-Bank OTA Preservation**:
  - Retain the fail-safe dual-bank A/B OTA partition scheme (`ota_0` / `ota_1` with Ed25519 signature verification) on standard 8 MB or 16 MB flash modules across both hardware targets.

## Capabilities

### New Capabilities
- `audio-intercom`: Defines delta requirements for audio streaming on next-gen hardware, including sustained voiced speech stability, 100 ms PSRAM jitter buffering with PLC, 10 ms click-free direction ramps, and unified WebRTC Opus transport.
- `hardware-platform`: Defines firmware hardware abstraction and resource guarantees, including dual-target portability (C6 vs S31) via feature flags, formal `PlatformDriver` trait contract, PSRAM tiered memory pools with fail-fast safety, structured memory telemetry, and dual-bank OTA.

### Modified Capabilities
- *(None: Existing domain capabilities such as `access-control` remain 100% agnostic to the hardware target thanks to the Hexagonal Architecture.)*

## Impact

- **Crates Affected**:
  - `intercom-firmware`: Feature flags (`esp32c6` default vs `esp32s31`), `PlatformDriver` trait implementation (`platform/c6.rs`, `platform/s31.rs`, `platform/mock.rs`), multi-core boot sequence, PSRAM allocator initialization.
  - `intercom-media`: New `OpusPacket` and `OpusChannel` types, 10 ms linear ramp helpers (`apply_ramp_in`, `apply_ramp_out`).
  - `intercom-adapter-webrtc`: `JitterBuffer<N>` implementation, Core 1 audio offload.
  - `intercom-adapter-gpio`, `intercom-adapter-matter`: Bubble `esp32c6` / `esp32s31` features without hardcoded chip dependencies.
  - `intercom-adapter-http`: Update `/api/status` JSON serializer to emit structured `"memory"` map.
  - `intercom-core`: **Zero breaking changes**; remains 100% domain-pure.
- **Toolchain**:
  - Both targets use standard RISC-V `riscv32...` toolchain. Host tests run via `MockPlatform` on `x86_64-apple-darwin`.
