# Intercom Bridge

Universal smart intercom controller built with **Hexagonal Architecture** (Ports & Adapters) for the ESP32-C6 in bare-metal Rust (`no_std` + Embassy async).

Connects analog intercom systems (such as the IR204E) to modern smart home ecosystems like **Apple Home (HomeKit)**, **Home Assistant**, and **Aqara Home** via the Matter standard.

---

## Features

- 🔐 **Door Lock & Auto-Relock** — Unlock intercom door latch from Apple Home, Home Assistant, Aqara Home, or any Matter controller.
- 🔔 **Ring Detection** — Get notified when a visitor rings (exposed as an Occupancy/Motion sensor for instant push notifications and automations across Apple Home, Home Assistant, and Aqara Home).
- 🧩 **Hexagonal Architecture** — Core business domain is completely isolated from hardware peripherals and smart home protocols. Easily swap or add new adapters (e.g. Home Assistant / MQTT, different intercom hardware).
- ⚡ **Host-Testable Core** — Run unit tests for domain state machines and application services in ~0.7 seconds on your development machine (macOS/Linux) without flashing or hardware simulators.
- 💡 **Status Indicator** — Non-blocking LED patterns indicating WiFi connecting, connected, ringing, and unlocking states.
- 🔘 **Hardware Factory Reset** — Hold the BOOT button (GPIO9) for 10 seconds to wipe Matter commissioning credentials and restart pairing mode.

---

## Hexagonal Architecture

The project is structured as a **Virtual Cargo Workspace** following the Ports and Adapters pattern:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            intercom-firmware                                │
│                     (Composition Root / App Runner)                         │
└──────────────┬───────────────────────────────────────────────┬──────────────┘
               │                                               │
               ▼                                               ▼
┌──────────────────────────────┐               ┌──────────────────────────────┐
│    intercom-adapter-gpio     │               │   intercom-adapter-matter    │
│       (Hardware Port)        │               │       (Protocol Port)        │
│  - GpioRingDriver (HCPL-817) │               │  - IntercomDoorLock (Lock)   │
│  - GpioDoorDriver (AQY212)   │               │  - IntercomOccupancy (Motion)│
│  - GpioLedDriver             │               │  - MatterAccessControlDriver │
│  - GpioBootButtonDriver      │               │  - MatterController (WiFi)   │
└──────────────┬───────────────┘               └───────────────┬──────────────┘
               │                                               │
               │ implements                                    │ implements
               ▼                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                             intercom-core                                   │
│                       (Universal Domain Logic)                              │
│                                                                             │
│  Inbound & Outbound Ports:                                                  │
│   ├── AccessControllerHandle (Inbound)    ├── IntercomReporter (Outbound)    │
│   ├── RingDriver (Outbound)               ├── ResetTriggerDriver (Outbound)  │
│   ├── DoorDriver (Outbound)               └── SystemResetDriver (Outbound)   │
│                                                                             │
│  Application Services & Domain Core:                                        │
│   ├── AccessController (AccessStateMachine)                                 │
│   ├── RingWatcher                         ├── ResetService                  │
│   ├── SystemStateWatch (Lifecycle)        └── CommandChannel (MPSC)         │
└─────────────────────────────────────────────────────────────────────────────┘
```

See [GLOSSARY.md](GLOSSARY.md) for domain terminology and architecture definitions.

---

## Workspace Crates

| Crate | Target | Purpose |
|-------|--------|---------|
| [`intercom-core`](crates/intercom-core) | Universal (`no_std`, host + embedded) | Pure domain logic, application services, and port trait definitions. Has **zero** hardware or protocol dependencies. |
| [`intercom-adapter-gpio`](crates/intercom-adapter-gpio) | RISC-V (`esp32c6`) | Hardware edge adapter implementing core ports using `esp-hal` (optocoupler debounce, PhotoMOS timed pulse, LED patterns, button press detection). |
| [`intercom-adapter-matter`](crates/intercom-adapter-matter) | RISC-V (`esp32c6`) | Smart home protocol adapter implementing Matter clusters (`DoorLock`, `OccupancySensing`) using `rs-matter` and bare-metal WiFi/BLE via `rs-matter-embassy`. |
| [`intercom-firmware`](crates/intercom-firmware) | RISC-V (`esp32c6`) | Composition root. Initializes chip peripherals, constructs drivers, wires ports into `App`, and builds the final `intercom-bridge` ELF binary. |

---

## Quick Start

### Prerequisites

- [mise](https://mise.jdx.dev/) (selects the pinned Rust 1.99.1 and Bun 1.3.14 toolchains; firmware targets RISC-V `riscv32imac-unknown-none-elf`)
- [espflash](https://github.com/esp-rs/espflash) (`cargo install espflash`)

### 1. Run Host Unit Tests

Core domain logic, application services, and state transitions are tested on your local host (macOS / Linux) in under a second:

```bash
mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin
# Or on Linux:
# mise exec -- cargo test -p intercom-core --target x86_64-unknown-linux-gnu
```

For the dashboard, `cargo fe-check` runs nonmutating type checking and linting. Run `bun run format` from `crates/intercom-adapter-http/frontend` when you want to apply formatting.

For Matter pairing codes without a connected board, run `cargo qr --mac 98:a3:16:a6:78:44` or `cargo qr --mac 98:a3:16:a6:78:44 --json`.

### 2. Build Firmware

Compile the RISC-V bare-metal binary:

```bash
mise exec -- cargo build -p intercom-firmware
```

The resulting binary is located at `target/riscv32imac-unknown-none-elf/debug/intercom-bridge`.

### 3. Flash & Monitor

Build, flash to the connected ESP32-C6, and open the serial monitor in one command:

```bash
mise exec -- cargo run
```

Or flash directly to a specific serial port:

```bash
./flash.sh /dev/cu.usbmodem14201
```

### 4. Non-Intrusive Serial Monitor

To view live serial logs without resetting the chip (standard serial monitors toggle DTR/RTS which resets the ESP32 and wipes in-memory Matter exchange sessions):

```bash
./attach.sh /dev/cu.usbmodem14201
```

Logs are also mirrored to `/tmp/intercom_serial.log`.

---

## Apple Home & Matter Integration

Once commissioned, the bridge exposes two endpoints:

| Accessory in HomeKit | Matter Endpoint | Matter Cluster | Function |
|----------------------|-----------------|----------------|----------|
| 🔐 **Intercom Lock** | Endpoint 1 | `DoorLock` (0x0101) | Tap to release intercom latch (700ms pulse, 4s auto-relock) |
| 🚶 **Ring Sensor** | Endpoint 2 | `OccupancySensing` (0x0406) | Triggers on visitor call (displayed as Motion Sensor for immediate push notifications) |

For troubleshooting WiFi, IPv6, or Apple Home hub connectivity issues, see [docs/commissioning_troubleshooting.md](docs/commissioning_troubleshooting.md).

---

## Hardware Reference

| Component | Part Number | Purpose |
|-----------|-------------|---------|
| Controller | DFRobot Beetle ESP32-C6 | Matter-capable RISC-V MCU with onboard antenna |
| Optocoupler | HCPL-817-000E | Optically isolated call signal detection (GPIO2) |
| PhotoMOS Relay | AQY212EHA | Optically isolated door unlock latch release (GPIO3) |
| Status LED | Onboard Blue LED | System state and activity indicator (GPIO15, active-low) |
| Boot Button | Onboard BOOT Button | Maintenance & 10-second factory reset (GPIO9) |

See [HARDWARE.md](HARDWARE.md) for full electrical wiring and schematics.

---

## Resetting Matter Pairing

To clear Matter fabric and WiFi credentials without re-flashing firmware:

1. **Via Hardware Button:** Press and hold the BOOT button (GPIO9) for **10 seconds** until the LED blinks rapidly.
2. **Via CLI:**
   ```bash
   espflash erase-parts --port /dev/cu.usbmodem14201 matter_kvs nvs
   ```
Then reboot the device to enter Bluetooth commissioning advertising.

---

## License

MIT License — see [LICENSE](LICENSE) for details.
