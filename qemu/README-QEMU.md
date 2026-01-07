# QEMU Emulation for Smart Intercom

This directory contains tools for running the Smart Intercom firmware in QEMU ESP32-C6 emulator.

## Overview

QEMU emulation allows you to:
- Test HTTP API endpoints without hardware
- Validate firmware builds
- Debug application logic
- Develop while waiting for hardware to arrive

## Limitations

QEMU ESP32-C6 emulation has limitations:

| Feature | QEMU Support | Notes |
|---------|--------------|-------|
| HTTP Server | ✅ Full | Works via host networking |
| FreeRTOS Tasks | ✅ Full | Task scheduling works normally |
| GPIO Output | ⚠️ Limited | Unlock pulse logged but no physical output |
| GPIO Interrupt | ⚠️ Limited | Buzz detection may not trigger |
| WiFi | ❌ None | Uses simulated host networking instead |
| Precise Timing | ⚠️ Varies | May differ from real hardware |

## Quick Start

### Prerequisites

- Docker and Docker Compose installed
- Firmware built (either locally or via Dev Container)

### Build Firmware (if not already built)

```bash
# Option 1: Build inside Docker
docker-compose --profile build up build

# Option 2: Build in Dev Container (VS Code)
# Open project in Dev Container, then run:
idf.py build
```

### Run in QEMU

```bash
# Start QEMU emulator
./run-qemu.sh

# In another terminal, test the API
./test-api.sh
```

### Manual Testing

While QEMU is running:

```bash
# Get system status
curl http://localhost:8080/status

# Trigger door unlock
curl -X POST http://localhost:8080/unlock

# Watch QEMU logs for GPIO operations
```

## Files

| File | Purpose |
|------|---------|
| `docker-compose.yml` | Docker configuration for QEMU and build services |
| `run-qemu.sh` | Script to launch QEMU with firmware |
| `test-api.sh` | Script to test HTTP API endpoints |
| `gpio-sim.py` | Python script for GPIO simulation (experimental) |

## Network Configuration

QEMU is configured with host networking:
- Firmware HTTP server (port 80) → Host port 8080
- QEMU monitor → Host port 5555
- GDB server → Host port 1234

## Debugging

### Connect to QEMU Monitor

```bash
telnet localhost 5555
```

Monitor commands:
- `info registers` - Show CPU registers
- `info status` - Show emulation status
- `quit` - Exit QEMU

### GDB Debugging

```bash
# In QEMU, add to command: -s -S
# Then connect GDB:
riscv32-esp-elf-gdb build/intercom-matter.elf
(gdb) target remote localhost:1234
(gdb) continue
```

## Troubleshooting

### "Firmware not found"

Build the firmware first:
```bash
docker-compose --profile build up build
```

### "Connection refused" on port 8080

- QEMU may still be starting up (wait a few seconds)
- Check QEMU logs for errors
- Verify firmware initialized HTTP server

### GPIO events not triggering

This is expected - QEMU GPIO emulation is limited. The firmware will still:
- Log when unlock is triggered
- Track unlock count
- Respond correctly to API calls

Test on real hardware for actual GPIO behavior.

## Production Testing

QEMU is for development only. Before deploying:

1. Flash firmware to real ESP32-C6
2. Test actual GPIO timing with oscilloscope
3. Verify optocoupler/PhotoMOS circuits
4. Test with real intercom system



