# ESP32-S3-N16R8 Bring-up

## Boot failure at checkpoint `d1ab74d` (2026-11-18)

USB flashing succeeded with the factory test build (`esp32s3,flash-4mb,ota-disabled`, `partitions-4mb.csv`, partition `factory`, physical flash size 16 MB). On three boots the uncommissioned board reached Matter Wi-Fi/BLE startup, hit the Core 0 stack guard in `Bump::alloc_with`, and rebooted.

| Evidence from the flashed ELF and crash trace | Bytes |
|---|---:|
| Linked Core 0 stack (`0x3fcd6110` to `0x3fcdb700`) | 22,000 |
| Stack headroom above the guard on entry to the failing call | 948 |
| Xtensa stack frame for the call | 4,768 |
| Space still available in the Matter bump arena | 42,160 |

The frame exceeds the available stack headroom. The disassembly shows a 904-byte copy crossing the guard. Core 1 has a separate 32 KiB stack. The local serial capture is at `/private/tmp/intercom-s3-stack-fix-boot.log`.

An earlier overflow occurred while constructing `WebRtcMediaResources`. Its current `init_in` workaround let the board reach Matter startup. Replacing its unsafe copy remains open. Verify a replacement against the optimized Xtensa output as well as the host test so a large stack temporary does not return.

The approved PSRAM move was implemented without editing companion libraries. The first flashed build exposed a zero-sized standard heap region, which panicked during allocator initialization; conditionally omitting empty regions fixed that. The board then reached BLE startup but the BTDM controller returned `-2` with only 48,640 bytes of internal heap free before Matter startup. After approval, the S3 internal heap was raised to 120 KiB. The rebuilt ELF leaves 86,752 bytes for Core 0's linked stack, above the 56,320-byte target. Hardware logs confirmed 8 MiB Octal PSRAM initialization, Matter Wi-Fi/BLE startup, `GATT: Advertising`, and stable test-audio operation. At about 15 minutes uptime the uncommissioned Matter window expired and BLE stopped advertising; the board continued running without a reset through at least 51 minutes uptime. The codec warning is expected on this bench because no codec is attached. Relay electrical checks, Matter commissioning, Wi-Fi operational state, sustained WebRTC, and OTA remain open.

### BLE commissioning window expires after 15 minutes

The uncommissioned startup path opens the Matter basic commissioning window for `MAX_COMM_WINDOW_TIMEOUT_SECS` (`15 * 60` seconds) in the local `rs-matter` checkout. The S3 serial capture later reported `PASE Commissioning Window expired, closing` and `Commissioning window closed; BLE peripheral stopped`; after that, the device is no longer available for BLE pairing. If pairing has not started within 15 minutes of boot, briefly press the board's Reset button or power-cycle it, then begin adding it again promptly. Do not hold BOOT for this: the firmware maps a 5-second BOOT hold to an ecosystem reset and a 10-second hold to a factory reset.

### Approved application-side plan (Core 0 stack & internal RAM)

Decision: **Option A — Move `MatterStack` (and `WEBRTC_DTLS_RESOURCES`) to external PSRAM on `esp32s3`.**

1. **Move `MatterStack<BUMP_SIZE, EmbassyWifiNetwork>` (~52 KiB) to PSRAM** in `crates/intercom-adapter-matter/src/controller.rs`:
   - Gate `static STACK_CELL` to `#[cfg(not(feature = "esp32s3"))]`.
   - On `esp32s3`, allocate `MatterStack` from `esp_alloc::HEAP.alloc_caps(esp_alloc::MemoryCapability::External.into(), layout)` (requiring `MatterController::new()` to run after `ActivePlatform::init()` registers PSRAM, which is already the startup order in `crates/intercom-firmware/src/main.rs`) and initialize in-place via `MaybeUninit::init_with(MatterStack::init(...))` so no 52 KiB temporary is staged on Core 0's stack.
   - Frees >52 KiB of internal `.bss`, lifting Core 0's linked stack above the 55 KiB target without patching `rs-matter` or shrinking `BUMP_SIZE` (`52_000`). Hardware validation later required raising `INTERNAL_HEAP_BYTES` from 48 KiB to 120 KiB for BTDM startup; the current Xtensa ELF retains 86,752 bytes of Core 0 stack.
2. **Move `WEBRTC_DTLS_RESOURCES` (`DtlsSrtpResources`, 32 KiB) to PSRAM** in `crates/intercom-firmware/src/platform/s3.rs` and `crates/intercom-firmware/src/main.rs`:
   - Add `allocate_dtls_srtp_resources()` using `MemoryCapability::External` and `core::ptr::write_bytes(..., 0, layout.size())` in-place.
   - Gate `WEBRTC_DTLS_RESOURCES` to `#[cfg(not(feature = "esp32s3"))]` and pass `0` instead of `core::mem::size_of::<DtlsSrtpResources>()` to `internal_heap_layout(...)`, reclaiming 32 KiB of `DRAM2_UNINIT` back into internal RAM.
3. **Keep in internal SRAM:** `AUDIO_STACK_BYTES` (`32_768`), I2S DMA buffers, `RX_AUDIO_CHANNEL` / `TX_AUDIO_CHANNEL` (~4.5 KiB total), and a 120 KiB `INTERNAL_HEAP_BYTES` budget. S3 BLE controller initialization failed with 48 KiB; after increasing the budget to 120 KiB, BLE GATT advertising started and the board ran without reboot for at least 60 seconds.
4. **Safety & `unsafe` cleanup (Option A):**
   - Consolidate all PSRAM allocations (`WebRtcMediaResources`, `WebRtcJitterBuffer`, `DtlsSrtpResources`, and `MatterStack` via an injected allocator/callback) behind one audited `alloc_psram_in_place<T>` helper in `crates/intercom-firmware/src/platform/s3.rs` so adapters and `main.rs` have zero `unsafe` allocation code.
   - Replace `static mut APP_CORE_STACK` and `static mut WEBRTC_DTLS_RESOURCES` with `StaticCell` (or PSRAM on `esp32s3`).
   - Fix the cross-core `&[u8]` data race in `sample_audio_stack_watermark` (`s3.rs`) using `read_volatile` or Core 1 self-sampling.
   - Replace `WebRtcMediaResources::init_in`'s `copy_nonoverlapping` (`crates/intercom-adapter-webrtc/src/session.rs`) with safe in-place `Init`.
5. **Verification:** See **Deterministic verification gates** below.

### Deterministic verification gates

Every gate below is binary (pass/fail) and must pass in order before moving to the next stage:

1. **Gate 1 — `unsafe` & `static mut` audit (source):**
   - `grep -nE '^[[:space:]]*static mut' crates/intercom-firmware/src/platform/s3.rs crates/intercom-firmware/src/main.rs` returns no mutable static declarations (exit 1). The plain substring also matches the required `&'static mut` return types of the PSRAM storage API.
   - `grep -n 'unsafe' crates/intercom-firmware/src/main.rs crates/intercom-adapter-webrtc/src/session.rs crates/intercom-adapter-matter/src/controller.rs` shows only `#[unsafe(no_mangle)]` on `custom_halt` (`main.rs`) and `NoInlineFuture::poll`'s `map_unchecked_mut` (`controller.rs`); zero `unsafe` in `session.rs` (`WebRtcMediaResources`).
   - `sample_audio_stack_watermark` in `crates/intercom-firmware/src/platform/s3.rs` does not call `core::slice::from_raw_parts`.
2. **Gate 2 — Host tests (exit 0):**
   - `mise exec -- cargo test --locked -p intercom-core -p intercom-media -p intercom-adapter-http -p intercom-adapter-webrtc -p intercom-firmware --lib --target x86_64-apple-darwin`
   - `mise exec -- cargo test --locked -p intercom-firmware --lib --no-default-features --features esp32s3 --target x86_64-apple-darwin`
   - `mise exec -- cargo test --locked -p intercom-firmware --lib --no-default-features --features esp32s31 --target x86_64-apple-darwin`
3. **Gate 3 — Release builds (exit 0):**
   - C6: `RUSTC_BOOTSTRAP=1 mise exec -- cargo build --locked -p intercom-firmware --release`
   - C6 DevKit: `RUSTC_BOOTSTRAP=1 mise exec -- cargo build --locked -p intercom-firmware --release --features devkit`
   - S3: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target mise exec -- cargo build-s3`
   - S3 factory: `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target mise exec -- cargo build-s3-factory`
4. **Gate 4 — ELF stack & disassembly check (Xtensa ELF):**
   - Core 0 linked stack (`_stack_end - _stack_start` from `nm` / `objdump`) is **≥ 56,320 bytes (55 KiB)**.
   - Neither `MatterStack`, `WebRtcMediaResources`, `WebRtcJitterBuffer`, nor `DtlsSrtpResources` appears as a multi-kilobyte static symbol in `.bss` or `.dram2_uninit`.
   - Disassembly of `MatterController::new` and `WebRtcMediaResources` initialization shows no frame `entry a1, <N>` exceeding **4,096 bytes** and no large stack-staged copy in either function.
5. **Gate 5 — Hardware boot & sustained-call acceptance (ESP32-S3-N16R8):**
   - Serial boot completes Matter Wi-Fi/BLE startup and reaches commissioning/operational state with **0** stack-guard panics or reboots.
   - `curl -fsS "http://$S3_IP:8080/api/status"` reports `state: Operational`, internal RAM, and 8 MiB PSRAM.
   - During a **>180s** Matter Listen session under concurrent Wi-Fi traffic, serial logs emit `WebRTC: ICE nominated peer`, `WebRTC: DTLS fingerprint verified; SRTP ready`, `WebRTC: first outbound SRTP audio packet sent`, and `min_internal_free_bytes >= 32768` and `min_audio_stack_free_bytes >= 16384`.

### Latest deterministic gate evidence (2026-11-18)

- **Gate 1: PASS.** `mise exec -- cargo fmt --all --check` and `git diff --check` pass. `grep -nE '^[[:space:]]*static mut' crates/intercom-firmware/src/platform/s3.rs crates/intercom-firmware/src/main.rs` returns no declarations. `main.rs` has only `#[unsafe(no_mangle)]` on `custom_halt`; `session.rs` has no `unsafe`; PSRAM allocation unsafety is inside `alloc_psram_in_place` in `platform/s3.rs`. `controller.rs` retains the pre-existing unsafe pin projection in `NoInlineFuture::poll`, unrelated to PSRAM. The S3 stack sampler has no `core::slice::from_raw_parts`.
- **Gate 2: PASS.** With `CARGO_TARGET_DIR=/Volumes/samsung-t7-black-2tb-ssd0/cargo-target/host`, the aggregate C6 host suite, S3 firmware suite, and S31 firmware suite all exited 0.
- **Gate 3: PASS.** The current C6 release, C6 DevKit release, S3 dual-bank release (`mise exec -- cargo build-s3`), and S3 factory release (`mise exec -- cargo build-s3-factory`) all exited 0. The S3 dual-bank app image is 3,106,512 bytes and fits the 3,473,408-byte `ota_0` slot. The incompatible dual-chip selection check failed at compile time with duplicate chip-generated macros, as expected.
- **Gate 4: PASS for the approved DoD functions.** In `/Volumes/samsung-t7-black-2tb-ssd0/cargo-target/s3-linux/xtensa-esp32s3-none-elf/release/intercom-bridge`, `_stack_start_cpu0=0x3fcdb700` and `_stack_end_cpu0=0x3fcc6420`, giving 86,752 bytes. `.bss` is `0x216b0` and `.dram2_uninit` is `0x12010`; `nm` found no static `MatterStack`, `WebRtcMediaResources`, `WebRtcJitterBuffer`, or `DtlsSrtpResources` object. Disassembly reports `MatterController::new` frame 464 bytes (`entry a1, 0x1d0`) and `WebRtcMediaResources::init_in` frame 32 bytes. The separate `Bump<52000>::alloc_with` monomorph for `EmbassyWifi::run` still has a 4,768-byte frame and stages a 4,624-byte future. This measurement is recorded explicitly; it is outside the current DoD, which scopes the 4,096-byte frame limit to the two named functions. No companion library was edited.
- **S3 flash and boot: PASS for commissioning readiness.** `lsof -nP /dev/cu.usbmodem5D161002871` showed the agent's monitor as the port owner; it was stopped and the port verified free before flashing. `mise exec -- espflash board-info --port /dev/cu.usbmodem5D161002871` identified ESP32-S3 revision v0.2, 40 MHz crystal, 16 MB flash, and Wi-Fi/BLE. `mise exec -- espflash flash --chip esp32s3 --port /dev/cu.usbmodem5D161002871 --flash-size 16mb --partition-table partitions.csv --target-app-partition ota_0 --erase-parts otadata /Volumes/samsung-t7-black-2tb-ssd0/cargo-target/s3-linux/xtensa-esp32s3-none-elf/release/intercom-bridge` exited 0. The boot log confirmed the dual-bank partition table and app at `0x20000`, 8 MiB Octal SPI PSRAM, Matter startup, commissioning window open, and `GATT: Advertising`. Runtime PSRAM allocations were 32,768 bytes (DTLS), 35,440 bytes (WebRTC media), 1,312 bytes (jitter buffer), and 112,816 bytes (Matter stack storage). The board has remained up for over 5 minutes after the latest reset with no stack-guard panic or reboot. Pairing code is `3116-642-2623`; QR payload is `MT:-24J0CEK01Y.VW21X3P0OUYH0R92G1GK5T1NDC42DNVK16MFO1O0`. The codec I2C warning is expected because no codec is connected; the test audio sample remains enabled. Relay electrical scope checks, commissioning, Wi-Fi operational checks, sustained WebRTC, and OTA remain open.

## Module profile

The selected board is ESP32-S3-DevKitC-1 with an ESP32-S3-N16R8 module: 16 MiB Quad SPI flash and 8 MiB Octal SPI PSRAM. Read the board revision from the attached hardware. GPIO routing below is a candidate profile and must be checked against its silkscreen and board schematic before connecting the codec or relay fixture.

Serial identification with `mise exec -- espflash board-info --port /dev/cu.usbmodem5D161002871` reported ESP32-S3 silicon revision v0.2, 40 MHz crystal, 16 MB flash, and Wi-Fi/BLE. The USB report does not identify the DevKitC PCB revision or PSRAM size/mode; confirm the module marking and board revision from the attached hardware before finalizing pins or memory configuration. The serial port can change after reset or reconnect.

The N16R8 module reserves GPIO33–37 for Octal PSRAM. Keep GPIO0, GPIO3, GPIO45, and GPIO46 available for strapping, GPIO19/20 for native USB, and GPIO43/44 for the default UART console. The supplied board pinout labels the onboard GRB WS2812 RGB LED on GPIO47 and adjacent GPIO48 as SPI, so this profile drives GPIO47 through RMT using GRB color order. DevKitC-1 v1.1 revisions may route the LED to GPIO38 instead; confirm the physical silkscreen against the supplied pinout before a hardware flash.

References: [ESP32-S3-WROOM-1 datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf), [ESP32-S3-DevKitC-1 user guide](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.1.html), [Espressif FAQ on DevKitC-1 RGB LED pins](https://docs.espressif.com/projects/esp-faq/en/latest/hardware-related/development-board.html).

## Candidate signal map

| Signal | GPIO | Safe/startup behavior |
|---|---:|---|
| Ring input | 4 | Input; confirm optocoupler polarity |
| Listen relay | 17 | Clear low before reset and panic recovery |
| Talk relay | 18 | Clear low before reset and panic recovery |
| Door relay | 40 | Clear low before reset and panic recovery |
| Boot button | 16 | Input; verify board button routing |
| WS2812 RGB status LED | 47 | Supplied board pinout; driven through RMT in GRB order, subject to silkscreen confirmation |
| Codec reset, active low | 21 | Keep asserted until I2S clocks are ready |
| I2S MCLK/BCLK/WS/DOUT/DIN | 10/11/12/13/14 | Scope clocks and data before connecting the intercom |
| Codec I2C SDA/SCL | 8/9 | 400 kHz; confirm pull-ups and voltage |

The relay outputs use both S3 GPIO output banks during emergency clear: GPIO17/18 are in the low bank and GPIO40 is in the high bank. Confirm inactive electrical levels with the isolated relay fixture before connecting a live intercom.

## Flash and partition profile

The module has 16 MiB flash. The current dual-bank partition table occupies the first 8 MiB and preserves the existing settings and Matter KVS offsets. Its two app slots are each `0x350000` bytes. Use it only if the S3 release image fits those slots and the selected bootloader supports rollback. The 4 MiB factory-only table is for the existing C6 DevKit profile and is not the N16R8 default.

Record `mise exec -- espflash board-info --port "$S3_PORT"` output, selected `S3_PARTITIONS`, `S3_APP_PARTITION`, and bootloader before first flash. Use a bench-only fixture for relay-clear, reset, panic, audio, and OTA rollback checks.

## Resource acceptance

- S3 seeds the shared HMAC-DRBG from the HAL SAR ADC-backed TRNG during platform startup, then releases the ADC entropy source before Matter starts Wi-Fi/BLE. Matter and WebRTC use the same provider; secure reads mix the hardware RNG after radio startup.
- Confirm PSRAM initializes at 8 MiB and `/api/status` reports internal RAM and PSRAM separately.
- Scope I2S clocks before codec reset is released. Use the embedded test audio sample as the input, as on the C6 DevKit profile; do not connect the dev board to a live intercom audio source. Verify sample playback, DMA stability, and half-duplex relay sequencing on an isolated fixture.
- Keep a Matter Listen session active for more than 180 seconds under concurrent Wi-Fi traffic.
- Record minimum internal free heap and audio-worker stack headroom. Acceptance thresholds are 32 KiB internal heap and 16 KiB worker stack headroom.
- During a WebRTC call, serial logs emit `WebRTC media resource low water` samples. The internal-heap value is the minimum across internal allocator regions; the stack value is the minimum untouched prefix measured from a `0xA5` watermark on Core 1's stack. The final `WebRTC media benchmark` line also reports the sampled internal-heap minimum. Record both minima from serial output for the sustained-call acceptance result.
- Verify relay outputs stay inactive through cold boot, software reset, and a controlled panic before connecting the live intercom.
