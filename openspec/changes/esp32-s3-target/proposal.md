# Proposal

## Why

The ESP32-S3 provides a dual-core, PSRAM-capable board for running and measuring the intercom's real audio path. The firmware currently builds for ESP32-C6 and ESP32-S31, and its build, GPIO, and flash paths assume RISC-V or C6-specific hardware.

## What Changes

- Add `esp32s3` as an explicit bare-metal firmware feature and build profile with a pinned Espressif Xtensa toolchain. Keep ESP32-C6 as the default and retain the ESP32-S31 and C6 DevKit profiles.
- Wire the S3 board's GPIO, relay shutdown, indicator, clock, I2S DMA, codec, and PSRAM through its platform configuration.
- Support BLE-assisted Matter commissioning, operational Wi-Fi and Matter/TCP, and authenticated WebRTC listen and talk. Establish cryptographic entropy across radio startup and reconnect.
- Build and flash the S3 release image on the available board, measure media and memory behavior, and verify signed dual-bank OTA on a flash configuration that supports it.
- Record reproducible companion dependency revisions, build commands for all targets, and evidence from the S3 hardware checks.

## Capabilities

### New Capabilities

- `hardware-platform` adds S3 target selection, board wiring and relay safety, radio startup, memory diagnostics, and signed image handling to the capability introduced by the pending S31 migration delta.
- `audio-intercom` adds physical S3 audio and WebRTC acceptance criteria to the capability introduced by the pending S31 migration delta.

### Modified Capabilities

## Impact

- Firmware composition and platform code, GPIO/audio/Matter/WebRTC adapters, build and image tooling, and board documentation.
- Companion `embedded-device-foundation` GPIO/OTA and `rs-matter-embassy` revisions used through local Cargo dependencies.
- The S3 board revision, pinout, flash density, and PSRAM mode must be identified during bring-up. Codec and isolated relay fixtures are required for physical audio and relay acceptance. Circuit-board redesign is outside this firmware change.
