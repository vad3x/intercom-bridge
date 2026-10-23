# GPIO Product Wrapper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse `edf-gpio` for the Intercom BOOT button, lifecycle LED, and ESP32 flash driver while preserving Intercom-specific ring/unlock feedback and all existing GPIO behavior.

**Architecture:** Add one product-neutral `blink` affordance to the shared foundation LED driver, then wrap the foundation boot/LED drivers and re-export the foundation flash driver from `intercom-adapter-gpio`. Ring and door drivers stay product-owned. The Intercom LED wrapper implements `IntercomReporter` by delegating access-event blinks to the shared driver.

**Tech Stack:** Rust 1.99.1, Embassy 0.8 sync / 0.5.1 time, `esp-hal` ESP32-C6, `edf-gpio`, Cargo through `mise`.

## Global Constraints

- Foundation checkout: `/Users/vadim/personal/projects/embedded-device-foundation`, current branch `codex/foundation-build`, base revision `1011623`.
- Intercom checkout: `/Users/vadim/personal/projects/intercom-matter`, branch for this slice starts from `main` at `46f7831`.
- Preserve active-low LED wiring, BOOT button thresholds (5000 ms / 10000 ms), debounce/poll timings, reset scopes, ring/unlock blink patterns, and all public `intercom-adapter-gpio` names.
- Keep `intercom-adapter-gpio` MCU-only. Do not add it to macOS host tests; ring and door modules remain RISC-V-only in practice.
- Do not modify ring or door behavior in this slice.
- Do not access hardware, flash firmware, upload OTA, read private keys, publish, push, create remotes, or change submodules.
- Use `CARGO_INCREMENTAL=0 mise exec -- cargo ...` with explicit targets and `--locked --offline` for verification.
- Run `rustfmt --check --config skip_children=true` on changed Rust files only; repository-wide formatting has a pre-existing failing baseline.
- Commit the foundation change first, then each Intercom task separately. No push.

## Preflight

- [x] Fast-forward local `main` from `e3ea8e0` to `46f7831` so the completed Matter slice is the base.
- [x] Create and switch to `codex/gpio-wrapper` from `main`.
- [x] Confirm `git status --short` is clean in both the Intercom and foundation checkouts.
- [x] Do not push either repository.

## Scope and File Structure

**Foundation:**

- Modify `/Users/vadim/personal/projects/embedded-device-foundation/crates/edf-gpio/src/led.rs`: expose the existing generic blink command publicly and cover it with an external integration test.

**Intercom:**

- Modify `Cargo.toml`: add the `edf-gpio` workspace path dependency.
- Modify `Cargo.lock`: add the local `edf-gpio` package.
- Modify `crates/intercom-adapter-gpio/Cargo.toml`: consume `edf-gpio`; drop direct `embedded-storage` dependencies after the flash re-export.
- Replace `crates/intercom-adapter-gpio/src/flash.rs`: compatibility re-export of the foundation flash driver.
- Replace `crates/intercom-adapter-gpio/src/led.rs`: Intercom reporter wrapper around the foundation lifecycle LED.
- Replace `crates/intercom-adapter-gpio/src/boot_button.rs`: Intercom API wrapper around the foundation reset button.
- Modify `crates/intercom-adapter-gpio/src/config.rs`: re-export shared lifecycle constants; retain only Intercom ring/door constants.
- Modify `docs/local-foundation-integration.md`: record the new foundation revision, API delta, verification, and remaining scope.

## Task 1: Expose a generic LED blink pattern in the foundation

**Files:**
- Modify: `/Users/vadim/personal/projects/embedded-device-foundation/crates/edf-gpio/src/led.rs`
- Test: `/Users/vadim/personal/projects/embedded-device-foundation/crates/edf-gpio/tests/gpio_integration.rs`

**Interfaces:**
- Consumes: the existing private `GpioLedDriver::blink(on_ms, off_ms, count)` implementation and `IndicatorCommand::Blink`.
- Produces: `pub fn GpioLedDriver::blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>)`, callable by downstream crates.

- [ ] **Step 1: Verify the foundation baseline**

Run in `/Users/vadim/personal/projects/embedded-device-foundation`:

```bash
git status --short
git rev-parse HEAD
CARGO_INCREMENTAL=0 mise exec -- cargo test -p edf-gpio --target x86_64-apple-darwin --locked --offline
```

Expected: clean status, revision `1011623`, 11 unit tests and 3 integration tests passing.

- [ ] **Step 2: Write the failing external integration test**

Append this test to `crates/edf-gpio/tests/gpio_integration.rs`:

```rust
#[test]
fn led_generic_blink_drives_finite_pattern() {
    // Arrange
    let driver = GpioLedDriver::new();
    let watch = SystemStateWatch::new();
    let receiver = watch.receiver().unwrap();

    // Act & Assert
    futures::executor::block_on(async {
        let steps = async {
            driver.blink(100, 100, Some(1));
            embassy_time::Timer::after_millis(20).await;
            assert!(driver.is_on());

            embassy_time::Timer::after_millis(100).await;
            assert!(!driver.is_on());

            embassy_time::Timer::after_millis(120).await;
            assert!(driver.is_on());
        };

        match embassy_futures::select::select(driver.run(receiver), steps).await {
            embassy_futures::select::Either::First(_) => unreachable!(),
            embassy_futures::select::Either::Second(()) => {}
        }
    });
}
```

- [ ] **Step 3: Run the test and confirm the public API is missing**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo test -p edf-gpio --test gpio_integration led_generic_blink_drives_finite_pattern --target x86_64-apple-darwin --locked --offline
```

Expected: compile error `E0624`, reporting that `blink` is a private method.

- [ ] **Step 4: Make the existing method public**

In `crates/edf-gpio/src/led.rs`, replace the private `fn blink` with:

```rust
    /// Signal a finite or continuous blink pattern.
    ///
    /// `count: None` blinks until interrupted. `Some(n)` blinks `n` times
    /// and returns to steady ON.
    pub fn blink(&self, on_ms: u64, off_ms: u64, count: Option<u8>) {
        self.signal.signal(IndicatorCommand::Blink {
            on_ms,
            off_ms,
            count,
        });
    }
```

Do not change `hold_feedback_stage1`, `hold_feedback_stage2`, or the command-loop behavior.

- [ ] **Step 5: Run the foundation tests**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo test -p edf-gpio --target x86_64-apple-darwin --locked --offline
```

Expected: 11 unit tests and 4 integration tests passing.

- [ ] **Step 6: Format and commit the foundation change**

Run:

```bash
mise exec -- rustfmt --edition 2021 --check --config skip_children=true crates/edf-gpio/src/led.rs crates/edf-gpio/tests/gpio_integration.rs
git diff --check
git add crates/edf-gpio/src/led.rs crates/edf-gpio/tests/gpio_integration.rs
git commit -m "feat: expose generic LED blink pattern"
```

Record the resulting commit:

```bash
git rev-parse HEAD
```

## Task 2: Consume `edf-gpio` and replace the local flash driver

**Files:**
- Modify: `/Users/vadim/personal/projects/intercom-matter/Cargo.toml`
- Modify: `/Users/vadim/personal/projects/intercom-matter/Cargo.lock`
- Modify: `/Users/vadim/personal/projects/intercom-matter/crates/intercom-adapter-gpio/Cargo.toml`
- Replace: `/Users/vadim/personal/projects/intercom-matter/crates/intercom-adapter-gpio/src/flash.rs`

**Interfaces:**
- Consumes: `edf_gpio::flash::{EspFlash, EspFlashError}`.
- Produces: the same `intercom_adapter_gpio::flash::{EspFlash, EspFlashError}` paths used by firmware.

- [ ] **Step 1: Add the workspace dependency**

In the root `Cargo.toml`, add beneath `edf-core`:

```toml
edf-gpio = { path = "../embedded-device-foundation/crates/edf-gpio" }
```

- [ ] **Step 2: Update the GPIO adapter manifest**

In `crates/intercom-adapter-gpio/Cargo.toml`, add:

```toml
edf-gpio = { workspace = true }
```

Remove these two direct dependencies because only the local flash implementation used them:

```toml
embedded-storage = { workspace = true }
embedded-storage-async = { workspace = true }
```

- [ ] **Step 3: Replace the flash implementation with a compatibility re-export**

Replace the entire contents of `crates/intercom-adapter-gpio/src/flash.rs` with:

```rust
//! ESP32 internal SPI flash driver re-exported from the shared foundation.

pub use edf_gpio::flash::{EspFlash, EspFlashError};
```

- [ ] **Step 4: Resolve and inspect the lockfile**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-adapter-gpio --target riscv32imac-unknown-none-elf --offline
```

Expected: the command succeeds and `Cargo.lock` gains only the local `edf-gpio` package and its needed dependency edges. Reject unrelated lockfile updates.

- [ ] **Step 5: Verify the adapter with locked dependencies**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-adapter-gpio --target riscv32imac-unknown-none-elf --locked --offline
```

Expected: success.

- [ ] **Step 6: Commit the flash reuse**

Run:

```bash
git add Cargo.toml Cargo.lock crates/intercom-adapter-gpio/Cargo.toml crates/intercom-adapter-gpio/src/flash.rs
git commit -m "refactor: reuse foundation flash driver"
```

## Task 3: Wrap the foundation LED and BOOT button drivers

**Files:**
- Replace: `crates/intercom-adapter-gpio/src/led.rs`
- Replace: `crates/intercom-adapter-gpio/src/boot_button.rs`

**Interfaces:**
- Consumes: foundation `GpioLedDriver::{new, run, blink, hold_feedback_press, hold_feedback_stage1, hold_feedback_stage2, restore_state}` and `GpioBootButtonConfig::{new, with_indicator}` / `GpioBootButtonDriver::new`.
- Produces: unchanged public `intercom_adapter_gpio::led::GpioLedDriver` and `intercom_adapter_gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver}`.

- [ ] **Step 1: Replace the LED driver with the reporter wrapper**

Replace the entire contents of `crates/intercom-adapter-gpio/src/led.rs` with:

```rust
//! Intercom access-event wrappers around the shared foundation LED driver.

use esp_hal::gpio::OutputPin;
use intercom_core::events::SystemStateReceiver;
use intercom_core::ports::reporter::{IntercomReporter, LockState};

use crate::config;

/// GPIO-based active-low LED indicator driver.
///
/// Lifecycle patterns come from `edf-gpio`; Intercom ring and unlock feedback
/// is layered on through the shared generic blink command.
pub struct GpioLedDriver {
    inner: edf_gpio::led::GpioLedDriver,
}

impl GpioLedDriver {
    /// Create a new LED driver with the given output pin.
    pub fn new(pin: impl OutputPin + 'static) -> Self {
        Self {
            inner: edf_gpio::led::GpioLedDriver::new(pin),
        }
    }

    /// Driver event loop — observes lifecycle states and drives the hardware pin.
    pub async fn run(&self, state_rx: SystemStateReceiver<'_>) {
        self.inner.run(state_rx).await;
    }

    /// Indicate that the reset/BOOT button is currently being pressed.
    pub fn hold_feedback_press(&self) {
        self.inner.hold_feedback_press();
    }

    /// Indicate that the ecosystem-reset hold threshold has been reached.
    pub fn hold_feedback_stage1(&self) {
        self.inner.hold_feedback_stage1();
    }

    /// Indicate that the factory-reset hold threshold has been reached.
    pub fn hold_feedback_stage2(&self) {
        self.inner.hold_feedback_stage2();
    }

    /// Re-apply the current lifecycle state after a button is released early.
    pub fn restore_state(&self) {
        self.inner.restore_state();
    }

    pub(crate) fn inner(&self) -> &edf_gpio::led::GpioLedDriver {
        &self.inner
    }
}

impl IntercomReporter for GpioLedDriver {
    fn report_ring(&self, ringing: bool) {
        if ringing {
            self.inner.blink(
                config::RING_BLINK_MS,
                config::RING_BLINK_MS,
                Some(config::RING_BLINK_COUNT),
            );
        }
    }

    fn report_lock_state(&self, state: LockState) {
        if matches!(state, LockState::Unlocked) {
            self.inner.blink(
                config::UNLOCK_BLINK_MS,
                config::UNLOCK_BLINK_MS,
                Some(config::UNLOCK_BLINK_COUNT),
            );
        }
    }
}
```

- [ ] **Step 2: Replace the BOOT button with the API-preserving wrapper**

Replace the entire contents of `crates/intercom-adapter-gpio/src/boot_button.rs` with:

```rust
//! Intercom BOOT button wrapper around the shared foundation reset driver.

use intercom_core::ports::{ResetScope, ResetTriggerDriver};

use crate::led::GpioLedDriver;

/// GPIO-based BOOT button driver for dual-stage system reset.
pub struct GpioBootButtonDriver {
    inner: edf_gpio::boot_button::GpioBootButtonDriver,
}

impl GpioBootButtonDriver {
    /// Create a new reset-button driver from the Intercom configuration.
    pub fn new(config: GpioBootButtonConfig) -> Self {
        Self {
            inner: edf_gpio::boot_button::GpioBootButtonDriver::new(config.inner),
        }
    }
}

/// Configuration for the BOOT button reset driver.
pub struct GpioBootButtonConfig {
    inner: edf_gpio::boot_button::GpioBootButtonConfig,
}

impl GpioBootButtonConfig {
    /// Create a config with the given pin and the foundation default hold times.
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            inner: edf_gpio::boot_button::GpioBootButtonConfig::new(pin),
        }
    }

    /// Attach the LED indicator used for live hold feedback.
    pub fn with_indicator(mut self, indicator: &'static GpioLedDriver) -> Self {
        self.inner = self.inner.with_indicator(indicator.inner());
        self
    }
}

impl ResetTriggerDriver for GpioBootButtonDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        self.inner.wait_for_reset().await
    }
}
```

- [ ] **Step 3: Verify the RISC-V adapter build**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-adapter-gpio --target riscv32imac-unknown-none-elf --locked --offline
```

Expected: success with no warnings from the changed modules. `intercom-adapter-gpio` stays MCU-only, so wrapper behavior is covered by the foundation integration test plus this compile gate rather than a macOS host test.

- [ ] **Step 4: Format and commit the wrappers**

Run:

```bash
mise exec -- rustfmt --edition 2021 --check --config skip_children=true crates/intercom-adapter-gpio/src/led.rs crates/intercom-adapter-gpio/src/boot_button.rs
git diff --check
git add crates/intercom-adapter-gpio/src/led.rs crates/intercom-adapter-gpio/src/boot_button.rs
git commit -m "refactor: wrap foundation GPIO boot and LED drivers"
```

## Task 4: Deduplicate GPIO timing constants, document, and run final verification

**Files:**
- Modify: `crates/intercom-adapter-gpio/src/config.rs`
- Modify: `docs/local-foundation-integration.md`

**Interfaces:**
- Consumes: `edf_gpio::config` shared lifecycle/timing constants.
- Produces: unchanged `intercom_adapter_gpio::config` names for existing consumers; only Intercom ring/door constants remain locally defined.

- [ ] **Step 1: Re-export shared timing constants**

Replace the lifecycle and LED-pattern constants in `crates/intercom-adapter-gpio/src/config.rs` with this re-export block, keeping the Intercom-specific ring/door constants below it:

```rust
pub use edf_gpio::config::{
    ACTIVITY_PULSE_MS, BOOTING_OFF_MS, BOOTING_ON_MS, BUTTON_DEBOUNCE_MS, BUTTON_POLL_INTERVAL_MS,
    COMMISSIONING_OFF_MS, COMMISSIONING_ON_MS, ECOSYSTEM_CONNECTING_OFF_MS,
    ECOSYSTEM_CONNECTING_ON_MS, ECOSYSTEM_RESET_HOLD_MS, ECOSYSTEM_RESET_OFF_MS,
    ECOSYSTEM_RESET_ON_MS, ERROR_BURST_COUNT, ERROR_OFF_MS, ERROR_ON_MS, ERROR_PAUSE_MS,
    FACTORY_RESET_HOLD_MS, FACTORY_RESET_OFF_MS, FACTORY_RESET_ON_MS, STARTING_OFF_MS,
    STARTING_ON_MS, UPDATING_STROBE_MS, WIFI_CONNECTING_OFF_MS, WIFI_CONNECTING_ON_MS,
};
```

The file must still define these Intercom-only values:

```rust
/// Duration of physical door strike relay pulse in milliseconds (PhotoMOS).
pub const UNLOCK_PULSE_MS: u64 = 5000;

/// Debounce time for ring optocoupler input in milliseconds.
pub const RING_DEBOUNCE_MS: u64 = 30;

/// Cooldown between ring optocoupler events in milliseconds.
pub const RING_COOLDOWN_MS: u64 = 2500;

/// Duration of each half of the ring indication blink cycle (ms).
pub const RING_BLINK_MS: u64 = 100;

/// Number of blinks for ring indication.
pub const RING_BLINK_COUNT: u8 = 2;

/// Duration of each half of the unlock indication blink cycle (ms).
pub const UNLOCK_BLINK_MS: u64 = 200;

/// Number of blinks for unlock indication.
pub const UNLOCK_BLINK_COUNT: u8 = 2;
```

Do not leave duplicate local definitions of the re-exported names.

- [ ] **Step 2: Verify the adapter and firmware compile**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-adapter-gpio --target riscv32imac-unknown-none-elf --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo check -p intercom-firmware --target riscv32imac-unknown-none-elf --locked --offline
```

Expected: both succeed.

- [ ] **Step 3: Run host regression tests**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-adapter-ota -p intercom-pairing-tool --target x86_64-apple-darwin --locked --offline
CARGO_INCREMENTAL=0 mise exec -- cargo test -p intercom-firmware --lib --test partitioned_kv_tests --target x86_64-apple-darwin --locked --offline
```

Expected: 114 host-package tests, 9 firmware library tests, and 8 partitioned-KV tests pass (131 total).

- [ ] **Step 4: Run strict Clippy and the MCU release build**

Run:

```bash
CARGO_INCREMENTAL=0 mise exec -- cargo clippy -p intercom-core --all-targets --target x86_64-apple-darwin --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 mise exec -- cargo build -p intercom-firmware --release --locked --offline
```

Expected: Clippy passes and the RISC-V release build succeeds.

- [ ] **Step 5: Verify the OTA image still fits**

Run:

```bash
mise exec -- espflash save-image --chip esp32c6 --flash-size 8mb target/riscv32imac-unknown-none-elf/release/intercom-bridge /private/var/folders/hf/5gzk0g1j68d21tm7rc05kt8m0000gn/T/opencode/intercom-bridge-gpio.bin
```

Expected: reported app size is below `3,473,408` bytes. Record the exact size in the integration document.

- [ ] **Step 6: Update the integration document**

In `docs/local-foundation-integration.md`:

1. Replace the validated foundation revision `1011623` with the Task 1 commit hash.
2. Add a `## GPIO product wrapper slice` section after the Matter section covering:
   - `edf-gpio` dependency and flash re-export.
   - Foundation generic LED `blink` API and why it is product-neutral.
   - LED lifecycle reuse plus Intercom `IntercomReporter` ring/unlock feedback.
   - BOOT button wrapper preserving reset thresholds and feedback.
   - Ring/door remaining product-owned.
   - Foundation `edf-gpio` test result (11 unit + 4 integration = 15).
   - Intercom host total (131), RISC-V build, image size, and formatting.
   - No hardware, flash, OTA upload, or private-key access.
3. Remove `GPIO product wrapper` from `## Remaining migration scope` and renumber the remaining items.

- [ ] **Step 7: Format, inspect, and commit**

Run:

```bash
git diff --name-only --diff-filter=ACM -- '*.rs' | xargs mise exec -- rustfmt --edition 2021 --check --config skip_children=true
git diff --check
git status --short
git diff --stat
```

Then commit:

```bash
git add crates/intercom-adapter-gpio/src/config.rs docs/local-foundation-integration.md
git commit -m "docs: record GPIO foundation reuse"
```

## Final Acceptance

- `edf-gpio` exposes a generic public LED blink method and all 15 `edf-gpio` tests pass.
- `intercom-adapter-gpio` no longer owns a flash implementation, LED lifecycle engine, or BOOT reset state machine.
- Ring and door behavior is unchanged.
- Public Intercom GPIO type names and reset/LED behavior are unchanged.
- 131 Intercom host tests pass.
- Strict core Clippy and the RISC-V release build pass.
- The OTA image fits the `3,473,408`-byte slot.
- Integration documentation records the new foundation revision and remaining scope.
- No hardware or network-affecting operation is performed.
