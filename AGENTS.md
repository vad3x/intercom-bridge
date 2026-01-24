# AGENTS.md

Guidelines for AI agents working on the Smart Intercom codebase.

## Project Overview

ESP32-C6 Matter controller for IR204E analog intercom. Provides door lock control and ring detection via Apple Home (HomeKit) using the Matter protocol.

**Tech Stack:**

- **Hardware:** ESP32-C6 (DFRobot Beetle)
- **Framework:** ESP-IDF v5.x with ESP-Matter SDK
- **Protocol:** Matter over WiFi (BLE for commissioning)
- **Language:** C++ (ESP-IDF style)

## Build Environment

⚠️ **CRITICAL:** This project must be built inside a Docker container. The ESP-Matter SDK is installed at `/opt/espressif/esp-matter/` inside the container.

### Matter SDK Source Locations

Inside the Docker container:

```bash
# List running containers and pick the intercom one
docker ps

# Open a shell in the container
docker exec -it <container_name_or_id> bash
```

Key paths:

- `/opt/espressif/esp-matter/` — ESP-Matter SDK root
- `/opt/espressif/esp-matter/connectedhomeip/` — upstream Matter (CHIP) source
- `/opt/espressif/esp-matter/components/esp_matter/` — ESP-Matter wrappers (clusters, attributes, endpoints)

### Build Commands

```bash
# Enter the Docker container
docker exec -it charming_easley bash

# Source the ESP-Matter environment (required before any idf.py commands)
source /opt/espressif/esp-matter/export.sh

# Navigate to workspace
cd /workspaces/intercom-matter

# Build
idf.py build

# Full clean (when config changes)
idf.py fullclean && idf.py build
```

### Flashing

From the **host machine** (not Docker), use the flash script:

```bash
./flash.sh
```

Or manually with esptool:

```bash
esptool --chip esp32c6 -p /dev/cu.usbserial-210 -b 460800 \
  --before default-reset --after hard-reset write-flash \
  --flash-mode dio --flash-freq 80m --flash-size 4MB \
  0x0 build/bootloader/bootloader.bin \
  0x20000 build/intercom-matter.bin \
  0xc000 build/partition_table/partition-table.bin
```

## Project Structure

```
intercom-matter/
├── main/
│   ├── app_main.cpp          # Entry point
│   ├── app.cpp/h             # Application class, wires components
│   ├── config/
│   │   └── app_config.h      # GPIO pins, timing constants
│   ├── drivers/
│   │   ├── gpio_ring_driver.cpp/h       # Ring detection (optocoupler input)
│   │   ├── gpio_door_driver.cpp/h       # Door unlock (PhotoMOS relay output)
│   │   └── matter_access_control_driver.cpp/h  # Matter endpoints
│   ├── services/
│   │   └── led_service.cpp/h # LED control (WS2812)
│   ├── events/
│   │   ├── events.h          # Event types
│   │   └── listener.h        # Event subscription
│   └── actors/
│       └── *.h               # Actor interfaces
├── sdkconfig.defaults        # Default SDK configuration
├── sdkconfig.defaults.esp32c6 # ESP32-C6 specific overrides
├── partitions.csv            # Flash partition layout
└── CMakeLists.txt
```

## Key Files to Understand

| File | Purpose |
|------|---------|
| `main/drivers/matter_access_control_driver.cpp` | Matter integration, endpoints, commissioning |
| `main/config/app_config.h` | Hardware constants (GPIO pins, timing) |
| `main/services/led_service.cpp` | Status LED control |
| `sdkconfig.defaults.esp32c6` | Chip-specific configuration |

## Architecture Patterns

### Actor Model

Components communicate via typed events through an EventBus:

- `RingingEvent` - Someone pressed the intercom button
- `UnlockDispatchedEvent` - Unlock command received from HomeKit

### Driver Pattern

Hardware drivers receive configuration at construction:

```cpp
GpioRingDriver ringDriver_({
    .gpioPin = RING_GPIO,
    .debounceMs = RING_DEBOUNCE_MS,
    .cooldownMs = RING_COOLDOWN_MS
});
```

### Matter Endpoints

- **Endpoint 1:** Door Lock Cluster (unlock command)
- **Endpoint 2:** Occupancy Sensor (ring detection, appears as Motion Sensor in HomeKit)

## Common Tasks

### Adding a New Driver

1. Create header in `main/drivers/` with config struct
2. Create implementation with initialization in constructor
3. Wire into `App` class in `main/app.cpp`
4. Update `main/CMakeLists.txt` if needed

### Modifying Matter Clusters

Edit `main/drivers/matter_access_control_driver.cpp`:

- Node configuration in `init()` method
- Attribute changes via `esp_matter::attribute::update()`
- Command handlers via callbacks

### Changing GPIO Pins

Edit `main/config/app_config.h`:

```cpp
#define RING_GPIO 2    // Input: optocoupler
#define UNLOCK_GPIO 3  // Output: PhotoMOS relay
```

## Configuration

### SDK Configuration Files

- `sdkconfig.defaults` - Base configuration for all builds
- `sdkconfig.defaults.esp32c6` - ESP32-C6 overrides
- `sdkconfig` - Generated file (do not edit directly)

### Important Config Options

| Option | Value | Purpose |
|--------|-------|---------|
| `CONFIG_ENABLE_ICD_SERVER` | n | Device always-on, not sleep-optimized |
| `CONFIG_ENABLE_WIFI_STATION` | y | WiFi mode enabled |
| `CONFIG_OPENTHREAD_ENABLED` | n | Thread disabled |
| `CONFIG_ENABLE_CHIP_SHELL` | y | Matter CLI commands available |

### Partition Table

Single factory partition (3MB) for simplicity:

```csv
nvs,      data, nvs,     0x9000,   24K,
nvs_keys, data, nvs_keys, 0xf000,  4K,
phy_init, data, phy,     0x10000,  4K,
fctry,    data, nvs,     0x11000,  24K,
esp_secure_cert, 0x3F, ,0x17000, 0x6000,
factory,  app,  factory, 0x20000,  3M,
```

## Code Style

- Follow ESP-IDF naming conventions
- Use `ESP_LOGI`, `ESP_LOGW`, `ESP_LOGE` for logging
- Prefer static allocation over dynamic
- Use FreeRTOS primitives for timing/synchronization
- Include `<inttypes.h>` for portable printf format macros (`PRIu32`, etc.)

## Debugging

### Serial Monitor

```bash
# Inside Docker
idf.py monitor

# Or from host with screen/minicom
screen /dev/cu.usbserial-210 115200
```

### Matter CLI Commands

```bash
matter esp qrcode        # Show QR code for commissioning
matter esp factoryreset  # Erase commissioning data
matter onboardingcodes ble  # Show manual pairing code
```

### Common Issues

| Issue | Solution |
|-------|----------|
| Build fails with undefined references | Run `idf.py fullclean && idf.py build` |
| Device not enumerating on USB | Use UART port, enter download mode (BOOT+RESET) |
| Apple Home "Unable to add" | Try Aqara Home, check QR code matches logs |
| Device stuck with yellow LED | Check sdkconfig for power management settings |

## ESP32-C6 Recovery

If native USB stops working:

1. Connect to UART USB port (not native USB)
2. Check port: `ls /dev/cu.usbserial*`
3. Enter download mode: Hold BOOT → Tap RESET → Release BOOT
4. Erase flash: `esptool --chip esp32c6 -p /dev/cu.usbserial-210 erase-flash`

## Testing Changes

1. Build in Docker: `idf.py build`
2. Flash from host: `./flash.sh`
3. Monitor logs for errors
4. Test with Aqara Home or chip-tool

## Dependencies

Managed components in `managed_components/`:

- `espressif__led_strip` - WS2812 LED control
- `espressif__button` - Button handling
- `espressif__mdns` - mDNS for Matter discovery

ESP-Matter provides:

- Matter protocol stack
- Door Lock cluster implementation
- Occupancy Sensing cluster implementation
- BLE/WiFi commissioning
