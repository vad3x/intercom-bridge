# Design: ESP32-S31 Firmware & Multi-Core Platform Migration

## Context

The Intercom Bridge currently runs on a single-core ESP32-C6 RISC-V SoC with 320 KiB usable internal SRAM and no external PSRAM. As documented in [`docs/matter-tcp-webrtc-architecture-and-target-migration.md`](file:///Users/vadim/.gemini/antigravity/worktrees/intercom-matter/esp32_migration_spec/docs/matter-tcp-webrtc-architecture-and-target-migration.md), the system is running at its absolute memory and compute boundaries. See `proposal.md` for complete motivation.

The migration introduces support for the **ESP32-S31** (dual-core 32-bit RISC-V up to 320 MHz with DDR Octal PSRAM) **alongside the existing ESP32-C6**, maintaining a single unified codebase driven by Cargo feature flags. Because the project is engineered with Hexagonal Architecture (Ports & Adapters), universal domain logic (`intercom-core`) remains completely target-agnostic.

## Goals / Non-Goals

**Goals:**
- Provide single-codebase dual-target support for both ESP32-C6 and ESP32-S31 selected via compile-time Cargo feature flags, defaulting to `esp32c6` for backwards compatibility.
- Decouple hardware targets using a formal static-dispatch trait (`trait PlatformDriver`) implemented by `C6Platform`, `S31Platform`, and a host-capable `MockPlatform`.
- Standardize all audio consumers (Matter Audio Doorbell and Solid.js Web Dashboard) on **WebRTC Opus**, unifying the stream format and eliminating raw PCM streaming on Core 0.
- Reuse the existing `CriticalSectionRawMutex`-backed event bus (`CallStateWatch`, `SystemStateWatch`) across cores directly via ESP32 hardware spinlocks.
- On ESP32-S31: Enable dual-core task isolation (`ProCpu` for network/Embassy, `AppCpu` for audio DSP) communicating over typed `embassy_sync::channel::Channel` queues.
- On ESP32-S31: Implement a sequence-ordered `JitterBuffer<5>` (100 ms) in DDR Octal PSRAM with Opus Packet Loss Concealment (PLC), and tiered memory with fail-fast boot-time verification.
- Provide a clean, structured memory diagnostic schema in `/api/status`: `"memory": { "internal": { ... }, "psram": { ... } }`.
- Centralize all target GPIO pin assignments in compile-time configuration modules (`src/platform/s31.rs` and `src/platform/c6.rs`).
- Verify multi-core abstractions and S31 logic through host-capable unit testing (`cargo test --target x86_64-apple-darwin`) via `MockPlatform`.

**Non-Goals:**
- Physical PCB hardware redesign or EasyEDA schematic authoring: Hardware board layout and manufacturing revisions are explicitly **out of scope** for this firmware specification and will be addressed in a dedicated hardware revision.
- Intermediate ESP32-S3 Xtensa toolchain setup: S3 physical testing is skipped to keep the codebase 100% pure RISC-V.
- On-device Acoustic Echo Cancellation (AEC): The PCB has no acoustic speakers or microphones; client devices (iOS CoreAudio / Android) handle acoustic echo cancellation natively.
- Full-duplex audio: The building amplifier is single-channel switched analog; full-duplex is physically unsupported by the building system.
- Rewriting core domain logic (`intercom-core`): Domain state machines and ports remain 100% target-agnostic.

## Decisions

### 1. Formal Static-Dispatch Trait (`trait PlatformDriver`) & Driver Bundle
- **Choice**: Define an explicit `PlatformDriver` trait in `crates/intercom-firmware/src/platform/mod.rs` returning a unified driver bundle:
  ```rust
  pub struct PlatformDrivers<A: AudioPort, I: IndicatorPort> {
      pub ring: GpioRingDriver,
      pub bus: GpioBusDriver,
      pub audio_pipeline: A,
      pub indicator: I,
      pub boot_button: GpioBootButtonDriver,
      pub matter_peripherals: MatterPeripherals,
      pub hardware_info: FirmwareHardwareInfo,
  }

  pub trait PlatformDriver {
      type AudioPipeline: AudioPort;
      type Indicator: IndicatorPort;

      fn init() -> (Self, PlatformDrivers<Self::AudioPipeline, Self::Indicator>)
      where
          Self: Sized;

      fn spawn_audio_worker(
          &mut self,
          worker: impl FnOnce() + Send + 'static,
      );
  }
  ```
  The active platform type is bound at compile time via type alias:
  ```rust
  #[cfg(not(target_arch = "riscv32"))]
  pub type ActivePlatform = mock::MockPlatform;

  #[cfg(target_arch = "riscv32")]
  #[cfg(feature = "esp32c6")]
  pub type ActivePlatform = c6::C6Platform;

  #[cfg(target_arch = "riscv32")]
  #[cfg(feature = "esp32s31")]
  pub type ActivePlatform = s31::S31Platform;
  ```
- **Rationale**: Strict trait contract eliminates duck-typing drift between C6 and S31, provides clean monomorphic dispatch in `main()`, and allows `MockPlatform` on macOS without importing hardware crates.

### 2. Unified WebRTC Opus Stream & Cross-Core Event Bus
- **Choice**: Both Matter Endpoint 1 and the Web Dashboard (`/api/debug/audio/webrtc/offer`) consume 20 ms Opus packets directly from `WebRtcMediaEngine`.
  - Frame definitions are placed in `crates/intercom-media`:
    ```rust
    pub const MAX_OPUS_PACKET_BYTES: usize = 256;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct OpusPacket {
        pub len: u16,
        pub payload: [u8; MAX_OPUS_PACKET_BYTES],
    }

    pub type OpusChannel = Channel<CriticalSectionRawMutex, OpusPacket, 4>;
    ```
  - **Shared Event Bus**: Both cores directly observe `CALL_STATE_WATCH.receiver()` and `SYSTEM_STATE_WATCH.receiver()`. Because these channels use `CriticalSectionRawMutex`, they synchronize safely across cores via the ESP32 hardware spinlock without extra IPC layers.

### 3. Cargo Workspace Feature Bubbling
- **Choice**: Remove hardcoded chip features from `Cargo.toml` root. In `intercom-adapter-gpio` and `intercom-adapter-matter`, define:
  ```toml
  [features]
  default = ["esp32c6"]
  esp32c6 = ["esp-hal/esp32c6", "esp-radio/esp32c6"]
  esp32s31 = ["esp-hal/esp32s31", "esp-radio/esp32s31"]
  ```
  `crates/intercom-firmware/Cargo.toml` forwards its active feature down to all internal adapter crates. `esp-wifi-sys-esp32c6` and `esp-wifi-sys-esp32s31` are optional target dependencies aliased as `crate::platform::wifi_sys`.

### 4. Sequence-Ordered PSRAM Jitter Buffer (`JitterBuffer<5>`)
- **Choice**: Implement an adaptive sequence-ordered ring buffer in `intercom-adapter-webrtc`:
  - Capacity: 5 slots (`[Option<OpusPacket>; 5]`, 100 ms playout window) backed by PSRAM heap on S31; 2 slots on C6 (SRAM).
  - Packet insertion: Indexed by 16-bit RTP sequence number modulo capacity.
  - Playout pacer: Emits a 20 ms frame. If the sequence slot is populated, it sends the packet to Core 1; if missing/delayed past deadline, it emits `None`, triggering Opus Packet Loss Concealment (PLC).

### 5. Tiered Memory Allocator, Fail-Fast Policy & Telemetry Schema
- **Choice**:
  - On `esp32s31`:
    - Internal SRAM: 64 KiB for ISR/DMA.
    - PSRAM: Initialized via `esp_alloc::psram_allocator!(p.PSRAM, esp_hal::psram)`.
    - Fail-Fast check: If `!esp_hal::psram::init_psram(...)`, panic immediately with diagnostic logging.
  - On `esp32c6`:
    - Internal SRAM heap: 120 KiB in `.bss`, preserving 48 KiB stack.
  - **Structured `/api/status` Schema**:
    ```json
    {
      "memory": {
        "internal": {
          "free_bytes": 48240,
          "total_bytes": 327680
        },
        "psram": {
          "free_bytes": 1401760,
          "total_bytes": 2097152
        }
      }
    }
    ```
    On C6, `"psram": null`.

### 6. Direction Switching Soft Ramp Specification
- **Choice**: Execute fixed-point 10 ms linear ramps (160 samples @ 16 kHz) in `crates/intercom-media` before and after relay transitions:
  - Fade-out: `sample[i] = (sample[i] * (160 - i)) / 160`.
  - Fade-in: `sample[i] = (sample[i] * i) / 160`.
  - Sequence: Fade-out active capture (10 ms) $\to$ switch relay $\to$ wait 30 ms settling delay $\to$ fade-in destination playback (10 ms).

### 7. Centralized Platform Pin Configuration
- **Choice**: Encapsulate all target-specific peripheral wiring in compile-time configuration modules (`src/platform/c6.rs` and `src/platform/s31.rs`), implementing `PlatformDriver`.

### 8. Flash Partition Scheme & Cryptographic OTA
- **Choice**: Standardize on symmetric 3.4 MB application slots (`ota_0`, `ota_1`), dual 4 KB `otadata` sectors, and Ed25519 signature verification across 8 MB and 16 MB flash configurations for both chips.

## Risks / Trade-offs

- **[Risk] Upstream `esp-hal` support timeline for S31**: S31 HAL crate may lag behind silicon samples.
  - *Mitigation*: Develop the dual-target abstraction now using `MockPlatform` on host tests (`x86_64-apple-darwin`) and `C6Platform` on physical hardware.
- **[Risk] DDR PSRAM latency impact on audio**: DMA underrun if PSRAM bus is saturated by Wi-Fi traffic.
  - *Mitigation*: Pin all hardware I2S DMA circular buffers strictly in internal SRAM. Only push completed 20 ms PCM frames across the Channel boundary into PSRAM.

## Migration Plan

1. **Phase 1: Feature Flag & PlatformDriver Trait**
   - Define `PlatformDriver` trait in `crates/intercom-firmware/src/platform/mod.rs`.
   - Implement `C6Platform`, `S31Platform`, and `MockPlatform`.
   - Configure Cargo workspace feature bubbling across adapter crates.
2. **Phase 2: Unified WebRTC Opus & IPC Channels**
   - Define `OpusPacket` and `OpusChannel` in `crates/intercom-media`.
   - Implement Core 1 audio worker task and shared watch event bus subscriptions.
3. **Phase 3: Jitter Buffer & PSRAM Memory Allocator**
   - Implement `JitterBuffer<5>` with Opus PLC.
   - Initialize DDR PSRAM heap with fail-fast panic on S31.
   - Update `/api/status` with structured `"memory"` schema.
4. **Phase 4: Dual-Target Build & Host Verification**
   - Verify both `cargo test --features esp32c6` and `cargo test --features esp32s31` pass on macOS.
