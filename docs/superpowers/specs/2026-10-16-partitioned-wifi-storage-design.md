# Partitioned Wi-Fi Storage Architecture Specification

- **Date:** 2026-10-16
- **Status:** Approved / Ready for Implementation
- **Target:** `partitions.csv`, `intercom-core`, `intercom-adapter-gpio`, `intercom-firmware`

---

## 1. Executive Summary

This specification defines the decoupling of Wi-Fi configuration and network credentials from the Matter Key-Value Store (`matter_kvs`) into a dedicated, ecosystem-neutral flash partition (`wifi_kvs`), and introduces a first-class, dual-stage physical reset mechanism.

### Motivation & Goals
1. **Cross-Ecosystem Independence:** Wi-Fi credentials are fundamentally a transport-level network property, not a Matter-specific property. Separating Wi-Fi credentials from Matter fabrics allows alternative or future ecosystem adapters (such as Home Assistant REST/MQTT, ESPHome, etc.) to reuse the active network configuration without initializing or coupling to the Matter stack.
2. **Dual-Stage Physical Reset Scopes:** Introduces a strongly-typed `ResetScope` (`Ecosystem` vs `Factory`) triggered via dual-stage hold durations on the physical BOOT button:
   - **5s hold:** Resets/unpairs the active ecosystem (clearing Matter fabrics/sessions) while preserving Wi-Fi credentials so the device stays on the LAN.
   - **10s hold:** Full factory reset, erasing both Wi-Fi credentials and ecosystem credentials.
3. **Seamless Zero-Downtime Migration:** Existing deployed devices with Wi-Fi credentials stored in `matter_kvs` will automatically and transparently migrate their Wi-Fi credentials to `wifi_kvs` on first boot without user intervention.
4. **Clean First-Class Interfaces:** No legacy backwards-compatibility shims or fallback methods. Hexagonal ports directly express `ResetScope`.

---

## 2. Flash Memory Layout & Partition Table

The ESP32-C6-WROOM-1-N8 features 8 MB (8,388,608 bytes) of Quad SPI flash. Currently, flash usage ends at `0x6c6000`, leaving over 1.2 MB of unallocated space.

`sequential-storage` (used by `SeqMapKvBlobStore`) requires a minimum of **2 erase sectors** (2 × 4096 bytes = 8192 bytes = `0x2000`) to guarantee safe wear-leveling and atomic sector compaction.

### 2.1 Updated `partitions.csv`

```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
matter_kvs,   data, nvs,     0x6c0000, 0x6000,
wifi_kvs,     data, nvs,     0x6c6000, 0x2000,
```

### 2.2 Memory Map Diagram

```
0x000000 ┌──────────────────────────────────────────────┐
         │ 2nd-stage bootloader (36 KB)                 │
0x009000 ├──────────────────────────────────────────────┤
         │ nvs (24 KB, PHY / RF calibration)            │
0x00f000 ├──────────────────────────────────────────────┤
         │ otadata (8 KB, dual 4 KB sectors)            │
0x020000 ├──────────────────────────────────────────────┤
         │ ota_0 (Slot A, 3.4 MB)                       │
0x370000 ├──────────────────────────────────────────────┤
         │ ota_1 (Slot B, 3.4 MB)                       │
0x6c0000 ├──────────────────────────────────────────────┤
         │ matter_kvs (24 KB, 6 sectors: fabrics/keys)  │
0x6c6000 ├──────────────────────────────────────────────┤
         │ wifi_kvs (8 KB, 2 sectors: Wi-Fi creds)      │  <-- NEW
0x6c8000 ├──────────────────────────────────────────────┤
         │ Unallocated flash (1.216 MB available)       │
0x800000 └──────────────────────────────────────────────┘
```

---

## 3. Core Domain Reset Model (`intercom-core`)

### 3.1 `ResetScope` & `SystemResetDriver` (`crates/intercom-core/src/ports/reset.rs`)

Clean, breaking, first-class enum and trait definition:

```rust
/// Scope of persistent credentials erasure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetScope {
    /// Erases only active ecosystem credentials (e.g. Matter fabrics & sessions).
    /// Preserves Wi-Fi credentials so the device stays on the local network.
    Ecosystem,
    /// Full factory reset: erases both Wi-Fi credentials and ecosystem credentials.
    Factory,
}

/// Outbound port for performing system credential erasure and rebooting hardware.
pub trait SystemResetDriver {
    /// Erases credentials according to the specified scope.
    fn erase_credentials(&self, scope: ResetScope);

    /// Reboots the hardware controller.
    fn reboot(&self) -> !;
}
```

### 3.2 Inbound Reset Trigger Port (`crates/intercom-core/src/ports/mod.rs`)

Replaces the single-purpose `FactoryResetDriver`:

```rust
/// Reset trigger port.
///
/// Implementors detect reset trigger conditions and determine the requested scope.
pub trait ResetTriggerDriver {
    /// Block until a reset trigger condition is met, returning the requested scope.
    async fn wait_for_reset(&mut self) -> ResetScope;
}
```

### 3.3 Domain Service (`crates/intercom-core/src/service/reset.rs`)

`ResetService` coordinates detection, state transition, erasure, and reboot:

```rust
pub struct ResetService<T: ResetTriggerDriver, R: SystemResetDriver> {
    driver: T,
    reset: R,
}

impl<T: ResetTriggerDriver, R: SystemResetDriver> ResetService<T, R> {
    pub fn new(driver: T, reset: R) -> Self {
        Self { driver, reset }
    }

    pub async fn run(&mut self, state_sender: SystemStateSender<'_>) {
        let scope = self.driver.wait_for_reset().await;

        state_sender.factory_reset();
        self.reset.erase_credentials(scope);

        Timer::after(Duration::from_millis(FACTORY_RESET_REBOOT_DELAY_MS)).await;
        self.reset.reboot();
    }
}
```

---

## 4. Physical BOOT Button Driver (`intercom-adapter-gpio`)

In `crates/intercom-adapter-gpio/src/boot_button.rs`:

### 4.1 Timing Constants
- **`ECOSYSTEM_RESET_HOLD_MS: u64 = 5_000`** (5 seconds)
- **`FACTORY_RESET_HOLD_MS: u64 = 10_000`** (10 seconds)

### 4.2 Behavior & Detection Loop
When the BOOT button (GPIO 9, active low) is pressed:
1. **Released < 5s:** Ignored as accidental press or normal button interaction.
2. **Released between 5s and 10s:** Returns **`ResetScope::Ecosystem`**. The device unpairs from the ecosystem (clearing `matter_kvs`) while retaining its Wi-Fi network connection (`wifi_kvs`).
3. **Held $\ge$ 10s:** Returns **`ResetScope::Factory`**. The device triggers immediately upon reaching 10s (or release) and performs a full factory reset (clearing both `matter_kvs` and `wifi_kvs`).

---

## 5. Routed Key-Value Blob Store (`PartitionedKvBlobStore`)

### 5.1 Key-Routing Strategy

In `rs-matter`, all persistent data flows through the `KvBlobStore` trait:
```rust
pub trait KvBlobStore {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error>;
    fn store(&mut self, key: u16, data: &[u8], buf: &mut [u8]) -> Result<(), Error>;
    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error>;
}
```

The Matter Core specification isolates wireless network credentials to a single singleton key:
- **`NETWORKS_KEY: u16 = 258`** (`BASIC_INFO_KEY + 2`)

`PartitionedKvBlobStore` implements `KvBlobStore` by composing two distinct `SeqMapKvBlobStore` instances:
1. `matter_store`: bound to `0x6c0000..0x6c6000` (`MATTER_KVS_RANGE`)
2. `wifi_store`: bound to `0x6c6000..0x6c8000` (`WIFI_KVS_RANGE`)

### 5.2 Transparent Auto-Migration Flow

When `load(NETWORKS_KEY)` is invoked:
1. Attempt to load from `wifi_store`.
2. If `Some(data)` is found, return it immediately.
3. If `None` is returned:
   - Attempt to load `NETWORKS_KEY` from `matter_store`.
   - If found in `matter_store`:
     - Store the data into `wifi_store`.
     - Remove `NETWORKS_KEY` from `matter_store`.
     - Log: `info!("Migrating Wi-Fi credentials from matter_kvs to dedicated wifi_kvs partition")`.
     - Return `Some(data)`.
   - If not found in either, return `None`.

When `store(key, data)` is invoked:
- If `key == NETWORKS_KEY`: store in `wifi_store`.
- Otherwise: store in `matter_store`.

When `remove(key)` is invoked:
- If `key == NETWORKS_KEY`: remove from `wifi_store` (and opportunistically from `matter_store`).
- Otherwise: remove from `matter_store`.

---

## 6. Concrete System Reset Driver (`intercom-firmware`)

In `crates/intercom-firmware/src/reset.rs`:
```rust
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
            info!("Reset skipped (dummy-persist)");
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
```
