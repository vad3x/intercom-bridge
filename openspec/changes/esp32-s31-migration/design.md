# Design: ESP32-S31 Firmware & Multi-Core Platform Migration

## Context

The Intercom Bridge currently runs on a single-core ESP32-C6 RISC-V SoC with 320 KiB usable internal SRAM and no external PSRAM. As documented in [`docs/matter-tcp-webrtc-architecture-and-target-migration.md`](file:///Users/vadim/.gemini/antigravity/worktrees/intercom-matter/esp32_migration_spec/docs/matter-tcp-webrtc-architecture-and-target-migration.md), the system is running at its absolute memory and compute boundaries. See `proposal.md` for complete motivation.

The migration introduces support for the **ESP32-S31** (dual-core 32-bit RISC-V up to 320 MHz with DDR Octal PSRAM) **alongside the existing ESP32-C6**, maintaining a single unified codebase driven by Cargo feature flags. Because the project is engineered with Hexagonal Architecture (Ports & Adapters), universal domain logic (`intercom-core`), Matter endpoints (`intercom-adapter-matter`), and HTTP handling (`intercom-adapter-http`) remain unchanged.

## Goals / Non-Goals

**Goals:**
- Provide single-codebase dual-target support for both ESP32-C6 and ESP32-S31 selected via compile-time Cargo feature flags, defaulting to `esp32c6` for backwards compatibility.
- On ESP32-S31: Enable dual-core task isolation (`ProCpu` for network/Embassy, `AppCpu` for audio DSP) communicating over typed `embassy_sync::channel::Channel` queues.
- On ESP32-S31: Enable tiered memory architecture utilizing fast internal SRAM for DMA/ISR routines and DDR Octal PSRAM for audio frame queues, WebRTC jitter buffers, and TLS session arenas, with fail-fast panic on PSRAM failure.
- On ESP32-C6: Retain the stable single-core cooperative Embassy async event loop and tuned 120 KiB internal SRAM allocation.
- Centralize all target GPIO pin assignments in compile-time configuration modules (`src/platform/s31.rs` and `src/platform/c6.rs`).
- Verify multi-core abstractions and S31 logic through host-capable unit testing (`cargo test --target x86_64-apple-darwin`).

**Non-Goals:**
- Physical PCB hardware redesign or EasyEDA schematic authoring: Hardware board layout and manufacturing revisions are explicitly **out of scope** for this firmware specification and will be addressed in a dedicated hardware revision.
- Intermediate ESP32-S3 Xtensa toolchain setup: S3 physical testing is skipped to keep the codebase 100% pure RISC-V.
- On-device Acoustic Echo Cancellation (AEC): The PCB has no acoustic speakers or microphones; client devices (iOS CoreAudio / Android) handle acoustic echo cancellation natively.
- Full-duplex audio: The building amplifier is single-channel switched analog; full-duplex is physically unsupported by the building system.
- Rewriting core domain logic (`intercom-core`): Domain state machines and ports remain 100% target-agnostic.

## Decisions

### 1. Feature-Driven Dual-Target Architecture (`esp32c6` default vs `esp32s31`)
- **Choice**: Structure target support in `crates/intercom-firmware` using mutually exclusive Cargo features:
  - `esp32c6` (default): Compiles HAL with `esp-hal/esp32c6`, runs single-core Embassy runtime, sets `HEAP_SIZE = 120 KiB` in internal SRAM, and runs audio I2S tasks cooperatively.
  - `esp32s31`: Compiles HAL with `esp-hal/esp32s31`, initializes `CpuControl` to boot Core 1 (`AppCpu`), runs dual independent Embassy executors, initializes DDR PSRAM allocator, and routes audio queues across inter-core channels.
- **Rationale**: Preserves 100% backwards compatibility for existing C6 builds, CI workflows, and developers while unlocking next-generation S31 capabilities on demand.

### 2. Multi-Core Concurrency Model on S31: Dual Embassy Executors
- **Choice**: When `feature = "esp32s31"` is enabled, instantiate an independent `embassy_executor::Executor` on each CPU core (`ProCpu` and `AppCpu`), communicating across core boundaries using static `embassy_sync::channel::Channel<CriticalSectionRawMutex, T, N>` queues carrying strongly typed 20 ms frame structures (`OpusPacket` and `PcmFrame`).
  ```
  +-----------------------------------------------------------------------------------------------+
  | Core 0: ProCpu (Executor 0)                                   | Core 1: AppCpu (Executor 1)   |
  | ---------------------------                                   | ---------------------------   |
  | - Wi-Fi 6 MAC & esp-radio                                     | - I2S DMA Speaker/Mic Driver  |
  | - Matter-over-TCP Protocol Stack                              | - Half-Duplex Direction Relay |
  | - Embedded Solid.js HTTP Server                               | - Opus SILK Encoder / Decoder |
  +---------------------------------------------------------------+-------------------------------+
                                   |                             ^
                  AUDIO_INBOUND    |                             | AUDIO_OUTBOUND
                  Channel::send()  v                             | Channel::receive()
              +------------------------------------------------------+
              | static AUDIO_INBOUND:  Channel<CriticalSection, ...> |
              | static AUDIO_OUTBOUND: Channel<CriticalSection, ...> |
              +------------------------------------------------------+
  ```
- **Rationale**: 100% safe Rust, strongly typed audio frames, automatic `wfi` sleep on empty queues, and architectural consistency with existing Embassy channels in the codebase.

### 3. Tiered Memory Architecture & Fail-Fast PSRAM Policy
- **Choice**:
  - On `esp32s31`:
    - **Internal SRAM (512 KiB)**: Reserved for ISR call stacks, RTOS task stacks, cache memory, Wi-Fi PHY descriptors, and I2S DMA ping-pong buffers.
    - **DDR Octal PSRAM (2 to 16 MB)**: Managed via `esp-alloc` external memory heap. Backs WebRTC jitter buffers, Opus encoder/decoder state structures, DTLS-SRTP packet scratchpads, and HTTP dashboard assets.
    - **Fail-Fast Policy**: If PSRAM is not detected or fails initialization at boot, the firmware logs diagnostic telemetry and panics immediately, triggering watchdog reset.
  - On `esp32c6`:
    - **Internal SRAM only**: Uses tuned `HEAP_SIZE = 120 KiB` in `.bss`, preserving 48 KiB for main task stack.
- **Rationale**: PSRAM is mandatory for S31 memory safety guarantees. Silently falling back to internal SRAM on an S31 build would recreate the C6 stack starvation issue.

### 4. Half-Duplex Analog Routing & Opus Codec Offload
- **Choice**: Implement clean half-duplex turn-taking between lobby listen (terminal 3 $\to$ WebRTC outbound) and apartment talk (WebRTC inbound $\to$ terminal 2). On S31, offload Opus SILK encoding and decoding to Core 1; on C6, execute on the single core within the 20 ms timeslice.
- **Rationale**: Matches the physical single-channel analog building amplifier (TekTone IR204E / PK543A) and Matter 1.5 Audio Doorbell specification. On-device AEC is omitted; client smartphones (iOS CoreAudio) handle acoustic echo cancellation natively.

### 5. Centralized Platform Pin Configuration
- **Choice**: Encapsulate all target-specific peripheral wiring in compile-time configuration modules (`src/platform/c6.rs` and `src/platform/s31.rs`), implementing a common `PlatformServices` trait.
- **Rationale**: Decouples firmware architecture from future PCB routing revisions. When the S31 PCB layout is finalized in a later hardware workstream, pin numbers can be adjusted in a single file without touching driver or domain code.

### 6. Flash Partition Scheme & Cryptographic OTA
- **Choice**: Standardize on symmetric 3.4 MB application slots (`ota_0`, `ota_1`), dual 4 KB `otadata` sectors, and Ed25519 signature verification across 8 MB and 16 MB flash configurations for both chips.

## Risks / Trade-offs

- **[Risk] Upstream `esp-hal` support timeline for S31**: S31 HAL crate may lag behind silicon samples.
  - *Mitigation*: Develop the dual-target abstraction now using the C6 implementation as the baseline; mock the S31 multi-core channel interface in host tests (`x86_64-apple-darwin`).
- **[Risk] DDR PSRAM latency impact on audio**: DMA underrun if PSRAM bus is saturated by Wi-Fi traffic.
  - *Mitigation*: Pin all hardware I2S DMA circular buffers strictly in internal SRAM. Only push completed 20 ms PCM frames across the Channel boundary into PSRAM.

## Migration Plan

1. **Phase 1: Feature Flag & Platform Encapsulation**
   - Refactor `crates/intercom-firmware` to support `esp32c6` (default) and `esp32s31` features.
   - Encapsulate hardware initialization into `platform/c6.rs` and `platform/s31.rs`.
2. **Phase 2: Multi-Core Embassy IPC (S31)**
   - Implement `CpuControl` boot of `AppCpu` and typed `Channel` audio queue pump under `cfg(feature = "esp32s31")`.
3. **Phase 3: Tiered PSRAM Allocator & Fail-Fast Safety (S31)**
   - Initialize DDR PSRAM heap with boot-time verification panic, configuring WebRTC jitter buffers to allocate from PSRAM on S31.
4. **Phase 4: Dual-Target Build & OTA Validation**
   - Verify both `cargo build-devkit` (C6) and `cargo build --features esp32s31` compile cleanly and pass automated host test suites on macOS.
