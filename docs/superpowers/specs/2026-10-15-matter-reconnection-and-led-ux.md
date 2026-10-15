# Matter Reconnection Acceleration & LED Indicator UX Design

**Date:** 2026-10-15  
**Status:** Draft / Brainstorming  
**Target:** ESP32-C6 Smart Intercom Bridge (`intercom-matter`, `rs-matter`, `rs-matter-embassy`)

---

## 1. Executive Summary & Problem Statement

Following a reboot (power cycle or OTA firmware update), the Intercom Bridge exhibits two related user-experience frictions:

1. **Slow Background Auto-Reconnect (Up to 10 Minutes):**  
   The Apple Home Hub (Apple TV / HomePod) establishes subscriptions with `max_interval = 600s` (10 minutes). When the ESP32 reboots, volatile RAM encryption keys are cleared. The Home Hub does not know the device rebooted and waits up to 10 minutes for its next subscription heartbeat before detecting the lost connection and re-subscribing.
2. **First Interaction Stale-Session Delay ("Waiting..." → "Not Responding" for 20s):**  
   If the user opens Apple Home during this window, iOS attempts to send commands over the pre-reboot session (`SID:1`). Under Matter Core Spec §4.10, the accessory *must silently drop* unauthenticated packets. The iOS app retries for ~20 seconds before declaring the session dead and establishing a fresh session (`CASESigma1`).
3. **Misleading LED Pattern During Waiting Period:**  
   While waiting for the Home Hub subscription, the LED indicator displays `SystemState::Connected` (900 ms ON / 100 ms OFF). Users perceive this 100 ms "wink" as a "short blink" indicating an error or disconnection, even though Wi-Fi is connected, the OTA server is up, and Matter is listening.

---

## 2. Part 1: LED Indicator UX & Intermediate States (Brainstorming)

### Current Architecture

Currently, `crates/intercom-core/src/events.rs` and `crates/intercom-adapter-gpio/src/config.rs` define the following progression:

| State | Pattern | Duty Cycle | Hardware / Protocol Meaning |
|---|---|---|---|
| `Initializing` | 200 ms ON / 1000 ms OFF | ~17% | Booting, allocating hardware peripherals |
| `Initialized` | 300 ms ON / 700 ms OFF | 30% | Hardware ready, waiting for network tasks |
| `Connecting` | 500 ms ON / 500 ms OFF | 50% | Wi-Fi associating, obtaining DHCP lease |
| `Connected` | **900 ms ON / 100 ms OFF** | **90%** | Wi-Fi operational, Matter stack listening |
| `Ready` | **Solid ON** | **100%** | Both Door Lock & Ring clusters subscribed by Home Hub |
| `Updating` | 50 ms ON / 50 ms OFF | 50% (10 Hz) | OTA streaming flash write in progress |
| `FactoryReset` | 50 ms ON / 50 ms OFF | 50% | Erasing credentials, reboot pending |
| `Error` | 100 ms ON / 100 ms OFF (2x) + 700 ms pause | Burst | Hardware fault or fatal state |

### The UX Problem

In `crates/intercom-adapter-matter/src/readiness.rs`, `ClusterReadinessBarrier` gates `SystemState::Ready` (Solid ON) on **both** Endpoint 1 (Door Lock) and Endpoint 2 (Occupancy) being read/subscribed by the Apple Home Hub.

- If the Apple Home Hub takes 7–10 minutes to subscribe in the background, the LED blinks (winks) the entire time.
- If the user directly unlocks the door via iPhone, the door opens, but the LED *continues blinking* because the iPhone only accessed Endpoint 1 without subscribing to Endpoint 2.
- **Root UX failure:** The visual indicator conflates *local device health / network readiness* with *external cloud/hub subscription state*.

---

### Brainstorming Proposals

#### Proposal A: Decouple Device Readiness from Hub Subscription (Recommended)

- **Definition:** The physical LED reflects **physical & network operational readiness** of the bridge.
  - As soon as Wi-Fi is associated, has an IP, and the Matter listener + OTA server are running, the system transitions to **Solid ON (`Ready`)**.
  - Whether Apple Home has primed its subscription is an internal protocol matter; the device is powered, connected, and ready to receive commands.
- **Pros:**
  - Zero ambiguity: Solid ON = "device is working normally".
  - Instant transition post-boot (~6 seconds after power-on).
  - Matches user mental model of household smart appliances (Hue bridge, smart plugs).
- **Cons:**
  - LED does not inform the user if the Apple Home Hub has stale subscription state.

#### Proposal B: Refined Intermediate State Hierarchy

If we want the LED to represent the full lifecycle including hub readiness, redefine the visual patterns to eliminate the inverted "wink":

```
Booting                Wi-Fi Connected           Matter Ready              Hub Subscribed
 [ Initializing ]  →   [ Connecting ]      →     [ Operational ]     →     [ Fully Primed ]
  Slow pulse            Even blink (1 Hz)         Solid ON                  Solid ON (or subtle
 (200ms/1000ms)        (500ms/500ms)             (Ready for commands)       heartbeat wink)
```

1. **`Connecting` (Wi-Fi associating):** 500 ms ON / 500 ms OFF (equal blink).
2. **`Operational` (Wi-Fi up, Matter stack listening, ready to accept commands):** **Solid ON**.
3. **`Subscribed` (Optional / Internal):** Device remains Solid ON. On active Matter transaction (unlock or ring), LED emits a brief double-flash confirmation (already supported by `report_lock_state` and `report_ring`).

#### Proposal C: Promote to `Ready` on First Operational Message

- Keep `ClusterReadinessBarrier`, but add an `or_any_interaction()` bypass:
  - If the barrier receives a subscription for both clusters → `Ready`.
  - **OR** if any valid CASE session or cluster command (e.g. `InvokeRequest` unlock) is executed → immediately trip the barrier to `Ready`.
- **Pros:** When the user taps unlock on their phone, the LED instantly turns solid, acknowledging the successful command.

---

## 3. Part 2: Accelerating Background Reconnection (Subscription Clamping)

### The Mechanism

In the Matter Interaction Model:
1. The client (Apple Home Hub) sends a `SubscribeRequest` with `min_interval_floor` and `max_interval_ceiling`.
   - Apple Home specifies `max_int_ceil = 600` (10 minutes).
2. The server (ESP32) records this in `rs-matter/src/im.rs`:
   ```rust
   let max_int_secs = core::cmp::max(
       req.max_int_ceil()?,
       self.matter.dev_det().min_subscription_max_interval_secs,
   );
   ```
3. The server replies with `SubscribeResponse` containing the selected `max_int_secs`.
4. The client expects a report at least once every `max_int_secs`. If that time elapses with no report, the client declares the subscription dead and re-initiates discovery and CASE.

### Proposed Improvement: Publisher Maximum Interval Clamping

Under the Matter Specification (Section 8.5.1):
> *"The publisher MAY decide on an interval that is smaller than MaxIntervalCeiling, or larger than MinIntervalFloor. The publisher returns the selected MaxInterval in the Subscribe Response."*

We can introduce a configurable `max_subscription_max_interval_secs` (e.g. **60 seconds** or **120 seconds**):

```rust
// Clamp client-requested max interval to device policy (e.g. 120s)
let max_int_secs = core::cmp::min(
    req.max_int_ceil()?,
    config::MAX_SUBSCRIPTION_REPORT_INTERVAL_SECS, // 120s
);
```

### Impact & Trade-Off Analysis

| Metric | With 600s (Default) | With 120s (Proposed) | Impact Assessment |
|---|---|---|---|
| **Max Auto-Reconnect Window** | 10 minutes | 2 minutes | **5x faster recovery** after power cuts or OTA |
| **Network Traffic (Quiet)** | ~60 bytes / 10 min | ~60 bytes / 2 min | ~0.5 bytes/sec on Wi-Fi (negligible) |
| **Power Consumption** | Identical | Identical | Intercom Bridge is mains/intercom powered |
| **Specification Compliance** | 100% compliant | 100% compliant | Allowed explicitly by Matter IM §8.5.1 |

---

## 4. Part 3: Stale Session Handling & Matter Spec Constraints

### Why the 20-Second First Tap Cannot Be Bypassed by the Accessory

When an accessory reboots, its RAM session keys are lost. When iOS sends a command on `SID:1`:
- **Matter Core Spec §4.10:** Unauthenticated frames referencing non-existent sessions **must be silently discarded**.
- If an accessory responded with an unauthenticated `CloseSession` or error packet, an off-path network attacker could spoof packets with arbitrary session IDs to tear down legitimate sessions across the home network.
- Therefore, the client-side MRP timeout (20 seconds) is an intentional security design of the Matter protocol.
- **Benefit of Proposal 2 (120s subscription):** Because the Home Hub will detect the reboot within 1–2 minutes in the background, the stale session will already be cleared by the time the user opens the app, making the 20-second delay unlikely to ever be seen by the user!

---

## 5. Implementation Roadmap for Future Sessions

### Phase 1: LED Indicator UX (Firmware & Core)
1. In `crates/intercom-adapter-matter/src/controller.rs`:
   - When network interface becomes operational (`network up → connected`), emit `ready()` instead of `connected()`, OR
2. In `crates/intercom-adapter-matter/src/readiness.rs`:
   - Add trigger to trip readiness barrier on first operational command / CASE session.
3. In `crates/intercom-adapter-gpio/src/config.rs`:
   - Retire the 900ms/100ms inverted wink in favor of solid ON for normal operational states.

### Phase 2: Subscription Interval Clamping (`rs-matter`)
1. In `rs-matter/src/im.rs`:
   - Add configurable ceiling to clamp `max_int_secs` to `120s` (or `60s`).
2. Test against Apple Home to confirm Hub accepts `max_interval = 120s` in `SubscribeResponse`.
3. Reboot device and verify automatic CASE re-subscription occurs within 120 seconds in serial monitor.

---

## 6. References

- **Matter Core Specification Release 1.3:** Section 4.10 (Secure Channel Protocol), Section 8.5 (Interaction Model Subscriptions).
- **Codebase Locations:**
  - `crates/intercom-adapter-gpio/src/led.rs`: LED driver command loop & blink patterns.
  - `crates/intercom-adapter-gpio/src/config.rs`: Asymmetric LED duty cycles.
  - `crates/intercom-adapter-matter/src/readiness.rs`: `ClusterReadinessBarrier`.
  - `crates/intercom-adapter-matter/src/controller.rs`: Network state observer.
  - `rs-matter/src/im.rs`: Subscription negotiation & `SubscribeResponse` encoding.
