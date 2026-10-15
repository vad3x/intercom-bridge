# LED Indicator UX & Lifecycle State Machine Design

**Date:** 2026-10-15  
**Status:** Draft / Brainstorming  
**Target:** `intercom-core`, `intercom-adapter-gpio`, `intercom-adapter-matter`  
**Scope:** LED indicator visual patterns, system lifecycle states, and decoupling physical device readiness from Apple Home Hub subscription latency. (For subscription interval clamping, see the separate [`subscription-interval-clamping-spec.md`](file:///Users/vadim/personal/projects/intercom-matter/docs/superpowers/specs/2026-10-15-subscription-interval-clamping-spec.md)).

---

## 1. Executive Summary & Problem Statement

Following a reboot or Wi-Fi reconnect, the physical LED indicator on the Intercom Bridge displays an inverted "wink" pattern (900 ms ON / 100 ms OFF) representing `SystemState::Connected`.

- **User Perception:** A human observer perceives a 100 ms periodic off-wink as a "short blink" or warning/searching pattern, leading them to believe the device is disconnected, unresponsive, or experiencing an error.
- **Underlying Reality:** The hardware is completely healthy, Wi-Fi is associated and has an IP address, the OTA update server (port 8080) is listening, and the Matter protocol stack is actively responding to network queries and direct commands.
- **Root Architectural Cause:** In `crates/intercom-adapter-matter/src/readiness.rs`, `ClusterReadinessBarrier` gates `SystemState::Ready` (Solid ON) on **both** Endpoint 1 (Door Lock) and Endpoint 2 (Occupancy) being subscribed by the Apple Home Hub. Because the Apple Home Hub operates on background polling intervals, this transition can take several minutes.
- **Goal:** Redefine the visual language and state transitions so the physical LED reflects **physical device & network operational readiness**, eliminating user confusion.

---

## 2. Current Architecture & State Machine

Currently, `crates/intercom-core/src/events.rs` and `crates/intercom-adapter-gpio/src/config.rs` define the following lifecycle progression:

| State | Hardware Pattern | Duty Cycle | Hardware / Network Reality |
|---|---|---|---|
| `Initializing` | 200 ms ON / 1000 ms OFF | ~17% | Bootloader complete, allocating GPIO peripherals |
| `Initialized` | 300 ms ON / 700 ms OFF | 30% | GPIO drivers ready, starting async network runtime |
| `Connecting` | 500 ms ON / 500 ms OFF | 50% | Wi-Fi associating, waiting for DHCP lease |
| `Connected` | **900 ms ON / 100 ms OFF** | **90%** | Wi-Fi up, IP assigned, Matter & OTA listeners running |
| `Ready` | **Solid ON** | **100%** | Both Door Lock & Ring clusters subscribed by Home Hub |
| `Updating` | 50 ms ON / 50 ms OFF | 50% (10 Hz) | OTA streaming flash write in progress |
| `FactoryReset` | 50 ms ON / 50 ms OFF | 50% | Erasing NVS/KVS credentials, reboot pending |
| `Error` | 100 ms ON / 100 ms OFF (2x) + 700 ms pause | Burst | Hardware fault or fatal state |

---

## 3. The UX Failure Modes

1. **Conflating Physical Readiness with Cloud/Hub Polling:**
   The bridge is fully operational as soon as Wi-Fi is connected and the Matter stack is listening. A user can directly unlock the door from their iPhone over local IPv6. However, because the background Apple Home Hub hasn't yet issued its wildcard subscription for the Ring Occupancy sensor, the LED continues winking.
2. **First Interaction Asymmetry:**
   When a user taps "Unlock" in Apple Home shortly after a reboot, the door strike clicks and unlocks. However, because the iPhone only accessed the Door Lock cluster and did not subscribe to the Ring cluster, the LED *continues blinking*, contradicting the physical success of the unlock action.
3. **Negative Connotation of the 100 ms Wink:**
   In consumer electronics, a solid light signifies "online and ready". Any form of periodic blinking—even a 90% duty cycle wink—signals "in-progress", "pairing", or "degraded".

---

## 4. Design Proposals for Future Sessions

```mermaid
flowchart TD
    Boot([Power On / Reset]) --> Init[Initializing: Slow Pulse]
    Init --> WifiConn[Connecting: 1 Hz Even Blink]
    WifiConn --> NetUp{Network Up & Matter Stack Listening?}
    NetUp -->|Proposal A (Recommended)| SolidReady[Solid ON: Device Operational & Ready]
    NetUp -->|Proposal B| NetOp[Operational: Solid ON]
    NetUp -->|Proposal C| WinkConnected[Connected Wink]
    WinkConnected --> AnyCmd{Any Command Executed or Hub Subscribed?}
    AnyCmd -->|Yes| SolidReady
```

### Proposal A: Decouple Device Readiness from Hub Subscription (Recommended)
- **Principle:** The physical indicator reflects whether the Intercom Bridge is locally powered, networked, and responsive.
- **Rule:** As soon as Wi-Fi is connected with an IP address and the Matter stack is operational (`network up → connected` in `controller.rs`), publish `SystemState::Ready` (Solid ON).
- **Subscribed State:** Home Hub subscription status is treated as background protocol telemetry rather than a gate on physical LED readiness.
- **Pros:**
  - Instant transition post-boot (~6 seconds from power-on).
  - Matches user expectations for smart home bridges (e.g. Philips Hue, Lutron Caseta).
  - Zero code complexity.

---

### Proposal B: Clear Three-Tier Visual Language
If distinguishing "Wi-Fi Up" from "Hub Fully Subscribed" is deemed strictly necessary:
1. **`Connecting` (Wi-Fi associating):** 500 ms ON / 500 ms OFF (standard medium blink).
2. **`Operational` (Wi-Fi up, Matter listening):** **Solid ON** (Ready to accept commands).
3. **`Active Interaction` (Unlock / Ring event):** Double-flash confirmation (already supported by `report_lock_state` and `report_ring`).
- **Eliminates:** Completely removes the 900ms/100ms inverted wink.

---

### Proposal C: Promote to `Ready` on First Operational Interaction
- Keep `ClusterReadinessBarrier`, but add a fast-path trigger:
  - If both Door Lock and Ring clusters are subscribed → `Ready`.
  - **OR** if *any* valid operational CASE session or cluster command (e.g. `InvokeRequest` unlock) is processed → immediately trip the barrier to `Ready`.
- **Pros:** Acknowledges direct user commands instantly even if the background Home Hub is delayed.

---

## 5. Scope Boundaries

This specification is **strictly isolated to LED indicators and system lifecycle reporting**:
- **Files Touched in this scope:**
  - `crates/intercom-core/src/events.rs`
  - `crates/intercom-adapter-gpio/src/led.rs`
  - `crates/intercom-adapter-gpio/src/config.rs`
  - `crates/intercom-adapter-matter/src/readiness.rs`
  - `crates/intercom-adapter-matter/src/controller.rs` (only where `ready()` vs `connected()` is published)
- **Non-Goals / Excluded from this scope:**
  - Does **not** modify `rs-matter` or subscription intervals (handled exclusively in `subscription-interval-clamping-spec.md`).
  - Does **not** modify Matter network transport or crypto.
