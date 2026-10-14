# AGENTS.md

Guidelines for AI agents working on the Smart Intercom codebase.

## Rust Usage

For accessing rust/cargo CLI, use `mise`. The project uses a nightly Rust toolchain targeting `riscv32imac-unknown-none-elf` (ESP32-C6 is RISC-V — no `espup` needed).

## Project Overview

ESP32-C6 Matter controller for IR204E analog intercom. Provides door lock control and ring detection via Apple Home (HomeKit) using the Matter protocol.

**Tech Stack:**

- **Hardware:** ESP32-C6 (DFRobot Beetle)
- **Language:** Rust (`no_std`, bare-metal)
- **Async Runtime:** Embassy (replaces FreeRTOS)
- **HAL:** `esp-hal` + `esp-radio` (WiFi/BLE)
- **Matter:** `rs-matter` + `rs-matter-embassy` (bare-metal Matter over WiFi)
- **Protocol:** Matter over WiFi (BLE for commissioning)

**Legacy C++ code** lives in `main/` (ESP-IDF + ESP-Matter SDK). The active codebase is Rust in `src/`.

## Build & Flash

```bash
# Build (from host — no Docker needed for Rust)
cargo build

# Build + flash + monitor (via cargo-espflash)
cargo run

# Flash only (uses .cargo/config.toml runner)
cargo-espflash flash --port /dev/cu.usbserial-210 --monitor
```

The `.cargo/config.toml` configures the RISC-V target, `build-std`, and the espflash runner.

## Project Structure

```
intercom-matter/
├── src/
│   ├── main.rs                     # Entry point, hardware init, callback wiring
│   ├── app.rs                      # App struct, wires actors + runs them
│   ├── config.rs                   # Timing constants, endpoint IDs
│   ├── events.rs                   # Event enum + PubSubChannel event bus
│   ├── utils/
│   │   ├── mod.rs
│   │   └── uptime_logger.rs        # Custom HH:MM:SS.mmm logger
│   ├── actors/
│   │   ├── mod.rs
│   │   ├── ring.rs                 # Publishes Ringing events from hardware
│   │   ├── door.rs                 # Subscribes to UnlockDispatched, triggers relay
│   │   ├── indicator.rs            # Subscribes to all events, drives status LED
│   │   └── access_control.rs       # Bridges events ↔ Matter state (occupancy, relock)
│   └── drivers/
│       ├── mod.rs                  # Re-exports driver traits from gpio/
│       ├── gpio/
│       │   ├── mod.rs              # Re-exports GPIO drivers
│       │   ├── ring.rs             # GpioRingDriver (optocoupler, debounce)
│       │   ├── door.rs             # GpioDoorDriver (PhotoMOS relay, timed pulse)
│       │   └── led.rs              # GpioLedDriver (active-low LED, blink patterns)
│       └── matter/
│           ├── mod.rs              # MatterCluster trait, cluster codegen, re-exports
│           ├── door_lock.rs        # IntercomDoorLock (DoorLock cluster handler)
│           ├── occupancy.rs        # IntercomOccupancy (OccupancySensing cluster handler)
│           ├── access_control.rs   # MatterAccessControlDriver (bridges actors ↔ clusters)
│           └── controller.rs       # MatterController builder+runner (WiFi stack)
├── main/                           # Legacy C++ codebase (ESP-IDF)
├── Cargo.toml                      # Dependencies, features, patch overrides
├── rust-toolchain.toml             # Nightly toolchain + riscv32imac target
├── .cargo/config.toml              # Build target, runner, build-std
├── partitions.csv                  # Flash partition layout
└── sdkconfig.defaults*             # Legacy ESP-IDF config (referenced for pin/timing values)
```

## Key Files to Understand

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point: hardware init, creates `EventBus` locally, wires driver callbacks to bus, runs `App` + `MatterController` via `join` |
| `src/app.rs` | `App<R, D, I, A>` generic over driver traits, owns actors, runs them via `join4` |
| `src/events.rs` | `Event` enum (`Ringing`, `UnlockDispatched`, `SystemState`), `SystemState` enum, `EventBus` (Embassy `PubSubChannel`) |
| `src/config.rs` | Timing constants, Matter endpoint IDs |
| `src/drivers/matter/controller.rs` | `MatterController` — builder pattern for registering clusters + running the Matter stack; accepts `on_connecting`/`on_connected` callbacks at construction |
| `src/drivers/matter/door_lock.rs` | `IntercomDoorLock` — Matter DoorLock cluster, calls `on_unlock` callback |
| `src/drivers/matter/occupancy.rs` | `IntercomOccupancy` — Matter OccupancySensing cluster |
| `src/drivers/matter/access_control.rs` | `MatterAccessControlDriver` — bridges actors ↔ Matter cluster state |
| `src/actors/indicator.rs` | `IndicatorActor` + `IndicatorDriver` trait — handles LED blink patterns including connecting blink |

## Architecture Patterns

### Actor Model with PubSubChannel

Actors are separate structs with async `run()` methods. They communicate through an Embassy `PubSubChannel` event bus (fan-out to all subscribers):

```rust
// RingActor publishes:
publisher.publish(Event::Ringing).await;

// DoorActor subscribes:
if let Event::UnlockDispatched = subscriber.next_message_pure().await { ... }
```

Events:

- `Event::Ringing` — Someone pressed the intercom button
- `Event::UnlockDispatched` — Unlock command received from Matter/HomeKit
- `Event::SystemState(state)` — Lifecycle state change (Initializing, Ready, Connecting, Connected, Error)

### Callback Pattern for Drivers

**Drivers MUST NOT depend on `EventBus`.** Instead, drivers accept sync callbacks (`&dyn Fn()`) for outbound notifications. `main.rs` wires callbacks to the event bus. This keeps drivers decoupled and testable:

```rust
// Driver accepts a callback (no EventBus dependency):
let on_unlock = || {
    event_bus.immediate_publisher()
        .publish_immediate(Event::UnlockDispatched);
};
let door_lock = IntercomDoorLock::new(&on_unlock);

// MatterController accepts connection state callbacks at construction:
let matter = MatterController::new(options, &on_connecting, &on_connected);
```

All event bus wiring happens exclusively in `main.rs`.

### Trait-Based Drivers

Hardware drivers implement async traits, making actors generic and testable:

```rust
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
```

`App<R: RingDriver, D: DoorDriver, I: IndicatorDriver, A: AccessControlDriver>` is generic over these traits.

### Matter Cluster Handlers

Cluster handlers implement `rs_matter::dm::AsyncHandler` and the `MatterCluster` trait. They are registered with `MatterController` via a builder pattern:

```rust
let door_lock = IntercomDoorLock::new(&on_unlock);
let occupancy = IntercomOccupancy::new();

let matter = MatterController::new(
    MatterControllerOptions { wifi, bt, rng, adc1 },
    &on_connecting,
    &on_connected,
)
.register(DOOR_LOCK_ENDPOINT_ID, &door_lock)
.register(OCCUPANCY_SENSOR_ENDPOINT_ID, &occupancy);

matter.run().await;
```

### Matter Endpoints

- **Endpoint 0:** Root node (system clusters, handled by rs-matter-embassy)
- **Endpoint 1:** Door Lock Cluster (lock/unlock commands)
- **Endpoint 2:** Occupancy Sensor (ring detection, appears as Motion Sensor in HomeKit)

## Common Tasks

### Adding a New GPIO Driver

1. Define an async trait in `src/actors/<name>.rs` (trait lives with the actor)
2. Create implementation in `src/drivers/gpio/<name>.rs`
3. Add the new actor in `src/actors/<name>.rs`
4. Wire into `App` struct in `src/app.rs`
5. Instantiate with concrete GPIO pin in `src/main.rs`

### Adding a New Matter Cluster

1. Add the cluster to `rs_matter::import!()` in `src/drivers/matter/mod.rs`
2. Create a handler struct in `src/drivers/matter/<name>.rs` implementing `AsyncHandler` + `MatterCluster`
3. Use callbacks (`&dyn Fn()`) for outbound events — **never import EventBus in drivers**
4. Register with `MatterController::register()` in `src/main.rs`
5. Wire callbacks to event bus in `src/main.rs`

### Changing GPIO Pins

Edit `src/main.rs` — pins are selected via ESP-HAL peripheral tokens:

```rust
let ring = GpioRingDriver::new(peripherals.GPIO2, GpioRingConfig::default());
let door = GpioDoorDriver::new(peripherals.GPIO18, GpioDoorConfig::default());
let indicator = GpioLedDriver::new(peripherals.GPIO19, GpioLedConfig::default());
```

### Changing Timing Constants

Edit `src/config.rs`:

```rust
pub const UNLOCK_PULSE_MS: u64 = 5000;
pub const RING_DEBOUNCE_MS: u64 = 30;
pub const RING_COOLDOWN_MS: u64 = 2500;
pub const RING_HOLD_MS: u64 = 15000;
pub const AUTO_RELOCK_SEC: u64 = 4;
```

## Code Style

- `no_std` + `alloc` — no standard library
- Use `log::info!`, `log::warn!`, `log::error!` for logging (via `esp-println`)
- All drivers and actors are async (Embassy)
- **Drivers use callbacks (`&dyn Fn()`) for outbound events — never `EventBus`**
- **Only `main.rs` wires callbacks to the event bus**
- Traits for hardware abstraction, generics for composition
- Actors receive `&EventBus` as `run()` parameter

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

## Debugging

### Serial Monitor

```bash
# Via cargo runner (build + flash + monitor)
cargo run

# Manual
screen /dev/cu.usbserial-210 115200
```

### Common Issues

| Issue | Solution |
|-------|----------|
| Build fails with linker errors | Check `Cargo.toml` patch overrides, run `cargo clean` |
| `espflash` can't find port | Check `ls /dev/cu.usbserial*`, update `.cargo/config.toml` runner |
| Device not enumerating on USB | Use UART port, enter download mode (BOOT+RESET) |
| Apple Home "Unable to add" | See [docs/commissioning_troubleshooting.md](docs/commissioning_troubleshooting.md) for Omada/IPv6/Wi-Fi fixes |
| Stack overflow / alloc failure | Increase `HEAP_SIZE` in `src/main.rs` |
| Commissioning unreliable | Increase `BUMP_SIZE` in `src/drivers/matter/mod.rs` |

## ESP32-C6 Recovery

If native USB stops working:

1. Connect to UART USB port (not native USB)
2. Check port: `ls /dev/cu.usbserial*`
3. Enter download mode: Hold BOOT → Tap RESET → Release BOOT
4. Erase flash: `espflash erase-flash --port /dev/cu.usbserial-210`

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
