# Smart Intercom

ESP32-C6 Matter controller for IR204E analog intercom with HomeKit integration.

## Features

- 🔐 **Door Lock** - Unlock intercom door from Apple Home
- 🔔 **Ring Detection** - Get notified when someone rings (Motion Sensor)
- 🏠 **Matter/HomeKit** - Native Apple Home support via Matter
- ⚡ **Safe Design** - Optical isolation protects your ESP32

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Matter Device                                │
│                                                                      │
│  Endpoint 1: Door Lock              Endpoint 2: Occupancy Sensor    │
│  ┌─────────────────────┐            ┌─────────────────────┐         │
│  │ Door Lock Cluster   │            │ Occupancy Sensing   │         │
│  │ - Unlock command    │            │ - Occupancy bitmap  │         │
│  │ - Auto-relock (1s)  │            │ (appears as Motion  │         │
│  └──────────┬──────────┘            │  Sensor in HomeKit) │         │
│             │                       └──────────┬──────────┘         │
└─────────────┼──────────────────────────────────┼────────────────────┘
              │                                  │
              ▼                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Event Bus                                    │
│                                                                      │
│  RingingEvent ──────────────────► AccessControlActor ──► Matter     │
│       ▲                                    │                         │
│       │                           UnlockDispatchedEvent              │
│       │                                    │                         │
│  RingActor ◄── GpioRingDriver              ▼                         │
│                                       DoorActor ──► GpioDoorDriver   │
└─────────────────────────────────────────────────────────────────────┘
              │                                  │
              ▼                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     IR204E Intercom                                  │
│                                                                      │
│  GPIO2 (input)                       GPIO3 (output)                 │
│  └─► HCPL-817 optocoupler            └─► AQY212EHA PhotoMOS         │
│      detects ring signal                 releases door latch        │
└─────────────────────────────────────────────────────────────────────┘
```

## HomeKit Integration

After pairing, your Home app will show:

| Accessory | Type | Function |
|-----------|------|----------|
| 🔐 Intercom Lock | Door Lock | Tap to unlock door |
| 🚶 Ring Sensor | Motion Sensor | Detects when someone rings |

### Why Motion Sensor?

The ring detection appears as a Motion Sensor in HomeKit because:
- Native notification support ("Motion detected")
- Better automation triggers than Contact Sensor
- 4-second auto-clear matches typical ring duration

### Example Automations

- "When motion detected → Send notification"
- "When motion detected → Flash porch lights"
- "When motion detected → Announce on HomePod"

## Hardware

| Component | Part Number | Purpose |
|-----------|-------------|---------|
| Controller | DFRobot Beetle ESP32-C6 | Matter-capable MCU |
| Optocoupler | HCPL-817-000E | Ring detection (isolated) |
| PhotoMOS Relay | AQY212EHA | Door unlock (isolated) |
| Connectors | WAGO 221 | Non-destructive wiring |

See [HARDWARE.md](HARDWARE.md) for wiring diagrams.

## Configuration

All hardware settings are in `main/config/app_config.h`:

```cpp
// GPIO Configuration
#define RING_GPIO 2    // Input: Optocoupler output (active LOW)
#define UNLOCK_GPIO 3  // Output: PhotoMOS relay control (active HIGH)

// Timing Constants (milliseconds)
#define UNLOCK_PULSE_MS 700    // Duration of relay activation
#define RING_DEBOUNCE_MS 30    // Ring signal debounce time
#define RING_COOLDOWN_MS 2500  // Minimum time between ring events
#define RING_HOLD_MS 4000      // How long motion stays active in HomeKit
```

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
5. Device appears as Door Lock + Motion Sensor

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
│   ├── app_main.cpp              # Entry point, initializes App
│   ├── app.cpp/h                 # Application class, wires actors
│   │
│   ├── config/
│   │   └── app_config.h          # GPIO pins, timing constants
│   │
│   ├── events/
│   │   ├── events.h              # RingingEvent, UnlockDispatchedEvent
│   │   └── listener.h            # Event subscription template
│   │
│   ├── actors/
│   │   ├── ring_actor.h          # Hardware ring → RingingEvent
│   │   ├── access_control_actor.h # Business logic, Matter bridge
│   │   └── door_actor.h          # UnlockDispatchedEvent → hardware
│   │
│   └── drivers/
│       ├── gpio_ring_driver.cpp/h      # Optocoupler input
│       ├── gpio_door_driver.cpp/h      # PhotoMOS relay output
│       └── matter_access_control_driver.cpp/h  # Matter endpoints
│
├── CMakeLists.txt
├── sdkconfig.defaults
├── partitions.csv
├── README.md
└── HARDWARE.md
```

## How It Works

### Actor Model

The firmware uses an event-driven actor architecture:

| Actor | Responsibility |
|-------|----------------|
| `RingActor` | Listens to hardware, emits `RingingEvent` |
| `AccessControlActor` | Business logic, bridges events ↔ Matter |
| `DoorActor` | Listens to `UnlockDispatchedEvent`, triggers hardware |

Actors communicate via an `EventBus` with typed events.

### Driver Pattern

Hardware drivers use config structs for dependency injection:

```cpp
// Config passed at construction
GpioRingDriver ringDriver_({
    .gpioPin = RING_GPIO,
    .debounceMs = RING_DEBOUNCE_MS,
    .cooldownMs = RING_COOLDOWN_MS
});

// Timer callbacks use FreeRTOS timer ID pattern
xTimerCreate("timer", period, false, this, callback);
auto* self = static_cast<Driver*>(pvTimerGetTimerID(timer));
```

### Unlock Flow

1. User taps "Unlock" in Home app
2. Matter sends unlock command
3. `MatterAccessControlDriver` receives callback
4. Emits `UnlockDispatchedEvent` via `AccessControlActor`
5. `DoorActor` triggers `GpioDoorDriver::triggerUnlock()`
6. PhotoMOS relay closes for 700ms
7. Door latch releases, person can enter
8. Auto-relock timer fires after 1s (Matter state only)

### Ring Detection Flow

1. Visitor presses intercom button
2. Optocoupler pulls GPIO2 LOW
3. GPIO ISR fires, starts debounce timer
4. After 30ms, `GpioRingDriver` validates signal
5. Invokes callback → `RingActor` emits `RingingEvent`
6. `AccessControlActor` calls `MatterAccessControlDriver::ringing()`
7. Occupancy sensor set to "occupied" (motion detected)
8. After 4s, auto-clears to "unoccupied"

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
