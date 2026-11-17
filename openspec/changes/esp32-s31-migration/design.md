# Design: ESP32-S31 Hardware & Audio Platform Migration

## Context

The Intercom Bridge currently runs on a single-core ESP32-C6 RISC-V SoC with 320 KiB usable internal SRAM and no external PSRAM. As documented in [`docs/matter-tcp-webrtc-architecture-and-target-migration.md`](file:///Users/vadim/.gemini/antigravity/worktrees/intercom-matter/esp32_migration_spec/docs/matter-tcp-webrtc-architecture-and-target-migration.md), the system is running at its absolute memory and compute boundaries. See `proposal.md` for complete motivation.

The migration introduces support for the **ESP32-S31** (dual-core 32-bit RISC-V up to 320 MHz with DDR Octal PSRAM) **alongside the existing ESP32-C6**, maintaining a single unified codebase driven by Cargo feature flags. Because the project is engineered with Hexagonal Architecture (Ports & Adapters), universal domain logic (`intercom-core`), Matter endpoints (`intercom-adapter-matter`), and HTTP handling (`intercom-adapter-http`) remain unchanged.

## Goals / Non-Goals

**Goals:**
- Provide single-codebase dual-target support for both ESP32-C6 and ESP32-S31 selected via compile-time Cargo feature flags (`esp32c6` vs `esp32s31`).
- On ESP32-S31: Enable dual-core task isolation (`ProCpu` for network/Embassy, `AppCpu` for audio DSP) communicating over typed `embassy_sync::channel::Channel` queues.
- On ESP32-S31: Enable tiered memory architecture utilizing fast internal SRAM for DMA/ISR routines and DDR Octal PSRAM for audio frame queues, WebRTC jitter buffers, and TLS session arenas.
- On ESP32-C6: Retain the stable single-core cooperative Embassy async event loop and tuned 120 KiB internal SRAM allocation.
- Align audio routing with the native half-duplex architecture of the building amplifier (TekTone IR204E / PK543A: Terminal 3 Listen, Terminal 2 Talk) and Matter 1.5 Audio Doorbell (0x0141).
- Define the hardware PCB module replacement (ESP32-S31-WROOM), GPIO allocation matrix, and power decoupling specifications.

**Non-Goals:**
- Separate codebases or divergent branches for C6 and S31: Both targets must build from the same repository.
- On-device Acoustic Echo Cancellation (AEC): The PCB has no acoustic speakers or microphones; client devices (iOS CoreAudio / Android) handle acoustic echo cancellation natively.
- Full-duplex audio: The building amplifier is single-channel switched analog; full-duplex is physically unsupported by the building system.
- Rewriting core domain logic (`intercom-core`): Domain state machines and ports remain 100% target-agnostic.
- Supporting legacy Xtensa toolchains: Both ESP32-C6 and ESP32-S31 are RISC-V.

## Decisions

### 1. Feature-Driven Dual-Target Architecture (`esp32c6` vs `esp32s31`)
- **Choice**: Structure target support in `crates/intercom-firmware` and edge adapter crates using mutually exclusive Cargo features:
  - `esp32c6` (default): Compiles HAL with `esp-hal/esp32c6`, runs single-core Embassy runtime, sets `HEAP_SIZE = 120 KiB` in internal SRAM, and runs audio I2S tasks cooperatively.
  - `esp32s31`: Compiles HAL with `esp-hal/esp32s31`, initializes `CpuControl` to boot Core 1 (`AppCpu`), runs dual independent Embassy executors, initializes DDR PSRAM allocator, and routes audio queues across inter-core channels.
- **Rationale**: Enables seamless continuous integration and testing for existing ESP32-C6 hardware deployments while allowing next-generation S31 hardware to immediately utilize multi-core and PSRAM capabilities without branching codebases.
- **Alternatives Considered**:
  - *Runtime chip detection*: Rejected; memory layouts, linker scripts, and peripheral register maps differ at compile-time in `no_std` embedded systems.
  - *Forking repository / crates*: Rejected; maintenance burden and risk of domain logic divergence.

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
- **Rationale**:
  1. *Idiomatic Safe Rust*: 100% safe Rust without manual atomic pointer slicing or raw memory barriers.
  2. *Domain Data Types*: Audio travels as discrete, strongly typed 20 ms frames rather than unstructured byte slices.
  3. *Automatic Low-Power Sleep*: When no call is active, Core 1 executes `wfi` (Wait For Interrupt) automatically while suspended on `inbound_rx.receive().await`, waking with sub-microsecond latency when Core 0 calls `.send(packet).await`.

### 3. Tiered Memory Architecture: SRAM vs DDR Octal PSRAM
- **Choice**:
  - On `esp32s31`:
    - **Internal SRAM (512 KiB)**: Reserved for ISR call stacks, RTOS task stacks, cache memory, Wi-Fi PHY descriptors, and I2S DMA ping-pong buffers.
    - **DDR Octal PSRAM (2 to 16 MB)**: Managed via `esp-alloc` external memory heap. Backs WebRTC jitter buffers, Opus encoder/decoder state structures, DTLS-SRTP packet scratchpads, and HTTP dashboard assets.
  - On `esp32c6`:
    - **Internal SRAM only**: Uses tuned `HEAP_SIZE = 120 KiB` in `.bss`, preserving 48 KiB for main task stack.
- **Rationale**: DDR PSRAM has slightly higher access latency than internal SRAM, which can cause DMA underruns if used directly for cycle-critical peripheral DMA. Keeping DMA ping-pong buffers in internal SRAM while moving frame queues and state structures to PSRAM guarantees zero audio dropouts while providing virtually unlimited heap headroom on S31.

### 4. Half-Duplex Analog Routing & Opus Codec Offload
- **Choice**: Implement clean half-duplex turn-taking between lobby listen (terminal 3 $\to$ WebRTC outbound) and apartment talk (WebRTC inbound $\to$ terminal 2). On S31, offload Opus SILK encoding and decoding to Core 1; on C6, execute on the single core within the 20 ms timeslice.
- **Rationale**: The building intercom is inherently single-channel switched analog. The Matter 1.5 Audio Doorbell specification (0x0141) explicitly defines half-duplex turn-taking sessions. Because the PCB has no physical microphone and speaker in acoustic proximity, on-device AEC is redundant and omitted; client smartphones (iOS CoreAudio) handle their own acoustic echo cancellation natively.

### 5. Hardware Packaging & GPIO Pinout Matrix
- **Choice**: Support both ESP32-C6-WROOM-1 and ESP32-S31-WROOM module footprints across board revisions.
  - S31 GPIO matrix mapped to I2S (`I2S0_BCLK`, `I2S0_WS`, `I2S0_DI`, `I2S0_DO`), optocoupler ring input, PhotoMOS strike relay, boot button, and WS2812B RGB status LED via RMT.

### 6. Flash Partition Scheme & Cryptographic OTA
- **Choice**: Standardize on symmetric 3.4 MB application slots (`ota_0`, `ota_1`), dual 4 KB `otadata` sectors, and Ed25519 signature verification across 8 MB and 16 MB flash configurations for both chips.

## Risks / Trade-offs

- **[Risk] Feature flag bloat in firmware crate**: Multiple `#[cfg(feature = ...)]` attributes cluttering composition root.
  - *Mitigation*: Encapsulate platform-specific initialization inside dedicated modules (`intercom-firmware/src/platform/c6.rs` and `intercom-firmware/src/platform/s31.rs`), exposing a unified `PlatformServices` interface to `main()`.
- **[Risk] Upstream `esp-hal` support timeline for S31**: S31 HAL crate may lag behind chip release.
  - *Mitigation*: Develop the dual-target abstraction now using the C6 implementation as the baseline; mock the S31 multi-core channel interface in host tests (`x86_64-apple-darwin`).
- **[Risk] Peak current transients at 320 MHz + Wi-Fi 6 on S31**: Dual cores running at maximum frequency alongside RF bursts can create VDD supply dips.
  - *Mitigation*: Design power stage with a high-current LDO (minimum 1.0A rating) and a low-ESR decoupling capacitor array (22 µF bulk ceramic + 10 µF + 0.1 µF local high-frequency bypass).

## Migration Plan

1. **Phase 1: Feature Flag & Platform Architecture**
   - Refactor `crates/intercom-firmware` to support `esp32c6` and `esp32s31` features.
   - Encapsulate hardware initialization into `platform/c6.rs` and `platform/s31.rs`.
2. **Phase 2: Hardware Schematic & Layout (EasyEDA)**
   - Create ESP32-S31-WROOM footprint and update PCB layout.
3. **Phase 3: Multi-Core Embassy IPC (S31)**
   - Implement `CpuControl` boot of `AppCpu` and typed `Channel` audio queue pump under `cfg(feature = "esp32s31")`.
4. **Phase 4: Tiered PSRAM Allocator (S31)**
   - Initialize DDR PSRAM heap and configure WebRTC jitter buffers to allocate from PSRAM on S31.
5. **Phase 5: Dual-Target Build & OTA Validation**
   - Verify both `cargo build-devkit` (C6) and `cargo build --features esp32s31` compile cleanly and pass automated host test suites.
