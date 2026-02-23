# Apple Home Commissioning — Root Cause Analysis & Resolution

## Summary

The ESP32-C6 Matter device originally failed Apple Home commissioning because the 30-second Fabric-2 fail-safe expired before the Apple Home Hub could complete a CASE session over IP. After extensive debugging, the issue was resolved with two targeted patches to the `rs-matter` / `rs-matter-embassy` vendor crates. The device now commissions reliably with both Apple Home and Aqara Home.

Related: [rs-matter-embassy #37](https://github.com/sysgrok/rs-matter-embassy/issues/37)

## Commissioning Flow (Matter over WiFi with BLE)

Apple Home commissions in two phases:

1. **BLE phase** — PASE pairing, device attestation, WiFi credentials, AddNOC for **Fabric 1** (Apple's admin fabric)
2. **IP phase** — After WiFi connects and mDNS starts, the Hub opens a CASE session over WiFi, subscribes to the device, then commissions **Fabric 2** (Apple's operational/HomeKit fabric) with a **30-second fail-safe**

The critical requirement: the Hub must discover the device via mDNS, establish a CASE session, run AddNOC, and send CommissioningComplete — all within the fail-safe window.

## Why Commissioning Takes >30 Seconds

The 30-second fail-safe was designed for devices with hardware ECC accelerators. The ESP32-C6 (160 MHz RISC-V, no P-256 hardware acceleration) cannot finish the required crypto within that window.

### Software P-256 Crypto (Primary Bottleneck)

Matter commissioning requires multiple heavy elliptic curve operations:

| Step | Crypto operation | Approx. time on ESP32-C6 |
|------|-----------------|-------------------------|
| PASE/CASE key exchange | ECDH (P-256) | 2–5s |
| Attestation verification | ECDSA verify | 2–4s |
| CSR generation | ECDSA sign | 2–4s |
| NOC processing | ECDSA verify + key derivation | 2–5s |

**Total crypto time alone: ~10–18s**, leaving almost no margin for network overhead.

### MRP Retransmission Cascade

While the CPU crunches a multi-second ECDH computation, it cannot send MRP acknowledgements. The commissioner times out and re-sends, adding 1–3s per retry with exponential backoff. Typically 3–5 retransmissions occur per commissioning, adding **~5–10s**.

### mDNS Operational Discovery Gap

After `AddNOC`, the commissioner must discover the device's new operational instance via mDNS. The default `rs-matter` `BuiltinMdnsResponder` sends periodic announcements every **15 seconds**. If the commissioner misses the initial burst, it waits for the next cycle.

### Single-Core Cooperative Async

Embassy runs all tasks on one core. During crypto operations, mDNS responses and MRP ACKs are starved — there are no preemptive yield points in the crypto path. This amplifies all other delays.

### Wi-Fi Reconnect During Multi-Admin (Fixed)

During multi-admin commissioning (e.g., adding Aqara from Apple Home), the Matter stack re-calls `connect()` with the same Wi-Fi credentials. Without protection, this triggers a full Wi-Fi **stop → reconfigure → start** cycle, killing all active Matter sessions and causing "No valid session found" drops.

## Root Causes (Original Failure)

### 1. 30-Second Fail-Safe Too Short for Software Crypto

The commissioner (Apple Home Hub) sends `ArmFailSafe(30s)` for Fabric 2. With ~15–25s of crypto + retransmission + mDNS discovery overhead, the device cannot complete `CommissioningComplete` before the fail-safe expires.

### 2. Wi-Fi Reconnect Drops Active Sessions

The `EspWifiController::connect()` method unconditionally stopped and restarted Wi-Fi, even when already connected with the same SSID/password. During multi-admin flows, this killed active CASE sessions and forced the commissioner to retry from scratch.

### 3. Flash Persistence Offset Mismatch (Fixed Early)

The custom `EspFlash` driver was incorrectly treating `embedded-storage`'s region-relative offsets as absolute flash addresses. This corrupted Matter KVS data and caused `Error::StdIoError` panics at startup.

### 4. Stale CASE Session from Previous Commissioning

The Apple Home Hub remembers node identities from previous failed attempts. When the device appears on the network, the Hub tries the old identity first. The device rejects it, but the Hub never ACKs the error, causing 11 retransmissions over ~58 seconds before falling back to a fresh identity.

### 5. Debug Logging UART Saturation

At `Debug` log level, the `BuiltinMdnsResponder` floods UART with ~20 messages/second. Since `esp_println!` uses blocking I/O, this starves the CPU during commissioning and amplifies packet loss.

## Resolution

Two vendor patches are required for reliable commissioning:

### Patch 1: Fail-Safe Timeout Clamp (120s minimum)

**File:** `vendor/rs-matter/rs-matter/src/failsafe.rs`

Enforces a minimum 120-second fail-safe window on the device side, regardless of what the commissioner requests. This gives the ESP32-C6 enough time to complete all P-256 crypto, MRP retransmissions, and mDNS discovery.

```rust
const MIN_FAILSAFE_TIMEOUT_SECS: u16 = 120;

// In FailSafe::arm():
let effective_timeout_secs = timeout_secs.max(MIN_FAILSAFE_TIMEOUT_SECS);
```

**Why this is necessary:** The commissioner cannot be told to send a larger fail-safe value — `ArmFailSafe` timeout is decided by the commissioner, not the device. The `CommPolicy::failsafe_expiry_len_secs()` is informational only and Apple Home ignores it for Fabric 2.

### Patch 2: Wi-Fi Reconnect Guard

**File:** `vendor/rs-matter-embassy/src/wifi/esp.rs`

Skips redundant Wi-Fi stop/start cycles when `connect()` is called with the same credentials and the device is already connected. Uses a simple FNV-1a hash to detect identical credential sets.

```rust
// In EspWifiController::connect():
if ctl.is_connected()?
    && self.1.lock(|connected| connected.get())
    && self.2.lock(|last_hash| last_hash.get() == requested_hash)
{
    info!("Wifi already connected with requested credentials, skipping reconnect");
    return Ok(());
}
```

**Why this is necessary:** During multi-admin commissioning, the Matter stack calls `connect()` for each fabric. Without this guard, the Wi-Fi restart kills all active sessions and causes cascading failures.

### Other Fixes (In Main Codebase)

| Fix | File | Effect |
|-----|------|--------|
| Added `OperatingMode` + `SupportedOperatingModes` attrs | `src/drivers/matter/door_lock.rs` | DoorLock cluster spec-compliant |
| Per-module log filtering | `src/utils/uptime_logger.rs` | Suppresses noisy mDNS/BLE debug logs |
| Default log level set to `Info` | `src/main.rs` | Prevents UART saturation during commissioning |
| Fixed flash offset mismatch in KVS driver | `src/drivers/flash.rs` | Correct Matter KVS persistence |
| HEAP_SIZE increased to 210 KB | `src/main.rs` | Sufficient memory for Matter stack |

## Patches Explored and Rolled Back

These patches were tested during debugging but are **not required** for stable commissioning:

| Patch | File | Reason rolled back |
|-------|------|--------------------|
| Aggressive mDNS tuning (1s periodic, 30 bursts) | `vendor/rs-matter/.../builtin.rs` | Not needed with 120s fail-safe; added unnecessary traffic |
| MRP retransmission reduction (10→4, backoff 1.6→1.4) | `vendor/rs-matter/.../mrp.rs` | Caused premature exchange drops |
| BLE panic→retry conversion | `vendor/rs-matter-embassy/src/ble.rs` | Caused infinite retry loops instead of clean resets |
| Interruptible mDNS burst pacing | `vendor/rs-matter/.../builtin.rs` | Minor improvement, not worth the patch complexity |

## Commissioning Results

| Scenario | Status |
|----------|--------|
| Fresh commission to Apple Home | ✅ Works (~60s, within 120s fail-safe) |
| Fresh commission to Aqara Home | ✅ Works (even without patches) |
| Multi-admin: Apple Home → Aqara Home | ✅ Works (requires both patches) |
| Multi-admin: Aqara Home → Apple Home | ✅ Works (requires both patches) |
| Re-commission after device removal | ✅ Works (erase `matter_kvs` first recommended) |

## Potential Future Improvements

| Action | Expected improvement |
|--------|---------------------|
| Build with `--release` + LTO | Crypto 2–3× faster, may reduce total time to ~30–40s |
| Targeted mDNS speed-up (faster change-triggered burst) | Save ~5–10s on operational discovery |
| Yield points in crypto path (upstream `rs-matter`) | Allow MRP ACKs during ECDH — reduces retransmissions |
| Hardware with P-256 accelerator | Crypto drops to <1s per operation |

## Environment

- **Device:** ESP32-C6 (DFRobot Beetle), Rust/Embassy, `rs-matter` + `rs-matter-embassy`
- **Network:** TP-Link Omada SDN, device on 2.4 GHz WiFi, Apple Home Hub on 5 GHz WiFi
- **Hub:** Apple TV / HomePod (acts as Matter commissioner)
- **Alt Commissioner:** Aqara Hub M3 / Aqara Home app
- **C++ version:** Works within 30s fail-safe (ESP-IDF + mbedTLS likely has better-optimized P-256)

## Useful Commands

   ```bash
# Erase Matter KVS partition (clean commissioning state)
espflash erase-region --port /dev/cu.usbmodem2101 0x310000 0x6000

# Build + flash + monitor
cargo run

# Monitor only (after flash)
cargo espflash monitor --port /dev/cu.usbmodem2101
```
