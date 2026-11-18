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

The proposed Core 0 target of 55 KiB requires freeing about 33.5 KiB of internal RAM in this build. It has not been implemented or tested. Propose an application-side change for review before editing firmware or companion libraries. Stable boot, BLE commissioning, synthetic-audio streaming, and dual-bank OTA still await hardware verification. The C6 release build passed, the S31 release link exceeded RAM, and companion revision pinning remains open.

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
