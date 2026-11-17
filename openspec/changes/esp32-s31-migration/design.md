# Design: ESP32-S31 Hardware & Audio Platform Migration

## Context

The Intercom Bridge currently runs on a single-core ESP32-C6 RISC-V SoC with 320 KiB usable internal SRAM and no external PSRAM. As documented in [`docs/matter-tcp-webrtc-architecture-and-target-migration.md`](file:///Users/vadim/.gemini/antigravity/worktrees/intercom-matter/esp32_migration_spec/docs/matter-tcp-webrtc-architecture-and-target-migration.md), the system is running at its absolute memory and compute boundaries. See `proposal.md` for complete motivation.

The migration targets the **ESP32-S31**, a dual-core 32-bit RISC-V microcontroller with high-speed DDR Octal PSRAM. Because the project is engineered with Hexagonal Architecture (Ports & Adapters), universal domain logic (`intercom-core`), Matter endpoints (`intercom-adapter-matter`), and HTTP handling (`intercom-adapter-http`) remain unchanged.

## Goals / Non-Goals

**Goals:**
- Define the dual-core execution model isolating network/Embassy on Core 0 (`ProCpu`) and real-time audio DSP on Core 1 (`AppCpu`).
- Establish an idiomatic inter-core communication pattern using dual Embassy executors and typed `embassy_sync::channel::Channel` primitives.
- Establish a tiered memory architecture utilizing fast internal SRAM for DMA/ISR routines and DDR Octal PSRAM for audio frame queues, WebRTC jitter buffers, and TLS session arenas.
- Align audio routing with the native half-duplex architecture of the building amplifier (TekTone IR204E / PK543A: Terminal 3 Listen, Terminal 2 Talk) and Matter 1.5 Audio Doorbell (0x0141).
- Define the hardware PCB module replacement (ESP32-S31-WROOM), GPIO allocation matrix, and power decoupling specifications.

**Non-Goals:**
- On-device Acoustic Echo Cancellation (AEC): The PCB has no acoustic speakers or microphones; client devices (iOS CoreAudio / Android) handle acoustic echo cancellation natively.
- Full-duplex audio: The building amplifier is single-channel switched analog; full-duplex is physically unsupported by the building system.
- Rewriting core domain logic (`intercom-core`): Domain state machines and ports remain 100% target-agnostic.
- Altering Matter data model: Endpoint 1 (Audio Doorbell) and Endpoint 2 (Door Lock) clusters and WebRTC signaling remain identical.
- Supporting legacy Xtensa toolchains: ESP32-S31 is RISC-V; Xtensa support is explicitly out of scope.

## Decisions

### 1. Multi-Core Concurrency Model: Dual Embassy Executors with Typed Channels
- **Choice**: Run an independent `embassy_executor::Executor` on each CPU core (`ProCpu` and `AppCpu`), communicating across core boundaries using static, bounded `embassy_sync::channel::Channel<CriticalSectionRawMutex, T, N>` queues carrying strongly typed 20 ms frame structures (`OpusPacket` and `PcmFrame`).
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
  4. *Architectural Consistency*: Exactly matches the existing channel-driven architecture used throughout the codebase (`AccessControllerHandle`, `SystemStateWatch`).
- **Alternatives Considered**:
  - *Raw SPSC Ring Buffers (`bbqueue` / pointer atomics)*: Operates on raw byte slices rather than discrete frames; requires manual cross-core interrupt (IPI) management and unsafe code.
  - *Embassy Multi-Core SMP (Shared Executor)*: Scheduling tasks dynamically across cores introduces executor lock contention and unpredictable jitter on audio DMA deadlines.

### 2. Tiered Memory Architecture: SRAM vs DDR Octal PSRAM
- **Choice**:
  - **Internal SRAM (512 KiB)**: Reserved for ISR call stacks, RTOS task stacks, cache memory, Wi-Fi PHY descriptors, and I2S DMA ping-pong buffers.
  - **DDR Octal PSRAM (2 to 16 MB)**: Managed via `esp-alloc` external memory heap. Backs WebRTC jitter buffers, Opus encoder/decoder state structures, DTLS-SRTP packet scratchpads, and HTTP dashboard assets.
- **Rationale**: DDR PSRAM has slightly higher access latency than internal SRAM, which can cause DMA underruns if used directly for cycle-critical peripheral DMA. Keeping DMA ping-pong buffers in internal SRAM while moving frame queues and state structures to PSRAM guarantees zero audio dropouts while providing virtually unlimited heap headroom, permanently resolving the Opus SILK stack overflow.
- **Alternatives Considered**:
  - *Internal SRAM only*: Rejected; the existing 320 KiB ceiling risks recurring stack overflows during SILK recursive pitch analysis under heavy network load.

### 3. Half-Duplex Analog Routing & Opus Codec Offload
- **Choice**: Implement clean half-duplex turn-taking between lobby listen (terminal 3 $\to$ WebRTC outbound) and apartment talk (WebRTC inbound $\to$ terminal 2). Offload Opus SILK encoding and decoding entirely to Core 1.
- **Rationale**: The building intercom is inherently single-channel switched analog. The Matter 1.5 Audio Doorbell specification (0x0141) explicitly defines half-duplex turn-taking sessions. Because the PCB has no physical microphone and speaker in acoustic proximity, on-device AEC is redundant and omitted; client smartphones (iOS CoreAudio) handle their own acoustic echo cancellation natively.
- **Alternatives Considered**:
  - *On-device full-duplex with AEC*: Rejected; adds massive computational overhead and filter complexity for an analog bus that cannot physically support bidirectional audio.

### 4. Hardware Packaging & GPIO Pinout Matrix
- **Choice**: Adopt the **ESP32-S31-WROOM** module footprint in EasyEDA.
  - Assign dedicated I2S pins (`I2S0_BCLK`, `I2S0_WS`, `I2S0_DI`, `I2S0_DO`).
  - Optocoupler ring line connected to GPIO with glitch filtering and interrupt wake capability.
  - Door strike PhotoMOS relay driven by open-drain or push-pull GPIO with fail-safe pull-down resistor.
  - WS2812B RGB status LED driven via RMT peripheral.
- **Rationale**: Pre-certified WROOM modules simplify RF compliance and manufacturing.
- **Alternatives Considered**:
  - *Discrete QFN80 chip layout*: Increases PCB layer count and RF matching complexity.

### 5. Flash Partition Scheme & Cryptographic OTA
- **Choice**: Retain standard 8 MB / 16 MB partition table with symmetric 3.4 MB application slots (`ota_0`, `ota_1`), dual 4 KB `otadata` sectors, and Ed25519 signature verification.
- **Rationale**: Preserves the established zero-brick fail-safe rollback architecture and compatibility with `scripts/ota.py`.

## Risks / Trade-offs

- **[Risk] Upstream `esp-hal` support for ESP32-S31**: As a brand new SoC, initial upstream HAL crate support may be in active development.
  - *Mitigation*: Develop the dual-core Channel interface and memory allocation abstractions behind trait boundaries (`intercom-core` ports). Mock and test audio algorithms on host (`x86_64-apple-darwin` / Linux) until hardware HAL crates are tagged.
- **[Risk] DDR PSRAM latency impact on audio**: DMA underrun if PSRAM bus is saturated by Wi-Fi traffic.
  - *Mitigation*: Pin all hardware I2S DMA circular buffers strictly in internal SRAM. Only push completed 20 ms PCM frames across the Channel boundary into PSRAM.
- **[Risk] Peak current transients at 320 MHz + Wi-Fi 6**: Dual cores running at maximum frequency alongside RF bursts can create VDD supply dips.
  - *Mitigation*: Design power stage with a high-current LDO (minimum 1.0A rating) and a low-ESR decoupling capacitor array (22 µF bulk ceramic + 10 µF + 0.1 µF local high-frequency bypass).

## Migration Plan

1. **Phase 1: Hardware Schematic & Layout (EasyEDA)**
   - Update `intercom-head-rev2` schematic with ESP32-S31-WROOM module.
   - Verify power decoupling, LDO thermal rating, and I2S line impedance.
2. **Phase 2: Firmware Workspace & Build Targets**
   - Configure Cargo targets and memory layouts for dual-core RISC-V.
   - Implement `MultiCoreAudioEngine` port adapter.
3. **Phase 3: Tiered PSRAM Allocator**
   - Initialize internal SRAM and external DDR PSRAM heaps in `crates/intercom-firmware/src/main.rs`.
   - Update WebRTC and Opus buffers to allocate from the PSRAM pool.
4. **Phase 4: Half-Duplex Audio Engine & Opus Offload**
   - Deploy Core 1 Opus SILK encoding/decoding and half-duplex direction interlock between terminal 3 (listen) and terminal 2 (talk).
5. **Phase 5: Flash & OTA Validation**
   - Verify flashing, partition layouts, and seamless OTA push updates via `scripts/ota.py`.
