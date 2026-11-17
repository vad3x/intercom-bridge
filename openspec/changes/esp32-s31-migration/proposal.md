# Proposal: ESP32-S31 Firmware & Multi-Core Platform Migration

## Why

The current ESP32-C6 silicon has reached its architectural limit: its single 160 MHz RISC-V core spends ~48% CPU time on Opus SILK encoding while competing with Wi-Fi 6 MAC interrupts, and its 320 KiB usable internal SRAM (with zero external PSRAM) forces strict 120 KiB heap and 48 KiB stack caps that risk recursive stack overflows during Opus pitch analysis. Migrating the firmware to support the **ESP32-S31**—Espressif's dual-core 32-bit RISC-V SoC (up to 320 MHz) with high-speed DDR Octal PSRAM—preserves our existing `riscv32...` `no_std` Rust toolchain, isolates real-time audio from Wi-Fi networking, and provides ample memory headroom, all while maintaining the half-duplex audio architecture native to the IR204E building intercom and Matter 1.5 Audio Doorbell specification.

This specification focuses strictly on the **firmware platform, multi-core concurrency, and memory allocation architecture**. Physical PCB schematic and hardware redesign in EasyEDA are explicitly **out of scope** and will be addressed in a dedicated hardware revision. The firmware maintains single-codebase dual-target support for both ESP32-C6 (default baseline) and ESP32-S31 via compile-time Cargo features.

## What Changes

- **Dual-Target Feature-Driven Configuration**:
  - Introduce mutually exclusive target features in `crates/intercom-firmware`: `esp32c6` (default baseline) and `esp32s31` (next-gen flagship).
  - Abstract target initialization behind a unified `PlatformServices` trait (`platform/c6.rs` and `platform/s31.rs`). Centralize S31 GPIO assignments in `src/platform/s31.rs` for easy future PCB routing alignment.
  - Universal domain logic (`intercom-core`), Matter endpoints (`intercom-adapter-matter`), and HTTP services (`intercom-adapter-http`) remain 100% target-agnostic with zero code duplication.
- **ESP32-S31 Dual-Core Task Isolation (Feature-Gated)**:
  - On `esp32s31`: Dedicate `ProCpu` (Core 0) to Wi-Fi 6, Matter-over-TCP, and HTTP server; dedicate `AppCpu` (Core 1) to real-time I2S DMA and Opus encoding/decoding. Cores communicate over typed `embassy_sync::channel::Channel` frame queues.
  - On `esp32c6`: Retain the existing single-core cooperative Embassy async event loop.
- **ESP32-S31 DDR Octal PSRAM Allocation & Fail-Fast Safety (Feature-Gated)**:
  - On `esp32s31`: Initialize `esp-alloc` external heap allocator for DDR PSRAM (2 to 16 MB) to back WebRTC jitter buffers and DTLS scratchpads, eliminating internal SRAM stack starvation. If PSRAM initialization fails at boot, the firmware fails-fast with a diagnostic panic.
  - On `esp32c6`: Retain the tuned 120 KiB internal SRAM heap and 48 KiB stack layout.
- **Half-Duplex Audio Alignment with Matter 1.5 & IR204E**:
  - Maintain the native half-duplex turn-taking architecture matching the physical analog building amplifier (Terminal 3 Listen / Terminal 2 Talk) and Matter 1.5 Audio Doorbell (0x0141).
  - Omit on-device AEC, relying on resident client devices (iOS CoreAudio / Android) for acoustic echo cancellation.
- **Dual-Bank OTA Preservation**:
  - Retain the fail-safe dual-bank A/B OTA partition scheme (`ota_0` / `ota_1` with Ed25519 signature verification) on standard 8 MB or 16 MB flash modules across both hardware targets.

## Capabilities

### New Capabilities
- `audio-intercom`: Defines delta requirements for audio streaming on next-gen hardware, including sustained voiced speech stability, PSRAM jitter buffering, and non-blocking audio execution during network bursts.
- `hardware-platform`: Defines firmware hardware abstraction and resource guarantees, including dual-target portability (C6 vs S31) via feature flags, core-level task isolation, PSRAM tiered memory pools with fail-fast safety, centralized pin assignment configuration, and dual-bank OTA.

### Modified Capabilities
- *(None: Existing domain capabilities such as `access-control` remain 100% agnostic to the hardware target thanks to the Hexagonal Architecture.)*

## Impact

- **Crates Affected**:
  - `intercom-firmware`: Feature flags (`esp32c6` default vs `esp32s31`), modular platform encapsulation (`platform/c6.rs`, `platform/s31.rs`), multi-core boot sequence, PSRAM allocator initialization.
  - `intercom-adapter-gpio`: Centralized GPIO mappings for C6 and S31.
  - `intercom-adapter-webrtc`: Core 1 audio offload and PSRAM-backed jitter buffers when compiled for S31.
  - `intercom-core`, `intercom-adapter-matter`, `intercom-adapter-http`: **Zero breaking changes**; completely decoupled.
- **Toolchain**:
  - Both targets use the standard RISC-V `riscv32...` toolchain. S31 multi-core abstractions are host-tested on `x86_64-apple-darwin` without requiring intermediate Xtensa hardware setup.
