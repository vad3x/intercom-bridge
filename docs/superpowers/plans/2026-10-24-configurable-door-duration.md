# Configurable Door Opened Duration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a configurable door strike unlock duration (1 to 30 seconds, defaulting to 5s), persisted in `system_kvs` non-volatile flash memory, and controlled symmetrically via the Solid.js Web Dashboard and Matter protocol (`AutoRelockTime` attribute), requiring zero device resets or repartitioning.

**Architecture:** Hexagonal Architecture (Ports & Adapters). Domain `AccessStateMachine` in `intercom-core` is the single source of truth for duration and relock deadlines. Inbound commands flow through `AccessControllerHandle::set_unlock_duration`. Outbound effects configure `GpioDoorDriver::set_unlock_duration_ms` and persist to `system_kvs` (offset `0x6c0000`, 8 KB) via a persistent KV store. Matter `IntercomDoorLock` and `HttpServer` synchronize bidirectionally with zero-allocation telemetry.

**Tech Stack:** Rust (`no_std`, RISC-V `riscv32imac-unknown-none-elf` for ESP32-C6 / host testing on `x86_64-apple-darwin`), Embassy async runtime, `rs-matter`, `picoserve`, Solid.js, DaisyUI, Tailwind CSS, Bun.

## Global Constraints
- Target flash layout offset `0x6c0000` (8 KB) is preserved identically; partition label updated to `system_kvs`.
- Upgrades must be 100% additive: missing `DOOR_DURATION_KEY` on first boot defaults to 5s without wipe.
- Factory reset erases both `matter_kvs` and `system_kvs` (reverting to 5s); Ecosystem reset only erases `matter_kvs` (preserving custom duration and Wi-Fi credentials).
- Range is strictly constrained between 1 and 30 seconds inclusive.
- `trigger_unlock(&mut self)` on `DoorDriver` remains parameterless.

---

### Task 1: Core Domain Access State Machine & Ports (`intercom-core`)

**Files:**
- Modify: `crates/intercom-core/src/domain/access.rs`
- Modify: `crates/intercom-core/src/ports/inbound.rs`
- Modify: `crates/intercom-core/src/ports/mod.rs`
- Modify: `crates/intercom-core/src/ports/reporter.rs`
- Modify: `crates/intercom-core/src/domain/diagnostics.rs`
- Modify: `crates/intercom-core/src/service/access.rs`
- Modify: `crates/intercom-core/tests/service_tests.rs`

**Interfaces:**
- Produces:
  - `AccessStateMachine::with_duration(unlock_duration_sec: u32) -> Self`
  - `AccessStateMachine::unlock_duration_sec(&self) -> u32`
  - `AccessCommand::SetUnlockDuration(u32)`
  - `AccessControllerHandle::set_unlock_duration(&self, duration_sec: u32) -> Result<(), ()>`
  - `DoorDriver::set_unlock_duration_ms(&mut self, duration_ms: u32)` (default empty implementation)
  - `IntercomReporter::report_unlock_duration(&self, duration_sec: u32)`
  - `IntercomState { door_locked: bool, door_unlock_duration_sec: u32 }`

- [ ] **Step 1: Write failing unit tests for configurable duration in `service_tests.rs`**

Add tests in `crates/intercom-core/tests/service_tests.rs`:
```rust
#[test]
fn access_state_machine_default_duration_is_5_seconds() {
    let sm = AccessStateMachine::new();
    assert_eq!(sm.unlock_duration_sec(), 5);
}

#[test]
fn access_state_machine_set_duration_updates_deadline() {
    let mut sm = AccessStateMachine::new();
    let effects = sm.handle_command(AccessCommand::SetUnlockDuration(10), 1000);
    assert_eq!(sm.unlock_duration_sec(), 10);
    assert_eq!(effects.report_duration, Some(10));

    let unlock_effects = sm.handle_command(AccessCommand::Unlock, 2000);
    assert!(unlock_effects.trigger_unlock);
    assert_eq!(sm.next_deadline(), Some(2000 + 10_000));
}

#[test]
fn access_state_machine_clamps_duration_to_bounds() {
    let mut sm = AccessStateMachine::new();
    sm.handle_command(AccessCommand::SetUnlockDuration(0), 1000);
    assert_eq!(sm.unlock_duration_sec(), 1);

    sm.handle_command(AccessCommand::SetUnlockDuration(45), 1000);
    assert_eq!(sm.unlock_duration_sec(), 30);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `mise exec -- cargo test -p intercom-core --test service_tests --target x86_64-apple-darwin`
Expected: FAIL with missing fields/methods on `AccessStateMachine` and `AccessCommand`.

- [ ] **Step 3: Implement domain state machine and port updates**

In `crates/intercom-core/src/domain/access.rs`:
- Add `unlock_duration_sec: u32` to `AccessStateMachine`.
- In `AccessStateMachine::new()`, default `unlock_duration_sec: 5`.
- Add `pub const fn with_duration(mut self, duration_sec: u32) -> Self` clamping to `1..=30`.
- Add `pub const fn unlock_duration_sec(&self) -> u32 { self.unlock_duration_sec }`.
- In `handle_unlock(&mut self, now_ms: u64)`:
  `self.relock_deadline_ms = Some(now_ms + (self.unlock_duration_sec as u64 * 1000));`
- Add `pub report_duration: Option<u32>` to `AccessEffects`.
- In `handle_command`:
  Handle `AccessCommand::SetUnlockDuration(sec)`: clamp to `1..=30`, set `self.unlock_duration_sec = clamped`, return `AccessEffects { report_duration: Some(clamped), ..Default::default() }`.

In `crates/intercom-core/src/ports/inbound.rs`:
- Add `SetUnlockDuration(u32)` to `AccessCommand`.
- Add method on `AccessControllerHandle`:
  ```rust
  pub fn set_unlock_duration(&self, duration_sec: u32) -> Result<(), ()> {
      self.sender.try_send(AccessCommand::SetUnlockDuration(duration_sec)).map_err(|_| ())
  }
  ```

In `crates/intercom-core/src/ports/mod.rs`:
- In `pub trait DoorDriver`:
  ```rust
  pub trait DoorDriver {
      async fn trigger_unlock(&mut self);
      fn set_unlock_duration_ms(&mut self, _duration_ms: u32) {}
  }
  ```

In `crates/intercom-core/src/ports/reporter.rs`:
- In `pub trait IntercomReporter`:
  ```rust
  fn report_unlock_duration(&self, duration_sec: u32);
  ```
- Implement for tuples `(A, B)`:
  ```rust
  impl<A: IntercomReporter, B: IntercomReporter> IntercomReporter for (A, B) {
      fn report_ring(&self, ringing: bool) { self.0.report_ring(ringing); self.1.report_ring(ringing); }
      fn report_lock_state(&self, state: LockState) { self.0.report_lock_state(state); self.1.report_lock_state(state); }
      fn report_unlock_duration(&self, duration_sec: u32) { self.0.report_unlock_duration(duration_sec); self.1.report_unlock_duration(duration_sec); }
  }
  ```

In `crates/intercom-core/src/domain/diagnostics.rs`:
- Update `IntercomState`:
  ```rust
  pub struct IntercomState {
      pub door_locked: bool,
      pub door_unlock_duration_sec: u32,
  }
  impl Default for IntercomState {
      fn default() -> Self {
          Self { door_locked: true, door_unlock_duration_sec: 5 }
      }
  }
  ```

In `crates/intercom-core/src/service/access.rs`:
- In `dispatch_effects`:
  ```rust
  if let Some(duration_sec) = effects.report_duration {
      self.door.set_unlock_duration_ms(duration_sec * 1000);
      self.reporter.report_unlock_duration(duration_sec);
  }
  ```

- [ ] **Step 4: Run tests to verify pass**

Run: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS.

- [ ] **Step 5: Commit changes**

```bash
git add crates/intercom-core
git commit -m "feat(core): support configurable door unlock duration in access state machine"
```

---

### Task 2: Hardware Driver Door Pulse Duration Configuration (`intercom-adapter-gpio`)

**Files:**
- Modify: `crates/intercom-adapter-gpio/src/door.rs`
- Modify: `crates/intercom-adapter-gpio/src/led.rs` (implement `report_unlock_duration` no-op)

**Interfaces:**
- Consumes: `DoorDriver::set_unlock_duration_ms` from `intercom-core`
- Produces: `GpioDoorDriver` dynamically updating `self.config.unlock_pulse_ms`

- [ ] **Step 1: Update `GpioDoorDriver` with `set_unlock_duration_ms`**

In `crates/intercom-adapter-gpio/src/door.rs`:
Implement `set_unlock_duration_ms` in `DoorDriver for GpioDoorDriver`:
```rust
impl DoorDriver for GpioDoorDriver {
    async fn trigger_unlock(&mut self) {
        if self.unlocking {
            warn!("Unlock already in progress, ignoring");
            return;
        }

        info!("Unlock pulse start ({}ms)", self.config.unlock_pulse_ms);
        self.unlocking = true;

        self.config.pin.set_high();
        Timer::after(Duration::from_millis(self.config.unlock_pulse_ms)).await;
        self.config.pin.set_low();

        self.unlocking = false;
        info!("Unlock pulse end");
    }

    fn set_unlock_duration_ms(&mut self, duration_ms: u32) {
        info!("Updating unlock pulse duration: {}ms", duration_ms);
        self.config.unlock_pulse_ms = duration_ms as u64;
    }
}
```

In `crates/intercom-adapter-gpio/src/led.rs`:
Implement `report_unlock_duration` for `GpioLedDriver`:
```rust
impl IntercomReporter for GpioLedDriver {
    fn report_ring(&self, ringing: bool) { ... }
    fn report_lock_state(&self, state: LockState) { ... }
    fn report_unlock_duration(&self, _duration_sec: u32) {
        // LED indicator does not change based on duration setting
    }
}
```

- [ ] **Step 2: Verify compilation and tests**

Run: `mise exec -- cargo check -p intercom-adapter-gpio`
Expected: PASS.

- [ ] **Step 3: Commit changes**

```bash
git add crates/intercom-adapter-gpio
git commit -m "feat(gpio): implement dynamic unlock pulse duration in GpioDoorDriver"
```

---

### Task 3: Partition Table Renaming & Settings Persistence Driver (`intercom-firmware`, `partitions.csv`)

**Files:**
- Modify: `partitions.csv`
- Modify: `crates/intercom-firmware/src/config.rs`
- Modify: `crates/intercom-firmware/src/reset.rs`
- Modify: `crates/intercom-firmware/tests/partitioned_kv_tests.rs`

**Interfaces:**
- Produces:
  - `SYSTEM_KVS_OFFSET = 0x6c0000`, `SYSTEM_KVS_SIZE = 0x2000`, `SYSTEM_KVS_RANGE`
  - `DOOR_DURATION_KEY: u16 = 0x1001`
  - `PartitionedWifiStore` routes `NETWORKS_KEY` and `DOOR_DURATION_KEY` to `system_store`
  - `save_door_duration(store, duration_sec: u32) -> Result<(), ()>`
  - `load_door_duration(store) -> u32` (defaults to 5)

- [ ] **Step 1: Write unit test in `partitioned_kv_tests.rs` for `DOOR_DURATION_KEY` routing**

In `crates/intercom-firmware/tests/partitioned_kv_tests.rs`:
```rust
#[test]
fn test_partitioned_kv_store_routes_door_duration_key_to_system_store() {
    let matter_store = MockKvStore::new();
    let system_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut scratch = [0u8; 32];

    const DOOR_DURATION_KEY: u16 = 0x1001;
    let duration_bytes = 7u32.to_le_bytes();
    partitioned.store(DOOR_DURATION_KEY, &duration_bytes, &mut scratch).unwrap();

    // Verify written to system store, not matter store
    assert_eq!(partitioned.matter_store.load(DOOR_DURATION_KEY, &mut scratch).unwrap(), None);
    assert_eq!(partitioned.system_store.load(DOOR_DURATION_KEY, &mut scratch).unwrap(), Some(duration_bytes.as_slice()));
    assert_eq!(partitioned.load(DOOR_DURATION_KEY, &mut scratch).unwrap(), Some(duration_bytes.as_slice()));
}
```

- [ ] **Step 2: Update `partitions.csv` and `crates/intercom-firmware/src/config.rs`**

In `partitions.csv`:
```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
system_kvs,   data, nvs,     0x6c0000, 0x2000,
matter_kvs,   data, nvs,     0x6c2000, 0x6000,
```

In `crates/intercom-firmware/src/config.rs`:
```rust
pub const SYSTEM_KVS_OFFSET: u32 = 0x6c0000;
pub const SYSTEM_KVS_SIZE: u32 = 0x2000;
pub const SYSTEM_KVS_RANGE: Range<u32> = SYSTEM_KVS_OFFSET..SYSTEM_KVS_OFFSET + SYSTEM_KVS_SIZE;

// Backward compatibility alias:
pub const WIFI_KVS_OFFSET: u32 = SYSTEM_KVS_OFFSET;
pub const WIFI_KVS_SIZE: u32 = SYSTEM_KVS_SIZE;
pub const WIFI_KVS_RANGE: Range<u32> = SYSTEM_KVS_RANGE;

pub const DOOR_DURATION_KEY: u16 = 0x1001;
pub const DEFAULT_DOOR_DURATION_SEC: u32 = 5;
```

- [ ] **Step 3: Update `reset.rs` & store routing**

In `crates/intercom-firmware/src/reset.rs`:
- Update `erase_flash_range` calls in `FirmwareResetDriver::erase_credentials`:
  - `ResetScope::Ecosystem`: erases `MATTER_KVS_RANGE`.
  - `ResetScope::Factory`: erases `MATTER_KVS_RANGE` and `SYSTEM_KVS_RANGE`.
- Update `PartitionedWifiStore` routing wrapper (or alias) so `key == NETWORKS_KEY || key == DOOR_DURATION_KEY` routes to system store.
- Add helper functions:
  ```rust
  pub fn load_door_duration<S: rs_matter::persist::KvBlobStore>(store: &mut S) -> u32 {
      let mut buf = [0u8; 8];
      match store.load(crate::config::DOOR_DURATION_KEY, &mut buf) {
          Ok(Some(slice)) if slice.len() >= 4 => {
              let val = u32::from_le_bytes(slice[..4].try_into().unwrap());
              if (1..=30).contains(&val) { val } else { crate::config::DEFAULT_DOOR_DURATION_SEC }
          }
          _ => crate::config::DEFAULT_DOOR_DURATION_SEC,
      }
  }

  pub fn save_door_duration<S: rs_matter::persist::KvBlobStore>(store: &mut S, duration_sec: u32) -> Result<(), ()> {
      let mut buf = [0u8; 8];
      let bytes = duration_sec.to_le_bytes();
      store.store(crate::config::DOOR_DURATION_KEY, &bytes, &mut buf).map_err(|_| ())
  }
  ```

- [ ] **Step 4: Run tests to verify pass**

Run: `mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin`
Expected: PASS.

- [ ] **Step 5: Commit changes**

```bash
git add partitions.csv crates/intercom-firmware
git commit -m "feat(firmware): rename wifi_kvs to system_kvs and add door duration persistence"
```

---

### Task 4: Matter Protocol AutoRelockTime Attribute Read/Write (`intercom-adapter-matter`)

**Files:**
- Modify: `crates/intercom-adapter-matter/src/door_lock.rs`
- Modify: `crates/intercom-adapter-matter/src/access_control.rs`
- Modify: `crates/intercom-adapter-matter/src/lib.rs`

**Interfaces:**
- Consumes: `IntercomReporter::report_unlock_duration`
- Produces:
  - `IntercomDoorLock::set_unlock_duration(&self, duration_sec: u32)`
  - Spec-compliant `AutoRelockTime` (attribute id 35) read and write handlers

- [ ] **Step 1: Add `unlock_duration_sec: AtomicU32` and implement write handler in `door_lock.rs`**

In `crates/intercom-adapter-matter/src/door_lock.rs`:
- Add `unlock_duration_sec: core::sync::atomic::AtomicU32` to `IntercomDoorLock`.
- In `IntercomDoorLock::new()`:
  `unlock_duration_sec: AtomicU32::new(5)`.
- Add public method:
  ```rust
  pub fn set_unlock_duration(&self, duration_sec: u32) {
      let clamped = duration_sec.clamp(1, 30);
      let prev = self.unlock_duration_sec.swap(clamped, Ordering::SeqCst);
      if prev != clamped {
          info!("Door lock auto-relock duration updated: {}s -> {}s", prev, clamped);
          self.dataver.changed();
          self.state_changed.signal(());
      }
  }
  ```
- In `read()` for `door_lock::AttributeId::AutoRelockTime`:
  ```rust
  x if x == door_lock::AttributeId::AutoRelockTime as u16 => {
      Reply::set(writer, self.unlock_duration_sec.load(Ordering::Relaxed))
  }
  ```
- In `write()` for `door_lock::AttributeId::AutoRelockTime`:
  ```rust
  x if x == door_lock::AttributeId::AutoRelockTime as u16 => {
      let val = ctx.data().u32().map_err(|_| ErrorCode::InvalidDataType)?;
      if !(1..=30).contains(&val) {
          return Err(ErrorCode::ConstraintError.into());
      }
      self.unlock_duration_sec.store(val, Ordering::Relaxed);
      self.dataver.changed();
      ctx.notify_changed();
      let _ = self.config.access.set_unlock_duration(val);
      Ok(())
  }
  ```

In `crates/intercom-adapter-matter/src/access_control.rs`:
Implement `report_unlock_duration` on `MatterAccessControlDriver`:
```rust
impl IntercomReporter for MatterAccessControlDriver<'_> {
    fn report_ring(&self, ringing: bool) {
        self.occupancy.set_occupancy(ringing);
    }
    fn report_lock_state(&self, state: LockState) {
        match state {
            LockState::Locked => self.door_lock.lock(),
            LockState::Unlocked => self.door_lock.unlock(),
        }
    }
    fn report_unlock_duration(&self, duration_sec: u32) {
        self.door_lock.set_unlock_duration(duration_sec);
    }
}
```

- [ ] **Step 2: Check compilation**

Run: `mise exec -- cargo check -p intercom-adapter-matter`
Expected: PASS.

- [ ] **Step 3: Commit changes**

```bash
git add crates/intercom-adapter-matter
git commit -m "feat(matter): implement AutoRelockTime attribute read and write handling"
```

---

### Task 5: REST API & Status Telemetry (`intercom-adapter-http`)

**Files:**
- Modify: `crates/intercom-adapter-http/src/router.rs`
- Modify: `crates/intercom-adapter-http/src/server.rs`
- Modify: `crates/intercom-adapter-http/tests/router_tests.rs`

**Interfaces:**
- Produces:
  - Route `Route::ActionSetDoorDuration(u32)`
  - `POST /api/action/door-duration?seconds=N`
  - Telemetry output: `,"door_unlock_duration_sec":N` in `/api/status`

- [ ] **Step 1: Write failing router test in `router_tests.rs`**

In `crates/intercom-adapter-http/tests/router_tests.rs`:
```rust
#[test]
fn parse_route_action_door_duration() {
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=7 HTTP/1.1"),
        Route::ActionSetDoorDuration(7)
    );
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=0 HTTP/1.1"),
        Route::NotFound
    );
    assert_eq!(
        parse_route("POST /api/action/door-duration?seconds=35 HTTP/1.1"),
        Route::NotFound
    );
}

#[test]
fn format_status_json_includes_door_unlock_duration() {
    let mut diag = test_diagnostics();
    diag.domain.door_unlock_duration_sec = 8;
    let mut buf = [0u8; 1024];
    let len = format_status_json(&diag, &mut buf).unwrap();
    let json = core::str::from_utf8(&buf[..len]).unwrap();
    assert!(json.contains("\"door_unlock_duration_sec\":8"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
Expected: FAIL.

- [ ] **Step 3: Implement router parsing and JSON serialization**

In `crates/intercom-adapter-http/src/router.rs`:
- Update `Route`:
  ```rust
  pub enum Route {
      Dashboard,
      Status,
      Logs,
      ActionRing,
      ActionUnlock,
      ActionResetMatter,
      ActionSetDoorDuration(u32),
      OtaFlash,
      NotFound,
  }
  ```
- In `parse_route`:
  Parse `POST /api/action/door-duration`:
  Extract query param `seconds=N`. Parse as `u32`. If `1..=30`, return `Route::ActionSetDoorDuration(N)`, else `Route::NotFound`.
- In `IntercomPayload::write_fields`:
  ```rust
  write!(
      out,
      ",\"door_locked\":{},\"door_unlock_duration_sec\":{}",
      if self.0.door_locked { "true" } else { "false" },
      self.0.door_unlock_duration_sec
  )
  ```

In `crates/intercom-adapter-http/src/server.rs`:
- Register `picoserve` route:
  ```rust
  #[derive(serde::Deserialize)]
  struct DoorDurationQuery {
      seconds: u32,
  }
  ```
  ```rust
  .route(
      "/api/action/door-duration",
      picoserve::routing::post(
          |picoserve::extract::State(state): picoserve::extract::State<AppState<'a, H, E, O, T>>,
           picoserve::extract::Query(query): picoserve::extract::Query<DoorDurationQuery>| async move {
              if (1..=30).contains(&query.seconds) {
                  let _ = state.access_handle.set_unlock_duration(query.seconds);
                  picoserve::response::Response::ok("{\"status\":\"ok\"}")
                      .with_content_type("application/json")
              } else {
                  picoserve::response::Response::bad_request("{\"error\":\"invalid_duration\"}")
                      .with_content_type("application/json")
              }
          },
      ),
  )
  ```

- [ ] **Step 4: Run router tests to verify pass**

Run: `mise exec -- cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: PASS.

- [ ] **Step 5: Commit changes**

```bash
git add crates/intercom-adapter-http
git commit -m "feat(http): add /api/action/door-duration endpoint and status serialization"
```

---

### Task 6: Web Dashboard Frontend Controls (`intercom-adapter-http/frontend`)

**Files:**
- Modify: `crates/intercom-adapter-http/frontend/src/types.ts`
- Modify: `crates/intercom-adapter-http/frontend/src/api.ts`
- Modify: `crates/intercom-adapter-http/frontend/src/components/IntercomControls.tsx`
- Modify: `crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`
- Output: `crates/intercom-adapter-http/assets/dashboard.html`

**Interfaces:**
- Produces:
  - `IntercomStatus { door_locked: boolean; door_unlock_duration_sec?: number }`
  - `setDoorUnlockDuration(seconds: number): Promise<void>`
  - Stepper controls on Door Strike Card in `IntercomControls.tsx`

- [ ] **Step 1: Update TypeScript types and API client**

In `crates/intercom-adapter-http/frontend/src/types.ts`:
```typescript
export interface IntercomStatus {
  door_locked: boolean;
  door_unlock_duration_sec?: number;
}
```

In `crates/intercom-adapter-http/frontend/src/api.ts`:
```typescript
export async function setDoorUnlockDuration(seconds: number): Promise<void> {
  const res = await fetch(`/api/action/door-duration?seconds=${seconds}`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to update duration: HTTP ${res.status}`);
  }
}
```

- [ ] **Step 2: Add duration state and handler in `intercomStore.ts`**

In `crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`:
- Expose `doorUnlockDurationSec: () => number` (default 5).
- Add `updateDoorUnlockDuration(seconds: number): Promise<void>` that optimistically updates the local duration and calls `api.setDoorUnlockDuration(seconds)`.

- [ ] **Step 3: Add stepper controls to `IntercomControls.tsx`**

In `crates/intercom-adapter-http/frontend/src/components/IntercomControls.tsx`:
- Add props:
  ```typescript
  export interface IntercomControlsProps {
    doorLocked: boolean | undefined;
    doorUnlockDurationSec?: number;
    isUnlocking: boolean;
    isRinging: boolean;
    onUnlock: () => void;
    onRing: () => void;
    onDurationChange?: (seconds: number) => void;
  }
  ```
- On the Door Strike Card:
  - Dynamic subtitle: `PhotoMOS Relay • Auto-relocks after {duration}s strike pulse`.
  - Add inline stepper control below subtitle:
    ```tsx
    <div class="flex items-center justify-center gap-2 mt-3">
      <span class="text-xs text-base-content/70">Duration:</span>
      <div class="join border border-base-content/20 rounded-lg">
        <button
          type="button"
          class="join-item btn btn-xs btn-ghost px-2 font-bold"
          disabled={props.isUnlocking || (props.doorUnlockDurationSec ?? 5) <= 1}
          onClick={() => props.onDurationChange?.((props.doorUnlockDurationSec ?? 5) - 1)}
        >
          -
        </button>
        <span class="join-item px-2.5 py-0.5 text-xs font-mono font-bold flex items-center bg-base-200">
          {(props.doorUnlockDurationSec ?? 5)}s
        </span>
        <button
          type="button"
          class="join-item btn btn-xs btn-ghost px-2 font-bold"
          disabled={props.isUnlocking || (props.doorUnlockDurationSec ?? 5) >= 30}
          onClick={() => props.onDurationChange?.((props.doorUnlockDurationSec ?? 5) + 1)}
        >
          +
        </button>
      </div>
    </div>
    ```

- [ ] **Step 4: Run frontend validation and build bundle**

Run:
```bash
cargo fe-check
cargo fe-build
```
Expected:
`cargo fe-check` passes without lint or type errors.
`cargo fe-build` compiles `assets/dashboard.html`.

- [ ] **Step 5: Commit changes**

```bash
git add crates/intercom-adapter-http/frontend crates/intercom-adapter-http/assets
git commit -m "feat(fe): add door unlock duration stepper controls to dashboard"
```

---

### Task 7: Firmware Composition Root Wiring & End-to-End Verification (`intercom-firmware`)

**Files:**
- Modify: `crates/intercom-firmware/src/main.rs`
- Modify: `crates/intercom-firmware/src/hardware_info.rs` (ensure telemetry snapshot populates `door_unlock_duration_sec`)

**Interfaces:**
- Connects:
  - Initial load of `DOOR_DURATION_KEY` from `system_kvs`
  - Passes duration into `AccessStateMachine::with_duration` and `IntercomDoorLock::new`
  - Wires persistence save callback when duration changes

- [ ] **Step 1: Wire persistence into `main.rs`**

In `crates/intercom-firmware/src/main.rs`:
- Create `system_kvs` store before controller and load initial duration:
  ```rust
  let initial_duration_sec = reset::load_door_duration(&mut store);
  info!("Loaded door unlock duration: {}s", initial_duration_sec);
  ```
- Initialize `door_lock` with initial duration:
  `door_lock.set_unlock_duration(initial_duration_sec);`
- In `AccessStateMachine`:
  Initialize with `AccessStateMachine::new().with_duration(initial_duration_sec)`.
- Configure `GpioDoorDriver` with initial duration:
  `door.set_unlock_duration_ms(initial_duration_sec * 1000);`
- In `HardwareInfoDriver` / `DiagnosticsService`:
  Ensure `IntercomState { door_locked, door_unlock_duration_sec }` reflects the active duration.

- [ ] **Step 2: Run all workspace host tests**

Run:
```bash
mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin
mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin
mise exec -- cargo test -p intercom-firmware --test partitioned_kv_tests --target x86_64-apple-darwin
```
Expected: All tests PASS.

- [ ] **Step 3: Compile firmware for target**

Run:
```bash
mise exec -- cargo build -p intercom-firmware
mise exec -- cargo build -p intercom-firmware --release
```
Expected: Successful compile of `riscv32imac-unknown-none-elf` firmware binary.

- [ ] **Step 4: Commit changes**

```bash
git add crates/intercom-firmware
git commit -m "feat(firmware): wire door duration persistence and composition root"
```

---

## Plan Self-Review Check
- [x] **Spec coverage**:
  - Additive flash persistence with `system_kvs` at `0x6c0000`? Covered in Task 3.
  - Parameterless `trigger_unlock` and `self.config.unlock_pulse_ms`? Covered in Task 1 & 2.
  - Matter `AutoRelockTime` read/write? Covered in Task 4.
  - REST API endpoint `POST /api/action/door-duration`? Covered in Task 5.
  - Solid.js stepper controls? Covered in Task 6.
  - Composition root wiring & verification? Covered in Task 7.
- [x] **No Placeholders**: All tests, method signatures, file paths, and exact CLI commands are explicitly written.
- [x] **Type consistency**: `door_unlock_duration_sec` and `u32` (1..=30) used consistently across all tasks.
