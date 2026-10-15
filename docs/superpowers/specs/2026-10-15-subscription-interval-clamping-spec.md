# Matter Subscription Interval Clamping Specification

**Date:** 2026-10-15  
**Status:** Approved / Ready for Implementation  
**Target:** `rs-matter` (Matter Data Model / Interaction Model) and `intercom-matter` (`intercom-adapter-matter`)  
**Objective:** Accelerate background Matter auto-reconnects from 10 minutes to 1–2 minutes following a reboot or network disruption.

---

## 1. Problem Statement & Context

When the Smart Intercom Bridge reboots (due to a power cycle, crash recovery, or wireless OTA firmware update):

1. **Volatile Session Loss:** All RAM-stored cryptographic session keys (CASE sessions) are wiped.
2. **Controller Unawareness:** Matter controllers (such as the resident Apple Home Hub: Apple TV / HomePod) do not know the accessory rebooted.
3. **The 10-Minute Dead Zone:** During subscription setup, Apple Home specifies `max_interval_ceiling = 600s` (10 minutes). Under default `rs-matter` behavior, the server accepts this 600s ceiling. As a result, when the device reboots, the Apple Home Hub waits up to 10 minutes for its next expected subscription report before marking the session lost and initiating a fresh CASE handshake (`CASESigma1`).
4. **User Impact:** If a user opens the Apple Home app within this 10-minute window, iOS attempts to communicate over the stale pre-reboot session (`SID:1`). Under Matter Core Spec §4.10, the accessory silently drops the unauthenticated frame, forcing the user to endure a 20-second MRP timeout (`Waiting...` → `Not responding`) before a retry establishes a new session.

---

## 2. Matter Standard Compliance

Under the **Matter Core Specification Release 1.3, Section 8.5.1 ("Subscribe Request Action")**:

> *"The publisher MAY decide on an interval that is smaller than MaxIntervalCeiling, or larger than MinIntervalFloor. The publisher returns the selected MaxInterval in the Subscribe Response."*

- **Spec Guarantee:** A publisher (accessory) has the explicit right to grant a `MaxInterval` smaller than what the client requested in `MaxIntervalCeiling`.
- **Client Behavior:** When the Apple Home Hub receives the `SubscribeResponse` containing the publisher's chosen `MaxInterval`, it updates its local timer to match. If the publisher goes silent for that duration, the Hub detects the connection loss and initiates recovery immediately.

---

## 3. Architecture & Technical Design

### A. Current Implementation in `rs-matter`

In `rs-matter/src/dm/clusters/basic_info.rs`:
```rust
pub struct BasicInfoConfig<'a> {
    // ...
    /// The least max interval granted to a subscription, in seconds: what the
    /// subscriber asks for, but no less than this.
    pub min_subscription_max_interval_secs: u16, // Defaults to 40s
}
```

In `rs-matter/src/im.rs` (lines 1063–1066):
```rust
let max_int_secs = match self.matter.icd_advertisement() {
    Some(icd) => icd_max_int_secs(icd.idle_mode_duration_s, min_int_secs),
    None => core::cmp::max(
        req.max_int_ceil()?,
        self.matter.dev_det().min_subscription_max_interval_secs,
    ),
};
```
Currently, `rs-matter` only enforces a **lower bound** (`core::cmp::max(req.max_int_ceil()?, min)`). When Apple Home requests 600s, `max(600, 40) = 600`.

---

### B. Proposed Changes in `rs-matter`

#### 1. Add `max_subscription_max_interval_secs` to `BasicInfoConfig`
In `rs-matter/src/dm/clusters/basic_info.rs`:

```rust
pub struct BasicInfoConfig<'a> {
    // ...
    /// The least max interval granted to a subscription, in seconds (lower bound).
    pub min_subscription_max_interval_secs: u16,

    /// The greatest max interval granted to a subscription, in seconds (upper bound).
    ///
    /// If `Some(cap)`, subscriptions will never be granted a max interval greater
    /// than `cap`. This bounds how long a subscriber will wait before timing out a
    /// silent device, dramatically speeding up background reconnects after reboots.
    ///
    /// If `None`, the subscriber's requested ceiling is accepted without an upper cap.
    pub max_subscription_max_interval_secs: Option<u16>,
}
```

In `BasicInfoConfig::new()`:
```rust
max_subscription_max_interval_secs: None, // Backward-compatible default
```

#### 2. Clamp `max_int_secs` in `rs-matter/src/im.rs`
In `rs-matter/src/im.rs`:

```rust
let mut max_int_secs = match self.matter.icd_advertisement() {
    Some(icd) => icd_max_int_secs(icd.idle_mode_duration_s, min_int_secs),
    None => core::cmp::max(
        req.max_int_ceil()?,
        self.matter.dev_det().min_subscription_max_interval_secs,
    ),
};

if let Some(max_cap) = self.matter.dev_det().max_subscription_max_interval_secs {
    // Ensure the upper bound is not smaller than the lower bound or min_int_floor
    let effective_cap = core::cmp::max(max_cap, min_int_secs);
    max_int_secs = core::cmp::min(max_int_secs, effective_cap);
}
```

---

### C. Proposed Changes in `intercom-matter`

#### 1. Define Constant in `intercom-adapter-matter/src/config.rs`
```rust
/// Maximum reporting interval granted to Matter subscriptions in seconds.
///
/// Apple Home requests 600s (10 min). Clamping this to 120s (2 min) forces the
/// Apple Home Hub to check in and re-establish CASE within 2 minutes of an ESP32 reboot.
pub const MAX_SUBSCRIPTION_INTERVAL_SECS: u16 = 120;
```

#### 2. Configure in `dev_det()` in `intercom-adapter-matter/src/controller.rs`
```rust
fn dev_det(info: &DeviceInfo, serial_no: &'static str) -> BasicInfoConfig<'static> {
    BasicInfoConfig {
        vid: VID,
        pid: PID,
        hw_ver: info.hw_ver,
        hw_ver_str: info.hw_ver_str,
        sw_ver: info.sw_ver,
        sw_ver_str: info.sw_ver_str,
        serial_no,
        vendor_name: info.vendor_name,
        product_name: info.product_name,
        device_name: info.device_name,
        max_subscription_max_interval_secs: Some(config::MAX_SUBSCRIPTION_INTERVAL_SECS),
        ..BasicInfoConfig::new()
    }
}
```

---

## 4. Interval Value Evaluation: 60s vs. 120s

| Criterion | 60 Seconds | 120 Seconds (Recommended) |
|---|---|---|
| **Post-Reboot Auto-Reconnect** | ~60 seconds (1 minute) | ~120 seconds (2 minutes) |
| **Keep-Alive Traffic When Idle** | 1 frame (~60 B) every 30s | 1 frame (~60 B) every 60s |
| **Network Bandwidth Overhead** | ~2.0 B/s on Wi-Fi | ~1.0 B/s on Wi-Fi (imperceptible) |
| **Tolerance to Wi-Fi Jitter / AP Roaming** | Moderate (misses after 60s) | High (tolerates temporary 60s Wi-Fi packet drops) |
| **Verdict** | Aggressive | **Optimal balance of speed & robustness** |

**Recommendation:** Default to **120 seconds** (`2 minutes`). It achieves a 5x improvement over Apple Home's 10-minute default while providing plenty of headroom for transient home network packet loss.

---

## 5. End-to-End Verification Plan

### Test 1: Unit Tests in `rs-matter`
Add unit tests in `rs-matter/src/im.rs` verifying that:
1. When `max_subscription_max_interval_secs = None`, `max_int_ceil = 600` results in `max_int_secs = 600`.
2. When `max_subscription_max_interval_secs = Some(120)`, `max_int_ceil = 600` results in `max_int_secs = 120`.
3. When `max_subscription_max_interval_secs = Some(120)` and client requests `max_int_ceil = 30`, `max_int_secs` respects `30` (or `min_subscription_max_interval_secs`).

### Test 2: Compilation & Host Tests
```bash
cargo test -p rs-matter
cargo test -p intercom-core -p intercom-adapter-ota --target x86_64-apple-darwin
mise exec -- cargo build -p intercom-firmware --release
```

### Test 3: Hardware Verification with Apple Home Hub
1. Deploy release binary to ESP32-C6 (`scripts/ota.py`).
2. Observe serial log during Apple Home subscription setup:
   - Verify `Added subscription ... max_int_secs: 120` (instead of `600`).
   - Verify `IM::SubscribeResponse` carries `max_interval = 120`.
3. Reboot device (`software_reset` or manual reset).
4. Do **not** touch the phone or open the Apple Home app.
5. Watch serial monitor:
   - Confirm Apple Home Hub initiates `CASESigma1` and restores subscriptions within **≤ 120 seconds** automatically (`Subscription primed` in logs).

---

## 6. Implementation Checklist

- [ ] **Task 1 (`rs-matter`):** Add `max_subscription_max_interval_secs: Option<u16>` to `BasicInfoConfig` in `rs-matter/src/dm/clusters/basic_info.rs`.
- [ ] **Task 2 (`rs-matter`):** Implement interval clamping logic in `rs-matter/src/im.rs`.
- [ ] **Task 3 (`rs-matter`):** Add unit tests for subscription interval negotiation in `rs-matter`.
- [ ] **Task 4 (`intercom-adapter-matter`):** Add `MAX_SUBSCRIPTION_INTERVAL_SECS = 120` in `config.rs`.
- [ ] **Task 5 (`intercom-adapter-matter`):** Pass `max_subscription_max_interval_secs: Some(MAX_SUBSCRIPTION_INTERVAL_SECS)` in `controller.rs`.
- [ ] **Task 6 (Hardware Verification):** Flash to ESP32-C6, reboot without touching phone, and verify auto-reconnect within 120 seconds.
