# ESP32-C6-DevKitM-1 (4 MB Flash) Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable building and flashing the Smart Intercom firmware to standard ESP32-C6-DevKitM-1 boards (ESP32-C6-MINI-1 with 4 MB flash and WS2812 RGB LED on GPIO 8) for hardware-free bench testing of Matter over Wi-Fi, Apple Home, Web Dashboard, and REST API.

**Architecture:** Introduce a modular Cargo feature system with an umbrella `devkit` preset that combines `flash-4mb`, `ota-disabled`, and `led-smartled`. Provide a dedicated `partitions-4mb.csv` partition table, stub OTA updates to protect flash, and drive the onboard WS2812 RGB LED via the ESP32-C6 RMT peripheral to display live lifecycle and Matter status.

**Tech Stack:** Rust (`no_std`, riscv32imac-unknown-none-elf), `esp-hal` v1.2, `esp-hal-smartled` v0.18, `rs-matter`, Embassy async runtime.

## Global Constraints

- Default build without flags must remain 100% untouched for the production 8 MB PCB (`partitions.csv`, dual-bank OTA, GPIO 19 LED).
- Flash usage in 4 MB mode must never exceed `0x400000` (4,194,304 bytes).
- All partitions in `partitions-4mb.csv` must align to 4 KiB (0x1000) sector boundaries.
- No heap allocations in critical paths; zero regressions across existing host test suites.

---

### Task 1: Cross-Compilation Toolchain Environment & Build Alias

**Files:**
- Modify: `.cargo/config.toml:1-17`

**Interfaces:**
- Consumes: `/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-gcc` and `ar`.
- Produces: `[env]` bindings so `cargo check` and `cargo build` on `riscv32imac-unknown-none-elf` succeed without prepending manual environment variables.

- [ ] **Step 1: Check existing cargo config**
  Confirm `.cargo/config.toml` structure.

- [ ] **Step 2: Add `[env]` section and `build-devkit` alias**
  Add the following to `.cargo/config.toml`:
  ```toml
  [env]
  CC_riscv32imac_unknown_none_elf = "/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-gcc"
  AR_riscv32imac_unknown_none_elf = "/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-ar"
  ```
  And alias under `[alias]`:
  ```toml
  build-devkit = ["build", "-p", "intercom-firmware", "--release", "--features", "devkit"]
  ```

- [ ] **Step 3: Verify cargo check works directly**
  Run: `mise exec -- cargo check -p intercom-firmware --target riscv32imac-unknown-none-elf`
  Expected: Exits with code 0 without complaining about missing `riscv32-unknown-elf-gcc`.

- [ ] **Step 4: Commit**
  Commit message: `build(cargo): add riscv32 toolchain env and build-devkit alias`

---

### Task 2: 4 MB Partition Table & Modular Flash Memory Configuration

**Files:**
- Create: `partitions-4mb.csv`
- Modify: `crates/intercom-firmware/Cargo.toml:17-20`
- Modify: `crates/intercom-firmware/src/config.rs:50-80`
- Test: `crates/intercom-firmware/src/config.rs` (unit tests)

**Interfaces:**
- Consumes: Cargo feature `flash-4mb`.
- Produces: `config::SYSTEM_KVS_OFFSET` and `config::MATTER_KVS_OFFSET` at `0x3d0000` and `0x3d2000`.

- [ ] **Step 1: Create `partitions-4mb.csv`**
  ```csv
  # Name,       Type, SubType, Offset,   Size,     Flags
  nvs,          data, nvs,     0x9000,   0x6000,
  factory,      app,  factory, 0x20000,  0x3b0000,
  system_kvs,   data, nvs,     0x3d0000, 0x2000,
  matter_kvs,   data, nvs,     0x3d2000, 0x6000,
  ```

- [ ] **Step 2: Write failing unit test for 4 MB partition offsets**
  In `crates/intercom-firmware/src/config.rs`, write a test that verifies when `feature = "flash-4mb"` is enabled, `SYSTEM_KVS_OFFSET == 0x3d0000`, `MATTER_KVS_OFFSET == 0x3d2000`, and all ranges fit within `0x400000`.
  Run: `mise exec -- cargo test -p intercom-firmware --lib --features flash-4mb --target x86_64-apple-darwin`
  Expected: FAIL (unrecognized feature or values mismatch).

- [ ] **Step 3: Update `Cargo.toml` and `config.rs`**
  In `crates/intercom-firmware/Cargo.toml`:
  ```toml
  [features]
  default = []
  devkit = ["flash-4mb", "led-smartled", "ota-disabled"]
  flash-4mb = []
  led-smartled = ["dep:esp-hal-smartled"]
  ota-disabled = []
  dummy-persist = []
  ```
  In `crates/intercom-firmware/src/config.rs`:
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

- [ ] **Step 4: Run test to verify it passes**
  Run: `mise exec -- cargo test -p intercom-firmware --lib --features flash-4mb --target x86_64-apple-darwin`
  Expected: PASS.
  Also run standard test: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`
  Expected: PASS (8 MB offsets retained).

- [ ] **Step 5: Commit**
  Commit message: `feat(firmware): add 4mb partition table and modular flash-4mb layout`

---

### Task 3: OTA Disabling & Bootloader Bypass (`ota-disabled`)

**Files:**
- Create: `crates/intercom-firmware/src/disabled_ota.rs`
- Modify: `crates/intercom-firmware/src/lib.rs`
- Modify: `crates/intercom-firmware/src/main.rs:365-410`
- Test: `crates/intercom-firmware/src/disabled_ota.rs`

**Interfaces:**
- Consumes: `edf_core::ports::ota::{OtaFlashDriver, OtaStream, OtaDriverError}`.
- Produces: `DisabledOtaDriver` implementing `OtaFlashDriver`.

- [ ] **Step 1: Write failing test for `DisabledOtaDriver`**
  Create `crates/intercom-firmware/src/disabled_ota.rs` with test:
  Verify `DisabledOtaDriver.handle_flash_stream(...)` immediately returns `Err(OtaDriverError::FlashWriteFailed)` and does not touch flash.
  Run: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`
  Expected: FAIL (`DisabledOtaDriver` not implemented).

- [ ] **Step 2: Implement `DisabledOtaDriver`**
  Implement struct `DisabledOtaDriver`:
  ```rust
  pub struct DisabledOtaDriver;
  impl OtaFlashDriver for DisabledOtaDriver {
      async fn handle_flash_stream<S: OtaStream>(&mut self, _stream: &mut S, _content_length: usize) -> Result<(), OtaDriverError> {
          log::warn!("OTA firmware update rejected: OTA is disabled on this hardware profile");
          Err(OtaDriverError::FlashWriteFailed)
      }
  }
  ```

- [ ] **Step 3: Wire into `crates/intercom-firmware/src/main.rs`**
  Under `#[cfg(feature = "ota-disabled")]`:
  - Use `DisabledOtaDriver` instead of `FlashOtaDriver::new(config::OTA_PUBLIC_KEY)`.
  - Skip calling `edf_ota::bootloader::mark_running_slot_valid()`.
  Under `#[cfg(not(feature = "ota-disabled"))]`:
  - Retain `FlashOtaDriver` and `mark_running_slot_valid()`.

- [ ] **Step 4: Verify tests pass**
  Run: `mise exec -- cargo test -p intercom-firmware --lib --features ota-disabled --target x86_64-apple-darwin`
  Expected: PASS.

- [ ] **Step 5: Commit**
  Commit message: `feat(firmware): implement DisabledOtaDriver for ota-disabled feature`

---

### Task 4: WS2812 RMT LED Indicator Driver (`led-smartled`)

**Files:**
- Create: `crates/intercom-firmware/src/devkit_led.rs`
- Modify: `crates/intercom-firmware/Cargo.toml` (add `esp-hal-smartled`)
- Modify: `crates/intercom-firmware/src/lib.rs`
- Modify: `crates/intercom-firmware/src/main.rs:330-360`
- Test: `crates/intercom-firmware/src/devkit_led.rs`

**Interfaces:**
- Consumes: `esp_hal::rmt::Rmt`, `esp_hal_smartled::RmtSmartLeds`, `SystemStateReceiver`, `IntercomReporter`.
- Produces: `DevKitLedDriver` providing `run(state_receiver)` and `HoldFeedback` operations for the BOOT button.

- [ ] **Step 1: Add dependency `esp-hal-smartled`**
  In `crates/intercom-firmware/Cargo.toml` under `[target.'cfg(target_arch = "riscv32")'.dependencies]`:
  ```toml
  esp-hal-smartled = { version = "0.18.0", features = ["esp32c6"], optional = true }
  smart-leds = { version = "0.4", default-features = false, optional = true }
  ```
  Enable in feature:
  ```toml
  led-smartled = ["dep:esp-hal-smartled", "dep:smart-leds"]
  ```

- [ ] **Step 2: Implement `DevKitLedDriver`**
  In `crates/intercom-firmware/src/devkit_led.rs`:
  - On `riscv32`, wraps `RmtSmartLeds` on GPIO 8 using RMT channel 0.
  - Implements:
    - `IntercomReporter` (`report_ring`, `report_lock_state`)
    - Hold feedback methods (`hold_feedback_press`, `hold_feedback_stage1`, `hold_feedback_stage2`, `restore_state`)
    - Async `run(state_receiver)` loop that translates system states into RGB colors:
      - `Booting`: Slow Amber pulse (`RGB8 { r: 30, g: 15, b: 0 }`).
      - `Commissioning`: Blue pulse (`RGB8 { r: 0, g: 0, b: 35 }`).
      - `WifiConnecting`: Fast Cyan blink (`RGB8 { r: 0, g: 25, b: 25 }`).
      - `EcosystemConnecting`: Purple blink (`RGB8 { r: 25, g: 0, b: 25 }`).
      - `Ready` / `Normal`: Dim breathing Green (`RGB8 { r: 0, g: 5, b: 0 }`).
      - `Ring`: Fast Amber strobe (`RGB8 { r: 60, g: 30, b: 0 }`).
      - `Unlock`: Bright Green strobe (`RGB8 { r: 0, g: 60, b: 0 }`).
      - `EcosystemResetHold`: Solid Yellow (`RGB8 { r: 50, g: 25, b: 0 }`).
      - `FactoryResetHold`: Solid Red (`RGB8 { r: 60, g: 0, b: 0 }`).
  - Provide host mock when `target_arch != "riscv32"` so unit tests can test state-to-color mapping without hardware.

- [ ] **Step 3: Wire into `crates/intercom-firmware/src/main.rs`**
  Conditionally instantiate `DevKitLedDriver::new(peripherals.RMT, peripherals.GPIO8)` when `feature = "led-smartled"`, or `GpioLedDriver::new(peripherals.GPIO19)` when not.
  Attach indicator to `GpioBootButtonDriver` and `reporter` tuple.

- [ ] **Step 4: Verify host tests and riscv32 compilation**
  Run: `mise exec -- cargo test -p intercom-firmware --lib --features devkit --target x86_64-apple-darwin`
  Run: `mise exec -- cargo check -p intercom-firmware --features devkit --target riscv32imac-unknown-none-elf`
  Expected: Both PASS with exit code 0.

- [ ] **Step 5: Commit**
  Commit message: `feat(firmware): add DevKit WS2812 RMT LED driver on GPIO 8`

---

### Task 5: End-to-End Build, Flashing Docs & Verification

**Files:**
- Modify: `AGENTS.md` (DevKit flashing instructions)
- Modify: `README.md` (DevKit flashing instructions)

**Interfaces:**
- Consumes: All features compiled under `--features devkit`.
- Produces: Flashable release ELF at `target/riscv32imac-unknown-none-elf/release/intercom-bridge` and verified partition table `partitions-4mb.csv`.

- [ ] **Step 1: Build release binary with `devkit` preset**
  Run: `mise exec -- cargo build -p intercom-firmware --release --features devkit`
  Expected: Exit code 0.

- [ ] **Step 2: Check ELF section sizes**
  Run: `/private/tmp/riscv32-esp-elf/bin/riscv32-esp-elf-size target/riscv32imac-unknown-none-elf/release/intercom-bridge`
  Verify: `text + data` < 3,784,704 bytes (3.61 MiB).

- [ ] **Step 3: Run full host test pipeline**
  Run:
  - `mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`
  - `mise exec -- cargo test -p intercom-adapter-webrtc --target x86_64-apple-darwin`
  - `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`
  - `mise exec -- cargo test -p intercom-firmware --lib --features devkit --target x86_64-apple-darwin`
  - `mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin`
  - `cargo fe-check`
  Expected: All PASS.

- [ ] **Step 4: Update documentation**
  Add DevKit section to `AGENTS.md` and `README.md` with:
  ```bash
  # Build optimized release firmware for ESP32-C6-DevKitM-1 (4 MB):
  cargo build -p intercom-firmware --release --features devkit

  # Flash to connected DevKit board via USB:
  espflash flash target/riscv32imac-unknown-none-elf/release/intercom-bridge \
      --partition-table partitions-4mb.csv \
      --monitor
  ```

- [ ] **Step 5: Commit**
  Commit message: `docs: document ESP32-C6-DevKitM-1 flashing workflow and verify devkit build`
