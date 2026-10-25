# OTA Firmware Update Troubleshooting & Dual-Bank Partition Resolution

**Date:** 2026-10-15  
**Component:** `intercom-adapter-ota`, `scripts/ota.py`, ESP-IDF Dual-Bank Bootloader  
**Target Hardware:** ESP32-C6-WROOM-1-N8 (8 MB Quad SPI Flash)

---

## 1. Executive Summary

During testing of the Matter subscription interval clamping feature, wireless OTA updates and USB flashing encountered three distinct edge cases:
1. **mDNS discovery timeout:** `scripts/ota.py` could not discover the device via `_intercom-ota._tcp.local.`.
2. **Dual-bank partition mismatch (A/B slot confusion):** `espflash flash` wrote new binaries to `ota_0`, but the ESP-IDF 2nd-stage bootloader continued booting the stale firmware from `ota_1`.
3. **USB ROM download bootloader latch:** The chip remained halted in download boot mode (`boot:0x4 (DOWNLOAD)`).

This document details the root causes and the exact solutions implemented to resolve each issue.

---

## 2. Issue 1: Dual-Bank Bootloader Partition Confusion (Silent Flash Failure)

### Symptom
After running `espflash flash target/riscv32imac-unknown-none-elf/release/intercom-bridge`, the terminal reported:
```text
[INFO ] Flashing has completed!
```
However, upon inspection of serial logs after reboot, the device was **still running the previous firmware revision**:
- `softwareVersion` remained `100092` instead of `100109`.
- Subscriptions still granted `max_int_secs: 600` instead of `120`.

### Root Cause
The ESP32-C6 has an 8 MB flash partitioned according to [`partitions.csv`](../partitions.csv):
```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
matter_kvs,   data, nvs,     0x6c0000, 0x6000,
```

The device's `otadata` partition (at `0x00f000`) tracks the active boot slot using `esp_ota_select_entry_t`. The bootloader reported:
```text
OTA Bootloader initialized: active slot=Ota1, seq=2, state=Valid
```
By default, `espflash flash` without partition parameters writes only to the **first** app partition (`ota_0` at `0x20000`). Because `otadata` had sequence number 2 pointing to `ota_1` (`0x370000`), the 2nd-stage bootloader completely ignored `ota_0` and launched the old firmware in `ota_1`.

### Resolution
Instruct `espflash` to write specifically to the active partition slot (`--target-app-partition ota_1`):
```bash
espflash flash target/riscv32imac-unknown-none-elf/release/intercom-bridge \
  --port /dev/cu.usbmodem14201 \
  --partition-table partitions.csv \
  --target-app-partition ota_1 \
  --non-interactive
```
The bootloader confirmed launching the new binary from `ota_1`:
```text
I (146) esp_image: segment 1: paddr=003b5148 vaddr=40800000 size... (0x370000 base)
00:00:03.335 DEBUG EventTLVWrite::StartUp::softwareVersion -> 100109
00:00:03.345 INFO  OTA Bootloader initialized: active slot=Ota1, seq=2, state=Valid
```

---

## 3. Issue 2: Initial OTA Connection Timeout (Port 8080)

### Symptom
Running `python3 scripts/ota.py --target 192.168.1.100 target/...` failed with:
```text
❌ Connection error communicating with Manual Target (192.168.1.100:8080): timed out
```

### Root Cause
The firmware image running on the device prior to our flash was built before the `OtaServer` background task was wired into `main.rs`. Because `OtaServer` was not running, the TCP port 8080 socket was closed.

### Resolution
Once the updated firmware was flashed to the active slot (`ota_1`), the composition root in `main.rs` initialized the server:
```text
00:00:03.346 INFO  intercom_adapter_ota::server - OTA Server listening on TCP port 8080
```
Verified via TCP probe:
```python
s = socket.socket(); s.connect(('192.168.1.100', 8080))
# Port 8080 is OPEN and reachable!
```

---

## 4. Issue 3: mDNS Auto-Discovery vs. Direct IP Targeting

### Symptom
Running `scripts/ota.py` without flags scanned the local network for `_intercom-ota._tcp.local.` and printed:
```text
No Intercom OTA devices found on local network.
```

### Root Cause
Matter devices advertise standard Matter mDNS records (`_matter._tcp`, `_matterc._udp`) through `rs-matter-stack`. A dedicated mDNS service announcement for `_intercom-ota._tcp` was not yet registered in the network stack, so Zeroconf discovery could not resolve the hostname.

### Resolution
Use the `--target` CLI flag to push directly to the device IP or hostname:
```bash
python3 scripts/ota.py --target 192.168.1.100 target/riscv32imac-unknown-none-elf/release/intercom-bridge
```

---

## 5. Issue 4: USB ROM Bootloader Download Mode Latch

### Symptom
Connecting with `python3 scripts/monitor.py --no-reset` showed:
```text
ESP-ROM:esp32c6-20220919
rst:0x15 (USB_UART_HPSYS),boot:0x4 (DOWNLOAD(USB/UART0/SDIO_FEI_FEO))
waiting for download
```
The CPU was completely halted in ROM bootloader mode and did not boot into flash.

### Root Cause
`espflash` uses USB-JTAG-Serial DTR and RTS signals to put the ESP32 into ROM download mode for flashing. Passing `--no-reset` to `monitor.py` suppresses DTR/RTS manipulation, leaving the chip in bootloader mode.

### Resolution
Run `monitor.py` without `--no-reset` (or with reset enabled):
- DTR is cleared (GPIO9 pulled HIGH $\rightarrow$ normal SPI flash boot).
- RTS is pulsed HIGH then LOW (EN pin reset).
The chip exits download mode and executes the 2nd-stage bootloader normally.

---

## 6. How Dual-Bank Fail-Safe OTA Works (Zero-Brick Guarantee)

Now that `intercom-adapter-ota` is running on `ota_1`:

```
Flash Layout (8 MB):
0x00f000: [ otadata ] -> Points to active slot (e.g. ota_1, seq=2, state=Valid)
0x020000: [ ota_0   ] -> Inactive Slot (3.4 MB) -> Target for NEXT OTA update
0x370000: [ ota_1   ] -> Active Slot (3.4 MB) -> Running firmware
```

### The Update Cycle:
1. **Push:** `scripts/ota.py` sends signed binary stream over HTTP to port 8080.
2. **Zero In-Place Risk:** `OtaWriter` streams the update exclusively into the **inactive slot** (`ota_0`). The running firmware in `ota_1` is completely untouched.
3. **Pre-Commit Ed25519 Verification:** Signature is verified over the entire payload *before* touching `otadata`. If invalid, `ota_0` is discarded; system continues running.
4. **Bootloader Handoff:** `otadata` writes `seq=3`, `state=New` (`0x0`) to boot `ota_0`.
5. **Self-Validation:** When the new firmware boots and establishes network connectivity, it calls `mark_running_slot_valid()`, transitioning to `Valid` (`0x2`).
6. **Automatic Fallback:** If the new image panics or watchdogs before validation, the bootloader automatically reverts to `ota_1` on the subsequent reset.
