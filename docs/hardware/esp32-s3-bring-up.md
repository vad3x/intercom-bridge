# ESP32-S3-N16R8 Bring-up

## Module Profile

The selected board is **ESP32-S3-DevKitC-1** with an **ESP32-S3-N16R8** module: 16 MiB Quad SPI flash and 8 MiB Octal SPI PSRAM. Confirm the module marking and DevKitC PCB revision from the attached hardware before connecting the codec or relay fixture.

- **Octal PSRAM:** Reserves GPIO33–37.
- **Strapping / USB / Console:** Keep GPIO0, GPIO3, GPIO45, GPIO46 for strapping, GPIO19/20 for native USB, and GPIO43/44 for the default UART console.
- **RGB Status LED:** Driven via RMT in GRB order on **GPIO47** (DevKitC-1 v1.1 revisions may route the LED to GPIO38; verify silkscreen before flashing).

References:
- [ESP32-S3-WROOM-1 datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf)
- [ESP32-S3-DevKitC-1 user guide](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.1.html)

## Candidate Signal Map

| Signal | GPIO | Safe / Startup Behavior |
|---|---:|---|
| Ring input | 4 | Input; confirm optocoupler polarity |
| Listen relay | 17 | Clear low before reset and panic recovery |
| Talk relay | 18 | Clear low before reset and panic recovery |
| Door relay | 40 | Clear low before reset and panic recovery |
| Boot button | 16 | Input; verify board button routing |
| WS2812 RGB status LED | 47 | Driven via RMT in GRB order (confirm silkscreen) |
| Codec reset, active low | 21 | Keep asserted until I2S clocks are ready |
| I2S MCLK/BCLK/WS/DOUT/DIN | 10/11/12/13/14 | Scope clocks and data before connecting intercom |
| Codec I2C SDA/SCL | 8/9 | 400 kHz; confirm pull-ups and voltage |

Relay outputs span both GPIO output banks during emergency clear (GPIO17/18 in low bank, GPIO40 in high bank). Verify inactive electrical levels on an isolated relay fixture before connecting a live intercom.

## Memory & Flash Architecture

- **Internal RAM & PSRAM:** `MatterStack` (~110 KiB), `WebRtcMediaResources` (~35 KiB), `DtlsSrtpResources` (32 KiB), and `WebRtcJitterBuffer` are allocated in 8 MiB Octal SPI PSRAM via `platform::s3::alloc_psram_uninit` / `alloc_psram_in_place`. Core 1's 32 KiB audio-worker stack uses `#[esp_hal::ram(unstable(dcache_reclaimed))]`, and `INTERNAL_HEAP_BYTES` is 120 KiB, preserving >55 KiB for Core 0's linked stack.
- **Dual-Core Flash Safety:** `EspFlash` (`intercom-adapter-gpio`) wraps `esp_storage::FlashStorage` with `.multicore_auto_park()` on `esp32s3`, running ROM SPI flash calls from IRAM while parking Core 1.
- **Partition Layout:** The 8 MiB dual-bank `partitions.csv` layout provides two `0x350000`-byte OTA slots (`ota_0` / `ota_1`) and preserves existing `system_kvs` / `matter_kvs` offsets. The 4 MiB `partitions-4mb.csv` factory-only profile (`esp32s3,flash-4mb,ota-disabled`) is also supported.

## Build & Flash

```bash
# Build S3 release ELF (via pinned Linux container):
mise exec -- cargo build-s3

# Build S3 4 MiB factory-only ELF:
mise exec -- cargo build-s3-factory

# Identify connected board & check for stale serial monitors:
mise exec -- espflash board-info --port "$S3_PORT"
lsof -nP "$S3_PORT"

# Flash dual-bank image (omit `matter_kvs` from --erase-parts to preserve pairing):
mise exec -- espflash flash --chip esp32s3 --port "$S3_PORT" --flash-size 16mb \
    --partition-table partitions.csv --erase-parts otadata \
    target/s3-linux/xtensa-esp32s3-none-elf/release/intercom-bridge

# Non-intrusive serial monitor:
python3 scripts/monitor.py --no-reset --port "$S3_PORT"
```

## Resource Acceptance Criteria

- Shared HMAC-DRBG seeds from SAR ADC TRNG before radio startup and releases `ADC1` prior to Wi-Fi/BLE init.
- PSRAM initializes at 8 MiB and `/api/status` reports internal RAM and PSRAM separately.
- Sustained Matter Listen session (>180s under concurrent Wi-Fi traffic) emits ICE nomination, DTLS verification, and outbound SRTP packets, maintaining `min_internal_free_bytes >= 32768` and `min_audio_stack_free_bytes >= 16384`.
- Relay outputs stay inactive through cold boot, software reset, and controlled panic on an isolated fixture.

## Known Limitations

- On bench without an external TLV320AIC3204 codec wired, I2C codec init logs a warning and outbound Listen audio is fed from the embedded 16 kHz speech sample (`audio_input_future` in `main.rs`).
- Dual-bank OTA rollback (`tasks.md` 6.6) and live codec/relay fixture validation (`tasks.md` 6.5) remain to be completed on wired hardware.
