# Spec Delta

## Purpose

Defines hardware delta requirements for migrating from ESP32-C6 to ESP32-S31, resolving memory allocation limits, single-core CPU contention, 4 MB DevKit OTA limitations, and power transient stability at 320 MHz, while maintaining dual-target firmware portability.

## ADDED Requirements

### Requirement: Dual-Target Portability via Feature Configuration
The firmware SHALL support compilation for either ESP32-C6 or ESP32-S31 from a single unified codebase, selecting hardware peripherals, multi-core execution, and memory allocators via mutually exclusive Cargo feature flags.

#### Scenario: Compiling for ESP32-C6 vs ESP32-S31 targets
- **WHEN** the firmware is built with the target feature flag (esp32c6 or esp32s31)
- **THEN** the build system SHALL configure HAL drivers, memory allocators, and core execution models matching the selected silicon with zero manual source modifications

### Requirement: Core-Level Execution Isolation
When compiled for the ESP32-S31, the architecture SHALL physically isolate real-time audio I2S DMA and Opus encoding onto Core 1, preventing preemption by Wi-Fi MAC and network interrupts on Core 0.

#### Scenario: Network burst does not preempt audio processing
- **WHEN** maximum-throughput Wi-Fi 6 transmission bursts and Matter TCP handshakes execute on Core 0
- **THEN** Core 1 audio I2S DMA servicing and Opus encoding SHALL experience zero interrupt delay or scheduling preemption

### Requirement: Tiered Memory Allocation and Headroom Guarantees
When compiled for the ESP32-S31, the system memory allocator SHALL allocate all dynamic audio packet queues, WebRTC jitter buffers, and TLS session contexts from external PSRAM, guaranteeing internal SRAM stack margin under peak load.

#### Scenario: Memory headroom during active audio and network load
- **WHEN** concurrent Matter-over-TCP, active WebRTC audio streaming, and HTTP dashboard requests run simultaneously on S31
- **THEN** internal SRAM call-stack headroom SHALL remain above 32 KiB and all dynamic heap growth SHALL be served from external DDR PSRAM

### Requirement: Universal Dual-Bank A/B OTA Support
The platform flash configuration SHALL provide symmetric 3.4 MB dual-bank application slots (ota_0 and ota_1) across all development and production targets, eliminating single-bank OTA restrictions.

#### Scenario: Seamless A/B update with automated rollback
- **WHEN** a signed firmware image is uploaded over HTTP OTA to either a development board or production unit
- **THEN** the update SHALL write to the inactive partition and boot successfully with Ed25519 cryptographic validation and fail-safe watchdog rollback

### Requirement: Power Rail Stability Under Peak 320 MHz Transient Load
The power delivery subsystem SHALL maintain voltage stability across simultaneous dual-core 320 MHz computation bursts and Wi-Fi 6 transmission pulses.

#### Scenario: Dual-core burst during Wi-Fi 6 transmission
- **WHEN** Core 0 and Core 1 operate at maximum 320 MHz clock speed while the Wi-Fi 6 transmitter fires at peak output power
- **THEN** the 3.3V core supply rail SHALL not droop below 3.0V, preventing brownout resets or clock phase jitter

### Requirement: ESP32-S31 GPIO Mapping for Intercom Bus
The hardware interface SHALL route the ESP32-S31 GPIO matrix to the IR204E building terminals, maintaining electrical isolation between building common and digital ground.

#### Scenario: Isolated strike actuation and ring detection
- **WHEN** a ring signal occurs on terminal X or an unlock command is executed to terminals 2-3
- **THEN** signals SHALL interface via optocouplers and PhotoMOS relays without ground loop noise coupling into I2S audio lines
