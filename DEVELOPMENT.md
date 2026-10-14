# Smart Intercom Development Guide

Development guide for the **Intercom Bridge** — a bare-metal Rust firmware bridging analog intercom systems (IR204E) to smart home networks (Matter over Wi-Fi / Apple Home) on the ESP32-C6.

---

## Architecture Overview

The firmware is designed with **Hexagonal Architecture** (Ports & Adapters) in a virtual Cargo workspace:

- **`intercom-core`**: Universal domain logic (`no_std`), deterministic state machine (`AccessStateMachine`), async application services (`AccessController`, `RingWatcher`, `FactoryResetService`), and port traits. Host-testable without hardware.
- **`intercom-adapter-gpio`**: Hardware adapters implementing core ports using `esp-hal` (optocoupler ring debounce, PhotoMOS timed unlock pulse, non-blocking LED patterns, BOOT button reset).
- **`intercom-adapter-matter`**: Smart home protocol adapters implementing Matter clusters (`DoorLock`, `OccupancySensing`) using `rs-matter` and Embassy networking via `rs-matter-embassy`.
- **`intercom-firmware`**: Composition root binary (`intercom-bridge`). Initializes peripherals, instantiates adapters, binds them to services, and joins async tasks.

---

## Hardware Configuration

| Pin | Function | Hardware | Logic |
|-----|----------|----------|-------|
| **GPIO2** | Intercom Call / Buzz Input | Optocoupler (HCPL-817-000E) | Active LOW (internal pull-up) |
| **GPIO3** | Door Unlock Relay | PhotoMOS Relay (AQY212EHA) | Active HIGH (timed 700ms pulse) |
| **GPIO9** | Boot / Maintenance Button | Onboard Button | Active LOW (hold 10s for factory reset) |
| **GPIO15** | Status LED | Onboard Blue LED | Active LOW |

See [HARDWARE.md](HARDWARE.md) for complete schematics, BOM, and wiring diagrams.

---

## Timing Constants

Defined in [`crates/intercom-core/src/config.rs`](crates/intercom-core/src/config.rs):

| Constant | Value | Purpose |
|----------|-------|---------|
| `UNLOCK_PULSE_MS` | 700 ms | PhotoMOS relay activation duration |
| `AUTO_RELOCK_SEC` | 4 s | Apple Home lock status auto-relock timeout |
| `RING_DEBOUNCE_MS` | 30 ms | Optocoupler ring debounce filter |
| `RING_COOLDOWN_MS` | 2,500 ms | Minimum cooldown between consecutive ring events |
| `RING_HOLD_MS` | 15,000 ms | Occupancy sensor active hold duration in HomeKit |

---

## Prerequisites

1. **Toolchain Manager:** [mise](https://mise.jdx.dev/) manages the pinned nightly Rust toolchain and RISC-V target:
   ```bash
   mise install
   ```
2. **Flasher Utility:** [espflash](https://github.com/esp-rs/espflash):
   ```bash
   cargo install espflash
   ```
3. **Python 3:** With `pyserial` installed (for the non-reset serial monitor):
   ```bash
   pip3 install pyserial
   ```

---

## Build, Test & Flash

### 1. Run Host Unit Tests

Runs all core domain state machine and application service tests on your local machine in under 1 second without hardware:

```bash
# macOS host:
mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin

# Linux host:
mise exec -- cargo test -p intercom-core --target x86_64-unknown-linux-gnu
```

### 2. Build Firmware Binary

Compiles the RISC-V bare-metal firmware (`target/riscv32imac-unknown-none-elf/debug/intercom-bridge`):

```bash
mise exec -- cargo build -p intercom-firmware
```

### 3. Build, Flash & Monitor

Builds and flashes using cargo runner (configured in `.cargo/config.toml`):

```bash
mise exec -- cargo run
```

Or flash directly to a specific port:

```bash
./flash.sh /dev/cu.usbmodem14201
```

### 4. Non-Intrusive Serial Monitor

> [!WARNING]
> Standard terminal monitors (`screen`, `picocom`, `espflash monitor`) toggle DTR/RTS when opening, which **reboots the ESP32-C6** and destroys in-memory Matter session state.

Use the custom non-intrusive monitor script:

```bash
./attach.sh /dev/cu.usbmodem14201
# Or directly:
python3 scripts/monitor.py --port /dev/cu.usbmodem14201 --no-reset
```

Logs are mirrored in real time to `/tmp/intercom_serial.log`.

---

## Commissioning & Reset

### Apple Home Pairing Flow

1. Flash firmware and monitor serial output.
2. The device advertises via Bluetooth LE (`Starting BLE advertising...`).
3. Open **Apple Home** → **+** → **Add Accessory**.
4. Scan the Matter QR code or enter the manual pairing code printed in the serial logs.
5. Once paired, Apple Home will show:
   - **Intercom Lock** (Endpoint 1, `DoorLock` cluster): tap to unlock the door latch.
   - **Ring Sensor** (Endpoint 2, `OccupancySensing` cluster): triggers instant push notifications on incoming calls.

### Factory Reset (Re-Commissioning)

To wipe Matter pairing credentials so the device returns to pairing mode without re-flashing:

1. **Hardware Method:** Press and hold the BOOT button (GPIO9) for **10 seconds** until the LED blinks rapidly.
2. **CLI Method:**
   ```bash
   espflash erase-parts --port /dev/cu.usbmodem14201 matter_kvs nvs
   ```
   Then reboot the board.

For Wi-Fi, IPv6, or router/AP troubleshooting, consult [docs/commissioning_troubleshooting.md](docs/commissioning_troubleshooting.md).
