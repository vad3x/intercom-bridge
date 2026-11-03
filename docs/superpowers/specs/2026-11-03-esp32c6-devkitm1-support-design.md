# Technical Design: ESP32-C6-DevKitM-1 (4 MB Flash) Bench Support

- **Date:** 2026-11-03
- **Status:** Approved
- **Target Hardware:** ESP32-C6-DevKitM-1 (ESP32-C6-MINI-1 module, 4 MB SPI flash)
- **Primary Goal:** Enable compiling, flashing, and testing the Smart Intercom firmware (Matter over Wi-Fi, Apple Home, Web Dashboard, and REST API) on a standalone ESP32-C6 devboard without requiring physical intercom or audio hardware.

---

## 1. Problem Statement & Constraints

The production Intercom Bridge hardware uses an ESP32-C6-WROOM-1-N8 with 8 MB Quad SPI flash and a single active-low status LED on GPIO 19. The firmware release binary is ~2.91 MB.

When running on an **ESP32-C6-DevKitM-1**:
1. **Flash Space Constraint (4 MB Total)**:
   - The default `partitions.csv` allocates two 3.4 MB OTA slots (`ota_0` and `ota_1`), totaling ~6.8 MB + KVS partitions up to `0x6c8000`.
   - On a 4 MB flash module (max address `0x400000`), the 8 MB partition table and high KVS offsets cause flashing failures and ROM SPI flash access faults.
   - Dual-bank A/B OTA cannot fit two 2.9 MB app slots in 4 MB flash. A single `factory` app partition (~3.61 MB) is required.
2. **Different LED Hardware**:
   - The DevKitM-1 has no standard single GPIO LED. Its only user LED is an addressable **WS2812B RGB LED on GPIO 8**.
   - WS2812 LEDs require precise 800 kHz NRZ pulse timing (driven via the ESP32-C6 RMT peripheral), not simple high/low logic levels.
3. **Hardware-Free Bench Execution**:
   - The board will run without an attached TLV320AIC3204 audio codec, intercom ring optocoupler, or PhotoMOS door strike relay. The firmware must boot, initialize, and operate reliably without hanging or crashing.

---

## 2. Architecture & Design

### 2.1 Modular Feature Flags & `devkit` Preset Group
Cargo features will be organized modularly so capabilities can be composed independently or grouped together under the convenient `devkit` preset:

```toml
[features]
default = []

# Umbrella preset for ESP32-C6-DevKitM-1 bench testing
devkit = ["flash-4mb", "led-smartled", "ota-disabled"]

# Granular capability flags:
flash-4mb    = []                      # Relocates KVS ranges into 4 MB boundaries (0x3d0000 / 0x3d2000)
led-smartled = ["dep:esp-hal-smartled"]# Uses WS2812B RGB LED on GPIO 8 via RMT
ota-disabled = []                      # Stubs OTA writer & skips bootloader rollback check
dummy-persist = []
```

This modular separation allows:
1. `cargo build --features devkit`: One-shot preset for the ESP32-C6-DevKitM-1.
2. Independent mixing: E.g., `--features flash-4mb,ota-disabled` for a 4 MB board that uses a standard GPIO LED instead of WS2812, or `--features led-smartled` for an 8 MB board with an addressable LED.

### 2.2 4 MB Partition Table (`partitions-4mb.csv`)
A dedicated partition table file `partitions-4mb.csv` will be created in the repository root:

```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
factory,      app,  factory, 0x20000,  0x3b0000,
system_kvs,   data, nvs,     0x3d0000, 0x2000,
matter_kvs,   data, nvs,     0x3d2000, 0x6000,
```

- `nvs`: `0x9000` (24 KiB) — standard ESP-IDF Wi-Fi/NVS partition.
- `factory`: `0x20000` to `0x3d0000` (3,784,704 bytes = 3.61 MiB) — comfortably fits the ~2.91 MB release binary with >700 KiB headroom.
- `system_kvs`: `0x3d0000` (8 KiB = 2 sectors) — stores system settings and door unlock duration.
- `matter_kvs`: `0x3d2000` (24 KiB = 6 sectors) — stores Matter fabric keys, node credentials, and bindings.
- Total flash footprint: `0x3d8000` (3.84 MiB), leaving the remainder below the 4.0 MiB boundary (`0x400000`).

### 2.3 Flash Memory Layout Selection (`config.rs`)
Partition offsets in `crates/intercom-firmware/src/config.rs` will be conditionally compiled based on `feature = "flash-4mb"`:

```rust
#[cfg(feature = "flash-4mb")]
pub const SYSTEM_KVS_OFFSET: u32 = 0x3d0000;
#[cfg(not(feature = "flash-4mb"))]
pub const SYSTEM_KVS_OFFSET: u32 = 0x6c0000;

#[cfg(feature = "flash-4mb")]
pub const MATTER_KVS_OFFSET: u32 = 0x3d2000;
#[cfg(not(feature = "flash-4mb"))]
pub const MATTER_KVS_OFFSET: u32 = 0x6c2000;

#[cfg(feature = "devkit")]
pub const HW_VER_STR: &str = "2.0-devkit";
#[cfg(not(feature = "devkit"))]
pub const HW_VER_STR: &str = "2.0";
```

### 2.4 OTA Behavior in DevKit Mode (`ota-disabled`)
When `feature = "ota-disabled"` is active:
- **`DisabledOtaDriver`**: A lightweight driver implementing `edf_core::ports::ota::OtaFlashDriver` that rejects incoming flash requests with `Err(OtaDriverError::FlashWriteFailed)`.
- **Bootloader Verification**: In `crates/intercom-firmware/src/main.rs`, skipping `edf_ota::bootloader::mark_running_slot_valid()`, avoiding flash writes to a non-existent `otadata` partition.
- **REST / Web API**: Any upload attempt to `/api/ota/flash` cleanly returns an HTTP 400 error rather than attempting flash operations.

### 2.5 DevKit WS2812 RGB LED Driver (`led-smartled`)
Under `feature = "led-smartled"` on `riscv32`:
- Uses `esp-hal-smartled = "0.18.0"` with `esp-hal/unstable` on ESP32-C6.
- Allocates `peripherals.RMT` channel 0 and `peripherals.GPIO8`.
- Creates `DevKitLedDriver` which implements:
  - `IntercomReporter` (`report_ring`, `report_lock_state`)
  - Hold feedback methods (`hold_feedback_press`, `hold_feedback_stage1`, `hold_feedback_stage2`, `restore_state`)
  - An async `run` loop observing `SystemStateReceiver` and driving RGB patterns:
    - **Booting**: Slow Amber pulsing (`RGB8 { r: 30, g: 15, b: 0 }`).
    - **Commissioning (PASE / Ready to pair)**: Blue pulsing (`RGB8 { r: 0, g: 0, b: 35 }`).
    - **Wi-Fi Connecting**: Fast Cyan blinking (`RGB8 { r: 0, g: 25, b: 25 }`).
    - **Ecosystem Connecting (CASE)**: Purple blinking (`RGB8 { r: 25, g: 0, b: 25 }`).
    - **Operational / Idle**: Off or very dim breathing Green (`RGB8 { r: 0, g: 5, b: 0 }`).
    - **Ring Event**: Bright Yellow/Amber strobe (`RGB8 { r: 60, g: 30, b: 0 }`).
    - **Unlock Event**: Bright Green strobe (`RGB8 { r: 0, g: 60, b: 0 }`).
    - **Ecosystem Reset Hold**: Solid Yellow (`RGB8 { r: 50, g: 25, b: 0 }`).
    - **Factory Reset Hold**: Solid Red (`RGB8 { r: 60, g: 0, b: 0 }`).

### 2.6 Bench Hardware Gracefulness
- **BOOT Button**: On the ESP32-C6-DevKitM-1, the physical `BOOT` button is connected to **GPIO 9** (active low with pull-up), which matches our production pin configuration (`peripherals.GPIO9`). Pressing and holding `BOOT` triggers ecosystem and factory resets identically.
- **Ring Input**: GPIO 2 defaults to high impedance / pull-up, remaining idle without false triggers.
- **Audio Codec (TLV320AIC3204)**: When absent, I2C probe returns an error which is already caught and logged (`"TLV320AIC3204 init failed (bench without codec?)"`).
- **I2S DMA Stream**: Starts normally as master, clocking silence to DMA without stalling.

### 2.7 Tooling & Build Environment
1. Add environment variables to `.cargo/config.toml`:
   ```toml
   [env]
   CC_riscv32imac_unknown_none_elf = "/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-gcc"
   AR_riscv32imac_unknown_none_elf = "/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-ar"
   ```
   Eliminates the need for manual environment prefixes on every cargo command.
2. Add build alias in `.cargo/config.toml`:
   ```toml
   build-devkit = ["build", "-p", "intercom-firmware", "--release", "--features", "devkit"]
   ```
3. Documented flash command:
   ```bash
   espflash flash target/riscv32imac-unknown-none-elf/release/intercom-bridge \
       --partition-table partitions-4mb.csv \
       --monitor
   ```

---

## 3. Verification Plan

1. **Compilation Check**:
   - `cargo build -p intercom-firmware --release --features devkit` builds with exit code 0.
   - Text + data size is ~2.91 MB, fitting well inside the 3.61 MiB `factory` partition.
2. **Host Regression Check**:
   - `cargo test -p intercom-firmware --lib --target x86_64-apple-darwin` passes cleanly.
   - `cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin` passes cleanly.
   - Default production build (`cargo build -p intercom-firmware --release`) remains unaffected.
3. **Partition Table Validation**:
   - Verify all partitions in `partitions-4mb.csv` are 4 KiB sector-aligned and total size does not exceed `0x400000` (4194304 bytes).
