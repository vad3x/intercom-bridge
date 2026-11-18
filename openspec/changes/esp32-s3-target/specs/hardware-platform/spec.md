# Spec Delta

## Purpose

Defines supported firmware targets, board integration, safe startup and reset behavior, and target-specific flash and memory capabilities.

## ADDED Requirements

### Requirement: ESP32-S3 Firmware Target Selection
The build SHALL support ESP32-S3 as an explicit `esp32s3` feature with `xtensa-esp32s3-none-elf` and a reproducible Espressif toolchain. The regular ESP32-C6, C6 DevKit, and ESP32-S31 build profiles SHALL remain available, with ESP32-C6 selected by default.

#### Scenario: Build a supported target
- **WHEN** a supported chip and its corresponding toolchain and board profile are selected
- **THEN** the release build SHALL link bootable firmware for that chip using the documented companion dependency revisions

#### Scenario: Reject an invalid target selection
- **WHEN** a bare-metal build selects multiple chips, selects no chip, or uses a Rust target that does not match the chip
- **THEN** the build SHALL fail with a diagnostic that identifies the required selection

### Requirement: ESP32-S3 Board Wiring and Relay Safety
The supported S3 board profile SHALL identify physical pins by their signal roles and keep the listen, talk, and door relay outputs de-energized during startup, reset, and panic recovery.

#### Scenario: Check board pin assignments
- **WHEN** the selected module and board revision are configured
- **THEN** ring, relay, button, indicator, and codec signals SHALL use available pins, reserving the pins required for flash, PSRAM, and the selected console

#### Scenario: Reset or panic while a relay is active
- **WHEN** a controlled reset or panic occurs during a bench relay operation
- **THEN** the firmware SHALL de-energize all relay outputs before requesting software reset
- **AND** the outputs SHALL stay de-energized during reboot until a new authorized operation requests them

### Requirement: ESP32-S3 Matter Commissioning and Operation
The ESP32-S3 SHALL support BLE-assisted Wi-Fi commissioning, operational Matter/TCP, and Wi-Fi reconnection using persisted credentials. Matter and WebRTC SHALL obtain cryptographically secure randomness throughout initial startup and radio transitions.

#### Scenario: Commission a fresh bridge and reconnect
- **WHEN** a Matter controller commissions an S3 bridge over BLE and the bridge restarts or Wi-Fi is interrupted
- **THEN** the bridge SHALL join or rejoin Wi-Fi, restore its fabric and settings, and accept operational Matter/TCP without recommissioning

#### Scenario: Start a session during an entropy-source transition
- **WHEN** a Matter or WebRTC operation requests secure randomness during radio startup or reconnect
- **THEN** the operation SHALL receive cryptographically secure bytes or return a retryable unavailable error

### Requirement: ESP32-S3 PSRAM and Memory Diagnostics
An S3 profile that requires PSRAM SHALL report an explicit startup error if PSRAM is unavailable. An operational S3 bridge SHALL report internal SRAM and registered PSRAM totals and free space separately via `/api/status`.

#### Scenario: Boot a PSRAM-enabled profile
- **WHEN** the S3 bridge boots with the profile's configured PSRAM
- **THEN** the memory status SHALL include nonzero totals for internal memory and PSRAM
- **AND** the WebRTC jitter storage SHALL be placed in external memory

#### Scenario: Detect absent or unusable PSRAM
- **WHEN** the configured S3 profile requires PSRAM and the module cannot initialize it
- **THEN** the bridge SHALL log a diagnostic before serving Matter or media

### Requirement: ESP32-S3 Image and Partition Compatibility
For a supported S3 layout with two application slots, firmware and host tooling SHALL validate the chip ID and permitted image size for that layout. The device SHALL verify the signature before selecting the staged image to boot, preserve credentials, and support rollback to the previous slot. A factory-only layout SHALL reject OTA requests.

#### Scenario: Reject a target or layout mismatch
- **WHEN** an S3 update upload identifies a different chip or exceeds the target slot capacity
- **THEN** the device SHALL reject the update before writing the payload or switching slots

#### Scenario: Boot and roll back a signed update
- **WHEN** a correctly signed S3 image is staged on an OTA-enabled layout
- **THEN** the firmware SHALL stage it in the inactive slot and retain the existing settings
- **AND** the bootloader SHALL restore the previous slot if the staged image fails to complete its boot health check

#### Scenario: Use a factory-only board layout
- **WHEN** an S3 bridge runs on a documented profile with a single application partition
- **THEN** it SHALL report OTA as unavailable and allow recovery by USB flash
