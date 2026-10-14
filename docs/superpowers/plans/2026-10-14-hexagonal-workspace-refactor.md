# Intercom Bridge: Hexagonal Architecture Workspace Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor and rename the project into **`intercom-bridge`**, adopting a Hexagonal Architecture (Ports & Adapters) virtual Cargo workspace consisting of 4 dependency-isolated crates (`intercom-core`, `intercom-adapter-gpio`, `intercom-adapter-matter`, `intercom-firmware`), producing the `intercom-bridge` binary and enabling host-based unit testing of universal domain logic.

**Architecture:**
- **`intercom-core`**: Universal, hardware-agnostic and protocol-agnostic domain orchestrator (state machines, event bus, actors, and abstract port traits in `intercom_core::ports`). Zero hardware or network dependencies; fully testable on host machine.
- **`intercom-adapter-gpio`**: Intercom hardware adapter for physical intercom lines (IR204E) via `esp-hal` GPIO pins and flash storage.
- **`intercom-adapter-matter`**: Smart home protocol adapter binding core ports and events to the Matter data model (`rs-matter` + `rs-matter-embassy`).
- **`intercom-firmware`**: Composition root binary producing the `intercom-bridge` executable on ESP32-C6, wiring the GPIO adapter and Matter adapter into `intercom_core::App`.

**Tech Stack:** Rust (`no_std`, nightly `riscv32imac-unknown-none-elf`), Embassy runtime, `esp-hal`, `esp-radio`, `rs-matter`, `rs-matter-embassy`.

## Global Constraints
- Target architecture for firmware: `riscv32imac-unknown-none-elf`.
- Toolchain: Nightly Rust via `mise exec -- cargo ...`.
- Public before private and high-level abstractions above low-level details in all Rust files.
- All tests must strictly follow the **Arrange → Act → Assert** pattern.
- Adapters must NOT depend on `EventBus`; they communicate via sync callbacks or port trait implementations.
- Preserve all existing flash partition addresses, timing constants, and Matter vendor/device info.

---

### Task 1: Initialize Virtual Workspace Root Configuration

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/.gitkeep`

**Interfaces:**
- Consumes: Existing dependency specifications from root `Cargo.toml`.
- Produces: Root virtual `[workspace]` declaring `members = ["crates/*"]` and `default-members = ["crates/intercom-firmware"]`.

- [ ] **Step 1: Convert root `Cargo.toml` into a virtual workspace manifest**

```toml
[workspace]
members = [
    "crates/intercom-core",
    "crates/intercom-adapter-gpio",
    "crates/intercom-adapter-matter",
    "crates/intercom-firmware",
]
default-members = ["crates/intercom-firmware"]
resolver = "2"

[patch.crates-io]
rs-matter = { path = "../rs-matter/rs-matter" }
rs-matter-stack = { path = "../rs-matter-stack" }

[profile.dev]
opt-level = "z"

[profile.release]
codegen-units = 1
debug = 2
debug-assertions = false
opt-level = "s"
overflow-checks = false
```

- [ ] **Step 2: Create directory tree under `crates/`**
```bash
mkdir -p crates/intercom-core/src/ports
mkdir -p crates/intercom-core/tests
mkdir -p crates/intercom-adapter-gpio/src
mkdir -p crates/intercom-adapter-matter/src
mkdir -p crates/intercom-firmware/src
```

- [ ] **Step 3: Verify workspace parsing with cargo metadata**
```bash
mise exec -- cargo check --workspace || true
```

- [ ] **Step 4: Commit workspace scaffolding**
```bash
git add Cargo.toml crates/
git commit -m "chore: scaffold intercom-bridge virtual cargo workspace"
```

---

### Task 2: Extract `intercom-core` (Universal Domain, Actors, Events, Ports) & Host Unit Tests

**Files:**
- Create: `crates/intercom-core/Cargo.toml`
- Create: `crates/intercom-core/src/lib.rs`
- Create: `crates/intercom-core/src/config.rs`
- Create: `crates/intercom-core/src/events.rs`
- Create: `crates/intercom-core/src/app.rs`
- Create: `crates/intercom-core/src/ports/mod.rs`
- Create: `crates/intercom-core/src/actors/mod.rs`
- Create: `crates/intercom-core/src/actors/ring.rs`
- Create: `crates/intercom-core/src/actors/door.rs`
- Create: `crates/intercom-core/src/actors/indicator.rs`
- Create: `crates/intercom-core/src/actors/access_control.rs`
- Create: `crates/intercom-core/src/actors/factory_reset.rs`
- Create: `crates/intercom-core/src/actors/matter_controller.rs`
- Test: `crates/intercom-core/tests/actor_tests.rs`

**Interfaces:**
- Consumes: `embassy-sync`, `embassy-time`, `embassy-futures`, `log`.
- Produces: `intercom_core::ports::{RingDriver, DoorDriver, IndicatorDriver, AccessControlDriver, FactoryResetDriver}`, `intercom_core::app::App`, `intercom_core::events::{Event, SystemState, EventBus, EventBusExt}`.

- [ ] **Step 1: Write `crates/intercom-core/Cargo.toml`**

```toml
[package]
name = "intercom-core"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
log = "0.4"
embassy-sync = "0.8"
embassy-time = { version = "0.5", features = ["generic-queue-8"] }
embassy-futures = "0.1"

[dev-dependencies]
embassy-time = { version = "0.5", features = ["generic-queue-8"] }
portable-atomic = "1"
```

- [ ] **Step 2: Define port traits in `crates/intercom-core/src/ports/mod.rs`**

```rust
//! Outbound port traits for Smart Intercom core.

use crate::events::SystemState;

pub trait RingDriver {
    async fn wait_for_ring(&mut self);
}

pub trait DoorDriver {
    async fn trigger_unlock(&mut self);
}

pub trait IndicatorDriver {
    async fn set_state(&mut self, state: SystemState);
    async fn indicate_ring(&mut self);
    async fn indicate_unlock(&mut self);
    async fn off(&mut self);
}

pub trait AccessControlDriver {
    fn update_door_state(&self, unlocked: bool);
    fn update_occupancy(&self, occupied: bool);
}

pub trait FactoryResetDriver {
    async fn wait_for_press(&mut self);
    async fn is_pressed(&mut self) -> bool;
}
```

- [ ] **Step 3: Move and adapt domain events, config, and actors**
Move `config.rs`, `events.rs`, `app.rs`, and actor implementations from `src/` to `crates/intercom-core/src/`. Update actors to import their traits from `crate::ports::*`.

- [ ] **Step 4: Write failing host unit test in `crates/intercom-core/tests/actor_tests.rs`**

```rust
use intercom_core::actors::door::DoorActor;
use intercom_core::events::{Event, EventBus};
use intercom_core::ports::DoorDriver;
use portable_atomic::{AtomicBool, Ordering};

struct MockDoorDriver {
    triggered: AtomicBool,
}

impl DoorDriver for &MockDoorDriver {
    async fn trigger_unlock(&mut self) {
        self.triggered.store(true, Ordering::SeqCst);
    }
}

#[test]
fn door_actor_triggers_driver_on_unlock_event() {
    // Arrange
    let bus = EventBus::new();
    let driver = MockDoorDriver { triggered: AtomicBool::new(false) };
    let mut actor = DoorActor::new(&driver);

    // Act
    bus.immediate_publisher().publish_immediate(Event::UnlockDispatched);

    // Assert
    // Verify driver was triggered
}
```

- [ ] **Step 5: Run host test to verify baseline**
```bash
cargo test -p intercom-core
```

- [ ] **Step 6: Complete actor tests following Arrange-Act-Assert and verify green**
```bash
cargo test -p intercom-core
```

- [ ] **Step 7: Commit `intercom-core`**
```bash
git add crates/intercom-core/
git commit -m "feat(core): extract intercom-core domain, actors, and ports with host tests"
```

---

### Task 3: Extract `intercom-adapter-gpio` (Hardware Adapters for IR204E)

**Files:**
- Create: `crates/intercom-adapter-gpio/Cargo.toml`
- Create: `crates/intercom-adapter-gpio/src/lib.rs`
- Move: `src/drivers/gpio/ring.rs` -> `crates/intercom-adapter-gpio/src/ring.rs`
- Move: `src/drivers/gpio/door.rs` -> `crates/intercom-adapter-gpio/src/door.rs`
- Move: `src/drivers/gpio/led.rs` -> `crates/intercom-adapter-gpio/src/led.rs`
- Move: `src/drivers/gpio/boot_button.rs` -> `crates/intercom-adapter-gpio/src/boot_button.rs`
- Move: `src/drivers/flash.rs` -> `crates/intercom-adapter-gpio/src/flash.rs`

**Interfaces:**
- Consumes: `intercom-core::ports::{RingDriver, DoorDriver, IndicatorDriver, FactoryResetDriver}`, `esp-hal`.
- Produces: `GpioRingDriver`, `GpioDoorDriver`, `GpioLedDriver`, `GpioBootButtonDriver`, `EspFlash`.

- [ ] **Step 1: Write `crates/intercom-adapter-gpio/Cargo.toml`**

```toml
[package]
name = "intercom-adapter-gpio"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
intercom-core = { path = "../intercom-core" }
log = "0.4"
embassy-time = { version = "0.5", features = ["generic-queue-8"] }
esp-hal = { version = "~1.2", features = ["log-04", "unstable", "esp32c6"] }
embedded-storage = "0.3"
```

- [ ] **Step 2: Implement port traits in GPIO drivers**
Ensure:
- `GpioRingDriver` implements `intercom_core::ports::RingDriver`.
- `GpioDoorDriver` implements `intercom_core::ports::DoorDriver`.
- `GpioLedDriver` implements `intercom_core::ports::IndicatorDriver`.
- `GpioBootButtonDriver` implements `intercom_core::ports::FactoryResetDriver`.

- [ ] **Step 3: Verify crate builds for RISC-V target**
```bash
mise exec -- cargo build -p intercom-adapter-gpio --target riscv32imac-unknown-none-elf
```

- [ ] **Step 4: Commit `intercom-adapter-gpio`**
```bash
git add crates/intercom-adapter-gpio/
git commit -m "feat(gpio): extract intercom-adapter-gpio hardware adapters"
```

---

### Task 4: Extract `intercom-adapter-matter` (Smart Home Matter Protocol Adapters)

**Files:**
- Create: `crates/intercom-adapter-matter/Cargo.toml`
- Create: `crates/intercom-adapter-matter/src/lib.rs`
- Move: `src/drivers/matter/door_lock.rs` -> `crates/intercom-adapter-matter/src/door_lock.rs`
- Move: `src/drivers/matter/occupancy.rs` -> `crates/intercom-adapter-matter/src/occupancy.rs`
- Move: `src/drivers/matter/access_control.rs` -> `crates/intercom-adapter-matter/src/access_control.rs`
- Move: `src/drivers/matter/controller.rs` -> `crates/intercom-adapter-matter/src/controller.rs`
- Move: `src/drivers/matter/commissioning.rs` -> `crates/intercom-adapter-matter/src/commissioning.rs`

**Interfaces:**
- Consumes: `intercom-core::ports::AccessControlDriver`, `rs-matter`, `rs-matter-embassy`, `esp-radio`.
- Produces: `IntercomDoorLock`, `IntercomOccupancy`, `MatterAccessControlDriver`, `MatterController`.

- [ ] **Step 1: Write `crates/intercom-adapter-matter/Cargo.toml`**

```toml
[package]
name = "intercom-adapter-matter"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
intercom-core = { path = "../intercom-core" }
log = "0.4"
portable-atomic = "1"
rs-matter = { version = "0.4", default-features = false }
rs-matter-embassy = { path = "../../../rs-matter-embassy/rs-matter-embassy", default-features = false, features = ["esp", "trouble", "embassy-net", "log", "rustcrypto", "kv-blob-store-8192", "mld-v1-compat", "nd-learn-from-frames"] }
esp-radio = { version = "1.0.0-beta.1", features = ["ble", "coex", "log-04", "unstable", "esp32c6"] }
esp-hal = { version = "~1.2", features = ["log-04", "unstable", "esp32c6"] }
esp-rtos = { version = "0.4", features = ["esp-radio", "embassy", "esp32c6"] }
esp-alloc = "0.11"
embedded-storage = "0.3"
embedded-storage-async = "0.4"
static_cell = "2"
tinyrlibc = { version = "0.5", default-features = false, features = ["utoa", "strtoul"] }
```

- [ ] **Step 2: Implement `AccessControlDriver` port in `MatterAccessControlDriver`**
Ensure `MatterAccessControlDriver` implements `intercom_core::ports::AccessControlDriver`.

- [ ] **Step 3: Run commissioning unit tests in `commissioning.rs`**
```bash
mise exec -- cargo test -p intercom-adapter-matter
```

- [ ] **Step 4: Verify crate builds for RISC-V target**
```bash
mise exec -- cargo build -p intercom-adapter-matter --target riscv32imac-unknown-none-elf
```

- [ ] **Step 5: Commit `intercom-adapter-matter`**
```bash
git add crates/intercom-adapter-matter/
git commit -m "feat(matter): extract intercom-adapter-matter protocol adapters"
```

---

### Task 5: Create `intercom-firmware` (Composition Root & `intercom-bridge` Binary)

**Files:**
- Create: `crates/intercom-firmware/Cargo.toml`
- Move: `src/main.rs` -> `crates/intercom-firmware/src/main.rs`
- Move: `src/utils/` -> `crates/intercom-firmware/src/utils/`
- Delete: `src/` (from project root)

**Interfaces:**
- Consumes: `intercom-core`, `intercom-adapter-gpio`, `intercom-adapter-matter`, target runtime crates.
- Produces: `intercom-bridge` binary executable.

- [ ] **Step 1: Write `crates/intercom-firmware/Cargo.toml` producing binary `intercom-bridge`**

```toml
[package]
name = "intercom-firmware"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[[bin]]
name = "intercom-bridge"
path = "src/main.rs"
harness = false

[dependencies]
intercom-core = { path = "../intercom-core" }
intercom-adapter-gpio = { path = "../intercom-adapter-gpio" }
intercom-adapter-matter = { path = "../intercom-adapter-matter" }

log = "0.4"
embassy-executor = "0.10"
embassy-futures = "0.1"
esp-backtrace = { version = "0.20", features = ["panic-handler", "println", "custom-halt", "esp32c6"] }
esp-hal = { version = "~1.2", features = ["log-04", "unstable", "exception-handler", "esp32c6"] }
esp-rtos = { version = "0.4", features = ["esp-radio", "embassy", "esp32c6"] }
esp-alloc = "0.11"
esp-println = { version = "0.18", features = ["log-04", "esp32c6"] }
esp-bootloader-esp-idf = { version = "0.6", features = ["log-04", "esp32c6"] }
esp-metadata-generated = { version = "0.5", features = ["esp32c6"] }
portable-atomic = { version = "1", features = ["critical-section"] }
rs-matter-embassy = { path = "../../../rs-matter-embassy/rs-matter-embassy", default-features = false, features = ["esp", "trouble", "embassy-net", "log", "rustcrypto", "kv-blob-store-8192", "mld-v1-compat", "nd-learn-from-frames"] }
```

- [x] **Step 2: Update `main.rs` imports to use the adapter boundaries**
```rust
use intercom_core::app::App;
use intercom_core::config;
use intercom_core::events::{Event, EventBus, EventBusExt};
use intercom_adapter_gpio::{
    EspFlash, GpioBootButtonConfig, GpioBootButtonDriver, GpioDoorConfig, GpioDoorDriver,
    GpioLedDriver, GpioRingConfig, GpioRingDriver,
};
use intercom_adapter_matter::{
    DeviceInfo, IntercomDoorLock, IntercomDoorLockConfig, IntercomOccupancy,
    IntercomOccupancyConfig, MatterAccessControlDriver, MatterController, MatterControllerConfig,
    MatterPeripherals,
};
```

- [x] **Step 3: Remove old root `src/` directory**
```bash
rm -rf src/
```

- [x] **Step 4: Verify `intercom-bridge` builds for target**
```bash
mise exec -- cargo build -p intercom-firmware --target riscv32imac-unknown-none-elf
```

- [x] **Step 5: Commit `intercom-firmware`**
```bash
git add crates/intercom-firmware/ src/
git commit -m "feat(firmware): wire composition root in intercom-firmware and build intercom-bridge binary"
```

---

### Task 6: Update Tooling, Scripts, and Verification

**Files:**
- Modify: `.cargo/config.toml`
- Modify: `flash.sh`
- Modify: `espflash.toml`
- Modify: `AGENTS.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: Binary name `intercom-bridge`.
- Produces: Working developer runners, flash scripts, and documentation.

- [x] **Step 1: Update `.cargo/config.toml` to target `intercom-bridge` binary**
Ensure `cargo run` defaults to running `intercom-bridge`.

- [x] **Step 2: Update `flash.sh` and `espflash.toml`**
Update binary path to:
`target/riscv32imac-unknown-none-elf/debug/intercom-bridge`

- [x] **Step 3: Update `AGENTS.md` and `README.md`**
Update project name to `intercom-bridge` and document the new 4-crate architecture and commands.

- [x] **Step 4: Execute full test and build verification**
```bash
# 1. Host unit tests for core domain logic
cargo test -p intercom-core

# 2. Firmware compilation for ESP32-C6
mise exec -- cargo build -p intercom-firmware --target riscv32imac-unknown-none-elf
```

- [x] **Step 5: Commit tooling and documentation updates**
```bash
git add .cargo/config.toml flash.sh espflash.toml AGENTS.md README.md
git commit -m "docs: update project documentation and tooling for intercom-bridge"
```
