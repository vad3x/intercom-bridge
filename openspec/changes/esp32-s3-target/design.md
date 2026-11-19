# Design

## Context

The firmware composition root uses `PlatformDriver` to start the hardware and spawn an audio worker. `PlatformPeripherals` and `main.rs` still pass C6-numbered pins through shared code. S31 starts a Core 1 worker with synthetic audio. The real I2S/DMA pipeline is currently enabled only for C6. GPIO, persistence, and OTA also depend on local Embedded Device Foundation crates that currently select real flash and some peripherals through RISC-V cfgs.

## Goals / Non-Goals

Keep target selection, pin routing, RNG, flash policy, and memory ownership at the firmware and adapter boundaries. Preserve the existing intercom state machine, command ports, Matter endpoints, and audio session ownership. This change covers firmware on the identified S3 board with a codec and isolated relay fixture. It leaves PCB design and S31 physical qualification to their own changes.

## Decisions

### Build and companion revisions

Use `esp32s3` with `xtensa-esp32s3-none-elf` and an explicit `mise exec -- cargo +esp` invocation. Document a pinned Espressif compiler, `rust-src`, the Xtensa C compiler and archiver used by `cc`, and the S3 linker setup. Keep the standard `mise` Rust pin for RISC-V and host tests. The build matrix covers the default C6, C6 DevKit, S31 with `riscv32imafc-unknown-none-elf`, and S3. A host-only matrix can check feature selection quickly. Release linking is a separate gate because it resolves the chip runtime and vendored C symbols.

Use `target_os = "none"` at the existing host/hardware boundaries and chip features where a peripheral or implementation differs. Pair that with explicit chip/target validation and the existing `MockPlatform` for host tests. Forward the feature through firmware, Matter, GPIO, audio, and WebRTC to the pinned HAL, runtime, radio, allocator, and bootloader dependencies. Retain feature-free host test paths. Publish immutable compatible revisions for EDF GPIO/OTA and `rs-matter-embassy` and document how the manifest path dependencies resolve in a clean checkout. The current S31 EDF feature forwarding and the `rs-matter-embassy` Wi-Fi-only support are present as local changes in sibling checkouts. Include the already-patched `rs-matter` revision in the build inputs.

### Pins, startup, and relay shutdown

Identify the board/module revision, USB console, soldered flash/PSRAM sizes and modes, and exposed header pins. For the supported S3 profile, group initialized peripherals by signal role in `platform/s3.rs` and hand role-named resources to the shared composition root. Derive output clearing from the same board mapping. Keep chip-specific clear-register access in the platform, including the correct GPIO bank when a relay is above GPIO31. Preserve the panic path's ability to clear outputs before software reset without relying on an executor, allocator, or existing peripheral ownership. Select safe pins and confirm that the fixture's pull resistors hold the relays inactive before firmware gains GPIO control.

### Radio and entropy

Share the C6 operational and BLE coexistence structure with S3 using the S3 radio bindings. Preserve the S31 Wi-Fi-only path. Bench-verify S3 scan, advertisement, connection, persistence, and reconnect, including the current upstream power overrides. Use the S3 SAR ADC entropy source during startup to seed a cryptographic DRBG, then release that source before bringing up Wi-Fi or BLE. Share the approved provider with Matter and WebRTC, reseed from the HAL's RF-backed TRNG when available, and gate operations until seeding succeeds. This keeps secure randomness available while the radio changes state. [ESP32-S3 random number generation](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/api-reference/system/random.html)

### Physical audio and memory

For the supported hardware profile, reuse the I2S pipeline and codec driver and start S3 I2S clocks before releasing codec reset. Start the media worker through `PlatformDriver::spawn_audio_worker`, with networking on Core 0 and the audio worker on Core 1. Validate DMA interrupt routing and the existing half-duplex sequence on that worker. Reuse the established call-state watches, frame channels, and WebRTC transport.

Size the S3 internal heap and audio-worker stack with its own linker map and measured high-water marks. Place DMA descriptors and buffers, worker stacks, radio allocations that require internal RAM, and flash-critical state in internal memory. Reuse the S31 external-capability allocation pattern for the jitter buffer on the S3 PSRAM profile. Expose capability-specific heap totals in `/api/status` and record internal heap and worker stack low-water values for the sustained-call gate. A combined internal-plus-external free-space number cannot establish the internal heap budget.

### Flash layout and OTA

USB-flash the first S3 image with an explicit board-matched table and bootloader configuration. Reuse the current symmetric `partitions.csv` layout on an S3 with at least 8 MiB flash if the release image fits and `espflash` verifies the packaged image; keep the layout's current KVS offsets. Use a board-specific table and a matching firmware layout if the device or image requires different geometry. A 4 MiB factory-only profile uses the tested no-OTA path. Discover the available module's capacity before selecting the layout.

Have EDF OTA consume the target chip ID and slot geometry from the firmware profile for header validation, staging, and boot selection. Make `scripts/ota.py` accept the matching chip, flash size, and partition table during ELF conversion and size checking, retaining the C6 defaults. Keep the ICOT Ed25519 format, inactive-slot write, and bootloader rollback. Seed the gitignored public-key file from the bench signing key when exercising signed OTA. Provide USB recovery using the same table and preserve Matter and system KVS. Use an isolated bench rollback fixture that fails before validating the staged slot, or reset at the corresponding debugger breakpoint.

## Risks / Trade-offs

- S3 wrappers can link while a code path quietly resolves to a host implementation. Check release ELF architecture, boot logs, real flash persistence, and diagnostics on the device.
- Board revisions expose different pins and module variants reserve some pins for memory. Confirm the exact board and map its pins before connecting the relay and codec fixtures. Use the identified console when checking for pin conflicts. [ESP32-S3-DevKitC-1 hardware reference](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/)
- Initial bring-up can use USB even when the board is factory-only. Full dual-bank OTA acceptance requires hardware with sufficient flash and a rollback-capable bootloader.
- Signed-image acceptance depends on both host packaging and device validation. Test wrong-chip, oversized, and invalid-signature images with host fixtures and measure rollback with the signed test image on the bench.

## Migration Plan

Qualify the companion revisions and build all release profiles from a clean dependency checkout. USB-flash the S3, verify boot and relay safety, commission it and check TCP, then run codec/media and memory checks. For a compatible dual-bank profile, upload a signed release, confirm it becomes valid, exercise rollback, and retain logs from both updates. Record the exact board profile and evidence with the target documentation. Coordinate archival with `esp32-s31-migration` so both sets of additions land in the same `hardware-platform` and `audio-intercom` capabilities. The existing `Universal Dual-Bank A/B OTA Support` requirement needs reconciliation during sync because the implemented 4 MiB C6 DevKit layout and the proposed S3 factory profile both use a single slot.
