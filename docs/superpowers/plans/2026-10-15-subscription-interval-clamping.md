# Matter Subscription Interval Clamping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Accelerate background Matter auto-reconnects from 10 minutes to 1–2 minutes following an ESP32 reboot or network disruption by clamping the negotiated subscription interval (`max_int_secs`).

**Architecture:**
- In `rs-matter`: Extend `BasicInfoConfig` with `max_subscription_max_interval_secs: Option<u16>` and implement `subscription_max_int_secs` pure negotiation logic in `rs-matter/src/im/subscriptions.rs` and `rs-matter/src/im.rs`, designed for clean upstream PR submission to `project-chip/rs-matter`.
- In `intercom-matter`: Configure `MAX_SUBSCRIPTION_INTERVAL_SECS = 120` in `intercom-adapter-matter` and wire into `dev_det()`.

**Tech Stack:** Rust (`no_std`, `riscv32imac-unknown-none-elf`), `rs-matter` (Matter Core Spec 1.3), `intercom-adapter-matter`, `esp-hal`, `embassy`.

## Global Constraints
- Target hardware: ESP32-C6-WROOM-1-N8 (8 MB Quad SPI Flash).
- 100% backward compatibility for `rs-matter`: `max_subscription_max_interval_secs` defaults to `None`, preserving existing behavior unless explicitly configured.
- Upstream alignment: Mirror conventions established in `project-chip/rs-matter` PR #590 (`e633f8d`).
- No breaking changes to existing `BasicInfoConfig` callers (use `..BasicInfoConfig::new()`).
- Matter Core Specification §8.5.1 / §8.5.2 compliance: Granted `max_interval` must never be less than `min_int_floor`.
- All tests follow Arrange-Act-Assert structure.
- Public items ordered above private items in all Rust files.

---

### Task 1: Add `max_subscription_max_interval_secs` to `BasicInfoConfig` in `rs-matter`

**Files:**
- Modify: `../rs-matter/rs-matter/src/dm/clusters/basic_info.rs:243-288`

**Interfaces:**
- Produces: `BasicInfoConfig.max_subscription_max_interval_secs: Option<u16>`.

- [ ] **Step 1: Add field and default to `BasicInfoConfig`**

In `/Users/vadim/personal/projects/rs-matter/rs-matter/src/dm/clusters/basic_info.rs`:
Add field to `BasicInfoConfig<'a>` right below `min_subscription_max_interval_secs`:

```rust
    /// The least max interval granted to a subscription, in seconds: what the
    /// subscriber asks for, but no less than this. A subscription with nothing to
    /// report keeps alive halfway through its max interval, so this bounds how
    /// often such keep-alive reports go out.
    ///
    /// Not used by an Intermittently Connected Device, whose subscriptions take
    /// their max interval from its idle mode duration.
    ///
    /// Not a real attribute.
    pub min_subscription_max_interval_secs: u16,
    /// The greatest max interval granted to a subscription, in seconds: if specified,
    /// what the subscriber asks for, but no more than this. This bounds how long a
    /// subscriber will wait before timing out a silent device, speeding up background
    /// reconnects after reboots.
    ///
    /// Not used by an Intermittently Connected Device, whose subscriptions take
    /// their max interval from its idle mode duration.
    ///
    /// If `None`, the subscriber's requested ceiling is accepted without an upper cap.
    ///
    /// Not a real attribute.
    pub max_subscription_max_interval_secs: Option<u16>,
```

And in `BasicInfoConfig::new()`:

```rust
            sai: None,
            sii: None,
            tcp_supported: false,
            min_subscription_max_interval_secs: DEFAULT_MIN_SUBSCRIPTION_MAX_INTERVAL_SECS,
            max_subscription_max_interval_secs: None,
```

- [ ] **Step 2: Verify `rs-matter` builds with new field**

Run:
```bash
RUSTUP_TOOLCHAIN=1.99.0 /Users/vadim/.cargo/bin/cargo check -p rs-matter
```
Expected: `Finished` with zero errors.

- [ ] **Step 3: Commit change in `rs-matter` repository**

In `/Users/vadim/personal/projects/rs-matter`:
```bash
git add rs-matter/src/dm/clusters/basic_info.rs
git commit -m "basic_info: add max_subscription_max_interval_secs to BasicInfoConfig"
```

---

### Task 2: Implement Subscription Interval Clamping & Unit Tests in `rs-matter`

**Files:**
- Modify: `../rs-matter/rs-matter/src/im/subscriptions.rs:1987-1995` and tests around `3475-3500`
- Modify: `../rs-matter/rs-matter/src/im.rs:1060-1067`

**Interfaces:**
- Produces: `pub(crate) fn subscription_max_int_secs(req_max_int_ceil: u16, min_int_floor: u16, min_subscription_max_interval_secs: u16, max_subscription_max_interval_secs: Option<u16>) -> u16`
- Consumes: `BasicInfoConfig.min_subscription_max_interval_secs`, `BasicInfoConfig.max_subscription_max_interval_secs`, `req.max_int_ceil()`, `req.min_int_floor()`.

- [ ] **Step 1: Write failing unit test in `rs-matter/src/im/subscriptions.rs`**

In `/Users/vadim/personal/projects/rs-matter/rs-matter/src/im/subscriptions.rs`, in the `#[cfg(test)] mod tests` block around line 3485:

```rust
    #[test]
    fn subscription_max_int_clamping() {
        // Arrange & Act & Assert

        // 1. Uncapped (None) accepts client ceiling (e.g. 600s from Apple Home)
        assert_eq!(subscription_max_int_secs(600, 0, 40, None), 600);

        // 2. Uncapped (None) enforces min floor (40s default) when client asks for less
        assert_eq!(subscription_max_int_secs(10, 0, 40, None), 40);

        // 3. Capped at 120s clamps 600s down to 120s
        assert_eq!(subscription_max_int_secs(600, 0, 40, Some(120)), 120);

        // 4. Capped at 120s allows smaller client ceiling (e.g. 80s)
        assert_eq!(subscription_max_int_secs(80, 0, 40, Some(120)), 80);

        // 5. Capped at 120s respects min_subscription floor (40s) when client asks for 10s
        assert_eq!(subscription_max_int_secs(10, 0, 40, Some(120)), 40);

        // 6. Effective cap never drops below client's min_int_floor (Matter Spec §8.5.1)
        assert_eq!(subscription_max_int_secs(600, 50, 40, Some(30)), 50);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
RUSTUP_TOOLCHAIN=1.99.0 /Users/vadim/.cargo/bin/cargo test -p rs-matter --lib im::subscriptions::tests::subscription_max_int_clamping
```
Expected: FAIL with `cannot find function 'subscription_max_int_secs' in this scope`.

- [ ] **Step 3: Implement `subscription_max_int_secs` and wire into `im.rs`**

In `/Users/vadim/personal/projects/rs-matter/rs-matter/src/im/subscriptions.rs` (placed right next to `icd_max_int_secs` at line 1987):

```rust
pub(crate) fn subscription_max_int_secs(
    req_max_int_ceil: u16,
    min_int_floor: u16,
    min_subscription_max_interval_secs: u16,
    max_subscription_max_interval_secs: Option<u16>,
) -> u16 {
    let mut max_int = core::cmp::max(req_max_int_ceil, min_subscription_max_interval_secs);
    if let Some(max_cap) = max_subscription_max_interval_secs {
        let effective_cap = core::cmp::max(max_cap, min_int_floor);
        max_int = core::cmp::min(max_int, effective_cap);
    }
    max_int
}
```

In `/Users/vadim/personal/projects/rs-matter/rs-matter/src/im.rs`:
Update imports from `subscriptions` around line 53 to include `subscription_max_int_secs`:
```rust
    icd_max_int_secs, subscription_max_int_secs, KeepAlive, ReportContext, Subscriptions,
    SubscriptionsBuffers,
```

And in `/Users/vadim/personal/projects/rs-matter/rs-matter/src/im.rs` at line 1060:

Replace:
```rust
        let min_int_secs = req.min_int_floor()?;
        let max_int_secs = match self.matter.icd_advertisement() {
            Some(icd) => icd_max_int_secs(icd.idle_mode_duration_s, min_int_secs),
            None => core::cmp::max(
                req.max_int_ceil()?,
                self.matter.dev_det().min_subscription_max_interval_secs,
            ),
        };
```
With:
```rust
        let min_int_secs = req.min_int_floor()?;
        let max_int_secs = match self.matter.icd_advertisement() {
            Some(icd) => icd_max_int_secs(icd.idle_mode_duration_s, min_int_secs),
            None => subscription_max_int_secs(
                req.max_int_ceil()?,
                min_int_secs,
                self.matter.dev_det().min_subscription_max_interval_secs,
                self.matter.dev_det().max_subscription_max_interval_secs,
            ),
        };
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run:
```bash
RUSTUP_TOOLCHAIN=1.99.0 /Users/vadim/.cargo/bin/cargo test -p rs-matter --lib im::subscriptions::tests::subscription_max_int_clamping
```
Expected: `test im::subscriptions::tests::subscription_max_int_clamping ... ok`.

Run all `rs-matter` lib tests:
```bash
RUSTUP_TOOLCHAIN=1.99.0 /Users/vadim/.cargo/bin/cargo test -p rs-matter --lib
```
Expected: `test result: ok. 1014 passed; 0 failed`.

- [ ] **Step 5: Commit change in `rs-matter` repository**

In `/Users/vadim/personal/projects/rs-matter`:
```bash
git add rs-matter/src/im/subscriptions.rs rs-matter/src/im.rs
git commit -m "im: clamp subscription max interval with max_subscription_max_interval_secs"
```

---

### Task 3: Configure Subscription Interval Clamping in `intercom-matter`

**Files:**
- Modify: `crates/intercom-adapter-matter/src/config.rs:16-18`
- Modify: `crates/intercom-adapter-matter/src/controller.rs:450-464`

**Interfaces:**
- Produces: `config::MAX_SUBSCRIPTION_INTERVAL_SECS: u16 = 120`.
- Consumes: `rs_matter::dm::clusters::basic_info::BasicInfoConfig.max_subscription_max_interval_secs`.

- [ ] **Step 1: Define `MAX_SUBSCRIPTION_INTERVAL_SECS` in `crates/intercom-adapter-matter/src/config.rs`**

In `/Users/vadim/personal/projects/intercom-matter/crates/intercom-adapter-matter/src/config.rs`, add the subscription timing constant:

```rust
// ============================================================================
// Subscription Interval Policies (Matter Spec 8.5.1)
// ============================================================================

/// Maximum reporting interval granted to Matter subscriptions in seconds.
///
/// Apple Home requests 600s (10 min). Clamping this to 120s (2 min) forces the
/// Apple Home Hub to check in and re-establish CASE within 2 minutes of an ESP32 reboot.
pub const MAX_SUBSCRIPTION_INTERVAL_SECS: u16 = 120;
```

- [ ] **Step 2: Pass `max_subscription_max_interval_secs` in `dev_det()` in `crates/intercom-adapter-matter/src/controller.rs`**

In `/Users/vadim/personal/projects/intercom-matter/crates/intercom-adapter-matter/src/controller.rs`:
Update `dev_det`:

```rust
/// Build device identity for the Basic Information cluster.
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
        max_subscription_max_interval_secs: Some(crate::config::MAX_SUBSCRIPTION_INTERVAL_SECS),
        ..BasicInfoConfig::new()
    }
}
```

- [ ] **Step 3: Run intercom host unit tests**

Run:
```bash
mise exec -- cargo test -p intercom-core -p intercom-adapter-ota --target x86_64-apple-darwin
```
Expected: `test result: ok. 43 passed; 0 failed`.

- [ ] **Step 4: Build release firmware binary**

Run:
```bash
mise exec -- cargo build -p intercom-firmware --release
```
Expected: `Finished release [optimized + debuginfo] target(s)` without warnings or errors.

- [ ] **Step 5: Commit change in `intercom-matter` repository**

In `/Users/vadim/personal/projects/intercom-matter`:
```bash
git add crates/intercom-adapter-matter/src/config.rs crates/intercom-adapter-matter/src/controller.rs
git commit -m "feat(matter): clamp subscription max interval to 120s for fast reboot recovery"
```

---

### Task 4: End-to-End Verification & Validation

**Files:**
- Host verification: test runner commands.
- Hardware verification: physical ESP32-C6 testing with Apple Home Hub.

- [ ] **Step 1: Execute all host tests across both repositories**

Run in `intercom-matter`:
```bash
mise exec -- cargo test -p intercom-core -p intercom-adapter-ota --target x86_64-apple-darwin
```
Run in `rs-matter`:
```bash
RUSTUP_TOOLCHAIN=1.99.0 /Users/vadim/.cargo/bin/cargo test -p rs-matter --lib
```
Expected: All tests pass cleanly.

- [ ] **Step 2: Deploy firmware update to ESP32-C6 via OTA**

Run:
```bash
python3 scripts/ota.py target/riscv32imac-unknown-none-elf/release/intercom-bridge
```
Expected: OTA image streamed, verified via Ed25519 signature, committed to inactive slot, and ESP32 rebooted.

- [ ] **Step 3: Monitor serial log for clamped subscription interval**

Monitor device logs:
```bash
python3 scripts/monitor.py
```
Observe the Apple Home subscription establishment:
- Confirm log: `Added subscription SubscriptionIds { ... }, min_int_secs: ..., max_int_secs: 120` (previously `600`).
- Confirm `IM: Subscribe response` writes `max_int_secs = 120`.

- [ ] **Step 4: Verify automatic reconnect after reboot**

1. Trigger a reboot (e.g. power cycle or reset).
2. Do **not** touch your iPhone or open the Apple Home app.
3. Observe the Apple Home Hub log:
   - Within **≤ 120 seconds**, verify the Apple Home Hub initiates `CASESigma1`.
   - Verify `Subscription primed` is logged automatically.
