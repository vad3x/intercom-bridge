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

### 5. Missing mDNS Service Subtypes (Primary Cause of ~120s Delay)

The Rust `BuiltinMdnsResponder` did not include compressed fabric ID subtypes (`_I<fabric-id>._sub._matter._tcp`) in mDNS records. After `AddNOC`, Apple uses these subtypes to quickly identify the device's operational instance. Without them, Apple falls back to a slow generic browse, adding ~120 seconds per fabric.

### 6. Debug Logging UART Saturation

At `Debug` log level, the `BuiltinMdnsResponder` floods UART with ~20 messages/second. Since `esp_println!` uses blocking I/O, this starves the CPU during commissioning and amplifies packet loss.

## Resolution

Three vendor patches are required for reliable commissioning:

### Patch 1: Fail-Safe Timeout Clamp (120s minimum)

**File:** `vendor/rs-matter/rs-matter/src/failsafe.rs`

Enforces a minimum 120-second fail-safe window on the device side, regardless of what the commissioner requests. This gives the ESP32-C6 enough time to complete all P-256 crypto, MRP retransmissions, and mDNS discovery.

```rust
const MIN_FAILSAFE_TIMEOUT_SECS: u16 = 120;

// In FailSafe::arm():
let effective_timeout_secs = timeout_secs.max(MIN_FAILSAFE_TIMEOUT_SECS);
```

**Why this is necessary:** The commissioner cannot be told to send a larger fail-safe value — `ArmFailSafe` timeout is decided by the commissioner, not the device. The `CommPolicy::failsafe_expiry_len_secs()` is informational only and Apple Home ignores it for Fabric 2.

### Patch 2: mDNS Compressed Fabric ID Subtypes

**Files:** `vendor/rs-matter/rs-matter/src/transport/network/mdns.rs`, `vendor/rs-matter/.../builtin/proto.rs`

Adds `_I<compressed-fabric-id>` service subtypes to the `Commissioned` mDNS service and includes them in PTR query responses. Apple uses these subtypes to quickly find the device's operational instance after `AddNOC`.

```rust
// In MatterMdnsService::Commissioned — populate subtype:
let mut subtype_str = heapless::String::<24>::new();
write!(subtype_str, "_I{:016X}", compressed_fabric_id).unwrap();
service_subtypes: &[subtype_str.as_str()],

// In answer_one — include subtypes in PTR response:
service.add_service_subtypes(answer, ttl_sec)?;
```

**Why this is necessary:** C++ firmware includes fabric subtypes in every mDNS packet. Without them, Apple falls back to a slow generic service browse, adding ~120 seconds per fabric to commissioning.

**Note:** Subtypes are included in PTR **query responses** only (not proactive broadcasts), because the full broadcast packet with subtypes exceeds the 1232-byte mDNS TX buffer limit.

### Patch 3: Wi-Fi Reconnect Guard

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
| Subtypes in mDNS broadcast | `vendor/rs-matter/.../builtin/proto.rs` | Overflows 1232B TX buffer → `BufferTooSmall` on every broadcast |

## Commissioning Results

| Scenario | Status |
|----------|--------|
| Fresh commission to Apple Home | ✅ Works (~60s, within 120s fail-safe) |
| Fresh commission to Aqara Home | ✅ Works (even without patches) |
| Multi-admin: Apple Home → Aqara Home | ✅ Works (requires both patches) |
| Multi-admin: Aqara Home → Apple Home | ✅ Works (requires both patches) |
| Re-commission after device removal | ✅ Works (erase `matter_kvs` first recommended) |

## mDNS Fixes (Resolved)

### Issue 1: Missing TXT Records in Broadcasts

`Host::broadcast` in `proto.rs` had `service.add_txt()` commented out. Broadcasts included PTR + SRV + A/AAAA but no TXT records. Uncommented the call.

### Issue 2: Dummy TXT Content for Commissioned Services

`MatterMdnsService::Commissioned` advertised `dummy=dummy` instead of the required `SII`/`SAI`/`T` operational TXT records per Matter Core Spec §4.3.1. Fixed to publish correct values (`SII=5000`, `SAI=300`, `T=0`).

### Issue 3: SRV/TXT Queries Not Answered for Fabric 1 (CONFIRMED FIXED)

**Discovery:** Side-by-side pcap comparison (`mdns_capture.pcap` C++ vs `mdns_capture_rust-1.pcap` Rust) showed that after `AddNOC` for Fabric 1, the phone's SRV+TXT queries for the device's operational instance name received **zero answers** — 122 unanswered queries over ~2 minutes (`new-log52.log`). Fabric 2's SRV/TXT queries worked fine.

**Root cause:** The combination of Issues 1 and 2 above. Without proper TXT records, Apple's mDNS resolver could not complete service resolution and kept retrying.

**Fix verification (`new-log54.log` + `mdns_capture_rust-3.pcap`):**

| Fabric | Instance Name | SRV Query Time | Result |
|--------|--------------|:---:|:---:|
| **Fabric 1** (Aqara) | `FD99106B3D86A0DE-000000003ED0E7CB` | 00:01:12 | **HIT** ✅ 279B reply |
| **Fabric 2** (Apple) | `DC86FEB5BB9F8FDF-000000001200A4C9` | 00:03:20 | **HIT** ✅ 279B reply |

Both fabrics now resolve instantly via mDNS. The device correctly answers SRV+TXT queries for all its operational instance names.

### Issue 4: Missing `_I<compressed-fabric-id>` Service Subtypes (ROOT CAUSE OF ~120s DELAY)

**Discovery:** C++ vs Rust pcap comparison revealed the **primary reason** for the ~120s delay per fabric. The C++ firmware includes fabric-specific service subtypes in every mDNS packet:

```
_matter._tcp.local PTR 47F49EDF43015297-0000000003C8DC09._matter._tcp.local.
_I47F49EDF43015297._sub._matter._tcp.local PTR 47F49EDF43015297-...  ← SUBTYPE
```

The Rust firmware had `service_subtypes: &[]` — **empty**. No subtype was ever advertised.

**Why this matters:** After `AddNOC`, Apple knows the compressed fabric ID and uses a targeted subtype browse for `_I<fabric-id>._sub._matter._tcp.local` to discover the operational device. Without subtypes, Apple falls back to a slow generic `_matter._tcp.local` browse and retries for ~120 seconds before completing discovery. This explains why C++ commissions in **~8s per fabric** while Rust took **~122s per fabric**.

| | C++ (pcap) | Rust (before fix) |
|---|:---:|:---:|
| Service subtypes | `_I47F49EDF43015297._sub._matter._tcp` ✅ | **Empty** (`&[]`) ❌ |
| AddNOC → CASE Sigma1 (Fabric 1) | **8.5s** | **122s** |
| AddNOC → CASE Sigma1 (Fabric 2) | **1.1s** | **122s** |
| Total commissioning | **~53s** | **~330s** |

**Key insight: Apple does NOT query for subtypes.** It learns the fabric→instance mapping **passively** from the device's mDNS packets. The C++ device includes subtype PTR records in every broadcast response.

**Fix (two parts):**

1. **Populated `service_subtypes`** in `MatterMdnsService::Commissioned` (`vendor/rs-matter/.../mdns.rs`):

   ```rust
   // Compressed fabric ID subtype per Matter Core Spec §4.3.1
   let mut subtype_str = heapless::String::<24>::new();
   write!(subtype_str, "_I{:016X}", compressed_fabric_id).unwrap();
   // ...
   service_subtypes: &[subtype_str.as_str()],
   ```

2. **Added subtypes to the PTR query answer** in `answer_one` (`vendor/rs-matter/.../builtin/proto.rs`):

   ```rust
   // When Apple queries _matter._tcp.local PTR, include subtype PTR records
   // so Apple learns the fabric→instance mapping from this response.
   service.add_service_subtypes(answer, ttl_sec)?;
   ```

**Why not include subtypes in broadcasts?** Attempted but causes `BufferTooSmall` — the mDNS TX buffer is **1232 bytes** (IPv6 min MTU 1280 − 40 IPv6 hdr − 8 UDP hdr). With PTR + SRV + TXT + subtypes for multiple services, the broadcast packet overflows. Including subtypes only in the PTR **query response** path works because the response is much smaller (~551B) — Apple queries for `_matter._tcp.local PTR` and gets the subtype records in the reply.

**Buffer size fix:** The `service_subtype_fqdn` buffer in `proto.rs` was `heapless::String::<40>` but the full FQDN `_IFD99106B3D86A0DE._sub._matter._tcp.local.` needs **44 bytes**. Increased to `String::<64>` for headroom.

## Remaining Apple Delay

With the subtype fix, Apple should now discover the device via the targeted `_I<fabric-id>` subtype in the PTR response instead of falling back to the slow generic browse. If a delay persists, it is Apple's cloud Device Attestation Verification (DAV) processing test certificates.

**Workaround:** The 180s minimum fail-safe timeout (`failsafe.rs`) accommodates any remaining delay.

## Potential Future Improvements

| Action | Expected improvement |
|--------|---------------------|
| Build with `--release` + LTO | Crypto 2–3× faster, may reduce total time to ~30–40s |
| Targeted mDNS speed-up (faster change-triggered burst) | Save ~5–10s on operational discovery |
| Yield points in crypto path (upstream `rs-matter`) | Allow MRP ACKs during ECDH — reduces retransmissions |
| Hardware with P-256 accelerator | Crypto drops to <1s per operation |
| Increase mDNS TX buffer for subtypes in broadcasts | Would allow subtypes in proactive broadcasts (currently limited to PTR query responses) |
| Real PAI/DAC certificates | May eliminate any Apple DAV cloud delay on test certs |

## Environment

- **Device:** ESP32-C6 (DFRobot Beetle), Rust/Embassy, `rs-matter` + `rs-matter-embassy`
- **Network:** TP-Link Omada SDN, device on 2.4 GHz WiFi, Apple Home Hub on 5 GHz WiFi
- **Hub:** Apple TV / HomePod (acts as Matter commissioner)
- **Alt Commissioner:** Aqara Hub M3 / Aqara Home app
- **C++ version:** Commissions in ~53s total (ESP-Matter SDK includes proper mDNS subtypes + mbedTLS with optimized P-256)

## Useful Commands

   ```bash
# Erase Matter KVS partition (clean commissioning state)
espflash erase-region --port /dev/cu.usbmodem2101 0x310000 0x6000

# Build + flash + monitor
cargo run

# Monitor only (after flash)
cargo espflash monitor --port /dev/cu.usbmodem2101
```
