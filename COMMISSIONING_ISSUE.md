# Apple Home Commissioning Failure — Root Cause Analysis

## Summary

The ESP32-C6 Matter device fails to complete Apple Home commissioning. The first fabric (Fabric 1) commissions successfully, but the second fabric (Fabric 2 — Apple's operational fabric) always fails because its 30-second fail-safe timer expires before the Apple Home Hub establishes a CASE session over IP.

This is the same issue as [rs-matter-embassy #37](https://github.com/sysgrok/rs-matter-embassy/issues/37).

## Commissioning Flow (Matter over WiFi with BLE)

Apple Home commissions in two phases:

1. **BLE phase** — PASE pairing, device attestation, WiFi credentials, AddNOC for **Fabric 1** (Apple's admin fabric)
2. **IP phase** — After WiFi connects and mDNS starts, the Hub opens a CASE session over WiFi, subscribes to the device, then commissions **Fabric 2** (Apple's operational/HomeKit fabric) with a **30-second fail-safe**

The critical requirement: the Hub must discover the device via mDNS, establish a CASE session, run AddNOC, and send CommissioningComplete — all within 30 seconds.

## What Happens (from `new-log5.log`)

| Time | Event | Status |
|------|-------|--------|
| 00:01:36 | ArmFailSafe(120s) via BLE | ✅ |
| 00:01:42 | AddNOC → Fabric 1 added | ✅ |
| 00:01:42 | ArmFailSafe(177s), ConnectNetwork | ✅ |
| 00:01:47 | WiFi up, IP 192.168.0.136 | ✅ |
| 00:01:51 | mDNS starts broadcasting | ✅ |
| 00:01:51 | CASE Sigma1 from stale node `b7da0709…` | ❌ "Fabric Index mismatch" |
| 00:01:51–00:02:49 | Device retransmits StatusReport 11× — Hub never ACKs | ⏳ 58s wasted |
| 00:02:49 | New CASE Sigma1 from correct node `d21069b0…` | ✅ |
| 00:02:51 | CommissioningComplete → Fabric 1 done | ✅ |
| 00:02:53 | ArmFailSafe(**30s**) — Fabric 2 commissioning starts | ⏱️ |
| 00:02:55 | AddNOC → Fabric 2 added to device | ✅ |
| ~00:03:23 | **30s fail-safe expires** (no CommissioningComplete received) | ❌ |
| 00:03:55 | CASE Sigma1 for Fabric 2 finally arrives (59s after AddNOC) | ✅ (too late) |
| 00:03:58 | CommissioningComplete → **NoFailSafe** | ❌ |
| 00:03:58 | Hub removes Fabric 1, device left in limbo | 💀 |

## Root Causes

### 1. Stale CASE Session from Previous Commissioning

The Apple Home Hub remembers a node identity (`b7da0709…`) from a **previous failed commissioning attempt**. When the device appears on the network, the Hub immediately tries to reach it using the old identity. The device (which was erased) rejects it with "Fabric Index mismatch" and sends a StatusReport error.

The Hub **never acknowledges** this StatusReport, so the device retransmits it 11 times over **58 seconds** with exponential backoff. Only after giving up does the Hub retry with the correct identity.

**Impact:** Wastes ~60 seconds before Fabric 1's CASE session succeeds.

### 2. ~60-Second mDNS Discovery Delay for Fabric 2

After Fabric 2's AddNOC, the device updates its mDNS records with a new operational instance ID. The Hub needs to discover this new instance and initiate a CASE session. This takes **~59 seconds**, but the fail-safe is only **30 seconds**.

The delay is likely caused by:
- The Hub's mDNS browse/retry interval (~60 seconds)
- Possible multicast propagation delays between 2.4 GHz (device) and 5 GHz (Hub) on the Omada network

### 3. RX QUEUE FULL During Crypto Operations

During ECDSA signing (CSR Request), the CPU is fully occupied for ~1 second, causing `RX QUEUE FULL` warnings. While not the primary cause, this contributes to packet loss and retransmissions.

## Environment

- **Device:** ESP32-C6 (DFRobot Beetle), Rust/Embassy, `rs-matter` + `rs-matter-embassy`
- **Network:** TP-Link Omada SDN, device on 2.4 GHz WiFi, Apple Home Hub on 5 GHz WiFi
- **Hub:** Apple TV / HomePod (acts as Matter commissioner)
- **C++ version:** Works fine with the same network setup (ESP-IDF + ESP-Matter SDK)

## Diagnostic Findings

### mDNS Works Across Bands

`dns-sd -B _matterc._udp` from a Mac on the Omada network **does** see the device's commissioning advertisement, confirming multicast flows between 2.4 GHz and 5 GHz.

### DoorLock Attributes (Fixed)

Two mandatory DoorLock attributes were missing (`OperatingMode` and `SupportedOperatingModes`). These have been added but are not the cause of the commissioning timeout.

### Debug Logging Exacerbates the Issue

Setting log level to `Debug` causes the `BuiltinMdnsResponder` to flood the UART with mDNS service enumeration (~20 messages/second). Since `esp_println!` uses blocking I/O, this starves the CPU and causes packet loss. Per-module log filtering was added to `uptime_logger.rs` to suppress noisy modules while keeping debug output for transport/CASE/PASE.

### Bump Allocator Memory

`BUMP_SIZE` is set to 36000 bytes. After initialization, ~19748 bytes remain free — sufficient for CASE sessions and subscriptions.

## Fixes Applied So Far

| Fix | File | Effect |
|-----|------|--------|
| Added `OperatingMode` + `SupportedOperatingModes` attrs | `src/drivers/matter/door_lock.rs` | DoorLock cluster now spec-compliant |
| Per-module log filtering | `src/utils/uptime_logger.rs` | Suppresses mDNS/bump spam at Debug level |
| BUMP_SIZE restored to 36000 | `src/drivers/matter/mod.rs` | Sufficient memory for CASE sessions |

## Next Steps

### Immediate: Clean Stale State

1. **Remove the device from Apple Home** (Settings → Home → device → Remove Accessory)
2. **Erase the matter_kvs partition:**
   ```bash
   espflash erase-region --port /dev/cu.usbserial-210 0x310000 0x6000
   ```
3. Re-flash and attempt commissioning from a clean state

Without the stale `b7da0709…` identity, the first CASE session should succeed in milliseconds (not 58 seconds), which may leave enough time for Fabric 2 to complete within 30 seconds.

### If Still Failing: Test Network Isolation

Commission using an iPhone hotspot (both device and Hub on same 2.4 GHz network) to rule out Omada multicast issues:
- Change WiFi SSID/password in code to match hotspot
- Flash and commission

### Upstream: rs-matter-embassy #37

The ~60-second CASE discovery delay for Fabric 2 may be an issue with how `BuiltinMdnsResponder` announces new operational instances after AddNOC. The C++ ESP-IDF mDNS implementation sends gratuitous announcements immediately; `rs-matter` may not. This needs investigation in the upstream `rs-matter` / `rs-matter-embassy` crates.
