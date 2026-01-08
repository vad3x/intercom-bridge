# Smart Intercom

ESP32-C6 firmware for making an IR204E analog intercom "smart" with:
- Buzz/ring notification detection
- Remote door unlock via HTTP API
- Future: Matter/HomeKit integration

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        IR204E Intercom                          │
│  ┌──────────┐                              ┌──────────┐        │
│  │  Ring    │                              │  Door    │        │
│  │  Signal  │                              │  Button  │        │
│  └────┬─────┘                              └────┬─────┘        │
└───────┼─────────────────────────────────────────┼──────────────┘
        │                                         │
        │ (AC/DC)                                 │ (Parallel)
        ▼                                         ▼
┌───────────────┐                        ┌───────────────┐
│  Optocoupler  │                        │ PhotoMOS Relay│
│  HCPL-817     │                        │  AQY212EHA    │
└───────┬───────┘                        └───────┬───────┘
        │                                         │
        │ (GPIO Input)                           │ (GPIO Output)
        ▼                                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                      ESP32-C6 (Beetle)                          │
│                                                                 │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│   │ GPIO Handler│───▶│    Main     │───▶│ HTTP Server │       │
│   │ (Interrupt) │    │  App Logic  │    │ /unlock     │       │
│   └─────────────┘    └─────────────┘    │ /status     │       │
│                                          └─────────────┘       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ WiFi
                              ▼
                    ┌─────────────────┐
                    │   Home Network  │
                    │   Apple Home    │
                    │   (Future)      │
                    └─────────────────┘
```

## Hardware

| Component | Part Number | Purpose |
|-----------|-------------|---------|
| Controller | DFRobot Beetle ESP32-C6 (DFR1117) | WiFi + GPIO + Matter-ready |
| Optocoupler | Broadcom HCPL-817-000E | Isolate ring detection |
| PhotoMOS Relay | Panasonic AQY212EHA | Isolated door unlock |
| Connectors | WAGO 221 | Non-destructive intercom tap |

See [HARDWARE.md](HARDWARE.md) for wiring diagrams and specifications.

## Quick Start

### Prerequisites

- VS Code with Dev Containers extension
- Docker Desktop
- (Optional) ESP32-C6 board for hardware testing

### Setup

1. Clone the repository:
   ```bash
   git clone <repo-url>
   cd intercom-matter
   ```

2. Open in VS Code and reopen in Dev Container when prompted

3. Configure WiFi credentials:
   ```bash
   idf.py menuconfig
   # Navigate to: Component config → WiFi Configuration
   # Or edit main/main.c directly
   ```

4. Build firmware:
   ```bash
   idf.py build
   ```

### Testing with QEMU (No Hardware)

```bash
cd qemu
./run-qemu.sh

# In another terminal:
./test-api.sh
```

### Flash to Hardware

```bash
idf.py flash monitor
```

## HTTP API

### Get Status

```bash
curl http://<device-ip>/status
```

Response:
```json
{
  "device": "smart-intercom",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "buzz": {
    "count": 5,
    "last_time_ms": 1234567890
  },
  "unlock": {
    "count": 2,
    "last_time_ms": 1234567000,
    "busy": false
  },
  "config": {
    "buzz_gpio": 2,
    "unlock_gpio": 3,
    "unlock_pulse_ms": 700,
    "unlock_lockout_ms": 1500,
    "buzz_cooldown_ms": 2500,
    "debounce_ms": 30
  }
}
```

### Trigger Unlock

```bash
curl -X POST http://<device-ip>/unlock
```

Response (success):
```json
{
  "success": true,
  "message": "Door unlock triggered",
  "pulse_ms": 700
}
```

Response (lockout active):
```json
{
  "success": false,
  "error": "Unlock in progress or lockout active",
  "retry_after_ms": 800
}
```

## Configuration

### GPIO Pins

| Function | Default GPIO | Notes |
|----------|--------------|-------|
| Buzz Input | GPIO2 | Pull-up, falling edge interrupt |
| Unlock Output | GPIO3 | Active HIGH pulse |

### Timing

| Parameter | Value | Purpose |
|-----------|-------|---------|
| Debounce | 30 ms | Filter noise on buzz input |
| Buzz Cooldown | 2500 ms | Prevent duplicate buzz events |
| Unlock Pulse | 700 ms | Duration relay is activated |
| Unlock Lockout | 1500 ms | Minimum time between unlocks |

## Project Structure

```
intercom-matter/
├── main/
│   ├── main.c              # Application entry, WiFi init
│   ├── gpio_handler.c/h    # Buzz detection, unlock control
│   ├── http_handler.c/h    # REST API endpoints
│   └── CMakeLists.txt      # Component build config
├── qemu/
│   ├── docker-compose.yml  # QEMU container config
│   ├── run-qemu.sh         # Launch emulator
│   └── test-api.sh         # API test script
├── .devcontainer/
│   └── devcontainer.json   # VS Code Dev Container
├── CMakeLists.txt          # Project build config
├── sdkconfig.defaults      # Default ESP-IDF config
├── partitions.csv          # Flash partition table
└── HARDWARE.md             # Wiring documentation
```

## Safety Notes

- **Never connect intercom wires directly to ESP32 GPIO**
- Always use optical isolation (optocoupler for input, PhotoMOS for output)
- Power ESP32 from its own USB supply, not from intercom
- This design is reversible and non-destructive

## Future Roadmap

1. ✅ HTTP API for remote control
2. ⬜ WiFi provisioning (SmartConfig or BLE)
3. ⬜ ESP-Matter integration
4. ⬜ HomeKit Lock + Doorbell accessory
5. ⬜ Audio passthrough (optional)

## License

MIT License - See LICENSE file for details.

## Contributing

Contributions welcome! Please open an issue first to discuss changes.
