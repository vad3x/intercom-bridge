# Multi-Ecosystem Architecture & SystemState Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a strongly-typed, protocol-agnostic Ecosystem domain model and port in `intercom-core`, update `SystemState` to disambiguate boot/commissioning/Wi-Fi/ecosystem lifecycle phases, wire Matter live subscription & pairing status in `intercom-adapter-matter` and `intercom-firmware`, and add the interactive `Ecosystem` tab to the embedded web dashboard in `intercom-adapter-http`.

**Architecture:** `intercom-core` defines `EcosystemId`, `EcosystemType`, `EcosystemValue`, `EcosystemStatus`, and `EcosystemInfoDriver` with zero concrete protocol names (`Matter` / `HomeAssistant`), plus the updated `SystemState` progression (`Booting` -> `Starting` -> `Commissioning` / `WifiConnecting` -> `EcosystemConnecting` -> `Operational`). `intercom-adapter-matter` implements `EcosystemInfoDriver` on `ClusterReadinessBarrier` and emits accurate commissioning/network/subscription transitions, `intercom-firmware` defines `SupportedEcosystem` (`Matter`, `HomeAssistant`), and `intercom-adapter-http` serializes `available_ecosystems` + `active_ecosystem` into `/api/status` and renders the new `Ecosystem` tab in `dashboard.html`.

**Tech Stack:** Rust (`no_std`, `riscv32imac-unknown-none-elf` + `x86_64-apple-darwin` host tests), Embassy (`embassy-sync`), `rs-matter-embassy`, `picoserve`, HTML5/CSS/JS (gzipped via `flate2`).

## Global Constraints

- For accessing rust/cargo CLI, always use `mise exec -- cargo ...`.
- Host unit tests run via `mise exec -- cargo test -p <crate> --target x86_64-apple-darwin`.
- All tests MUST follow the Arrange -> Act -> Assert pattern with `// Arrange`, `// Act`, and `// Assert` comments.
- Public items MUST appear before private items in every file.
- `intercom-core` must have zero references to `"Matter"` or `"HomeAssistant"` in `src/`.
- Compressed `dashboard.html.gz` MUST remain strictly under `5120` bytes (5 KiB).

---

### Task 1: Core `SystemState` Lifecycle & Ecosystem Domain/Port Models (`intercom-core`)

**Files:**
- Create: `crates/intercom-core/src/domain/ecosystem.rs`
- Modify: `crates/intercom-core/src/events.rs`
- Modify: `crates/intercom-core/src/app.rs`
- Modify: `crates/intercom-core/src/domain/mod.rs`
- Modify: `crates/intercom-core/src/domain/diagnostics.rs`
- Modify: `crates/intercom-core/src/ports/ecosystem.rs`
- Modify: `crates/intercom-core/src/service/diagnostics.rs`
- Modify: `crates/intercom-core/src/lib.rs`
- Test: `crates/intercom-core/tests/service_tests.rs`
- Test: `crates/intercom-core/tests/diagnostics_service_tests.rs`

**Interfaces:**
- Consumes: `embassy_sync::watch::Watch`
- Produces:
  - `intercom_core::events::SystemState::{Booting, Starting, Commissioning, WifiConnecting, EcosystemConnecting, Operational, Updating, FactoryReset, Error}`
  - `intercom_core::events::SystemStateSender::{booting, starting, commissioning, wifi_connecting, ecosystem_connecting, operational, updating, factory_reset, error}`
  - `intercom_core::domain::ecosystem::{EcosystemId, EcosystemType, EcosystemValue, EcosystemStatus, MAX_ECOSYSTEM_PROPERTIES}`
  - `intercom_core::ports::ecosystem::EcosystemInfoDriver::status(&self) -> Option<EcosystemStatus>`
  - `intercom_core::domain::diagnostics::SystemDiagnostics<T: EcosystemType = EcosystemId>`
  - `intercom_core::service::diagnostics::DiagnosticsService::with_ecosystem(hardware, ecosystem, available_ecosystems, state_watch)`

- [ ] **Step 1: Write the failing tests in `crates/intercom-core/tests/service_tests.rs` and `crates/intercom-core/tests/diagnostics_service_tests.rs`**

Update `crates/intercom-core/tests/service_tests.rs` lines 215-347 to test the new `SystemState` variants (`Booting`, `Starting`, `Commissioning`, `WifiConnecting`, `EcosystemConnecting`, `Operational`):

```rust
#[test]
fn system_state_watch_sender_and_receiver() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut receiver = watch.receiver().unwrap();

    // Act
    sender.operational();
    let state = futures::executor::block_on(receiver.changed());

    // Assert
    assert_eq!(state, SystemState::Operational);
}

#[test]
fn system_state_watch_broadcasts_to_multiple_receivers() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut rx1 = watch.receiver().unwrap();
    let mut rx2 = watch.receiver().unwrap();

    // Act
    sender.wifi_connecting();
    let (s1, s2) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert - both independent receivers observed the state transition
    assert_eq!(s1, SystemState::WifiConnecting);
    assert_eq!(s2, SystemState::WifiConnecting);

    // Act - second transition
    sender.operational();
    let (s3, s4) = futures::executor::block_on(async {
        embassy_futures::join::join(rx1.changed(), rx2.changed()).await
    });

    // Assert
    assert_eq!(s3, SystemState::Operational);
    assert_eq!(s4, SystemState::Operational);
}

#[test]
fn system_state_watch_notifies_updating_transition() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);
    let mut receiver = watch.receiver().unwrap();

    // Act
    sender.send(SystemState::Updating);

    // Assert
    assert_eq!(receiver.try_get(), Some(SystemState::Updating));

    // Act - via helper method
    sender.operational();
    sender.updating();

    // Assert
    assert_eq!(receiver.try_get(), Some(SystemState::Updating));
}

#[test]
fn system_state_sender_queries_current_state() {
    // Arrange
    let watch = SystemStateWatch::new();
    let sender = SystemStateSender::new(&watch);

    // Act
    sender.booting();
    assert_eq!(sender.current_state(), Some(SystemState::Booting));
    sender.starting();
    assert_eq!(sender.current_state(), Some(SystemState::Starting));
    sender.commissioning();
    assert_eq!(sender.current_state(), Some(SystemState::Commissioning));
    sender.ecosystem_connecting();

    // Assert
    assert_eq!(sender.current_state(), Some(SystemState::EcosystemConnecting));
}
```

Replace `crates/intercom-core/tests/diagnostics_service_tests.rs` with tests for `EcosystemId`, `EcosystemType`, `EcosystemStatus`, and `DiagnosticsService`:

```rust
//! Unit tests for DiagnosticsService, SystemDiagnostics, EcosystemStatus, HardwareInfoDriver, and EcosystemInfoDriver.
//!
//! Follows the Arrange -> Act -> Assert pattern strictly.

use intercom_core::domain::diagnostics::{SystemDiagnostics, WifiSsid};
use intercom_core::domain::ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue,
};
use intercom_core::events::{SystemState, SystemStateSender, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::service::diagnostics::DiagnosticsService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestEcosystem {
    ProtoA,
    ProtoB,
}

impl EcosystemType for TestEcosystem {
    fn id(&self) -> EcosystemId {
        match self {
            Self::ProtoA => EcosystemId::new("ProtoA"),
            Self::ProtoB => EcosystemId::new("ProtoB"),
        }
    }
}

const TEST_AVAILABLE_ECOSYSTEMS: &[TestEcosystem] = &[TestEcosystem::ProtoA, TestEcosystem::ProtoB];

pub struct MockHardwareInfoDriver {
    pub uptime_secs: u64,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub wifi_ssid: Option<WifiSsid>,
    pub wifi_rssi: Option<i8>,
    pub ip_address: Option<[u8; 4]>,
    pub mac_address: [u8; 6],
    pub active_slot: &'static str,
    pub is_slot_valid: bool,
    pub firmware_version: &'static str,
    pub git_commit: &'static str,
    pub build_date: &'static str,
}

impl MockHardwareInfoDriver {
    pub fn new() -> Self {
        Self {
            uptime_secs: 12345,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
            wifi_rssi: Some(-65),
            ip_address: Some([192, 168, 1, 100]),
            mac_address: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            is_slot_valid: true,
            firmware_version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    }
}

impl HardwareInfoDriver for MockHardwareInfoDriver {
    fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }

    fn free_heap_bytes(&self) -> usize {
        self.free_heap_bytes
    }

    fn total_heap_bytes(&self) -> usize {
        self.total_heap_bytes
    }

    fn wifi_ssid(&self) -> Option<WifiSsid> {
        self.wifi_ssid
    }

    fn wifi_rssi(&self) -> Option<i8> {
        self.wifi_rssi
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        self.ip_address
    }

    fn mac_address(&self) -> [u8; 6] {
        self.mac_address
    }

    fn active_slot(&self) -> &'static str {
        self.active_slot
    }

    fn is_slot_valid(&self) -> bool {
        self.is_slot_valid
    }

    fn firmware_version(&self) -> &'static str {
        self.firmware_version
    }

    fn git_commit(&self) -> &'static str {
        self.git_commit
    }

    fn build_date(&self) -> &'static str {
        self.build_date
    }
}

pub struct MockEcosystemInfoDriver {
    pub status: Option<EcosystemStatus>,
}

impl MockEcosystemInfoDriver {
    pub fn new(status: Option<EcosystemStatus>) -> Self {
        Self { status }
    }
}

impl EcosystemInfoDriver for MockEcosystemInfoDriver {
    fn status(&self) -> Option<EcosystemStatus> {
        self.status
    }
}

#[test]
fn diagnostics_service_snapshot_with_default_ecosystem() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.operational();
    let service = DiagnosticsService::new(&hardware, &state_watch);

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            state: SystemState::Operational,
            door_locked: true,
            uptime_sec: 12345,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            available_ecosystems: &[],
            active_ecosystem: None,
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}

#[test]
fn diagnostics_service_snapshot_with_explicit_ecosystem_and_properties() {
    // Arrange
    let hardware = MockHardwareInfoDriver::new();
    let expected_status = EcosystemStatus::new(TestEcosystem::ProtoA.id(), true)
        .with_property("door_lock_subscribed", EcosystemValue::Bool(true))
        .with_property("ring_sensor_subscribed", EcosystemValue::Bool(false));
    let ecosystem = MockEcosystemInfoDriver::new(Some(expected_status));
    let state_watch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&state_watch);
    state_sender.ecosystem_connecting();
    let service = DiagnosticsService::with_ecosystem(
        &hardware,
        &ecosystem,
        TEST_AVAILABLE_ECOSYSTEMS,
        &state_watch,
    );

    // Act
    let snapshot = service.snapshot();

    // Assert
    assert_eq!(
        snapshot,
        SystemDiagnostics {
            state: SystemState::EcosystemConnecting,
            door_locked: true,
            uptime_sec: 12345,
            wifi_ssid: WifiSsid::new("HomeNet-IoT"),
            wifi_rssi_dbm: Some(-65),
            ip: Some([192, 168, 1, 100]),
            mac: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 45000,
            total_heap_bytes: 80000,
            available_ecosystems: TEST_AVAILABLE_ECOSYSTEMS,
            active_ecosystem: Some(expected_status),
            version: "1.0.0",
            git_commit: "abcdef0",
            build_date: "2026-10-15",
        }
    );
}

#[test]
fn ecosystem_status_iterates_populated_properties() {
    // Arrange
    let status = EcosystemStatus::new(EcosystemId::new("Custom"), false)
        .with_property("key_bool", EcosystemValue::Bool(true))
        .with_property("key_int", EcosystemValue::Int(42))
        .with_property("key_text", EcosystemValue::Text("online"));

    // Act
    let props: std::vec::Vec<_> = status.iter_properties().collect();

    // Assert
    assert_eq!(
        props,
        std::vec![
            ("key_bool", EcosystemValue::Bool(true)),
            ("key_int", EcosystemValue::Int(42)),
            ("key_text", EcosystemValue::Text("online")),
        ]
    );
}
```

- [ ] **Step 2: Run `intercom-core` tests to verify they fail**

Run: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: FAIL with unresolved imports `intercom_core::domain::ecosystem` and missing `SystemState::Operational` / `WifiConnecting` / `EcosystemConnecting`.

- [ ] **Step 3: Implement `events.rs`, `domain/ecosystem.rs`, `domain/diagnostics.rs`, `ports/ecosystem.rs`, `service/diagnostics.rs`, `app.rs`, and `lib.rs`**

Create `crates/intercom-core/src/domain/ecosystem.rs`:

```rust
//! Smart home ecosystem domain model.
//!
//! Protocol-agnostic value objects and status representations for smart home ecosystems.

/// Maximum number of free-form key-value telemetry properties per ecosystem.
pub const MAX_ECOSYSTEM_PROPERTIES: usize = 6;

/// Strongly-typed value object identifying a smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemId(&'static str);

impl EcosystemId {
    /// Create a new `EcosystemId` from a static string identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Return the underlying string slice identifier.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Trait implemented by composition-root ecosystem enums to map variants to [`EcosystemId`].
pub trait EcosystemType: Copy + Eq + core::fmt::Debug + 'static {
    /// Return the strongly-typed [`EcosystemId`] for this ecosystem variant.
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

/// Status and telemetry snapshot of a single active smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
}

impl EcosystemStatus {
    /// Create a new `EcosystemStatus` with no extra properties.
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

    /// Iterate over all populated `(key, value)` properties in insertion order.
    pub fn iter_properties(&self) -> impl Iterator<Item = (&'static str, EcosystemValue)> + '_ {
        self.properties.iter().filter_map(|prop| *prop)
    }
}
```

Update `crates/intercom-core/src/domain/mod.rs`:

```rust
//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;
pub mod diagnostics;
pub mod ecosystem;

pub use access::{AccessEffects, AccessStateMachine};
pub use diagnostics::{SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, MAX_ECOSYSTEM_PROPERTIES,
};
```

Update `crates/intercom-core/src/events.rs`:

```rust
//! Smart Intercom - Lifecycle Event System
//!
//! Embassy `Watch` channel for continuous system lifecycle state.
//! Replaces legacy broadcast event buses with typed, bounded state observation.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::{Receiver as WatchReceiver, Watch};

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

// ============================================================================
// Lifecycle Watch type aliases & sender
// ============================================================================

/// Maximum concurrent receivers for system lifecycle state.
pub const STATE_WATCH_RECEIVERS: usize = 3;

/// Continuous lifecycle state watch signal (zero-allocation, zero queue overflow).
pub type SystemStateWatch = Watch<CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Watch receiver for observing system lifecycle transitions.
pub type SystemStateReceiver<'a> =
    WatchReceiver<'a, CriticalSectionRawMutex, SystemState, STATE_WATCH_RECEIVERS>;

/// Lightweight, Copy handle for sending system lifecycle state updates.
#[derive(Clone, Copy)]
pub struct SystemStateSender<'a>(&'a SystemStateWatch);

impl<'a> SystemStateSender<'a> {
    /// Create a new sender handle wrapping a `SystemStateWatch`.
    pub const fn new(watch: &'a SystemStateWatch) -> Self {
        Self(watch)
    }

    /// Query the current system lifecycle state, if available.
    pub fn current_state(&self) -> Option<SystemState> {
        self.0.try_get()
    }

    /// Send a new system lifecycle state.
    pub fn send(&self, state: SystemState) {
        self.0.sender().send(state);
    }

    /// Convenience helper to transition to `SystemState::Booting`.
    pub fn booting(&self) {
        self.send(SystemState::Booting);
    }

    /// Convenience helper to transition to `SystemState::Starting`.
    pub fn starting(&self) {
        self.send(SystemState::Starting);
    }

    /// Convenience helper to transition to `SystemState::Commissioning`.
    pub fn commissioning(&self) {
        self.send(SystemState::Commissioning);
    }

    /// Convenience helper to transition to `SystemState::WifiConnecting`.
    pub fn wifi_connecting(&self) {
        self.send(SystemState::WifiConnecting);
    }

    /// Convenience helper to transition to `SystemState::EcosystemConnecting`.
    pub fn ecosystem_connecting(&self) {
        self.send(SystemState::EcosystemConnecting);
    }

    /// Convenience helper to transition to `SystemState::Operational`.
    pub fn operational(&self) {
        self.send(SystemState::Operational);
    }

    /// Convenience helper to transition to `SystemState::Updating`.
    pub fn updating(&self) {
        self.send(SystemState::Updating);
    }

    /// Convenience helper to transition to `SystemState::FactoryReset`.
    pub fn factory_reset(&self) {
        self.send(SystemState::FactoryReset);
    }

    /// Convenience helper to transition to `SystemState::Error`.
    pub fn error(&self) {
        self.send(SystemState::Error);
    }
}
```

Update `crates/intercom-core/src/app.rs` line 66-67 to publish `Starting`:

```rust
        info!("publishing Starting state");
        state_sender.starting();
```

Update `crates/intercom-core/src/ports/ecosystem.rs`:

```rust
//! Outbound port traits for smart home ecosystem integrations.

use crate::domain::ecosystem::EcosystemStatus;

/// Ecosystem status and telemetry provider.
///
/// Implemented by smart home protocol adapters.
/// Decouples the core domain and diagnostic telemetry from protocol-specific details.
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

Update `crates/intercom-core/src/domain/diagnostics.rs`:

```rust
//! System diagnostics domain model.

use crate::domain::ecosystem::{EcosystemId, EcosystemStatus, EcosystemType};
use crate::events::SystemState;

/// Real-time health, network, flash, ecosystem, and firmware telemetry snapshot.
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
(Keep `WifiSsid` below `SystemDiagnostics` unchanged).

Update `crates/intercom-core/src/service/diagnostics.rs`:

```rust
//! Diagnostics application service aggregating telemetry into system snapshots.

use crate::domain::diagnostics::SystemDiagnostics;
use crate::domain::ecosystem::{EcosystemId, EcosystemType};
use crate::events::{SystemState, SystemStateWatch};
use crate::ports::diagnostics::HardwareInfoDriver;
use crate::ports::ecosystem::EcosystemInfoDriver;

/// Service responsible for producing point-in-time system diagnostics snapshots.
pub struct DiagnosticsService<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver = (),
    T: EcosystemType = EcosystemId,
> {
    hardware: &'a H,
    ecosystem: &'a E,
    available_ecosystems: &'static [T],
    state_watch: &'a SystemStateWatch,
}

impl<'a, H: HardwareInfoDriver> DiagnosticsService<'a, H, (), EcosystemId> {
    /// Create a new DiagnosticsService instance without a dedicated smart home ecosystem driver.
    pub fn new(hardware: &'a H, state_watch: &'a SystemStateWatch) -> Self {
        Self {
            hardware,
            ecosystem: &(),
            available_ecosystems: &[],
            state_watch,
        }
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, T: EcosystemType>
    DiagnosticsService<'a, H, E, T>
{
    /// Create a new DiagnosticsService instance with a dedicated smart home ecosystem driver
    /// and a slice of available ecosystems provided by the composition root.
    pub fn with_ecosystem(
        hardware: &'a H,
        ecosystem: &'a E,
        available_ecosystems: &'static [T],
        state_watch: &'a SystemStateWatch,
    ) -> Self {
        Self {
            hardware,
            ecosystem,
            available_ecosystems,
            state_watch,
        }
    }

    /// Aggregate current domain state, ecosystem status, and hardware telemetry into a snapshot.
    pub fn snapshot(&self) -> SystemDiagnostics<T> {
        let state = self.state_watch.try_get().unwrap_or(SystemState::Booting);
        let door_locked = true;

        SystemDiagnostics {
            state,
            door_locked,
            uptime_sec: self.hardware.uptime_secs(),
            wifi_ssid: self.hardware.wifi_ssid(),
            wifi_rssi_dbm: self.hardware.wifi_rssi(),
            ip: self.hardware.ip_address(),
            mac: self.hardware.mac_address(),
            active_slot: self.hardware.active_slot(),
            slot_valid: self.hardware.is_slot_valid(),
            free_heap_bytes: self.hardware.free_heap_bytes(),
            total_heap_bytes: self.hardware.total_heap_bytes(),
            available_ecosystems: self.available_ecosystems,
            active_ecosystem: self.ecosystem.status(),
            version: self.hardware.firmware_version(),
            git_commit: self.hardware.git_commit(),
            build_date: self.hardware.build_date(),
        }
    }
}
```

Update `crates/intercom-core/src/lib.rs` to re-export the new domain types:

```rust
pub use domain::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, SystemDiagnostics, WifiSsid,
    MAX_ECOSYSTEM_PROPERTIES,
};
```

- [ ] **Step 4: Run `intercom-core` unit tests to verify they pass**

Run: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS (all tests in `diagnostics_service_tests` and `service_tests` pass).

- [ ] **Step 5: Commit Task 1**

```bash
git add crates/intercom-core/
git commit -m "feat(core): introduce protocol-agnostic Ecosystem domain model and update SystemState"
```

---

### Task 2: HTTP JSON Serialization, Server Generics, and Web Dashboard Ecosystem Tab (`intercom-adapter-http`)

**Files:**
- Modify: `crates/intercom-adapter-http/src/router.rs`
- Modify: `crates/intercom-adapter-http/src/server.rs`
- Modify: `crates/intercom-adapter-http/src/assets/dashboard.html`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Test: `crates/intercom-adapter-http/tests/router_tests.rs`
- Test: `crates/intercom-adapter-http/tests/server_tests.rs`

**Interfaces:**
- Consumes: `SystemDiagnostics<T>`, `EcosystemType`, `EcosystemStatus`, `EcosystemValue`, `SystemState` from `intercom-core`
- Produces:
  - `format_status_json<T: EcosystemType>(diag: &SystemDiagnostics<T>, out: &mut [u8]) -> Result<usize, ()>`
  - `HttpServer<'a, H, E, O, R, T: EcosystemType = EcosystemId>`
  - Embedded `DASHBOARD_HTML_GZ` with `Overview`, `Ecosystem`, and `System Logs` tabs

- [ ] **Step 1: Update failing tests in `crates/intercom-adapter-http/tests/router_tests.rs`, `tests/server_tests.rs`, and `src/lib.rs`**

In `crates/intercom-adapter-http/src/lib.rs`, add assertions for the new Ecosystem tab elements in `test_dashboard_gzip_decompresses_to_valid_html`:

```rust
        assert!(html.contains("tab-btn-ecosystem"));
        assert!(html.contains("tab-ecosystem"));
        assert!(html.contains("Home Assistant"));
        assert!(DASHBOARD_HTML_GZ.len() < 5120, "Gzip asset exceeds 5 KiB limit");
```

In `crates/intercom-adapter-http/tests/router_tests.rs`, update `test_format_status_json_success`, `test_format_status_json_with_none_options`, and `test_format_status_json_buffer_overflow_protection` to test `available_ecosystems` and `active_ecosystem` (including `properties`):

```rust
#[test]
fn test_format_status_json_success() {
    // Arrange
    const AVAILABLE: &[intercom_core::domain::EcosystemId] = &[
        intercom_core::domain::EcosystemId::new("Matter"),
        intercom_core::domain::EcosystemId::new("HomeAssistant"),
    ];
    let active = intercom_core::domain::EcosystemStatus::new(
        intercom_core::domain::EcosystemId::new("Matter"),
        true,
    )
    .with_property(
        "door_lock_subscribed",
        intercom_core::domain::EcosystemValue::Bool(true),
    )
    .with_property(
        "ring_sensor_subscribed",
        intercom_core::domain::EcosystemValue::Bool(false),
    );
    let diag = SystemDiagnostics {
        state: SystemState::Operational,
        door_locked: true,
        uptime_sec: 14250,
        wifi_ssid: intercom_core::domain::diagnostics::WifiSsid::new("MyHomeNetwork"),
        wifi_rssi_dbm: Some(-58),
        ip: Some([192, 168, 1, 142]),
        mac: [0x68, 0xB6, 0xB3, 0x21, 0x40, 0x8C],
        active_slot: "ota_0",
        slot_valid: true,
        free_heap_bytes: 151552,
        total_heap_bytes: 215040,
        available_ecosystems: AVAILABLE,
        active_ecosystem: Some(active),
        version: "0.1.0",
        git_commit: "a3f89b",
        build_date: "2026-10-15",
    };
    let mut buf = [0u8; 512];

    // Act
    let written = format_status_json(&diag, &mut buf).expect("serialization should succeed");
    let json_str = core::str::from_utf8(&buf[..written]).expect("valid utf-8");
    let val: Value = serde_json::from_str(json_str).expect("valid json format");

    // Assert
    assert_eq!(val["state"], "Operational");
    assert_eq!(val["door_locked"], true);
    assert_eq!(val["uptime_sec"], 14250);
    assert_eq!(val["wifi_ssid"], "MyHomeNetwork");
    assert_eq!(val["wifi_rssi_dbm"], -58);
    assert_eq!(val["ip"], "192.168.1.142");
    assert_eq!(val["mac"], "68:B6:B3:21:40:8C");
    assert_eq!(val["active_slot"], "ota_0");
    assert_eq!(val["slot_valid"], true);
    assert_eq!(val["free_heap_bytes"], 151552);
    assert_eq!(val["total_heap_bytes"], 215040);
    assert_eq!(val["available_ecosystems"], serde_json::json!(["Matter", "HomeAssistant"]));
    assert_eq!(val["active_ecosystem"]["type"], "Matter");
    assert_eq!(val["active_ecosystem"]["paired"], true);
    assert_eq!(val["active_ecosystem"]["properties"]["door_lock_subscribed"], true);
    assert_eq!(val["active_ecosystem"]["properties"]["ring_sensor_subscribed"], false);
    assert_eq!(val["version"], "0.1.0");
    assert_eq!(val["git_commit"], "a3f89b");
    assert_eq!(val["build_date"], "2026-10-15");
}
```

And in `test_format_status_json_with_none_options`, use `state: SystemState::WifiConnecting`, `available_ecosystems: &[]`, `active_ecosystem: None`, and assert:
```rust
    assert_eq!(val["state"], "WifiConnecting");
    assert_eq!(val["available_ecosystems"], serde_json::json!([]));
    assert!(val["active_ecosystem"].is_null());
```
And in `crates/intercom-adapter-http/tests/server_tests.rs`, replace `SystemState::Ready` with `SystemState::Operational`, and in `test_get_status_returns_valid_telemetry_json` assert `assert_eq!(v["state"], "Operational");`, `assert_eq!(v["available_ecosystems"], serde_json::json!([]));`, and `assert!(v["active_ecosystem"].is_null());`.

- [ ] **Step 2: Run `intercom-adapter-http` tests to verify they fail**

Run: `mise exec -- cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: FAIL with compilation errors on `SystemState` variants and `SystemDiagnostics` fields in `router.rs` and `server.rs`.

- [ ] **Step 3: Implement `router.rs`, `server.rs`, and `assets/dashboard.html`**

Update `crates/intercom-adapter-http/src/router.rs` to serialize `available_ecosystems` and `active_ecosystem` and map all 9 `SystemState` variants:

```rust
use core::fmt::Write;
use intercom_core::domain::diagnostics::SystemDiagnostics;
use intercom_core::domain::ecosystem::{EcosystemType, EcosystemValue};
use intercom_core::events::SystemState;
```

In `format_status_json`:
```rust
pub fn format_status_json<T: EcosystemType>(
    diag: &SystemDiagnostics<T>,
    out: &mut [u8],
) -> Result<usize, ()> {
    let mut writer = BufferWriter::new(out);

    write!(
        writer,
        "{{\"state\":\"{}\",\"door_locked\":{},\"uptime_sec\":{},\"wifi_ssid\":",
        state_to_str(diag.state),
        diag.door_locked,
        diag.uptime_sec
    )
    .map_err(|_| ())?;

    match &diag.wifi_ssid {
        Some(ssid) => write!(writer, "\"{}\"", ssid.as_str()).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(writer, ",\"wifi_rssi_dbm\":").map_err(|_| ())?;
    match diag.wifi_rssi_dbm {
        Some(rssi) => write!(writer, "{}", rssi).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(writer, ",\"ip\":").map_err(|_| ())?;
    match diag.ip {
        Some([a, b, c, d]) => write!(writer, "\"{}.{}.{}.{}\"", a, b, c, d).map_err(|_| ())?,
        None => write!(writer, "null").map_err(|_| ())?,
    }

    let [m0, m1, m2, m3, m4, m5] = diag.mac;
    write!(
        writer,
        ",\"mac\":\"{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}\",\"active_slot\":\"{}\",\"slot_valid\":{},\"free_heap_bytes\":{},\"total_heap_bytes\":{},\"available_ecosystems\":[",
        m0,
        m1,
        m2,
        m3,
        m4,
        m5,
        diag.active_slot,
        diag.slot_valid,
        diag.free_heap_bytes,
        diag.total_heap_bytes,
    )
    .map_err(|_| ())?;

    for (idx, eco) in diag.available_ecosystems.iter().enumerate() {
        if idx > 0 {
            write!(writer, ",").map_err(|_| ())?;
        }
        write!(writer, "\"{}\"", eco.id().as_str()).map_err(|_| ())?;
    }

    write!(writer, "],\"active_ecosystem\":").map_err(|_| ())?;
    match &diag.active_ecosystem {
        Some(active) => {
            write!(
                writer,
                "{{\"type\":\"{}\",\"paired\":{},\"properties\":{{",
                active.ecosystem_type.as_str(),
                active.paired
            )
            .map_err(|_| ())?;

            for (idx, (key, val)) in active.iter_properties().enumerate() {
                if idx > 0 {
                    write!(writer, ",").map_err(|_| ())?;
                }
                match val {
                    EcosystemValue::Bool(b) => write!(writer, "\"{}\":{}", key, b).map_err(|_| ())?,
                    EcosystemValue::Int(n) => write!(writer, "\"{}\":{}", key, n).map_err(|_| ())?,
                    EcosystemValue::Text(s) => {
                        write!(writer, "\"{}\":\"{}\"", key, s).map_err(|_| ())?
                    }
                }
            }
            write!(writer, "}}}}").map_err(|_| ())?;
        }
        None => write!(writer, "null").map_err(|_| ())?,
    }

    write!(
        writer,
        ",\"version\":\"{}\",\"git_commit\":\"{}\",\"build_date\":\"{}\"}}",
        diag.version, diag.git_commit, diag.build_date
    )
    .map_err(|_| ())?;

    Ok(writer.written())
}

fn state_to_str(state: SystemState) -> &'static str {
    match state {
        SystemState::Booting => "Booting",
        SystemState::Starting => "Starting",
        SystemState::Commissioning => "Commissioning",
        SystemState::WifiConnecting => "WifiConnecting",
        SystemState::EcosystemConnecting => "EcosystemConnecting",
        SystemState::Operational => "Operational",
        SystemState::Updating => "Updating",
        SystemState::FactoryReset => "FactoryReset",
        SystemState::Error => "Error",
    }
}
```

Update `crates/intercom-adapter-http/src/server.rs` to thread `T: EcosystemType = EcosystemId` through `AppState`, `HttpServer`, `build_router`, and `OtaFlashService`, and replace `sender.ready()` on line 399 with `sender.operational()`.

Update `crates/intercom-adapter-http/src/assets/dashboard.html` to:
1. Add `<button id="tab-btn-ecosystem" class="tab-btn">Ecosystem</button>` to `<nav class="tabs">`.
2. Update the Overview ecosystem card label from `Ecosystem (Matter)` to `Active Ecosystem`.
3. Add `<section id="tab-ecosystem" class="tab-content">` with the Option B two-card layout (`Matter` card with `Fabric Pairing`, `🔒 Door Lock (EP1)`, and `🔔 Ring Sensor (EP2)` rows + `✓ Active Ecosystem` button; `Home Assistant` card with `Planned` badge and disabled `Switch Ecosystem (Coming Soon)` button).
4. Update `fetchStatus()` in JS to:
   - Style `#status-badge` for `Operational` (green), `EcosystemConnecting` (blue), `Booting`/`Starting`/`Commissioning`/`WifiConnecting`/`Updating` (amber), and `FactoryReset`/`Error` (red).
   - Populate `#val-ecosystem` on Overview (`active_ecosystem.type + ': ' + (active_ecosystem.paired ? 'Paired' : 'Unpaired')`).
   - Populate `#eco-matter-badge`, `#eco-matter-pairing`, `#eco-matter-lock`, `#eco-matter-ring`, and `#eco-matter-btn` from `data.active_ecosystem`.
   - Poll `fetchStatus()` when `activeTab === 'overview' || activeTab === 'ecosystem'`.

- [ ] **Step 4: Run `intercom-adapter-http` tests to verify they pass**

Run: `mise exec -- cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: PASS (all router, server, and gzip size/HTML tests pass).

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/intercom-adapter-http/
git commit -m "feat(http): serialize active/available ecosystems and add Ecosystem dashboard tab"
```

---

### Task 3: Matter Adapter Readiness & GPIO LED Patterns (`intercom-adapter-matter` & `intercom-adapter-gpio`)

**Files:**
- Modify: `crates/intercom-adapter-gpio/src/config.rs`
- Modify: `crates/intercom-adapter-gpio/src/led.rs`
- Modify: `crates/intercom-adapter-matter/src/config.rs`
- Modify: `crates/intercom-adapter-matter/src/lib.rs`
- Modify: `crates/intercom-adapter-matter/src/readiness.rs`
- Modify: `crates/intercom-adapter-matter/src/controller.rs`

**Interfaces:**
- Consumes: `SystemState`, `EcosystemId`, `EcosystemStatus`, `EcosystemValue`, `EcosystemInfoDriver` from `intercom-core`
- Produces:
  - `intercom_adapter_matter::MATTER_ECOSYSTEM: EcosystemId`
  - `ClusterReadinessBarrier` implementing `EcosystemInfoDriver` with `set_paired(&self, paired: bool)`, `is_paired(&self) -> bool`, `is_door_lock_ready(&self) -> bool`, `is_ring_ready(&self) -> bool`
  - `MatterController::new(config, state_sender, readiness, reset)`

- [ ] **Step 1: Update `intercom-adapter-gpio/src/config.rs` and `led.rs` for the new `SystemState` variants**

In `crates/intercom-adapter-gpio/src/config.rs`, update the LED timing constants:

```rust
/// Booting: slow heartbeat — brief flash, long dark (~17% duty)
pub const BOOTING_ON_MS: u64 = 200;
pub const BOOTING_OFF_MS: u64 = 1000;

/// Starting: faster heartbeat — hardware ready, starting stack (30% duty)
pub const STARTING_ON_MS: u64 = 300;
pub const STARTING_OFF_MS: u64 = 700;

/// Commissioning: rapid 2 Hz blink — awaiting initial pairing (50% duty)
pub const COMMISSIONING_ON_MS: u64 = 250;
pub const COMMISSIONING_OFF_MS: u64 = 250;

/// WifiConnecting: even medium 1 Hz blink — associating with Wi-Fi (50% duty)
pub const WIFI_CONNECTING_ON_MS: u64 = 500;
pub const WIFI_CONNECTING_OFF_MS: u64 = 500;

/// EcosystemConnecting: mostly on, brief wink — LAN up, awaiting hub subscription (90% duty)
pub const ECOSYSTEM_CONNECTING_ON_MS: u64 = 900;
pub const ECOSYSTEM_CONNECTING_OFF_MS: u64 = 100;
```

In `crates/intercom-adapter-gpio/src/led.rs`, update `indicate_state(&self, state: SystemState)`:

```rust
    fn indicate_state(&self, state: SystemState) {
        match state {
            SystemState::Booting => {
                self.blink(config::BOOTING_ON_MS, config::BOOTING_OFF_MS, None);
            }
            SystemState::Starting => {
                self.blink(config::STARTING_ON_MS, config::STARTING_OFF_MS, None);
            }
            SystemState::Commissioning => {
                self.blink(config::COMMISSIONING_ON_MS, config::COMMISSIONING_OFF_MS, None);
            }
            SystemState::WifiConnecting => {
                self.blink(config::WIFI_CONNECTING_ON_MS, config::WIFI_CONNECTING_OFF_MS, None);
            }
            SystemState::EcosystemConnecting => {
                self.blink(
                    config::ECOSYSTEM_CONNECTING_ON_MS,
                    config::ECOSYSTEM_CONNECTING_OFF_MS,
                    None,
                );
            }
            SystemState::Operational => {
                self.solid();
            }
            SystemState::Updating => {
                self.strobe();
            }
            SystemState::FactoryReset => {
                self.blink(config::FACTORY_RESET_ON_MS, config::FACTORY_RESET_OFF_MS, None);
            }
            SystemState::Error => {
                self.burst(
                    config::ERROR_ON_MS,
                    config::ERROR_OFF_MS,
                    config::ERROR_BURST_COUNT,
                    config::ERROR_PAUSE_MS,
                );
            }
        }
    }
```

- [ ] **Step 2: Update `intercom-adapter-matter/src/config.rs`, `readiness.rs`, `controller.rs`, and `lib.rs`**

In `crates/intercom-adapter-matter/src/config.rs`, add:

```rust
use intercom_core::domain::EcosystemId;

/// Strongly-typed identifier for the Matter smart home ecosystem.
pub const MATTER_ECOSYSTEM: EcosystemId = EcosystemId::new("Matter");
```

In `crates/intercom-adapter-matter/src/lib.rs`, re-export `MATTER_ECOSYSTEM`:

```rust
pub use config::{DOOR_LOCK_ENDPOINT_ID, MATTER_ECOSYSTEM, OCCUPANCY_SENSOR_ENDPOINT_ID};
```

In `crates/intercom-adapter-matter/src/readiness.rs`, add `paired: AtomicBool`, implement `EcosystemInfoDriver`, and emit `state_sender.operational()`:

```rust
//! Smart Intercom - Matter Cluster Readiness Barrier
//!
//! Coordinates Apple Home subscription priming across endpoints and implements
//! `EcosystemInfoDriver` to report Matter pairing and cluster subscription status.

use core::sync::atomic::{AtomicBool, Ordering};
use intercom_core::domain::{EcosystemStatus, EcosystemValue};
use intercom_core::events::SystemStateSender;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use log::info;

use crate::config::MATTER_ECOSYSTEM;

/// Internal synchronization barrier coordinating Matter pairing and cluster readiness.
pub struct ClusterReadinessBarrier {
    paired: AtomicBool,
    door_lock_ready: AtomicBool,
    ring_ready: AtomicBool,
    state_sender: SystemStateSender<'static>,
}

impl ClusterReadinessBarrier {
    /// Create a new cluster readiness barrier.
    pub const fn new(state_sender: SystemStateSender<'static>) -> Self {
        Self {
            paired: AtomicBool::new(false),
            door_lock_ready: AtomicBool::new(false),
            ring_ready: AtomicBool::new(false),
            state_sender,
        }
    }

    /// Update whether the Matter node is commissioned with an active controller fabric.
    pub fn set_paired(&self, paired: bool) {
        self.paired.store(paired, Ordering::Release);
    }

    /// Returns true if the Matter node is commissioned with an active controller fabric.
    pub fn is_paired(&self) -> bool {
        self.paired.load(Ordering::Acquire)
    }

    /// Mark the DoorLock cluster as subscription-primed.
    pub fn mark_door_lock_ready(&self) {
        self.door_lock_ready.store(true, Ordering::Release);
        if self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
            self.state_sender.operational();
        }
    }

    /// Mark the Occupancy/Ring cluster as subscription-primed.
    pub fn mark_ring_ready(&self) {
        self.ring_ready.store(true, Ordering::Release);
        if self.is_ready() {
            info!("all clusters ready -> publishing Operational state");
            self.state_sender.operational();
        }
    }

    /// Returns true if the DoorLock cluster has been subscription-primed.
    pub fn is_door_lock_ready(&self) -> bool {
        self.door_lock_ready.load(Ordering::Acquire)
    }

    /// Returns true if the Occupancy/Ring cluster has been subscription-primed.
    pub fn is_ring_ready(&self) -> bool {
        self.ring_ready.load(Ordering::Acquire)
    }

    /// Reset cluster subscription readiness state on connection loss or reconnect.
    pub fn reset(&self) {
        self.door_lock_ready.store(false, Ordering::Release);
        self.ring_ready.store(false, Ordering::Release);
    }

    /// Returns true if all clusters have been subscription-primed by the controller.
    pub fn is_ready(&self) -> bool {
        self.is_door_lock_ready() && self.is_ring_ready()
    }
}

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

In `crates/intercom-adapter-matter/src/controller.rs`, pass `readiness: &'static ClusterReadinessBarrier` into `MatterController::new` and `ConnectionMonitor`:
- After `stack.startup(&crypto, &mut kv)`:
  ```rust
  let is_commissioned = has_active_controller_fabric(stack.matter());
  self.readiness.set_paired(is_commissioned);
  if is_commissioned {
      self.state_sender.wifi_connecting();
  } else {
      self.state_sender.commissioning();
  }
  ```
- In `ConnectionMonitor::run`:
  - Update `self.readiness.set_paired(is_commissioned)`.
  - When `is_operational && !was_operational`:
    ```rust
    if is_commissioned {
        if self.readiness.is_ready() {
            self.state_sender.operational();
        } else {
            self.state_sender.ecosystem_connecting();
        }
    }
    ```
  - When `!is_operational && was_operational`:
    ```rust
    self.readiness.reset();
    if is_commissioned {
        self.state_sender.wifi_connecting();
    } else {
        self.state_sender.commissioning();
    }
    ```
  - When `!was_commissioned && is_commissioned && is_operational`:
    ```rust
    if self.readiness.is_ready() {
        self.state_sender.operational();
    } else {
        self.state_sender.ecosystem_connecting();
    }
    ```

- [ ] **Step 3: Commit Task 3**

```bash
git add crates/intercom-adapter-gpio/ crates/intercom-adapter-matter/
git commit -m "feat(matter,gpio): wire live Matter ecosystem status and updated SystemState transitions"
```

---

### Task 4: Composition Root Wiring (`intercom-firmware`)

**Files:**
- Delete: `crates/intercom-firmware/src/ecosystem_info.rs`
- Modify: `crates/intercom-firmware/src/config.rs`
- Modify: `crates/intercom-firmware/src/lib.rs`
- Modify: `crates/intercom-firmware/src/main.rs`

**Interfaces:**
- Consumes: `MATTER_ECOSYSTEM`, `ClusterReadinessBarrier`, `MatterController` from `intercom-adapter-matter`; `EcosystemId`, `EcosystemType`, `DiagnosticsService` from `intercom-core`
- Produces: `SupportedEcosystem`, `AVAILABLE_ECOSYSTEMS` in `intercom-firmware::config`, compiled `intercom-bridge` binary

- [ ] **Step 1: Add `SupportedEcosystem` and `AVAILABLE_ECOSYSTEMS` to `crates/intercom-firmware/src/config.rs` and remove `ecosystem_info.rs`**

In `crates/intercom-firmware/src/config.rs`, add:

```rust
// ============================================================================
// Smart Home Ecosystems
// ============================================================================

use intercom_adapter_matter::MATTER_ECOSYSTEM;
use intercom_core::domain::{EcosystemId, EcosystemType};

/// Smart home ecosystems supported by this product configuration.
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

/// Ecosystems exposed in system diagnostics and the web dashboard.
pub const AVAILABLE_ECOSYSTEMS: &[SupportedEcosystem] = &[
    SupportedEcosystem::Matter,
    SupportedEcosystem::HomeAssistant,
];
```

Delete `crates/intercom-firmware/src/ecosystem_info.rs` and remove `pub mod ecosystem_info;` / `mod ecosystem_info;` from `crates/intercom-firmware/src/lib.rs` and `crates/intercom-firmware/src/main.rs`.

- [ ] **Step 2: Update `crates/intercom-firmware/src/main.rs` to inject `readiness` and `config::AVAILABLE_ECOSYSTEMS`**

In `crates/intercom-firmware/src/main.rs`:
- Pass `readiness` to `MatterController::new(..., state_sender, readiness, FirmwareResetDriver)`
- Wire `DiagnosticsService`:
  ```rust
  let hardware_info = FirmwareHardwareInfo;
  let diagnostics_service = DiagnosticsService::with_ecosystem(
      &hardware_info,
      readiness,
      config::AVAILABLE_ECOSYSTEMS,
      &STATE_WATCH,
  );
  ```

- [ ] **Step 3: Run all host unit tests and build the embedded RISC-V firmware binary**

Run:
```bash
mise exec -- cargo test -p intercom-core -p intercom-adapter-ota -p intercom-adapter-http --target x86_64-apple-darwin
mise exec -- cargo build -p intercom-firmware
```
Expected: All host unit tests PASS and `intercom-firmware` compiles with zero errors or warnings on `riscv32imac-unknown-none-elf`.

- [ ] **Step 4: Commit Task 4**

```bash
git rm crates/intercom-firmware/src/ecosystem_info.rs
git add crates/intercom-firmware/
git commit -m "feat(firmware): define SupportedEcosystem in composition root and wire live Matter readiness"
```
