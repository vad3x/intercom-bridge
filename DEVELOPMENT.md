# Smart Intercom Development Guide

This document summarizes the project setup from the initial conversation.

## Project Overview

ESP32-C6 Matter controller for IR204E analog intercom, providing:
- **Door Lock** - Unlock via Apple Home (Matter)
- **Contact Sensor** - Ring/buzz notification to HomeKit

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                      Matter Device                              │
│                                                                 │
│  Endpoint 1: Door Lock              Endpoint 2: Contact Sensor │
│  ┌─────────────────────┐            ┌─────────────────────┐    │
│  │ - Unlock → GPIO3    │            │ - GPIO2 interrupt   │    │
│  │ - Lock = no-op      │            │ - Shows "Open" when │    │
│  │ - Auto-relock 1s    │            │   someone rings     │    │
│  └─────────────────────┘            └─────────────────────┘    │
└────────────────────────────────────────────────────────────────┘
```

## Hardware Configuration

| Pin | Function | Hardware |
|-----|----------|----------|
| GPIO2 | Buzz input (pull-up, active LOW) | Optocoupler HCPL-817 |
| GPIO3 | Unlock output (active HIGH) | PhotoMOS AQY212EHA |

## Timing Constants

Defined in `main/app_priv.h`:

| Constant | Value | Purpose |
|----------|-------|---------|
| `UNLOCK_PULSE_MS` | 700 | Relay activation duration |
| `DEBOUNCE_MS` | 30 | Buzz signal filtering |
| `BUZZ_COOLDOWN_MS` | 2500 | Min time between ring events |
| `BUZZ_ACTIVE_MS` | 2000 | How long sensor stays "open" |

## First-Time Setup (ESP-Matter SDK)

The project requires ESP-Matter SDK. Run this once in your Dev Container:

```bash
# Clone ESP-Matter (takes a few minutes)
cd /opt/esp
git clone --recursive https://github.com/espressif/esp-matter.git

# Install Matter dependencies
cd /opt/esp/esp-matter
./install.sh

# Set environment (add to ~/.bashrc for persistence)
export ESP_MATTER_PATH=/opt/esp/esp-matter
source $ESP_MATTER_PATH/export.sh
```

## Build Commands

```bash
# Inside Dev Container (after ESP-Matter setup):

# Make sure environment is set
export ESP_MATTER_PATH=/opt/esp/esp-matter
source $ESP_MATTER_PATH/export.sh

# Set target (first time only)
cd /workspaces/intercom-matter
idf.py set-target esp32c6

# Build
idf.py build

# Flash (with device connected)
idf.py flash

# Monitor serial output
idf.py monitor

# Build, flash, and monitor
idf.py flash monitor

# Clean build
idf.py fullclean
```

## Key Files

| File | Purpose |
|------|---------|
| `main/app_main.cpp` | Creates Matter node with lock + contact sensor endpoints |
| `main/app_driver.cpp` | GPIO control, unlock pulse, buzz detection interrupt |
| `main/app_priv.h` | Pin definitions, timing constants |
| `main/lock/door_lock_manager.cpp` | Simplified lock/unlock logic |
| `main/lock/door_lock_callbacks.cpp` | Matter Door Lock cluster callbacks |

## How It Works

### Unlock Flow
1. User taps "Unlock" in Home app
2. Matter stack calls `emberAfPluginDoorLockOnDoorUnlockCommand()`
3. `BoltLockMgr().Unlock()` calls `app_driver_unlock_pulse()`
4. GPIO3 goes HIGH for 700ms
5. PhotoMOS relay closes, door latch releases
6. Auto-relock timer returns state to "Locked" after 1s

### Buzz Detection Flow
1. Visitor presses intercom button
2. Optocoupler pulls GPIO2 LOW
3. ISR triggers, debounce timer starts
4. After 30ms, if still LOW → valid ring
5. Contact sensor state → `true` (HomeKit: "Open")
6. After 2s, state → `false` (HomeKit: "Closed")

## Commissioning

1. Build and flash firmware
2. Device enters commissioning mode automatically
3. Open Apple Home → "+" → "Add Accessory"
4. Scan QR code from serial output, or use manual code
5. Device appears as "Door Lock" + "Contact Sensor"

### Get QR Code
```bash
# In monitor, or use command:
matter esp qrcode
```

### Factory Reset
```bash
matter esp factoryreset
```

## Simplified Design Decisions

| Removed Feature | Reason |
|-----------------|--------|
| PIN credentials | Intercom has no keypad |
| User management | Single household use |
| Schedules | Unnecessary for door release |
| Lock command hardware action | Door auto-locks by spring |

## Troubleshooting

### Build Errors
```bash
# Clean and rebuild
idf.py fullclean
idf.py build
```

### Missing Dependencies
```bash
# Update ESP-IDF components
idf.py reconfigure
```

### Serial Monitor Issues
- Use `/dev/ttyUSB0` or `/dev/ttyACM0` on Linux
- Use `/dev/cu.usbserial-*` on macOS
- Check `idf.py -p PORT flash` to specify port

## Original Requirements

From initial conversation:
- IR204E analog intercom integration
- ESP32-C6 (DFRobot Beetle, DigiKey DFR1117)
- PhotoMOS relay AQY212EHA for door unlock
- Optocoupler HCPL-817-000E for ring detection
- WAGO 221 connectors for non-destructive wiring
- Safe optical isolation (never direct GPIO to intercom)
- Matter/HomeKit integration
- 700ms unlock pulse, 30ms debounce, 2.5s cooldown

## Next Steps

1. ✅ Build firmware in Dev Container
2. ⬜ Flash to ESP32-C6
3. ⬜ Commission to Apple Home
4. ⬜ Wire optocoupler circuit
5. ⬜ Wire PhotoMOS relay circuit
6. ⬜ Connect to IR204E intercom
7. ⬜ Test unlock and ring detection


