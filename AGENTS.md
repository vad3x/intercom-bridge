# AGENTS.md

Guidelines for AI agents working on the Smart Intercom codebase.

## Rust Usage

For accessing rust/cargo CLI, use `mise`. The project uses a nightly Rust toolchain targeting `riscv32imac-unknown-none-elf` (ESP32-C6 is RISC-V — no `espup` needed).

## Project Overview

**Intercom Bridge** — ESP32-C6 controller bridging analog intercom systems (IR204E) to smart home networks (Matter over Wi-Fi / Apple Home). Designed with Hexagonal Architecture (Ports & Adapters) to keep universal domain logic completely decoupled from hardware and protocol implementations.

**Tech Stack:**

- **Hardware:** ESP32-C6 (DFRobot Beetle)
- **Language:** Rust (`no_std`, bare-metal)
- **Architecture:** Hexagonal Architecture (Ports & Adapters) Virtual Workspace
- **Async Runtime:** Embassy (replaces FreeRTOS)
- **HAL:** `esp-hal` + `esp-radio` (WiFi/BLE)
- **Matter:** `rs-matter` + `rs-matter-embassy` (bare-metal Matter over WiFi)
- **Protocol:** Matter over WiFi (BLE for commissioning)


## Build, Test & Flash

```bash
# Run host unit tests (runs in ~1s on macOS/Linux without hardware)
cargo test -p intercom-core --target x86_64-apple-darwin

# Build firmware binary (intercom-bridge)
mise exec -- cargo build -p intercom-firmware

# Build + flash + monitor (via cargo runner in .cargo/config.toml)
mise exec -- cargo run

# Flash directly to specific port
espflash flash target/riscv32imac-unknown-none-elf/debug/intercom-bridge --port /dev/cu.usbmodem14201
```

The `.cargo/config.toml` configures the RISC-V target (`riscv32imac-unknown-none-elf`), `build-std`, and the `espflash` runner. USB ports are typically `/dev/cu.usbmodem*` (native ESP32-C6 USB) or `/dev/cu.usbserial*` (UART bridge).

## Project Structure

```
intercom-bridge/
├── crates/
│   ├── intercom-core/              # Pure no_std universal domain logic (host-testable)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── app.rs              # App coordinator generic over port traits
│   │   │   ├── config.rs           # Domain timing and indicator constants
│   │   │   ├── events.rs           # Event enum + PubSubChannel event bus
│   │   │   ├── ports/              # Abstract port traits (RingDriver, DoorDriver, etc.)
│   │   │   └── actors/             # RingActor, FactoryResetActor, AccessController
│   │   └── tests/                  # Host unit tests with mock drivers
│   ├── intercom-adapter-gpio/      # Hardware edge adapters (esp-hal)
│   │   └── src/
│   │       ├── config.rs           # Electrical timings and LED blink patterns
│   │       ├── ring.rs             # GpioRingDriver (optocoupler, debounce)
│   │       ├── door.rs             # GpioDoorDriver (PhotoMOS relay, timed pulse)
│   │       ├── led.rs              # GpioLedDriver (active-low LED, blink patterns)
│   │       ├── boot_button.rs      # GpioBootButtonDriver (maintenance button)
│   │       └── flash.rs            # EspFlash (ROM SPI flash persistence)
│   ├── intercom-adapter-matter/    # Smart home protocol adapters (rs-matter)
│   │   └── src/
│   │       ├── config.rs           # Matter endpoint IDs and serial format
│   │       ├── door_lock.rs        # IntercomDoorLock (DoorLock cluster handler)
│   │       ├── occupancy.rs        # IntercomOccupancy (OccupancySensing cluster handler)
│   │       ├── access_control.rs   # MatterAccessControlDriver (bridges actors ↔ clusters)
│   │       ├── commissioning.rs    # Commissioning helpers
│   │       └── controller.rs       # MatterController builder + runner (WiFi stack)
│   └── intercom-firmware/          # Composition root binary (intercom-bridge)
│       └── src/
│           ├── config.rs           # Hardware version, device info, flash partition layout
│           ├── main.rs             # Hardware init, peripheral allocation, joins actors
│           ├── version.rs          # Git and compile-time software versioning
│           └── utils/
│               └── uptime_logger.rs# Custom HH:MM:SS.mmm logger
├── GLOSSARY.md                     # Ubiquitous domain vocabulary
├── Cargo.toml                      # Virtual workspace root manifest
├── rust-toolchain.toml             # Nightly toolchain + riscv32imac target
├── .cargo/config.toml              # Build target, runner, build-std
└── partitions.csv                  # Flash partition layout
```

## Key Files to Understand

| File | Purpose |
|------|---------|
| `crates/intercom-firmware/src/main.rs` | Composition root: hardware init, creates channels & `SystemStateWatch`, instantiates adapters, wires them into `App` + `MatterController`, runs via `join` |
| `crates/intercom-core/src/domain/access.rs` | `AccessStateMachine` — pure, deterministic domain state machine (zero async/runtime dependencies) |
| `crates/intercom-core/src/actors/access_controller.rs` | `AccessController` — deep application use case orchestrating strike relay, auto-relock, and ring announcements |
| `crates/intercom-core/src/ports/inbound.rs` | `AccessControllerHandle` & `CommandChannel` — idiomatic Inbound Port handle for driving access commands |
| `crates/intercom-core/src/ports/reporter.rs` | `IntercomReporter` & `LockState` — protocol-agnostic domain reporting port |
| `crates/intercom-core/src/ports/reset.rs` | `SystemResetDriver` — outbound port for credential erasure and system reboot |
| `crates/intercom-core/src/ports/` | Abstract outbound port traits (`DoorDriver`, `RingDriver`, `IntercomReporter`, `FactoryResetDriver`, `SystemResetDriver`) |
| `crates/intercom-core/src/events.rs` | `SystemState` enum, `SystemStateWatch` (`Watch`), `SystemStateSender`, `SystemStateReceiver` |
| `crates/intercom-core/src/config.rs` | Domain access, relock, and reset timing policies (protocol- and hardware-agnostic) |
| `crates/intercom-adapter-matter/src/config.rs` | Matter endpoint IDs and serial number formatting |
| `crates/intercom-firmware/src/config.rs` | Hardware version, product identification, and flash memory layout |
| `crates/intercom-adapter-matter/src/readiness.rs` | `ClusterReadinessBarrier` — internal Apple Home subscription readiness coordination |
| `crates/intercom-adapter-matter/src/controller.rs` | `MatterController` — builder pattern for registering clusters + running the Matter stack |
| `crates/intercom-adapter-matter/src/door_lock.rs` | `IntercomDoorLock` — Matter DoorLock cluster, triggers `AccessControllerHandle` on unlock |
| `crates/intercom-adapter-matter/src/occupancy.rs` | `IntercomOccupancy` — Matter OccupancySensing cluster |
| `crates/intercom-adapter-matter/src/access_control.rs` | `MatterAccessControlDriver` — implements `IntercomReporter` port |
| `crates/intercom-adapter-gpio/src/` | GPIO implementations of port traits using `esp-hal` |

## Architecture Patterns

### Inbound Actor Handle Pattern
Inbound commands to the core domain flow through the **`AccessControllerHandle`**:
- Value semantics: cheap `Copy`, owns no heap memory, zero lifetime annotations in adapters.
- Encapsulates non-blocking bounded channel dispatch (`AccessCommand::{Unlock, Ring}`).
- Eliminates raw closure boilerplate in the composition root (`main.rs`).

```rust
// Adapters hold the handle directly:
let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
    access: access_handle,
    readiness: &readiness_barrier,
});
```

### Actor Model with Directed Channels & Lifecycle Watch
Actors communicate via Embassy channels and continuous state watch:
- **`CommandChannel`**: Point-to-point MPSC command channel driving `AccessController`.
- **`SystemStateWatch`**: Continuous lifecycle broadcast observed by edge adapters (e.g. `GpioLedDriver`) without queue overflow risk.
- **Direct Port Effects**: `AccessController` directly reports domain transitions to `IntercomReporter::report_ring()` and `IntercomReporter::report_lock_state()`. Edge adapters (such as `GpioLedDriver` or `MatterAccessControlDriver`) compose via tuples `(access_driver, indicator)` without intermediate core actors.

### Trait-Based Drivers

Hardware drivers implement async traits, making actors generic and testable:

```rust
pub trait RingDriver {
    async fn wait_for_ring(&mut self);
}

pub trait DoorDriver {
    async fn trigger_unlock(&mut self);
}

pub trait IntercomReporter {
    fn report_ring(&self, ringing: bool);
    fn report_lock_state(&self, state: LockState);
}

pub trait SystemResetDriver {
    fn erase_credentials(&self);
    fn reboot(&self) -> !;
}
```

`App<R, D, REP, F, RST>` is generic over these traits.

### Matter Cluster Handlers

Cluster handlers implement `rs_matter::dm::AsyncHandler` and the `MatterCluster` trait. They are registered with `MatterController` via a builder pattern:

```rust
let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
    access: access_handle,
    readiness: &readiness_barrier,
});
let occupancy = IntercomOccupancy::new(IntercomOccupancyConfig {
    readiness: &readiness_barrier,
});

let matter = MatterController::new(
    config,
    state_sender,
    FirmwareResetDriver,
)
.register(&door_lock)
.register(&occupancy);

matter.run().await;
```

### Matter Endpoints

- **Endpoint 0:** Root node (system clusters, handled by rs-matter-embassy)
- **Endpoint 1:** Door Lock Cluster (lock/unlock commands)
- **Endpoint 2:** Occupancy Sensor (ring detection, appears as Motion Sensor in HomeKit)

## Common Tasks

### Adding a New GPIO Driver

1. Define an async trait in `crates/intercom-core/src/ports/`
2. Create implementation in `crates/intercom-adapter-gpio/src/`
3. Add the new actor in `crates/intercom-core/src/actors/`
4. Wire into `App` struct in `crates/intercom-core/src/app.rs`
5. Instantiate with concrete GPIO pin in `crates/intercom-firmware/src/main.rs`

### Adding a New Matter Cluster

1. Add the cluster to `rs_matter::import!()` in `crates/intercom-adapter-matter/`
2. Create a handler struct in `crates/intercom-adapter-matter/src/` implementing `AsyncHandler` + `MatterCluster`
3. Wire to domain ports or inbound handles (`AccessControllerHandle`, `SystemStateSender`) — **never use raw closures or callbacks**
4. Register with `MatterController::register()` in `crates/intercom-firmware/src/main.rs`

### Changing GPIO Pins

Edit `crates/intercom-firmware/src/main.rs` — pins are selected via ESP-HAL peripheral tokens:

```rust
let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
let door = GpioDoorDriver::new(GpioDoorConfig::new(peripherals.GPIO18));
let indicator = INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));
```

### Changing Timing Constants

- **Domain Access & Lifecycle Policies:** Edit `crates/intercom-core/src/config.rs`:
  ```rust
  pub const RING_HOLD_MS: u64 = 15000;
  pub const AUTO_RELOCK_SEC: u64 = 4;
  pub const FACTORY_RESET_REBOOT_DELAY_MS: u64 = 2000;
  ```

- **Hardware GPIO & Electrical Actuation Timings:** Edit `crates/intercom-adapter-gpio/src/config.rs`:
  ```rust
  pub const UNLOCK_PULSE_MS: u64 = 5000;
  pub const RING_DEBOUNCE_MS: u64 = 30;
  pub const RING_COOLDOWN_MS: u64 = 2500;
  pub const FACTORY_RESET_HOLD_MS: u64 = 5000;
  ```

## Code Style

- `no_std` + `alloc` — no standard library
- Use `log::info!`, `log::warn!`, `log::error!` for logging (via `esp-println`)
- All drivers and actors are async (Embassy)
- **Zero callbacks/closures across adapters and core** — communication flows through Inbound Handles (`AccessControllerHandle`, `SystemStateSender`) and Outbound Traits (`SystemResetDriver`, `DoorDriver`, `RingDriver`, `IntercomReporter`)
- Traits for hardware abstraction, generics for composition

## Coding Standards

### Test Structure — Arrange / Act / Assert

All tests MUST follow the **Arrange → Act → Assert** pattern. Separate each phase with a blank line and a `// Act` / `// Assert` comment for clarity:

```rust
#[test]
fn door_driver_triggers_unlock() {
    // Arrange
    let mut door = MockDoorDriver::new();

    // Act
    door.trigger_unlock();

    // Assert
    assert!(door.was_triggered());
}
```

### Public Before Private

Within any file, **public items MUST appear above private items**. This applies to methods, functions, structs, enums, traits, and impls:

- `pub` functions / methods first, then private helpers
- `pub struct` / `pub enum` / `pub trait` definitions first, then internal types
- In `impl` blocks: `pub fn` methods first, then `fn` (private) methods

```rust
// ✅ Correct — public API on top, private helpers below
pub struct RingActor { /* ... */ }

impl RingActor {
    pub fn new(driver: impl RingDriver) -> Self { /* ... */ }
    pub async fn run(&mut self) { /* ... */ }

    fn debounce(&self) -> bool { /* ... */ }
}

struct InternalState { /* ... */ }
```

### High-Level Abstractions Above Low-Level Details

Within a single file, order items from **highest-level abstraction to lowest-level detail**:

1. Main public struct / trait (the primary purpose of the file)
2. `impl` blocks for the main struct
3. Supporting public types (config structs, error enums)
4. Private helper types and functions
5. Trait implementations for external traits (e.g., `AsyncHandler`)
6. `#[cfg(test)]` module at the very bottom

```rust
// ✅ Correct ordering within a file

// 1. Primary trait (highest-level abstraction)
pub trait DoorDriver {
    async fn trigger_unlock(&mut self);
}

// 2. Main struct
pub struct GpioDoorDriver { /* ... */ }

// 3. Main impl
impl GpioDoorDriver {
    pub fn new(pin: GpioPin, config: GpioDoorConfig) -> Self { /* ... */ }
}

// 4. Supporting config type
pub struct GpioDoorConfig { /* ... */ }

// 5. Trait impl (lower-level detail)
impl DoorDriver for GpioDoorDriver {
    async fn trigger_unlock(&mut self) { /* ... */ }
}

// 6. Tests at the bottom
#[cfg(test)]
mod tests { /* ... */ }
```

## Debugging & Monitoring

### Non-Intrusive Serial Monitor (Recommended)

> [!WARNING]
> Standard tools like `screen`, `picocom`, and `espflash monitor` toggle DTR/RTS upon opening, which **reboots the ESP32-C6** and wipes in-memory Matter state.

Use the custom `scripts/monitor.py` script to monitor serial output without triggering a chip reset:

```bash
# Monitor live output WITHOUT resetting the running chip
python3 scripts/monitor.py --no-reset

# Optional: target a specific serial port or baud rate
python3 scripts/monitor.py --port /dev/cu.usbmodem14201 --no-reset

# All logs are automatically mirrored to:
tail -f /tmp/intercom_serial.log
```

If you *do* want to trigger a reboot upon attaching, simply omit `--no-reset`:

```bash
python3 scripts/monitor.py
```

### Log Levels & Subsystems

Adjust logging level in `crates/intercom-firmware/src/main.rs`:

```rust
// In crates/intercom-firmware/src/main.rs:
uptime_logger::init(log::LevelFilter::Debug); // Options: Info, Debug, Trace
```

Key subsystems to monitor during debugging:
- **Matter Exchange & MRP:** `rs_matter::transport::exchange` (`Too many retransmissions`, `Ack`, `Msg`)
- **Wi-Fi Scanning & Association:** `wifi: Scan completed`, `wifi: AP[...]`, `wifi: Connected`, `DisconnectedInfo`
- **BLE PASE & Commissioning:** `Starting BLE`, `PASE complete`, `CommissioningComplete`
- **App Actors & Edge Adapters:** `RingActor`, `AccessController`, `GpioLedDriver` transitions

---

## Resetting the Device

### 1. Soft Reboot (Preserves Flash & Fabric Credentials)
- Tap the physical **RST** button on the DFRobot Beetle board, or
- Run `python3 scripts/monitor.py` (asserts RTS/DTR reset sequence).

### 2. Reset Matter Commissioning (Re-Pairing without Reflashing)
To wipe pairing credentials so the device returns to an uncommissioned state (starts BLE advertising again on boot) **without** deleting the flashed firmware:

```bash
espflash erase-parts --port /dev/cu.usbmodem14201 matter_kvs nvs
```

Then reboot the board.

### 3. Enter Hardware Download Mode (Bootloader)
If flashing hangs or native USB fails to enumerate:
1. Press and hold **BOOT** button (GPIO9).
2. Press and release **RST** button (EN).
3. Release **BOOT** button.
4. The device is now in ROM bootloader mode ready for flashing.

### 4. Full Flash Erase (Hard Factory Reset)
To wipe all partitions (bootloader, partitions, app, NVS, and Matter state):

```bash
espflash erase-flash --port /dev/cu.usbmodem14201
# Or UART port if native USB is unresponsive:
espflash erase-flash --port /dev/cu.usbserial-210
```
*Note: A complete re-flash (`mise exec -- cargo run`) is required after this.*

---

## Troubleshooting & Common Issues

| Issue | Solution |
|-------|----------|
| Device reboots whenever serial monitor opens | Use `python3 scripts/monitor.py --no-reset` to avoid DTR/RTS reset |
| Build fails with linker errors | Check `Cargo.toml` patch overrides, run `cargo clean` |
| `espflash` can't find port | Check `ls /dev/cu.usb*`, pass `--port /dev/cu.usbmodem14201` |
| Device not enumerating on USB | Enter download mode (BOOT+RST) or switch to UART bridge |
| Apple Home "Unable to add" | See [docs/commissioning_troubleshooting.md](docs/commissioning_troubleshooting.md) for Omada/IPv6/Wi-Fi fixes |
| Device already paired elsewhere | Erase Matter credentials: `espflash erase-parts --port <PORT> matter_kvs nvs` |
| Stack overflow / alloc failure | Increase `HEAP_SIZE` in `crates/intercom-firmware/src/main.rs` |
| Commissioning unreliable | Increase `BUMP_SIZE` in `crates/intercom-adapter-matter/src/` |

## Dependencies

Key crates (see `Cargo.toml` for full list):

| Crate | Purpose |
|-------|---------|
| `esp-hal` | Hardware abstraction (GPIO, timers, peripherals) |
| `esp-radio` | WiFi + BLE radio drivers |
| `esp-rtos` | Embassy runtime integration for ESP32 |
| `rs-matter` | Matter protocol implementation (clusters, TLV, crypto) |
| `rs-matter-embassy` | Bare-metal Matter stack for Embassy (WiFi coex, commissioning) |
| `embassy-sync` | `PubSubChannel`, mutexes, signals |
| `embassy-time` | Async timers and delays |
| `embassy-futures` | `join`, `select` combinators |
| `portable-atomic` | Lock-free atomics for shared state |

Patched crates (via `[patch.crates-io]` in `Cargo.toml`):

- `rs-matter` → `sysgrok/rs-matter` `next` branch
- `rs-matter-stack` → `sysgrok/rs-matter-stack` `next` branch
- `openthread` → `sysgrok/openthread`

## Testing Changes

1. Build: `cargo build`
2. Flash + monitor: `cargo run`
3. Check serial logs for errors
4. Test with Aqara Home or chip-tool
