# Spec Delta

## Purpose

Defines firmware platform delta requirements for supporting both ESP32-C6 and ESP32-S31 from a single codebase, resolving memory allocation limits, single-core CPU contention, 4 MB DevKit OTA limitations, and centralizing pin configuration while keeping PCB redesign out of scope.

## ADDED Requirements

### Requirement: Dual-Target Portability via Feature Configuration
The firmware SHALL support compilation for either ESP32-C6 or ESP32-S31 from a single unified codebase, defaulting to esp32c6 for backwards compatibility and enabling S31 hardware abstractions via the esp32s31 feature flag.

#### Scenario: Compiling with default vs explicit feature flags
- **WHEN** the firmware is built without feature flags, or with an explicit target feature flag
- **THEN** the build system SHALL compile for ESP32-C6 by default, or configure S31 HAL drivers and dual-core abstractions when esp32s31 is passed

### Requirement: Core-Level Execution Isolation
When compiled for the ESP32-S31, the architecture SHALL physically isolate real-time audio I2S DMA and Opus encoding onto Core 1, preventing preemption by Wi-Fi MAC and network interrupts on Core 0.

#### Scenario: Network burst does not preempt audio processing
- **WHEN** maximum-throughput Wi-Fi 6 transmission bursts and Matter TCP handshakes execute on Core 0
- **THEN** Core 1 audio I2S DMA servicing and Opus encoding SHALL experience zero interrupt delay or scheduling preemption

### Requirement: Tiered Memory Allocation and Fail-Fast Safety
When compiled for the ESP32-S31, the system memory allocator SHALL allocate all dynamic audio packet queues and WebRTC jitter buffers from external PSRAM, and SHALL fail-fast with a diagnostic panic if PSRAM fails to initialize at boot.

#### Scenario: PSRAM initialization verification at boot
- **WHEN** the firmware boots with the esp32s31 feature enabled
- **THEN** it SHALL verify external PSRAM availability before task scheduling, panicking immediately with diagnostic telemetry if PSRAM is absent or uninitialized

### Requirement: Universal Dual-Bank A/B OTA Support
The platform flash configuration SHALL provide symmetric 3.4 MB dual-bank application slots (ota_0 and ota_1) across all development and production targets, eliminating single-bank OTA restrictions.

#### Scenario: Seamless A/B update with automated rollback
- **WHEN** a signed firmware image is uploaded over HTTP OTA to either a development board or production unit
- **THEN** the update SHALL write to the inactive partition and boot successfully with Ed25519 cryptographic validation and fail-safe watchdog rollback

### Requirement: Centralized Platform Pin Configuration
All microcontroller pin assignments for the ESP32-S31 target SHALL be centralized in a single compile-time platform configuration module, decoupling firmware pin mapping from future PCB layout revisions.

#### Scenario: Pin reassignment for board revision
- **WHEN** GPIO assignments must be updated to match physical board trace routing
- **THEN** modifying the centralized platform configuration module SHALL reassign all edge peripheral connections without code modifications in domain or adapter crates
