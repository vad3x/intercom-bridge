# Proposal: ESP32-S31 Hardware & Audio Platform Migration

## Why

The current ESP32-C6 silicon has reached its architectural limit: its single 160 MHz RISC-V core spends ~48% CPU time on Opus SILK encoding while competing with Wi-Fi 6 MAC interrupts, and its 320 KiB usable internal SRAM (with zero external PSRAM) forces strict 120 KiB heap and 48 KiB stack caps that preclude full-duplex Acoustic Echo Cancellation (AEC). Migrating to the **ESP32-S31**—Espressif's dual-core 32-bit RISC-V SoC (up to 320 MHz) with a 128-bit audio/AI SIMD data path and high-speed DDR Octal PSRAM—preserves our existing `riscv32...` `no_std` Rust toolchain while unlocking true full-duplex bidirectional WebRTC audio, dedicated DSP core isolation, and eliminated memory starvation.

## What Changes

- **Target Architecture & Toolchain**:
  - Update composition root (`crates/intercom-firmware`) and Cargo configuration for ESP32-S31 RISC-V dual-core target (`ProCpu` and `AppCpu`).
  - Upgrade `esp-hal` feature flags and peripheral configurations to ESP32-S31 once upstream HAL support lands.
- **Dual-Core Multi-Processing Isolation**:
  - Dedicate `ProCpu` (Core 0) to Wi-Fi 6 networking, Matter-over-TCP protocol stack, embedded Solid.js HTTP server, and Embassy event loops.
  - Dedicate `AppCpu` (Core 1) with 128-bit SIMD to real-time audio DSP: I2S DMA, Opus SILK encoding/decoding, and full-duplex Acoustic Echo Cancellation (AEC).
  - Connect cores via lock-free Single-Producer Single-Consumer (SPSC) ring buffers in shared memory / PSRAM.
- **External DDR Octal PSRAM Integration**:
  - Integrate `esp-alloc` external memory allocator for DDR PSRAM (2 to 16 MB).
  - Allocate large WebRTC jitter buffers, Opus frame queues, and DTLS-SRTP packet scratchpads in PSRAM, reserving fast internal SRAM for ISR stacks and DMA descriptors.
- **Full-Duplex Bidirectional Audio**:
  - Transition from the existing software half-duplex turn-taking interlock to continuous full-duplex bidirectional streaming with real-time AEC.
  - Leverage S31 128-bit SIMD instructions for filter convolution and pitch search.
- **Hardware & PCB Module Revision**:
  - Replace ESP32-C6-WROOM-1 / DevKitM-1 module footprint with **ESP32-S31-WROOM** (QFN80 package / module).
  - Remap GPIO pins for analog intercom interface (ring optocoupler, door strike PhotoMOS relay, status RGB LED, boot button, and I2S codec lines).
  - Validate 3.3V power supply rail and LDO thermal headroom for dual-core 320 MHz + PSRAM peak RF/DSP transients.
- **Dual-Bank OTA Preservation**:
  - Retain the fail-safe dual-bank A/B OTA partition scheme (`ota_0` / `ota_1` with Ed25519 signature verification) on standard 8 MB or 16 MB flash modules.

## Capabilities

### New Capabilities
- `audio-intercom`: Defines requirements for bidirectional WebRTC audio communication over Matter, including full-duplex streaming, Acoustic Echo Cancellation (AEC), Opus encoding/decoding, and jitter buffer performance.
- `hardware-platform`: Defines hardware abstraction and system resource guarantees for the microcontroller platform, including dual-core workload separation, PSRAM memory allocation pools, GPIO pin mapping, power rail stability, and bootloader OTA partition layout.

### Modified Capabilities
- *(None: Existing domain capabilities such as `access-control` remain 100% agnostic to the hardware target thanks to the Hexagonal Architecture.)*

## Impact

- **Crates Affected**:
  - `intercom-firmware`: Composition root, multi-core boot sequence, PSRAM allocator initialization, partition table, build targets.
  - `intercom-adapter-gpio`: Pin assignments and peripheral drivers for S31 GPIO matrix.
  - `intercom-adapter-webrtc`: Transition audio pipeline from half-duplex interlock to full-duplex engine with AEC hooks and PSRAM buffer backing.
  - `intercom-core`, `intercom-adapter-matter`, `intercom-adapter-http`: **Zero breaking changes**; domain logic and network adapters are completely decoupled.
- **Hardware & CAD**:
  - EasyEDA schematic and PCB layout update to accommodate ESP32-S31-WROOM module pinout and power decoupling.
- **Toolchain**:
  - Retains RISC-V `riscv32...` architecture; no switch to Xtensa toolchain required.
