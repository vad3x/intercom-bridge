# Multi-Ecosystem Architecture & SystemState Lifecycle Specification

- **Date:** 2026-10-16
- **Status:** Draft (Pending Review)
- **Target:** `intercom-core`, `intercom-adapter-matter`, `intercom-adapter-gpio`, `intercom-adapter-http`, `intercom-firmware`

---

## 1. Executive Summary

This specification defines:
1. **First-Class Smart Home Ecosystem Abstraction (Hexagonal Architecture):** Introduces a strongly-typed, protocol-agnostic Ecosystem domain model and outbound port in `intercom-core` without leaking concrete ecosystem names (`Matter`, `HomeAssistant`) into the core domain. The Composition Root (`intercom-firmware`) defines the product's `SupportedEcosystem` enum (`Matter`, `HomeAssistant`) and injects the available ecosystems list, while the active ecosystem adapter (`intercom-adapter-matter`) reports its live pairing status and extensible key-value properties.
2. **`SystemState` Lifecycle Revisit:** Replaces the ambiguous `Initializing` / `Initialized` / `Connecting` / `Connected` / `Ready` progression with explicit states (`Booting`, `Starting`, `Commissioning`, `WifiConnecting`, `EcosystemConnecting`, `Operational`, `Updating`, `FactoryReset`, `Error`) that clearly separate hardware startup, initial pairing, Wi-Fi connectivity, and ecosystem subscription readiness.
3. **Web Dashboard "Ecosystem" Tab:** Adds a dedicated **Ecosystem** navigation tab (`Overview` | `Ecosystem` | `System Logs`) to the embedded web dashboard, displaying multi-ecosystem cards (`Matter` active with live fabric pairing and endpoint subscription indicators; `Home Assistant` planned with a disabled switcher button).

---

## 2. SystemState Lifecycle & Physical LED Patterns

### 2.1 `SystemState` Definition (`crates/intercom-core/src/events.rs`)

```rust
/// System lifecycle states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemState {
    /// Hardware peripherals initializing on boot
    Booting,
    /// Core domain services started, initializing network and ecosystem stack
    Starting,
    /// Device is unpaired/uncommissioned; advertising & awaiting initial pairing
    Commissioning,
    /// Associating with Wi-Fi and acquiring IP lease
    WifiConnecting,
    /// Wi-Fi & LAN operational (HTTP/OTA reachable); awaiting ecosystem controller subscription
    EcosystemConnecting,
    /// All endpoints and ecosystem subscriptions active and operational
    Operational,
    /// Firmware OTA update in progress (flashing inactive partition)
    Updating,
    /// Factory reset triggered (erasing persistent credentials, rebooting)
    FactoryReset,
    /// Fatal hardware or protocol fault
    Error,
}
```

### 2.2 `SystemStateSender` Convenience Methods

`SystemStateSender` exposes typed transition helpers matching the enum variants:
- `booting(&self)`
- `starting(&self)`
- `commissioning(&self)`
- `wifi_connecting(&self)`
- `ecosystem_connecting(&self)`
- `operational(&self)`
- `updating(&self)`
- `factory_reset(&self)`
- `error(&self)`

### 2.3 State Transitions Across Crates

| Source | Condition | Emitted `SystemState` |
|---|---|---|
| `DiagnosticsService::snapshot` | Fallback before first state published | `SystemState::Booting` |
| `App::run` (`intercom-core`) | Core services starting | `SystemState::Starting` |
| `MatterController::run` (`intercom-adapter-matter`) | Startup complete, no active controller fabric (`!has_primary_fabric`) | `SystemState::Commissioning` |
| `MatterController::run` (`intercom-adapter-matter`) | Startup complete, active controller fabric present (`has_primary_fabric`) | `SystemState::WifiConnecting` |
| `ConnectionMonitor::run` (`intercom-adapter-matter`) | `is_operational` transitions `false -> true` (or `is_commissioned` becomes `true` while operational) | `SystemState::Operational` if `readiness.is_ready()`, else `SystemState::EcosystemConnecting` |
| `ConnectionMonitor::run` (`intercom-adapter-matter`) | `is_operational` transitions `true -> false` | Resets `readiness.reset()`, emits `SystemState::WifiConnecting` (if commissioned) or `SystemState::Commissioning` (if uncommissioned) |
| `ClusterReadinessBarrier` (`intercom-adapter-matter`) | Both Door Lock (EP1) and Occupancy (EP2) clusters read/subscribed | `SystemState::Operational` |
| `OtaFlashService` (`intercom-adapter-http`) | OTA stream begins / fails | `SystemState::Updating` on start; `SystemState::Operational` on recoverable failure |
| `FactoryResetService` (`intercom-core`) | Maintenance button hold completed | `SystemState::FactoryReset` |

### 2.4 Physical LED Indicator Patterns (`crates/intercom-adapter-gpio`)

Duty cycle scales progressively with operational readiness, while `Commissioning`, `Updating`, `FactoryReset`, and `Error` use distinct rhythms:

| `SystemState` | Constants (`config.rs`) | Timings | Visual Pattern |
|---|---|---|---|
| `Booting` | `BOOTING_ON_MS` / `BOOTING_OFF_MS` | `200ms ON / 1000ms OFF` | Slow heartbeat (~17% duty) |
| `Starting` | `STARTING_ON_MS` / `STARTING_OFF_MS` | `300ms ON / 700ms OFF` | Faster heartbeat (30% duty) |
| `Commissioning` | `COMMISSIONING_ON_MS` / `COMMISSIONING_OFF_MS` | `250ms ON / 250ms OFF` | Rapid 2 Hz pairing blink (50% duty) |
| `WifiConnecting` | `WIFI_CONNECTING_ON_MS` / `WIFI_CONNECTING_OFF_MS` | `500ms ON / 500ms OFF` | Medium 1 Hz blink (50% duty) |
| `EcosystemConnecting` | `ECOSYSTEM_CONNECTING_ON_MS` / `ECOSYSTEM_CONNECTING_OFF_MS` | `900ms ON / 100ms OFF` | Mostly on, brief wink (90% duty) |
| `Operational` | Solid ON | Continuous ON | Steady solid light (100% duty) |
| `Updating` | `UPDATING_STROBE_MS` | `50ms ON / 50ms OFF` | 10 Hz rapid strobe |
| `FactoryReset` | `FACTORY_RESET_ON_MS` / `FACTORY_RESET_OFF_MS` | `50ms ON / 50ms OFF` | 10 Hz rapid strobe |
| `Error` | `ERROR_ON_MS` / `ERROR_OFF_MS` / `ERROR_BURST_COUNT` / `ERROR_PAUSE_MS` | `100ms ON / 100ms OFF` (2x) + `700ms pause` | Double-flash burst |

---

## 3. Hexagonal Ecosystem Domain & Port Architecture

### 3.1 Core Domain Model (`crates/intercom-core/src/domain/ecosystem.rs`)

`intercom-core` defines strongly-typed, zero-allocation (`no_std`, `Copy`) abstractions with **zero** references to concrete ecosystems (`Matter`, `HomeAssistant`):

```rust
/// Strongly-typed value object identifying a smart home ecosystem (e.g. "Matter").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemId(&'static str);

impl EcosystemId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Trait implemented by composition-root ecosystem enums to map variants to `EcosystemId`.
pub trait EcosystemType: Copy + Eq + core::fmt::Debug + 'static {
    fn id(&self) -> EcosystemId;
}

impl EcosystemType for EcosystemId {
    fn id(&self) -> EcosystemId {
        *self
    }
}

/// Typed, zero-allocation value for per-ecosystem telemetry properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcosystemValue {
    Bool(bool),
    Int(i64),
    Text(&'static str),
}

/// Maximum number of free-form key-value telemetry properties per ecosystem.
pub const MAX_ECOSYSTEM_PROPERTIES: usize = 6;

/// Status and telemetry snapshot of a single active smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
}

impl EcosystemStatus {
    pub const fn new(ecosystem_type: EcosystemId, paired: bool) -> Self {
        Self {
            ecosystem_type,
            paired,
            properties: [None; MAX_ECOSYSTEM_PROPERTIES],
        }
    }

    /// Builder method to append a key-value property into the next available slot.
    pub const fn with_property(mut self, key: &'static str, value: EcosystemValue) -> Self {
        let mut i = 0;
        while i < MAX_ECOSYSTEM_PROPERTIES {
            if self.properties[i].is_none() {
                self.properties[i] = Some((key, value));
                break;
            }
            i += 1;
        }
        self
    }

    /// Iterate over all populated `(key, value)` properties.
    pub fn iter_properties(&self) -> impl Iterator<Item = (&'static str, EcosystemValue)> + '_ {
        self.properties.iter().filter_map(|prop| *prop)
    }
}
```

### 3.2 Outbound Port (`crates/intercom-core/src/ports/ecosystem.rs`)

```rust
use crate::domain::ecosystem::EcosystemStatus;

/// Outbound port providing runtime status for the active smart home ecosystem.
pub trait EcosystemInfoDriver {
    /// Return the current status snapshot of the active ecosystem, or `None` if standalone.
    fn status(&self) -> Option<EcosystemStatus>;
}

impl EcosystemInfoDriver for () {
    fn status(&self) -> Option<EcosystemStatus> {
        None
    }
}
```

### 3.3 System Diagnostics Container (`crates/intercom-core/src/domain/diagnostics.rs` & `service/diagnostics.rs`)

Container-level ecosystem availability (`available_ecosystems`) is separated from the active ecosystem instance (`active_ecosystem`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDiagnostics<T: EcosystemType = EcosystemId> {
    pub state: SystemState,
    pub door_locked: bool,
    pub uptime_sec: u64,
    pub wifi_ssid: Option<WifiSsid>,
    pub wifi_rssi_dbm: Option<i8>,
    pub ip: Option<[u8; 4]>,
    pub mac: [u8; 6],
    pub active_slot: &'static str,
    pub slot_valid: bool,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub available_ecosystems: &'static [T],
    pub active_ecosystem: Option<EcosystemStatus>,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub build_date: &'static str,
}
```

In `DiagnosticsService<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver = (), T: EcosystemType = EcosystemId>`:
- `DiagnosticsService::new(hardware, state_watch)` sets `available_ecosystems: &[]` and `ecosystem: &()` (`active_ecosystem: None`).
- `DiagnosticsService::with_ecosystem(hardware, ecosystem, available_ecosystems, state_watch)` injects the composition root's `&'static [T]` slice and active `E` driver.

---

## 4. Matter Adapter & Composition Root Wiring

### 4.1 Matter Adapter (`crates/intercom-adapter-matter`)

1. **Typed Ecosystem Identifier (`crates/intercom-adapter-matter/src/config.rs`):**
   ```rust
   pub const MATTER_ECOSYSTEM: EcosystemId = EcosystemId::new("Matter");
   ```
2. **Live Status & Subscription Tracking (`crates/intercom-adapter-matter/src/readiness.rs`):**
   `ClusterReadinessBarrier` tracks `paired: AtomicBool` alongside `door_lock_ready: AtomicBool` and `ring_ready: AtomicBool`, and implements `EcosystemInfoDriver`:
   ```rust
   impl EcosystemInfoDriver for ClusterReadinessBarrier {
       fn status(&self) -> Option<EcosystemStatus> {
           Some(
               EcosystemStatus::new(MATTER_ECOSYSTEM, self.is_paired())
                   .with_property(
                       "door_lock_subscribed",
                       EcosystemValue::Bool(self.is_door_lock_ready()),
                   )
                   .with_property(
                       "ring_sensor_subscribed",
                       EcosystemValue::Bool(self.is_ring_ready()),
                   ),
           )
       }
   }
   ```
3. **`MatterController` & `ConnectionMonitor` (`crates/intercom-adapter-matter/src/controller.rs`):**
   - `MatterController` accepts `readiness: &'static ClusterReadinessBarrier` (or updates pairing state via `readiness.set_paired(...)`) so `ClusterReadinessBarrier::is_paired()` always reflects `has_active_controller_fabric(matter)`.

### 4.2 Composition Root (`crates/intercom-firmware`)

1. **Product Ecosystem Enum (`crates/intercom-firmware/src/config.rs`):**
   ```rust
   use intercom_adapter_matter::MATTER_ECOSYSTEM;
   use intercom_core::domain::{EcosystemId, EcosystemType};

   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum SupportedEcosystem {
       Matter,
       HomeAssistant,
   }

   impl EcosystemType for SupportedEcosystem {
       fn id(&self) -> EcosystemId {
           match self {
               Self::Matter => MATTER_ECOSYSTEM,
               Self::HomeAssistant => EcosystemId::new("HomeAssistant"),
           }
       }
   }

   pub const AVAILABLE_ECOSYSTEMS: &[SupportedEcosystem] = &[
       SupportedEcosystem::Matter,
       SupportedEcosystem::HomeAssistant,
   ];
   ```
2. **Removed Stub:** `crates/intercom-firmware/src/ecosystem_info.rs` (`FirmwareEcosystemInfo`) is deleted; `main.rs` passes `readiness` and `config::AVAILABLE_ECOSYSTEMS` directly to `DiagnosticsService::with_ecosystem(...)`.

---

## 5. HTTP JSON Contract & Web Dashboard (`crates/intercom-adapter-http`)

### 5.1 `GET /api/status` JSON Schema (`crates/intercom-adapter-http/src/router.rs`)

`format_status_json` serializes `available_ecosystems` as a JSON array of ecosystem ID strings and `active_ecosystem` as either `null` or a JSON object containing `type`, `paired`, and `properties`:

```json
{
  "state": "Operational",
  "door_locked": true,
  "uptime_sec": 120,
  "wifi_ssid": "MyHomeNet",
  "wifi_rssi_dbm": -58,
  "ip": "192.168.1.142",
  "mac": "68:B6:B3:21:40:8C",
  "active_slot": "ota_0",
  "slot_valid": true,
  "free_heap_bytes": 151552,
  "total_heap_bytes": 215040,
  "available_ecosystems": ["Matter", "HomeAssistant"],
  "active_ecosystem": {
    "type": "Matter",
    "paired": true,
    "properties": {
      "door_lock_subscribed": true,
      "ring_sensor_subscribed": true
    }
  },
  "version": "0.1.0",
  "git_commit": "a3f89b",
  "build_date": "2026-10-16"
}
```

### 5.2 Web Dashboard UI (`crates/intercom-adapter-http/src/assets/dashboard.html`)

1. **Navigation Bar:**
   Three tabs: `Overview` | `Ecosystem` | `System Logs`.
2. **Header Status Badge (`#status-badge`):**
   Displays `● <state>` with semantic styling:
   - `Operational`: Green (`--badge-bg`, `--badge-text`)
   - `EcosystemConnecting`: Blue (`rgba(59, 130, 246, 0.15)`, `--primary`)
   - `Booting`, `Starting`, `Commissioning`, `WifiConnecting`, `Updating`: Amber (`rgba(245, 158, 11, 0.15)`, `--warning`)
   - `FactoryReset`, `Error`, `Offline`: Red (`rgba(239, 68, 68, 0.15)`, `--danger`)
3. **Overview Tab:**
   - Card label updated from `Ecosystem (Matter)` to `Active Ecosystem`.
   - Value displays `<active_ecosystem.type>: <Paired | Unpaired>` (or `None` if `active_ecosystem` is `null`).
4. **Ecosystem Tab (Option B Layout):**
   - Renders cards for the ecosystems in `available_ecosystems` (`Matter` and `HomeAssistant`).
   - **Matter Card:**
     - Header: `Matter` + subtitle `Apple Home / Google Home / HA` + badge (`● Active` when `active_ecosystem.type === 'Matter'`).
     - Status rows (labels owned by Frontend, values driven by `active_ecosystem`):
       - `Fabric Pairing`: `Commissioned` (green) or `Uncommissioned` (amber) based on `active_ecosystem.paired`.
       - `🔒 Door Lock (EP1)`: `Subscribed` (green) or `Waiting` (amber) based on `active_ecosystem.properties.door_lock_subscribed`.
       - `🔔 Ring Sensor (EP2)`: `Subscribed` (green) or `Waiting` (amber) based on `active_ecosystem.properties.ring_sensor_subscribed`.
     - Action button: `✓ Active Ecosystem` (disabled).
   - **Home Assistant Card:**
     - Header: `Home Assistant` + subtitle `Native ESPHome / REST API` + `Planned` badge.
     - Description of direct local push entities for door strike relay and ring event sensor.
     - Action button: `Switch Ecosystem (Coming Soon)` (disabled).
5. **Polling & Size Constraints:**
   - When on either `Overview` or `Ecosystem` tab, the 3-second polling interval fetches `/api/status` and updates both views.
   - Total compressed asset `dashboard.html.gz` remains strictly under the 5 KiB (`5120` bytes) budget.

---

## 6. Verification & Testing Strategy

1. **Unit Tests (`intercom-core`):**
   - Verify `EcosystemId`, `EcosystemType`, and `EcosystemStatus` builder and property iteration.
   - Verify `DiagnosticsService` with `()` (no ecosystem) and with a mock `EcosystemInfoDriver` + custom `EcosystemType` enum.
   - Verify `SystemState` transitions in `FactoryResetService` and `App`.
2. **Unit Tests (`intercom-adapter-http`):**
   - Verify `format_status_json` with both `active_ecosystem: None` and `active_ecosystem: Some(EcosystemStatus)` containing `Bool`, `Int`, and `Text` properties.
   - Verify `dashboard.html.gz` decompression, presence of the `Ecosystem` tab elements, and `< 5120` byte size assertion.
3. **Target Compilation (`intercom-firmware`):**
   - Verify `mise exec -- cargo build -p intercom-firmware` compiles cleanly on `riscv32imac-unknown-none-elf`.
