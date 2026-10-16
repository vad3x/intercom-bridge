# Partitioned Wi-Fi Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple Wi-Fi configuration and network credentials from the Matter KVS partition into a dedicated `wifi_kvs` partition (`0x6c6000..0x6c8000`, 8 KB), introducing first-class `ResetScope` (`Ecosystem` vs `Factory`) with dual-stage physical button hold (5s / 10s), pure domain hold classification, and transparent auto-migration.

**Architecture:** A composite `PartitionedKvBlobStore<M, W>` wraps two `SeqMapKvBlobStore` instances in `intercom-firmware`, routing Matter's `NETWORKS_KEY` (`Blob 258`) to `wifi_kvs` and all other keys to `matter_kvs` with auto-migration. Core domain models `ResetScope` (`Ecosystem`, `Factory`) and pure `classify_reset_hold` in `intercom-core`, driven by `ResetTriggerDriver` and enacted by `SystemResetDriver`. The physical BOOT button in `intercom-adapter-gpio` delegates timing classification to core and triggers `ResetScope::Ecosystem` on 5s–10s release and `ResetScope::Factory` on $\ge$ 10s hold.

**Tech Stack:** Rust (`no_std`), `rs-matter`, `rs-matter-embassy`, `sequential-storage`, `esp-hal` (SPI Flash, GPIO), ESP32-C6.

## Global Constraints

- Use `mise exec -- cargo ...` for all cargo commands.
- Run host unit tests with `mise exec -- cargo test -p intercom-core -p intercom-firmware --target x86_64-apple-darwin`.
- Build embedded firmware with `mise exec -- cargo build -p intercom-firmware --release`.
- All tests MUST follow the **Arrange → Act → Assert** pattern with explicit `// Arrange`, `// Act`, and `// Assert` comments.
- Within any file, **public items MUST appear above private items**.
- High-level abstractions MUST be placed above low-level details.
- Zero references to "Matter" or "HomeAssistant" in `crates/intercom-core/src/`.
- No placeholders (no `TODO`, `TBD`, or unimplemented functions).

---

## File Structure

- **Modified:** `partitions.csv`
  - Defines the 8 KB `wifi_kvs` partition at offset `0x6c6000`.
- **Modified:** `crates/intercom-core/src/domain/mod.rs`
  - Exports `pub mod reset;`.
- **Create:** `crates/intercom-core/src/domain/reset.rs`
  - Defines `ResetScope` enum (`Ecosystem`, `Factory`) and pure `classify_reset_hold`.
  - Comprehensive unit tests for hold classification.
- **Modified:** `crates/intercom-core/src/ports/reset.rs`
  - Re-exports `ResetScope` and defines `SystemResetDriver::erase_credentials(&self, scope: ResetScope)`.
- **Modified:** `crates/intercom-core/src/ports/mod.rs`
  - Defines `ResetTriggerDriver` replacing `FactoryResetDriver`.
- **Modified:** `crates/intercom-core/src/service/reset.rs`
  - Renames `FactoryResetService` to `ResetService`, accepting `ResetScope`.
- **Modified:** `crates/intercom-core/src/app.rs`
  - Wires `ResetTriggerDriver` into `App`.
- **Modified:** `crates/intercom-core/tests/service_tests.rs`
  - Host unit tests verifying `ResetService` with `ResetScope::Ecosystem` and `ResetScope::Factory`.
- **Modified:** `crates/intercom-adapter-gpio/src/config.rs`
  - Defines `ECOSYSTEM_RESET_HOLD_MS` (5s) and `FACTORY_RESET_HOLD_MS` (10s).
- **Modified:** `crates/intercom-adapter-gpio/src/boot_button.rs`
  - Implements `ResetTriggerDriver` calling `classify_reset_hold`.
- **Modified:** `crates/intercom-firmware/src/config.rs`
  - Defines `WIFI_KVS_OFFSET`, `WIFI_KVS_SIZE`, and `WIFI_KVS_RANGE`.
  - Unit tests verifying sector alignment, non-overlapping partitions, and 8 MB boundaries.
- **Modified:** `crates/intercom-firmware/src/reset.rs`
  - Defines `PartitionedKvBlobStore<M, W>` implementing `KvBlobStore` with routing and auto-migration.
  - Implements scoped flash erasure in `FirmwareResetDriver`.
- **Create:** `crates/intercom-firmware/tests/partitioned_kv_tests.rs`
  - 8 host unit tests verifying `PartitionedKvBlobStore` routing, migration, idempotency, updates, and isolation.
- **Modified:** `crates/intercom-firmware/src/main.rs`
  - Wires `PartitionedKvBlobStore` and `ResetTriggerDriver`.

---

## Task Decomposition

### Task 1: Core Domain Reset Model, Timing Classifier & Service (`intercom-core`)

**Files:**
- Create: `crates/intercom-core/src/domain/reset.rs`
- Modify: `crates/intercom-core/src/domain/mod.rs`
- Modify: `crates/intercom-core/src/ports/reset.rs`
- Modify: `crates/intercom-core/src/ports/mod.rs`
- Modify: `crates/intercom-core/src/service/reset.rs`
- Modify: `crates/intercom-core/src/service/mod.rs`
- Modify: `crates/intercom-core/src/app.rs`
- Modify: `crates/intercom-core/tests/service_tests.rs`

**Interfaces:**
- Produces: `ResetScope` (`Ecosystem`, `Factory`).
- Produces: `classify_reset_hold(elapsed, released, ecosystem_hold, factory_hold) -> Option<ResetScope>`.
- Produces: `SystemResetDriver::erase_credentials(&self, scope: ResetScope)`.
- Produces: `ResetTriggerDriver::wait_for_reset(&mut self) -> ResetScope`.
- Produces: `ResetService<T, R>`.

- [ ] **Step 1: Write tests for `classify_reset_hold` in `crates/intercom-core/src/domain/reset.rs`**

```rust
//! System reset domain logic and hold duration classification.

use embassy_time::Duration;

/// Scope of persistent credentials erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetScope {
    /// Erases only active ecosystem credentials (e.g. Matter fabrics & sessions).
    /// Preserves Wi-Fi credentials so the device stays on the local network.
    Ecosystem,
    /// Full factory reset: erases both Wi-Fi credentials and ecosystem credentials.
    Factory,
}

/// Pure evaluation function classifying button press hold duration into a `ResetScope`.
pub fn classify_reset_hold(
    elapsed: Duration,
    released: bool,
    ecosystem_hold: Duration,
    factory_hold: Duration,
) -> Option<ResetScope> {
    if elapsed >= factory_hold {
        Some(ResetScope::Factory)
    } else if released && elapsed >= ecosystem_hold {
        Some(ResetScope::Ecosystem)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECOSYSTEM_HOLD: Duration = Duration::from_millis(5000);
    const FACTORY_HOLD: Duration = Duration::from_millis(10000);

    #[test]
    fn test_released_before_ecosystem_threshold_is_ignored() {
        // Arrange
        let elapsed = Duration::from_millis(4900);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, None);
    }

    #[test]
    fn test_held_at_ecosystem_threshold_without_release_waits() {
        // Arrange - user is holding past 5s, might be holding for 10s factory reset
        let elapsed = Duration::from_millis(6000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, None);
    }

    #[test]
    fn test_released_between_ecosystem_and_factory_triggers_ecosystem_reset() {
        // Arrange
        let elapsed = Duration::from_millis(7500);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Ecosystem));
    }

    #[test]
    fn test_released_just_under_factory_threshold_triggers_ecosystem_reset() {
        // Arrange
        let elapsed = Duration::from_millis(9990);
        let released = true;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Ecosystem));
    }

    #[test]
    fn test_held_at_or_above_factory_threshold_triggers_factory_reset_immediately() {
        // Arrange - button reaches 10s without waiting for release
        let elapsed = Duration::from_millis(10000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Factory));
    }

    #[test]
    fn test_held_well_beyond_factory_threshold_triggers_factory_reset() {
        // Arrange
        let elapsed = Duration::from_millis(15000);
        let released = false;

        // Act
        let result = classify_reset_hold(elapsed, released, ECOSYSTEM_HOLD, FACTORY_HOLD);

        // Assert
        assert_eq!(result, Some(ResetScope::Factory));
    }
}
```

- [ ] **Step 2: Write tests for `ResetService` in `crates/intercom-core/tests/service_tests.rs`**

```rust
#[test]
fn test_reset_service_erases_ecosystem_scope_and_reboots() {
    // Arrange
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;
    use intercom_core::events::{SystemState, SystemStateWatch};
    use core::sync::atomic::{AtomicBool, Ordering};

    struct MockTrigger { scope: ResetScope }
    impl ResetTriggerDriver for MockTrigger {
        async fn wait_for_reset(&mut self) -> ResetScope { self.scope }
    }

    struct MockReset {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
        reboot_called: AtomicBool,
    }
    impl SystemResetDriver for MockReset {
        fn erase_credentials(&self, scope: ResetScope) { self.erased_scope.set(Some(scope)); }
        fn reboot(&self) -> ! {
            self.reboot_called.store(true, Ordering::SeqCst);
            panic!("Mock reboot");
        }
    }

    let trigger = MockTrigger { scope: ResetScope::Ecosystem };
    let reset = MockReset {
        erased_scope: core::cell::Cell::new(None),
        reboot_called: AtomicBool::new(false),
    };
    let mut service = ResetService::new(trigger, reset);
    let state_watch = SystemStateWatch::new();
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);

    // Act
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        futures_lite::future::block_on(service.run(state_sender));
    }));

    // Assert
    assert_eq!(service.reset.erased_scope.get(), Some(ResetScope::Ecosystem));
    assert_eq!(state_watch.get(), SystemState::FactoryReset);
}

#[test]
fn test_reset_service_erases_factory_scope_and_reboots() {
    // Arrange
    use intercom_core::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};
    use intercom_core::service::ResetService;
    use intercom_core::events::{SystemState, SystemStateWatch};

    struct MockTrigger { scope: ResetScope }
    impl ResetTriggerDriver for MockTrigger {
        async fn wait_for_reset(&mut self) -> ResetScope { self.scope }
    }

    struct MockReset {
        erased_scope: core::cell::Cell<Option<ResetScope>>,
    }
    impl SystemResetDriver for MockReset {
        fn erase_credentials(&self, scope: ResetScope) { self.erased_scope.set(Some(scope)); }
        fn reboot(&self) -> ! { panic!("Mock reboot"); }
    }

    let trigger = MockTrigger { scope: ResetScope::Factory };
    let reset = MockReset { erased_scope: core::cell::Cell::new(None) };
    let mut service = ResetService::new(trigger, reset);
    let state_watch = SystemStateWatch::new();
    let state_sender = intercom_core::events::SystemStateSender::new(&state_watch);

    // Act
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        futures_lite::future::block_on(service.run(state_sender));
    }));

    // Assert
    assert_eq!(service.reset.erased_scope.get(), Some(ResetScope::Factory));
    assert_eq!(state_watch.get(), SystemState::FactoryReset);
}
```

- [ ] **Step 3: Update `crates/intercom-core/src/ports/reset.rs` & `ports/mod.rs`**

In `crates/intercom-core/src/ports/reset.rs`:
```rust
//! System reset domain port and scope definitions.

pub use crate::domain::reset::ResetScope;

/// Outbound port for performing system credential erasure and rebooting the hardware.
pub trait SystemResetDriver {
    /// Erases persistent credentials according to the specified scope.
    fn erase_credentials(&self, scope: ResetScope);

    /// Reboots the hardware controller.
    fn reboot(&self) -> !;
}
```

In `crates/intercom-core/src/ports/mod.rs`:
```rust
pub use reset::{ResetScope, SystemResetDriver};

/// Reset detection port.
///
/// Implementors detect reset trigger conditions and determine the requested scope.
pub trait ResetTriggerDriver {
    /// Block until a reset trigger condition is met, returning the requested scope.
    async fn wait_for_reset(&mut self) -> ResetScope;
}
```

- [ ] **Step 4: Update `crates/intercom-core/src/service/reset.rs` and `app.rs`**

In `crates/intercom-core/src/service/reset.rs`:
```rust
//! Smart Intercom - System Reset Service

use embassy_time::{Duration, Timer};
use log::info;

use crate::config::FACTORY_RESET_REBOOT_DELAY_MS;
use crate::events::SystemStateSender;
use crate::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};

/// System reset lifecycle service.
pub struct ResetService<T: ResetTriggerDriver, R: SystemResetDriver> {
    pub driver: T,
    pub reset: R,
}

impl<T: ResetTriggerDriver, R: SystemResetDriver> ResetService<T, R> {
    pub fn new(driver: T, reset: R) -> Self {
        Self { driver, reset }
    }

    pub async fn run(&mut self, state_sender: SystemStateSender<'_>) {
        info!("monitoring for reset trigger");

        let scope = self.driver.wait_for_reset().await;

        match scope {
            ResetScope::Ecosystem => info!("=== ECOSYSTEM RESET TRIGGERED ==="),
            ResetScope::Factory => info!("=== FACTORY RESET TRIGGERED ==="),
        }

        state_sender.factory_reset();
        self.reset.erase_credentials(scope);

        Timer::after(Duration::from_millis(FACTORY_RESET_REBOOT_DELAY_MS)).await;

        info!("Rebooting...");
        self.reset.reboot();
    }
}
```

In `crates/intercom-core/src/app.rs`:
Replace `FactoryResetDriver` with `ResetTriggerDriver`, and `FactoryResetService` with `ResetService`.

- [ ] **Step 5: Run host tests to verify all core tests pass**

Run: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS (all tests pass)

- [ ] **Step 6: Commit**

```bash
git add crates/intercom-core/
git commit -m "feat(core): introduce ResetScope, classify_reset_hold, and ResetService"
```

---

### Task 2: Dual-Stage BOOT Button Driver (`intercom-adapter-gpio`)

**Files:**
- Modify: `crates/intercom-adapter-gpio/src/config.rs`
- Modify: `crates/intercom-adapter-gpio/src/boot_button.rs`

**Interfaces:**
- Produces: `GpioBootButtonDriver` implementing `ResetTriggerDriver`.
- Uses: `intercom_core::domain::reset::classify_reset_hold`.

- [ ] **Step 1: Update timing constants in `crates/intercom-adapter-gpio/src/config.rs`**

```rust
/// How long the BOOT button must be held to trigger ecosystem reset (unpair Matter, keep Wi-Fi).
pub const ECOSYSTEM_RESET_HOLD_MS: u64 = 5000;
/// How long the BOOT button must be held to trigger full factory reset (erase Wi-Fi + Matter).
pub const FACTORY_RESET_HOLD_MS: u64 = 10000;
```

- [ ] **Step 2: Update `GpioBootButtonDriver` in `crates/intercom-adapter-gpio/src/boot_button.rs`**

```rust
//! Smart Intercom - BOOT Button Reset Driver
//!
//! Monitors GPIO9 (BOOT button) for dual-stage long press:
//! - Held 5s to 10s and released: Ecosystem Reset (unpairs active ecosystem, preserves Wi-Fi).
//! - Held >= 10s: Full Factory Reset (erases both Wi-Fi and ecosystem credentials).

use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Input, InputConfig, Pull};
use crate::config;
use intercom_core::domain::reset::classify_reset_hold;
use intercom_core::ports::{ResetScope, ResetTriggerDriver};
use log::info;

/// GPIO-based BOOT button driver for system reset.
pub struct GpioBootButtonDriver {
    config: GpioBootButtonConfig,
}

impl GpioBootButtonDriver {
    pub fn new(config: GpioBootButtonConfig) -> Self {
        info!(
            "BOOT button reset driver: ecosystem_hold={}ms, factory_hold={}ms",
            config.ecosystem_hold_ms, config.factory_hold_ms
        );
        Self { config }
    }
}

/// Configuration for the BOOT button reset driver.
pub struct GpioBootButtonConfig {
    /// Input pin (BOOT button, active LOW with pull-up)
    pub pin: Input<'static>,
    /// How long the button must be held to trigger ecosystem reset (ms)
    pub ecosystem_hold_ms: u64,
    /// How long the button must be held to trigger factory reset (ms)
    pub factory_hold_ms: u64,
}

impl GpioBootButtonConfig {
    pub fn new(pin: impl esp_hal::gpio::InputPin + 'static) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            ecosystem_hold_ms: config::ECOSYSTEM_RESET_HOLD_MS,
            factory_hold_ms: config::FACTORY_RESET_HOLD_MS,
        }
    }
}

impl ResetTriggerDriver for GpioBootButtonDriver {
    async fn wait_for_reset(&mut self) -> ResetScope {
        let ecosystem_duration = Duration::from_millis(self.config.ecosystem_hold_ms);
        let factory_duration = Duration::from_millis(self.config.factory_hold_ms);

        loop {
            self.config.pin.wait_for_falling_edge().await;

            let press_start = Instant::now();
            info!(
                "BOOT button pressed — hold {}s for ecosystem reset, {}s for full factory reset",
                self.config.ecosystem_hold_ms / 1000,
                self.config.factory_hold_ms / 1000
            );

            let scope = loop {
                Timer::after(Duration::from_millis(100)).await;

                let elapsed = Instant::now().duration_since(press_start);
                let released = self.config.pin.is_high();

                if let Some(scope) = classify_reset_hold(elapsed, released, ecosystem_duration, factory_duration) {
                    break Some(scope);
                }

                if released {
                    info!("BOOT button released early (ignored)");
                    break None;
                }
            };

            if let Some(triggered_scope) = scope {
                return triggered_scope;
            }
        }
    }
}
```

- [ ] **Step 3: Run target check on `intercom-adapter-gpio`**

Run: `mise exec -- cargo check -p intercom-adapter-gpio`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/intercom-adapter-gpio/
git commit -m "feat(gpio): wire classify_reset_hold into GpioBootButtonDriver"
```

---

### Task 3: Partitioned Key-Value Blob Store & Auto-Migration (`intercom-firmware`)

**Files:**
- Create: `crates/intercom-firmware/tests/partitioned_kv_tests.rs`
- Modify: `crates/intercom-firmware/src/reset.rs`
- Modify: `crates/intercom-firmware/src/lib.rs`

**Interfaces:**
- Produces: `PartitionedKvBlobStore<M, W>` implementing `rs_matter::persist::KvBlobStore`.
- Consumes: `NETWORKS_KEY` (`258`) from `rs_matter::persist`.

- [ ] **Step 1: Write 8 comprehensive unit tests in `tests/partitioned_kv_tests.rs`**

Create `crates/intercom-firmware/tests/partitioned_kv_tests.rs`:
```rust
use intercom_firmware::reset::PartitionedKvBlobStore;
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, BASIC_INFO_KEY, NETWORKS_KEY};

// Mock in-memory KvBlobStore for testing
struct MockKvStore {
    items: [(u16, Option<[u8; 64]>, usize); 8],
}

impl MockKvStore {
    fn new() -> Self {
        Self { items: [(0, None, 0); 8] }
    }
}

impl KvBlobStore for MockKvStore {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        for (k, data, len) in &self.items {
            if *k == key {
                if let Some(bytes) = data {
                    buf[..*len].copy_from_slice(&bytes[..*len]);
                    return Ok(Some(&buf[..*len]));
                }
            }
        }
        Ok(None)
    }

    fn store(&mut self, key: u16, data: &[u8], _buf: &mut [u8]) -> Result<(), Error> {
        for (k, stored, len) in &mut self.items {
            if *k == key || stored.is_none() {
                *k = key;
                let mut arr = [0u8; 64];
                arr[..data.len()].copy_from_slice(data);
                *stored = Some(arr);
                *len = data.len();
                return Ok(());
            }
        }
        panic!("Mock store full");
    }

    fn remove(&mut self, key: u16, _buf: &mut [u8]) -> Result<(), Error> {
        for (k, stored, len) in &mut self.items {
            if *k == key {
                *stored = None;
                *len = 0;
            }
        }
        Ok(())
    }
}

#[test]
fn test_partitioned_kv_store_routes_networks_key_to_wifi_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned.store(NETWORKS_KEY, b"ssid_secret_pass", &mut buf).unwrap();

    // Assert
    let mut read_buf = [0u8; 128];
    assert_eq!(
        partitioned.wifi_store.load(NETWORKS_KEY, &mut read_buf).unwrap(),
        Some(b"ssid_secret_pass".as_slice())
    );
    assert_eq!(
        partitioned.matter_store.load(NETWORKS_KEY, &mut read_buf).unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_routes_fabric_keys_to_matter_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned.store(BASIC_INFO_KEY, b"basic_info_data", &mut buf).unwrap();
    partitioned.store(1, b"fabric_1_credential", &mut buf).unwrap();

    // Assert
    let mut read_buf = [0u8; 128];
    assert_eq!(
        partitioned.matter_store.load(BASIC_INFO_KEY, &mut read_buf).unwrap(),
        Some(b"basic_info_data".as_slice())
    );
    assert_eq!(
        partitioned.matter_store.load(1, &mut read_buf).unwrap(),
        Some(b"fabric_1_credential".as_slice())
    );
    assert_eq!(
        partitioned.wifi_store.load(BASIC_INFO_KEY, &mut read_buf).unwrap(),
        None
    );
    assert_eq!(
        partitioned.wifi_store.load(1, &mut read_buf).unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_empty_returns_none() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act & Assert
    assert_eq!(partitioned.load(NETWORKS_KEY, &mut buf).unwrap(), None);
    assert_eq!(partitioned.load(BASIC_INFO_KEY, &mut buf).unwrap(), None);
}

#[test]
fn test_partitioned_kv_store_auto_migrates_legacy_networks_key() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store.store(NETWORKS_KEY, b"migrated_wifi_creds", &mut setup_buf).unwrap();

    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut read_buf = [0u8; 128];

    // Act
    let loaded = partitioned.load(NETWORKS_KEY, &mut read_buf).unwrap();

    // Assert
    assert_eq!(loaded, Some(b"migrated_wifi_creds".as_slice()));
    let mut check_buf = [0u8; 128];
    assert_eq!(
        partitioned.wifi_store.load(NETWORKS_KEY, &mut check_buf).unwrap(),
        Some(b"migrated_wifi_creds".as_slice())
    );
    assert_eq!(
        partitioned.matter_store.load(NETWORKS_KEY, &mut check_buf).unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_migration_only_happens_once() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store.store(NETWORKS_KEY, b"first_creds", &mut setup_buf).unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act - first load migrates
    let first = partitioned.load(NETWORKS_KEY, &mut buf).unwrap();
    // Overwrite in wifi_store
    partitioned.store(NETWORKS_KEY, b"updated_creds", &mut buf).unwrap();
    let second = partitioned.load(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(first, Some(b"first_creds".as_slice()));
    assert_eq!(second, Some(b"updated_creds".as_slice()));
}

#[test]
fn test_partitioned_kv_store_remove_networks_key_removes_from_both() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let mut wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store.store(NETWORKS_KEY, b"matter_copy", &mut setup_buf).unwrap();
    wifi_store.store(NETWORKS_KEY, b"wifi_copy", &mut setup_buf).unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned.remove(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(partitioned.wifi_store.load(NETWORKS_KEY, &mut buf).unwrap(), None);
    assert_eq!(partitioned.matter_store.load(NETWORKS_KEY, &mut buf).unwrap(), None);
}

#[test]
fn test_partitioned_kv_store_remove_matter_key_only_touches_matter_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];
    partitioned.store(NETWORKS_KEY, b"wifi_safe", &mut buf).unwrap();
    partitioned.store(BASIC_INFO_KEY, b"info_data", &mut buf).unwrap();

    // Act
    partitioned.remove(BASIC_INFO_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(partitioned.load(BASIC_INFO_KEY, &mut buf).unwrap(), None);
    assert_eq!(partitioned.load(NETWORKS_KEY, &mut buf).unwrap(), Some(b"wifi_safe".as_slice()));
}

#[test]
fn test_partitioned_kv_store_update_existing_networks_key() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];
    partitioned.store(NETWORKS_KEY, b"old_ssid_pass", &mut buf).unwrap();

    // Act
    partitioned.store(NETWORKS_KEY, b"new_ssid_pass", &mut buf).unwrap();

    // Assert
    assert_eq!(partitioned.load(NETWORKS_KEY, &mut buf).unwrap(), Some(b"new_ssid_pass".as_slice()));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin`
Expected: FAIL with `PartitionedKvBlobStore` not found.

- [ ] **Step 3: Implement `PartitionedKvBlobStore` in `crates/intercom-firmware/src/reset.rs`**

```rust
use log::{info, warn};
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, NETWORKS_KEY};

/// A composite `KvBlobStore` that routes Wi-Fi credentials (`NETWORKS_KEY`) to a dedicated
/// storage partition and all other Matter fabrics/credentials to the Matter KVS partition.
///
/// Automatically migrates legacy credentials from `matter_store` to `wifi_store` on first load.
pub struct PartitionedKvBlobStore<M, W> {
    pub matter_store: M,
    pub wifi_store: W,
}

impl<M, W> PartitionedKvBlobStore<M, W>
where
    M: KvBlobStore,
    W: KvBlobStore,
{
    pub const fn new(matter_store: M, wifi_store: W) -> Self {
        Self {
            matter_store,
            wifi_store,
        }
    }
}

impl<M, W> KvBlobStore for PartitionedKvBlobStore<M, W>
where
    M: KvBlobStore,
    W: KvBlobStore,
{
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        if key == NETWORKS_KEY {
            if let Some(data) = self.wifi_store.load(key, buf)? {
                return Ok(Some(data));
            }

            if let Some(legacy_data) = self.matter_store.load(key, buf)? {
                info!("Migrating Wi-Fi credentials from matter_kvs to dedicated wifi_kvs partition");
                let mut temp_copy = [0u8; 128];
                let len = legacy_data.len().min(temp_copy.len());
                temp_copy[..len].copy_from_slice(&legacy_data[..len]);

                let mut store_buf = [0u8; 128];
                if let Err(e) = self.wifi_store.store(key, &temp_copy[..len], &mut store_buf) {
                    warn!("Failed to store migrated Wi-Fi credentials: {:?}", e);
                } else {
                    let mut remove_buf = [0u8; 64];
                    let _ = self.matter_store.remove(key, &mut remove_buf);
                }

                return self.wifi_store.load(key, buf);
            }

            Ok(None)
        } else {
            self.matter_store.load(key, buf)
        }
    }

    fn store(&mut self, key: u16, data: &[u8], buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            self.wifi_store.store(key, data, buf)
        } else {
            self.matter_store.store(key, data, buf)
        }
    }

    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            let res = self.wifi_store.remove(key, buf);
            let _ = self.matter_store.remove(key, buf);
            res
        } else {
            self.matter_store.remove(key, buf)
        }
    }
}
```

Ensure `pub mod reset;` is exported in `crates/intercom-firmware/src/lib.rs`.

- [ ] **Step 4: Run tests to verify pass**

Run: `mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin`
Expected: PASS (all 8 tests pass)

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-firmware/
git commit -m "feat(firmware): implement PartitionedKvBlobStore with auto-migration and full test suite"
```

---

### Task 4: Partition Table, Hardware Config & Reset Wiring (`partitions.csv` & `intercom-firmware`)

**Files:**
- Modify: `partitions.csv`
- Modify: `crates/intercom-firmware/src/config.rs`
- Modify: `crates/intercom-firmware/src/reset.rs`
- Modify: `crates/intercom-firmware/src/main.rs`

**Interfaces:**
- Produces: `WIFI_KVS_RANGE` in `config.rs`.
- Updates: `FirmwareResetDriver::erase_credentials(&self, scope: ResetScope)`.
- Updates: `create_kv_blob_store(matter_range, wifi_range)` in `reset.rs`.

- [ ] **Step 1: Update `partitions.csv`**

Append `wifi_kvs` partition to `partitions.csv`:
```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
matter_kvs,   data, nvs,     0x6c0000, 0x6000,
wifi_kvs,     data, nvs,     0x6c6000, 0x2000,
```

- [ ] **Step 2: Update partition constants & unit tests in `crates/intercom-firmware/src/config.rs`**

Add partition definitions:
```rust
/// Start offset of the `matter_kvs` partition in flash (from partitions.csv).
pub const MATTER_KVS_OFFSET: u32 = 0x6c0000;
/// Size of the `matter_kvs` partition (24 KiB = 6 sectors × 4 KiB).
pub const MATTER_KVS_SIZE: u32 = 0x6000;
/// Flash range for the Matter KV blob store.
pub const MATTER_KVS_RANGE: Range<u32> =
    MATTER_KVS_OFFSET..MATTER_KVS_OFFSET + MATTER_KVS_SIZE;

/// Start offset of the `wifi_kvs` partition in flash (from partitions.csv).
pub const WIFI_KVS_OFFSET: u32 = 0x6c6000;
/// Size of the `wifi_kvs` partition (8 KiB = 2 sectors × 4 KiB).
pub const WIFI_KVS_SIZE: u32 = 0x2000;
/// Flash range for the dedicated Wi-Fi credentials store.
pub const WIFI_KVS_RANGE: Range<u32> =
    WIFI_KVS_OFFSET..WIFI_KVS_OFFSET + WIFI_KVS_SIZE;
```

Add layout validation unit tests to `crates/intercom-firmware/src/config.rs::tests`:
```rust
#[test]
fn test_flash_partitions_are_sector_aligned_and_non_overlapping() {
    // Arrange
    const SECTOR_SIZE: u32 = 4096;
    const FLASH_TOTAL_SIZE: u32 = 0x800000; // 8 MB

    // Act & Assert - Sector alignment (4 KiB)
    assert_eq!(MATTER_KVS_OFFSET % SECTOR_SIZE, 0);
    assert_eq!(MATTER_KVS_SIZE % SECTOR_SIZE, 0);
    assert_eq!(WIFI_KVS_OFFSET % SECTOR_SIZE, 0);
    assert_eq!(WIFI_KVS_SIZE % SECTOR_SIZE, 0);

    // Act & Assert - Non-overlapping and sequential
    assert_eq!(MATTER_KVS_OFFSET + MATTER_KVS_SIZE, WIFI_KVS_OFFSET);
    assert!(MATTER_KVS_RANGE.end <= WIFI_KVS_RANGE.start);

    // Act & Assert - Fits within 8 MB flash boundary
    assert!(WIFI_KVS_RANGE.end <= FLASH_TOTAL_SIZE);
}
```

- [ ] **Step 3: Update `FirmwareResetDriver` and `create_kv_blob_store` in `reset.rs`**

```rust
use intercom_core::ports::{ResetScope, SystemResetDriver};

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self, scope: ResetScope) {
        #[cfg(target_arch = "riscv32")]
        {
            #[cfg(not(feature = "dummy-persist"))]
            match scope {
                ResetScope::Ecosystem => {
                    erase_flash_range(&crate::config::MATTER_KVS_RANGE, "Matter KVS (ecosystem only)");
                }
                ResetScope::Factory => {
                    erase_flash_range(&crate::config::MATTER_KVS_RANGE, "Matter KVS");
                    erase_flash_range(&crate::config::WIFI_KVS_RANGE, "Wi-Fi KVS");
                }
            }
            #[cfg(feature = "dummy-persist")]
            info!("Reset {:?} skipped (dummy-persist)", scope);
        }
        #[cfg(not(target_arch = "riscv32"))]
        info!("Reset {:?} simulated on host", scope);
    }

    fn reboot(&self) -> ! {
        #[cfg(target_arch = "riscv32")]
        esp_hal::system::software_reset();
        #[cfg(not(target_arch = "riscv32"))]
        panic!("Host reboot requested");
    }
}

/// Helper function to erase all 4 KiB sectors in a given flash range.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn erase_flash_range(range: &core::ops::Range<u32>, label: &str) {
    const SECTOR_SIZE: u32 = 4096;

    info!(
        "Erasing {}: 0x{:x}..0x{:x} ({} sectors)",
        label,
        range.start,
        range.end,
        (range.end - range.start) / SECTOR_SIZE
    );

    let ret = unsafe { spiflash::esp_rom_spiflash_unlock() };
    if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
        warn!("Flash unlock failed for {}: {}", label, ret);
        return;
    }

    let mut addr = range.start;
    while addr < range.end {
        let sector = addr / SECTOR_SIZE;
        let ret = unsafe { spiflash::esp_rom_spiflash_erase_sector(sector) };
        if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash erase sector {} for {} failed: {}", sector, label, ret);
        }
        addr += SECTOR_SIZE;
    }

    info!("{} erased successfully", label);
}

/// Create the partitioned persistence layer spanning Matter KVS and Wi-Fi KVS.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn create_kv_blob_store(
    matter_range: core::ops::Range<u32>,
    wifi_range: core::ops::Range<u32>,
) -> PartitionedKvBlobStore<SeqMapKvBlobStore<EspFlash>, SeqMapKvBlobStore<EspFlash>> {
    let matter_flash = EspFlash::new(matter_range.clone());
    let wifi_flash = EspFlash::new(wifi_range.clone());
    PartitionedKvBlobStore::new(
        SeqMapKvBlobStore::new(matter_flash, matter_range),
        SeqMapKvBlobStore::new(wifi_flash, wifi_range),
    )
}
```

- [ ] **Step 4: Update `main.rs` wiring**

In `crates/intercom-firmware/src/main.rs`:
```rust
#[cfg(not(feature = "dummy-persist"))]
store: reset::create_kv_blob_store(config::MATTER_KVS_RANGE, config::WIFI_KVS_RANGE),
#[cfg(feature = "dummy-persist")]
store: DummyKvBlobStore,
```

- [ ] **Step 5: Verify build & tests**

Run host tests:
`mise exec -- cargo test -p intercom-core -p intercom-adapter-ota -p intercom-adapter-http -p intercom-firmware --target x86_64-apple-darwin`
Expected: PASS (all tests pass)

Build release firmware:
`mise exec -- cargo build -p intercom-firmware --release`
Expected: PASS (clean compile without errors)

- [ ] **Step 6: Commit**

```bash
git add partitions.csv crates/intercom-firmware/
git commit -m "feat(firmware): wire partitioned Wi-Fi flash storage and scoped reset erasure"
```
