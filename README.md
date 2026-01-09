# Smart Intercom

ESP32-C6 Matter controller for IR204E analog intercom with HomeKit integration.

## Features

- 🔐 **Door Lock** - Unlock intercom door from Apple Home
- 🔔 **Ring Detection** - Get notified when someone rings
- 🏠 **Matter/HomeKit** - Native Apple Home support via Matter
- ⚡ **Safe Design** - Optical isolation protects your ESP32

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                      Matter Device                              │
│                                                                 │
│  Endpoint 1: Door Lock              Endpoint 2: Contact Sensor │
│  ┌─────────────────────┐            ┌─────────────────────┐    │
│  │ Door Lock Cluster   │            │ Boolean State       │    │
│  │ - Unlock command    │            │ - StateValue        │    │
│  │ - Auto-relock (1s)  │            │ (false=idle,        │    │
│  └──────────┬──────────┘            │  true=ringing)      │    │
│             │                       └──────────┬──────────┘    │
└─────────────┼──────────────────────────────────┼───────────────┘
              │                                  │
              ▼                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                        GPIO Driver                               │
│                                                                  │
│  GPIO3 (output)                    GPIO2 (input)                │
│  └─► PhotoMOS relay                └─► Optocoupler              │
│      700ms pulse on unlock             Interrupt → debounce     │
│                                        → update contact sensor  │
└─────────────────────────────────────────────────────────────────┘
              │                                  │
              ▼                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                     IR204E Intercom                              │
│                                                                  │
│  Door Button (parallel)            Ring Signal                  │
│  └─► AQY212EHA closes              └─► HCPL-817 detects         │
│      door latch releases               sends to ESP32           │
└─────────────────────────────────────────────────────────────────┘
```

## HomeKit Integration

After pairing, your Home app will show:

| Accessory | Type | Function |
|-----------|------|----------|
| 🔐 Intercom Lock | Door Lock | Tap to unlock door |
| 🚪 Ring Sensor | Contact Sensor | Shows "Open" when ringing |

### Example Automations

- "When Ring Sensor opens → Send notification"
- "When Ring Sensor opens → Flash porch lights"
- "When Ring Sensor opens → Announce on HomePod"

## Hardware

| Component | Part Number | Purpose |
|-----------|-------------|---------|
| Controller | DFRobot Beetle ESP32-C6 | Matter-capable MCU |
| Optocoupler | HCPL-817-000E | Ring detection (isolated) |
| PhotoMOS Relay | AQY212EHA | Door unlock (isolated) |
| Connectors | WAGO 221 | Non-destructive wiring |

See [HARDWARE.md](HARDWARE.md) for wiring diagrams.

## GPIO Configuration

| Pin | Function | Direction | Hardware |
|-----|----------|-----------|----------|
| GPIO2 | Buzz input | Input, Pull-up | Optocoupler output |
| GPIO3 | Unlock output | Output | PhotoMOS relay LED |

## Timing

| Parameter | Value | Description |
|-----------|-------|-------------|
| Unlock pulse | 700ms | How long relay activates |
| Auto-relock | 1s | Matter state returns to locked |
| Debounce | 30ms | Ring signal filtering |
| Buzz cooldown | 2.5s | Minimum between ring events |
| Buzz active | 2s | How long sensor stays "open" |

## Building

### Prerequisites

- ESP-IDF v5.x
- ESP-Matter SDK

### Build Commands

```bash
# Set target
idf.py set-target esp32c6

# Build
idf.py build

# Flash and monitor
idf.py flash monitor
```

### Dev Commands

```bash
# Format all C++ files
find main \( -name "*.cpp" -o -name "*.h" \) -exec clang-format -i {} +
```

## Commissioning

1. Power on the device
2. Open Apple Home app
3. Tap "+" → "Add Accessory"
4. Scan QR code or enter manual code
5. Device appears as Door Lock + Contact Sensor

### Commission via CLI (optional)

```bash
# Get QR code
matter esp qrcode

# Factory reset
matter esp factoryreset
```

## Project Structure

```
intercom-matter/
├── main/
│   ├── app_main.cpp          # Matter node + endpoints
│   ├── app_driver.cpp        # GPIO control + buzz detection
│   ├── app_priv.h            # Configuration + pin definitions
│   └── lock/
│       ├── door_lock_manager.cpp/h   # Simplified lock logic
│       └── door_lock_callbacks.cpp   # Matter callbacks
├── CMakeLists.txt
├── sdkconfig.defaults
├── partitions.csv
├── README.md
└── HARDWARE.md
```

## How It Works

### Unlock Flow

1. User taps "Unlock" in Home app
2. Matter sends unlock command
3. `emberAfPluginDoorLockOnDoorUnlockCommand()` called
4. `BoltLockMgr().Unlock()` triggers GPIO pulse
5. PhotoMOS relay closes for 700ms
6. Door latch releases, person can enter
7. Auto-relock timer fires after 1s
8. Matter state returns to "Locked"

### Ring Detection Flow

1. Visitor presses intercom button
2. Intercom sends ring signal
3. Optocoupler pulls GPIO2 LOW
4. GPIO interrupt fires
5. Debounce timer validates signal (30ms)
6. Contact sensor updated to "Open"
7. HomeKit triggers automations
8. After 2s, sensor returns to "Closed"

## Simplified Design

This implementation removes complex features not needed for intercoms:

| Removed | Reason |
|---------|--------|
| PIN credentials | No keypad on intercom |
| User management | Single household use |
| Schedules | Overkill for door release |
| Credential OTA | Not applicable |

The Lock command is a **no-op** - the intercom door is always locked by its spring mechanism. Only Unlock triggers hardware.

## Safety Notes

⚠️ **Electrical Safety**

- Never connect intercom wires directly to ESP32
- Always use optical isolation (optocoupler + PhotoMOS)
- Power ESP32 from separate USB supply

⚠️ **Building Security**

- This modification affects building access
- Consult building management before installation
- Keep manual intercom button functional

## License

MIT License - See LICENSE for details.
