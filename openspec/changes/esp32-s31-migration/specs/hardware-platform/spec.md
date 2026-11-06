# Spec Delta

## Purpose

Defines hardware abstraction and system resource guarantees for the microcontroller platform, including real-time audio latency under network load, PSRAM memory allocation pools, GPIO pin mapping, power rail stability, and bootloader OTA partition layout.

## ADDED Requirements

### Requirement: Real-Time Audio Deadline Guarantee Under Load
The platform SHALL guarantee continuous, uninterrupted audio processing and I2S DMA servicing during concurrent high-throughput network activity, without frame drops or buffer underruns.

#### Scenario: Network burst during active audio stream
- **WHEN** a high-throughput network event, Matter-over-TCP transaction, or HTTP request occurs simultaneously with active WebRTC streaming
- **THEN** audio capture, encoding, and speaker DMA delivery SHALL continue with zero buffer underruns and frame processing latency under 5 milliseconds

### Requirement: External PSRAM Memory Pool
The system memory allocator SHALL route dynamic audio queues, WebRTC packet buffers, and TLS session state to external DDR Octal PSRAM, preserving internal SRAM for interrupt stacks and DMA descriptors.

#### Scenario: Memory allocation under peak load
- **WHEN** concurrent Matter-over-TCP, WebRTC streaming, and HTTP dashboard requests are active
- **THEN** internal SRAM stack headroom SHALL remain above 32 KiB and dynamic heap allocations SHALL be fulfilled from external PSRAM without out-of-memory errors

### Requirement: Intercom GPIO Matrix Mapping
The hardware edge adapter SHALL interface to the physical intercom hardware through dedicated GPIOs for ring optocoupler input, PhotoMOS door strike output, status indicator LED, and I2S codec lines.

#### Scenario: Door strike actuation signal
- **WHEN** an authenticated unlock command is issued by the access control service
- **THEN** the hardware driver SHALL pulse the door strike GPIO line for the configured duration while keeping audio and ring detection lines unaffected

### Requirement: Power Rail Stability
The power subsystem SHALL supply continuous, ripple-filtered 3.3V DC to the ESP32-S31 SoC and PSRAM across peak Wi-Fi 6 transmission pulses and dual-core 320 MHz computational bursts.

#### Scenario: Peak RF transmission transient
- **WHEN** the Wi-Fi 6 transmitter fires at maximum power simultaneously with dual-core peak load
- **THEN** the 3.3V core supply rail SHALL not droop below 3.0V, preventing brownout resets or clock instability

### Requirement: Fail-Safe Dual-Bank OTA Partitioning
The flash storage layout SHALL provide two symmetric application partitions (ota_0 and ota_1) with Ed25519 cryptographic verification and automatic bootloader rollback on failed health validation.

#### Scenario: Incomplete or corrupted firmware update
- **WHEN** an OTA update stream is interrupted or fails cryptographic signature verification
- **THEN** the bootloader SHALL reject the update, leave otadata unchanged, and continue booting from the known-good active slot
