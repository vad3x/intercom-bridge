# AGENTS.md

Guidelines for AI agents working on the Smart Intercom codebase.

## Rust Usage

For Rust and Cargo commands, use `mise`. The repositories pin Rust 1.99.1 and Bun 1.3.14 in `mise.toml`; the firmware target is `riscv32imac-unknown-none-elf` for ESP32-C6. `rust-analyzer` is pinned there too so editor and MCP integrations never resolve an uninstalled mise shim.

## Project Overview

**Intercom Bridge** — ESP32-C6 controller bridging analog intercom systems (IR204E) to smart home networks (Matter over Wi-Fi / Apple Home). Designed with Hexagonal Architecture (Ports & Adapters) to keep universal domain logic completely decoupled from hardware and protocol implementations.

**Tech Stack:**

- **Hardware:** ESP32-C6-WROOM-1-N8 (8 MB Quad SPI Flash) / DFRobot Beetle ESP32-C6
- **Language:** Rust (`no_std`, bare-metal)
- **Architecture:** Hexagonal Architecture (Ports & Adapters) Virtual Workspace
- **Async Runtime:** Embassy (replaces FreeRTOS)
- **HAL:** `esp-hal` + `esp-radio` (WiFi/BLE)
- **Matter:** `rs-matter` + `rs-matter-embassy` (bare-metal Matter over WiFi)
- **Protocol:** Matter over WiFi (BLE for commissioning)
- **OTA Updates:** HTTP streaming over Wi-Fi (port 8080) with Ed25519 signature verification and dual-bank bootloader rollback
- **Web Dashboard:** Embedded HTTP server (port 8080) serving pre-gzipped Solid.js + DaisyUI + Tailwind SPA and REST API

## Git Worktrees

This project is frequently worked on from isolated git worktrees (for example under `~/.codex/worktrees/`). A clean `git status` in the primary checkout does not mean changes are missing.

Before assuming work is absent, check the active checkout:

```bash
git rev-parse --show-toplevel
git worktree list
git status -sb
```

Open the worktree directory itself in the editor. Merge or remove completed worktrees at task boundaries so the primary checkout and the active branch do not drift.

## Build, Test & Flash

```bash
# Run host-capable tests on macOS without hardware
mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin
mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin
mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin

# Build debug firmware binary (intercom-bridge)
mise exec -- cargo build -p intercom-firmware

# Build optimized release firmware binary (recommended for OTA)
mise exec -- cargo build -p intercom-firmware --release

# Build + flash + monitor (via cargo runner in .cargo/config.toml)
# Note: Runner automatically passes `--partition-table partitions.csv --erase-parts otadata`
# to guarantee that newly flashed code in `ota_0` boots cleanly without A/B slot confusion.
mise exec -- cargo run

# Flash directly to specific port via USB (erases otadata so ota_0 boots without A/B confusion)
espflash flash target/riscv32imac-unknown-none-elf/debug/intercom-bridge --port /dev/cu.usbmodem14201 --partition-table partitions.csv --erase-parts otadata
```

### Viewing Commissioning QR & Pairing Codes

Display the Matter commissioning QR code and manual pairing codes from the connected ESP32-C6 device:

```bash
# Auto-detect connected USB device and show QR code + pairing codes:
cargo qr

# Or manually specify a MAC address without USB connection:
cargo qr --mac 98:a3:16:a6:78:44

# Or output raw JSON format:
cargo qr --json
```

The `.cargo/config.toml` configures the RISC-V target (`riscv32imac-unknown-none-elf`), `build-std`, and the `espflash` runner with `--erase-parts otadata`. Erasing `otadata` ensures that whenever firmware is flashed via USB, the 2nd-stage bootloader automatically boots the newly flashed binary in `ota_0`, preserving `nvs` (Wi-Fi) and `matter_kvs` (Matter credentials). USB ports are typically `/dev/cu.usbmodem*` (native ESP32-C6 USB) or `/dev/cu.usbserial*` (UART bridge).

## Frontend Web Dashboard (Solid.js + DaisyUI + Bun)

The embedded HTTP server serves a single-file dashboard (`assets/dashboard.html`). Root cargo aliases run all frontend tasks seamlessly from anywhere in the repository:

```bash
# Start local hot-reloading dev server (proxies /api to test board):
cargo fe-dev      # or `cargo fe dev`

# Run nonmutating TypeScript type checking and Biome linting:
cargo fe-check    # or `cargo fe check`

# Apply frontend formatting explicitly from the frontend directory:
bun run format

# Build single-file production asset for firmware (assets/dashboard.html):
cargo fe-build    # or `cargo fe build`

# Install frontend dependencies:
cargo fe-install  # or `cargo fe install`
```

### Remote Firmware Updates (OTA over Wi-Fi)

The Intercom Bridge supports secure, wireless Over-The-Air firmware updates over the local Wi-Fi network without requiring physical USB access.

#### 1. Generate Signing Key (One-Time Setup)

Firmware images are cryptographically signed using Ed25519 asymmetric cryptography. The private key resides exclusively on the operator's machine (`~/.intercom/ota_key.pem`); the matching 32-byte public key is loaded at compile time from the gitignored `crates/intercom-firmware/ota_public_key.bin`.

```bash
# Generate private key at ~/.intercom/ota_key.pem (0600 permissions)
# and write public key binary to crates/intercom-firmware/ota_public_key.bin:
python3 scripts/generate_keys.py
```

If a new key is generated, re-compile the firmware before deploying.

#### 2. Build Release Firmware Binary

Always deploy `--release` builds for OTA to ensure optimal performance and code size within partition boundaries:

```bash
mise exec -- cargo build -p intercom-firmware --release
```

The output binary is located at `target/riscv32imac-unknown-none-elf/release/intercom-bridge` (ELF executable).

You can either pass this ELF directly to `scripts/ota.py` (which automatically detects ELF executables and converts them to ESP32-C6 flat binaries via `espflash save-image`), or generate the flat binary image manually beforehand:

```bash
espflash save-image --chip esp32c6 --flash-size 8mb target/riscv32imac-unknown-none-elf/release/intercom-bridge firmware.bin
```

#### 3. Push OTA Update via Wi-Fi

Use the multi-device OTA push CLI tool (`scripts/ota.py`):

```bash
# Interactive mode: scans mDNS (_matter._tcp.local.) and prompts for target (ELF or flat .bin)
python3 scripts/ota.py target/riscv32imac-unknown-none-elf/release/intercom-bridge

# Push directly to a specific IP address:
python3 scripts/ota.py --target 192.168.1.142 target/riscv32imac-unknown-none-elf/release/intercom-bridge

# Push directly by device name (matches mDNS announcement):
python3 scripts/ota.py --target "Intercom Bridge" target/riscv32imac-unknown-none-elf/release/intercom-bridge
```

**CLI Flags & Options (`scripts/ota.py`):**

| Flag | Default | Description |
|------|---------|-------------|
| `--target`, `-t` | Interactive prompt | Direct IP address, hostname, device name, or MAC address |
| `--port`, `-p` | `8080` | Target HTTP OTA port on device |
| `--key`, `-k` | `~/.intercom/ota_key.pem` | Path to Ed25519 private key PEM file |
| `--timeout` | `3.5` | mDNS discovery timeout in seconds |
| `--http-timeout` | `60.0` | HTTP upload socket timeout in seconds |

#### 4. Dual-Bank A/B Partition Layout & Fail-Safe Rollback

The ESP32-C6-WROOM-1-N8 has 8 MB flash partitioned into two active app slots:

```
0x000000 ┌──────────────────────────────────────────────┐
         │ 2nd-stage bootloader (36 KB)                 │
0x009000 ├──────────────────────────────────────────────┤
         │ nvs (24 KB)                                  │
0x00f000 ├──────────────────────────────────────────────┤
         │ otadata (8 KB, dual 4 KB sectors)            │
0x020000 ├──────────────────────────────────────────────┤
         │ ota_0 (Slot A, 3.4 MB)                       │
0x370000 ├──────────────────────────────────────────────┤
         │ ota_1 (Slot B, 3.4 MB)                       │
0x6c0000 ├──────────────────────────────────────────────┤
         │ wifi_kvs (8 KB)                              │
0x6c2000 ├──────────────────────────────────────────────┤
         │ matter_kvs (24 KB)                           │
0x800000 └──────────────────────────────────────────────┘
```

**Fail-Safe Rollback Behavior:**
- **Zero-brick guarantee:** Updates are streamed exclusively into the *inactive* slot (`ota_0` or `ota_1`). The currently running firmware slot is never touched during the update.
- **Cryptographic verification before commit:** The entire binary is verified with Ed25519 before updating `otadata`. If the signature, magic bytes, or size are invalid, the flash slot is marked invalid and the running system continues uninterrupted without rebooting.
- **Bootloader rollback watchdog:** After verification, `otadata` writes `ESP_OTA_IMG_NEW` (`0x0`) with an incremented sequence number and reboots. Upon boot, the 2nd-stage ESP-IDF bootloader sets the state to `ESP_OTA_IMG_PENDING_VERIFY` (`0x1`).
- **Self-validation:** Once hardware, Wi-Fi, and the network stack are operational in `intercom-firmware/src/main.rs`, the new firmware calls `intercom_adapter_ota::bootloader::mark_running_slot_valid()`, transitioning state to `ESP_OTA_IMG_VALID` (`0x2`) and cancelling the rollback watchdog.
- **Automatic fallback:** If the new firmware crashes, encounters a boot loop, or watchdogs before reaching operational stability, the bootloader automatically rolls back to the previous valid slot on the subsequent reset.


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
│   │   │   └── service/            # Application services (AccessController, RingWatcher, FactoryResetService)
│   │   └── tests/                  # Host unit tests with mock drivers
│   ├── intercom-adapter-gpio/      # Hardware edge adapters (esp-hal)
│   │   └── src/
│   │       ├── config.rs           # Electrical timings and LED blink patterns
│   │       ├── ring.rs             # GpioRingDriver (optocoupler, debounce)
│   │       ├── door.rs             # GpioDoorDriver (PhotoMOS relay, timed pulse)
│   │       ├── led.rs              # GpioLedDriver (active-low LED, blink patterns)
│   │       ├── boot_button.rs      # GpioBootButtonDriver (maintenance button)
│   │       └── flash.rs            # EspFlash (ROM SPI flash persistence)
│   ├── intercom-adapter-http/      # Pre-gzipped Web Dashboard & REST API on port 8080
│   │   ├── assets/                 # Embedded dashboard.html production bundle
│   │   ├── frontend/               # Modern Solid.js + DaisyUI + Tailwind SPA source
│   │   ├── src/                    # Router, HTTP handlers, static asset streaming
│   │   └── tests/                  # Router and server unit/integration tests
│   ├── intercom-adapter-matter/    # Smart home protocol adapters (rs-matter)
│   │   └── src/
│   │       ├── config.rs           # Matter endpoint IDs and serial format
│   │       ├── door_lock.rs        # IntercomDoorLock (DoorLock cluster handler)
│   │       ├── occupancy.rs        # IntercomOccupancy (OccupancySensing cluster handler)
│   │       ├── access_control.rs   # MatterAccessControlDriver (bridges services ↔ clusters)
│   │       ├── commissioning.rs    # Commissioning helpers
│   │       └── controller.rs       # MatterController builder + runner (WiFi stack)
│   ├── intercom-pairing-tool/      # Host CLI (`cargo qr`) for Matter QR & manual pairing codes
│   └── intercom-firmware/          # Composition root binary (intercom-bridge)
│       └── src/
│           ├── config.rs           # Hardware version, device info, flash partition layout
│           ├── main.rs             # Hardware init, peripheral allocation, joins services
│           ├── version.rs          # Git and compile-time software versioning
│           └── utils/
│               └── uptime_logger.rs# Custom HH:MM:SS.mmm logger
├── scripts/
│   ├── cargo-fe                    # Frontend CLI runner (dev, build, check, install)
│   ├── generate_keys.py            # Ed25519 keypair generator for firmware signing
│   ├── ota.py                      # Multi-device mDNS discovery & OTA firmware pusher
│   └── monitor.py                  # Non-intrusive serial monitor without chip reset
├── GLOSSARY.md                     # Ubiquitous domain vocabulary
├── Cargo.toml                      # Virtual workspace root manifest
├── rust-toolchain.toml             # Rust 1.99.1 + riscv32imac target
├── .cargo/config.toml              # Build target, runner, build-std
└── partitions.csv                  # Flash partition layout
```

## Key Files to Understand

| File | Purpose |
|------|---------|
| `crates/intercom-firmware/src/main.rs` | Composition root: hardware init, creates channels & `SystemStateWatch`, instantiates adapters, wires them into `App` + `MatterController` + `HttpServer`, runs via `join4` |
| `crates/intercom-core/src/domain/access.rs` | `AccessStateMachine` — pure, deterministic domain state machine (zero async/runtime dependencies) |
| `crates/intercom-core/src/service/access.rs` | `AccessController` — deep application use case orchestrating strike relay, auto-relock, and ring announcements |
| `crates/intercom-core/src/ports/inbound.rs` | `AccessControllerHandle` & `CommandChannel` — idiomatic Inbound Port handle for driving access commands |
| `crates/intercom-core/src/ports/reporter.rs` | `IntercomReporter` & `LockState` — protocol-agnostic domain reporting port |
| `crates/intercom-core/src/ports/reset.rs` | `SystemResetDriver` — outbound port for credential erasure and system reboot |
| `crates/intercom-core/src/ports/` | Abstract outbound port traits (`DoorDriver`, `RingDriver`, `IntercomReporter`, `ResetTriggerDriver`, `SystemResetDriver`) |
| `crates/intercom-core/src/events.rs` | `SystemState` enum (`Updating`, etc.), `SystemStateWatch` (`Watch`), `SystemStateSender`, `SystemStateReceiver` |
| `crates/intercom-core/src/config.rs` | Domain access, relock, and reset timing policies (protocol- and hardware-agnostic) |
| `crates/intercom-adapter-http/src/server.rs` | Async HTTP server: static pre-gzipped dashboard & REST endpoints (`/api/status`, `/api/actions/*`, `/api/logs`, `/api/ota/flash`) |
| `crates/intercom-adapter-http/src/router.rs` | HTTP request parser and routing engine |
| `crates/intercom-adapter-http/frontend/` | Solid.js single-page application source (Vite + DaisyUI + Tailwind + Bun) |
| `crates/intercom-adapter-matter/src/config.rs` | Matter endpoint IDs and serial number formatting |
| `crates/intercom-firmware/src/config.rs` | Hardware version, product identification, flash layout, and `OTA_PUBLIC_KEY` |
| `crates/intercom-adapter-matter/src/readiness.rs` | `ClusterReadinessBarrier` — internal Apple Home subscription readiness coordination |
| `crates/intercom-adapter-matter/src/controller.rs` | `MatterController` — builder pattern for registering clusters + running the Matter stack |
| `crates/intercom-adapter-matter/src/door_lock.rs` | `IntercomDoorLock` — Matter DoorLock cluster, triggers `AccessControllerHandle` on unlock |
| `crates/intercom-adapter-matter/src/occupancy.rs` | `IntercomOccupancy` — Matter OccupancySensing cluster |
| `crates/intercom-adapter-matter/src/access_control.rs` | `MatterAccessControlDriver` — implements `IntercomReporter` port |
| `crates/intercom-adapter-gpio/src/` | GPIO implementations of port traits using `esp-hal` |
| `crates/intercom-adapter-matter/src/` | Matter protocol adapter implementation using `rs-matter` |
| `scripts/ota.py` | Host CLI: mDNS discovery, Ed25519 signing, and streaming HTTP firmware push |
| `scripts/generate_keys.py` | Host tool: generates Ed25519 private key and writes `ota_public_key.bin` |
| `scripts/cargo-fe` | Host frontend helper for `cargo fe-dev`, `cargo fe-build`, `cargo fe-check`, `cargo fe-install` |

## Architecture Patterns

### Inbound Command Handle Pattern
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

### Application Services with Directed Channels & Lifecycle Watch
Core application services communicate via Embassy channels and continuous state watch:
- **`CommandChannel`**: Point-to-point MPSC command channel driving `AccessController`.
- **`SystemStateWatch`**: Continuous lifecycle broadcast observed by edge adapters (e.g. `GpioLedDriver`) without queue overflow risk.
- **Direct Port Effects**: `AccessController` directly reports domain transitions to `IntercomReporter::report_ring()` and `IntercomReporter::report_lock_state()`. Edge adapters (such as `GpioLedDriver` or `MatterAccessControlDriver`) compose via tuples `(access_driver, indicator)` without intermediate core actors.

### Trait-Based Drivers

Hardware drivers implement async traits, making services generic and testable:

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
    fn erase_credentials(&self, scope: ResetScope);
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

#### Critical: Always Include `system;` in `with_attrs`
When defining a cluster with `with_attrs(with!(...))`, **always prefix the attribute list with `system;`**:

```rust
pub const CLUSTER: Cluster<'static> = occupancy_sensing::FULL_CLUSTER
    .with_revision(5)
    .with_features(occupancy_sensing::Feature::PASSIVE_INFRARED.bits())
    .with_attrs(with!(system;
        occupancy_sensing::AttributeId::Occupancy
            | occupancy_sensing::AttributeId::OccupancySensorType
            | occupancy_sensing::AttributeId::OccupancySensorTypeBitmap
    ))
    .with_cmds(with!());
```

**Why this matters:**
- Without `system;`, `rs-matter`'s `with!` macro filters out all global system attributes (`0xFFF8` - `0xFFFD`).
- This causes `AttributeList` (`0xFFFB`), `FeatureMap` (`0xFFFC`), `AcceptedCommandList` (`0xFFF9`), and `GeneratedCommandList` (`0xFFF8`) to return `AttributeNotFound`.
- Strict ecosystem controllers like **Aqara Hub M3** query `FeatureMap` (e.g., PIR bit for `OccupancySensing`) and `AttributeList` during interview. If `FeatureMap` is missing or 0, Aqara rejects the cluster as invalid and will silently hide the device component from the UI.
- Ensure device type revisions also match standard spec requirements (e.g. `OccupancySensor` is `dtype: 0x0107, drev: 4`, `DoorLock` is `dtype: 0x000A, drev: 3`).

### Matter Endpoints

- **Endpoint 0:** Root node (system clusters, handled by rs-matter-embassy)
- **Endpoint 1:** Door Lock Cluster (lock/unlock commands)
- **Endpoint 2:** Occupancy Sensor (ring detection, appears as Motion Sensor in HomeKit, Home Assistant, and Aqara Home)

### Over-The-Air (OTA) Firmware Updates & Fail-Safe Dual-Bank Flash

The OTA architecture provides secure, wireless firmware updates over the local Wi-Fi network with zero risk of bricking:

- **Zero-Allocation Async HTTP Server (`OtaServer`)**:
  - Binds dedicated TCP port 8080 over Embassy networking (`embassy-net`).
  - Strict single-connection concurrency: rejects secondary incoming connections immediately to prevent memory exhaustion and contention.
  - Imposes tarpit delay on invalid requests to protect against brute-force DoS attempts.

- **Wire Protocol & Asymmetric Ed25519 Cryptographic Verification**:
  - Binary update stream wire header format: `[b"ICOT"][u32 length (LE)][64-byte Ed25519 signature][binary payload...]`.
  - Signature is computed offline across the exact binary payload using the private key (`~/.intercom/ota_key.pem`).
  - Device verifies signature using compile-time embedded public key (`config::OTA_PUBLIC_KEY`). The private key is never stored on the microcontroller.
  - Verification occurs on the final SHA-512 digest *before* committing the bootloader partition, completely preventing corrupt or untrusted code execution.

- **Streaming Flash Sector Writer (`OtaWriter`)**:
  - Zero heap allocation: buffers streaming bytes into a fixed static 4 KB sector buffer (`[u8; 4096]`).
  - Streams bytes into an Ed25519 `StreamVerifier` as sectors are erased and programmed directly into the inactive flash slot (`ota_0` or `ota_1`).
  - Verifies ESP32-C6 executable image magic byte (`0xE9`) and chip ID headers before writing to prevent flashing incompatible binaries.

- **Dual-Bank Fail-Safe Bootloader State Machine (`OtaBootloader`)**:
  - Leverages standard ESP-IDF 2nd-stage bootloader dual-bank partition structure (`otadata`).
  - Increments active sequence counter and commits status `ESP_OTA_IMG_NEW` (`0x00000000`) with CRC-32 checksum protection.
  - On reboot, the bootloader transitions the slot to `ESP_OTA_IMG_PENDING_VERIFY` (`0x00000001`).
  - Once hardware, Wi-Fi, and the network stack are operational, `intercom_adapter_ota::bootloader::mark_running_slot_valid()` marks the slot `ESP_OTA_IMG_VALID` (`0x00000002`).
  - If the new image crashes, reboots unexpectedly, or encounters a watchdog panic before reaching operational state, the bootloader automatically rolls back to the previously functioning slot on the subsequent reset.

- **Priority Preemption & Lifecycle Coordination**:
  - OTA update activity does not block real-time interrupts or access control operations; door strikes and optocoupler events continue to take precedence.
  - Emits `SystemState::Updating` to the `SystemStateWatch` lifecycle bus during write activity, prompting `GpioLedDriver` to display a rapid 10 Hz strobe pattern (`UPDATING_STROBE_MS = 50ms`).

## Common Tasks

### Adding a New GPIO Driver

1. Define an async trait in `crates/intercom-core/src/ports/`
2. Create implementation in `crates/intercom-adapter-gpio/src/`
3. Add the new service in `crates/intercom-core/src/service/`
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
- All drivers and services are async (Embassy)
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
- **Core Services & Edge Adapters:** `RingWatcher`, `AccessController`, `GpioLedDriver` transitions

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
| OTA flash fails or times out | Verify Wi-Fi connection, verify private key matches embedded `OTA_PUBLIC_KEY`, check serial logs for bootloader state |

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
| `ed25519-dalek` | Fast, `no_std` Ed25519 asymmetric signature verification |
| `crc32fast` | CRC-32 integrity verification for ESP-IDF `otadata` entries |
| `sha2` | Streaming SHA-512 cryptographic hashing for OTA firmware image validation |

Patched crates (via `[patch.crates-io]` in `Cargo.toml`):

- `rs-matter` → `sysgrok/rs-matter` `next` branch
- `rs-matter-stack` → `sysgrok/rs-matter-stack` `next` branch
- `openthread` → `sysgrok/openthread`

## Testing Changes

1. Run host unit tests: `cargo test -p intercom-core -p intercom-adapter-http --target x86_64-apple-darwin`
2. Build firmware binary: `mise exec -- cargo build -p intercom-firmware --release`
3. Flash via USB: `mise exec -- cargo run` (or remotely via Wi-Fi: `python3 scripts/ota.py target/riscv32imac-unknown-none-elf/release/intercom-bridge`)
4. Monitor serial logs: `python3 scripts/monitor.py --no-reset`
5. Test with Apple Home / Aqara Home or chip-tool
