# Configurable Door Opened Duration Specification

- **Date:** 2026-10-24
- **Status:** Approved / Ready for Implementation
- **Target:** `partitions.csv`, `intercom-core`, `intercom-adapter-gpio`, `intercom-adapter-matter`, `intercom-adapter-http`, `intercom-firmware`

---

## 1. Executive Summary

This specification defines the implementation of a configurable door strike unlock duration (1 to 30 seconds, defaulting to 5 seconds) persisted in non-volatile device memory. The feature is exposed and synchronized symmetrically across both the **Web Dashboard** (UI stepper/slider + REST API) and the **Matter smart home protocol** (DoorLock cluster `AutoRelockTime` attribute).

### Motivation & Goals
1. **Dynamic Door Actuation:** Replace hardcoded domain constants (`AUTO_RELOCK_SEC = 4` and `UNLOCK_PULSE_MS = 5000`) with a single, synchronized source of truth for physical PhotoMOS strike hold and auto-relock scheduling.
2. **Zero-Downtime, Additive Upgrade:** The upgrade requires **no device resets, no repartitioning, and no data wipes**. Upgrading firmware via OTA preserves existing Wi-Fi credentials and Matter fabric pairings.
3. **Unified Controls:** Changes made from the Web Dashboard immediately synchronize to Matter controllers (pushing subscription updates to Apple Home, Home Assistant, etc.), and changes made from Matter controllers reflect immediately in the Web Dashboard.
4. **Persistent Lifecycle Management:**
   - **Factory Reset ($\ge$ 10s hold or web reset):** Clears credentials and restores default duration (5s).
   - **Ecosystem Reset (5s hold or web reset):** Unpairs Matter fabrics while **preserving** both Wi-Fi credentials and the user's custom door duration.

---

## 2. Flash Memory & Additive Storage Strategy

### 2.1 Partition Table Evolution (`system_kvs`)
The 8 KB flash partition at offset `0x6c0000` previously labeled `wifi_kvs` is generalized to `system_kvs` in `partitions.csv` and across the Rust codebase.

```csv
# Name,       Type, SubType, Offset,   Size,     Flags
nvs,          data, nvs,     0x9000,   0x6000,
otadata,      data, ota,     0xf000,   0x2000,
ota_0,        app,  ota_0,   0x20000,  0x350000,
ota_1,        app,  ota_1,   0x370000, 0x350000,
system_kvs,   data, nvs,     0x6c0000, 0x2000,
matter_kvs,   data, nvs,     0x6c2000, 0x6000,
```

### 2.2 Additive Migration Guarantee
- **Identical Flash Offset and Boundaries:** Because the partition remains at `0x6c0000..0x6c2000` (8 KiB, 2 sectors), existing devices flashed prior to this update keep their underlying data completely intact.
- **Log-Structured Storage (`sequential-storage`):** `SeqMapKvBlobStore` stores independent items keyed by `u16`.
  - Wi-Fi credentials remain at `NETWORKS_KEY = 258` (`0x0102`).
  - Configurable settings use vendor key: `DOOR_DURATION_KEY: u16 = 0x1001`.
- **First Boot on Existing Devices:**
  - `store.load(DOOR_DURATION_KEY)` returns `Ok(None)`.
  - System cleanly defaults to **5 seconds**.
  - No flash sectors are erased or rewritten until the user explicitly modifies the duration.
- **USB Flashing Safety:** USB flashes via `espflash flash ... --erase-parts otadata` only erase `otadata`. The partition table header at `0x8000` updates to `system_kvs` while flash data at `0x6c0000` is untouched.

---

## 3. Architecture & Component Design

### 3.1 Domain Core (`intercom-core`)

#### Domain Configuration (`config.rs`)
Single source of truth for duration policy and shared validation:
```rust
pub const MIN_DOOR_UNLOCK_DURATION_SEC: u32 = 1;
pub const MAX_DOOR_UNLOCK_DURATION_SEC: u32 = 30;
pub const DEFAULT_DOOR_UNLOCK_DURATION_SEC: u32 = 5;

pub const fn is_valid_door_unlock_duration(duration_sec: u32) -> bool {
    duration_sec >= MIN_DOOR_UNLOCK_DURATION_SEC && duration_sec <= MAX_DOOR_UNLOCK_DURATION_SEC
}

pub const fn clamp_door_unlock_duration(duration_sec: u32) -> u32 {
    if duration_sec < MIN_DOOR_UNLOCK_DURATION_SEC {
        MIN_DOOR_UNLOCK_DURATION_SEC
    } else if duration_sec > MAX_DOOR_UNLOCK_DURATION_SEC {
        MAX_DOOR_UNLOCK_DURATION_SEC
    } else {
        duration_sec
    }
}
```

#### `AccessStateMachine` (`domain/access.rs`)
- Tracks active `unlock_duration_sec: u32` (clamped via `clamp_door_unlock_duration`, defaulting to `DEFAULT_DOOR_UNLOCK_DURATION_SEC`).
- On `AccessCommand::Unlock`:
  ```rust
  self.relock_deadline_ms = Some(now_ms + (self.unlock_duration_sec as u64 * 1000));
  AccessEffects {
      trigger_unlock: true,
      report_lock: Some(LockState::Unlocked),
      report_ring: None,
      report_duration: None,
  }
  ```
- New domain command: `AccessCommand::SetUnlockDuration(u32)`.
  - Clamps via `clamp_door_unlock_duration(duration_sec)`.
  - Sets `self.unlock_duration_sec = duration_sec`.
  - Emits `AccessEffects { report_duration: Some(duration_sec), .. }`.
- In `AccessController`:
  When handling `SetUnlockDuration(sec)`:
  - Updates state machine.
  - Calls `self.door.set_unlock_duration_ms(sec * 1000)`.
  - Dispatches `report_unlock_duration` to reporter.

#### Port Interfaces
- **`DoorDriver` (`ports/mod.rs`)**:
  ```rust
  pub trait DoorDriver {
      async fn trigger_unlock(&mut self);
      fn set_unlock_duration_ms(&mut self, _duration_ms: u32) {}
  }
  ```
- **`AccessControllerHandle` (`ports/inbound.rs`)**:
  ```rust
  impl AccessControllerHandle<'_> {
      pub fn request_unlock(&self);
      pub fn request_ring(&self);
      pub fn set_unlock_duration(&self, duration_sec: u32) -> Result<(), ()>;
  }
  ```
- **`IntercomReporter` (`ports/reporter.rs`)**:
  ```rust
  pub trait IntercomReporter {
      fn report_ring(&self, ringing: bool);
      fn report_lock_state(&self, state: LockState);
      fn report_unlock_duration(&self, duration_sec: u32);
  }
  ```
- **`Diagnostics` (`domain/diagnostics.rs`)**:
  ```rust
  pub struct IntercomState {
      pub door_locked: bool,
      pub door_unlock_duration_sec: u32,
  }
  ```

---

### 3.2 Hardware Driver Adapter (`intercom-adapter-gpio`)

#### `GpioDoorDriver` (`door.rs`)
- `trigger_unlock(&mut self)` remains parameterless, using `self.config.unlock_pulse_ms` directly:
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

---

### 3.3 Matter Protocol Adapter (`intercom-adapter-matter`)

#### `IntercomDoorLock` (`door_lock.rs`)
- Atomic storage for active duration: `unlock_duration_sec: AtomicU32`.
- **Attribute Read (`AutoRelockTime`, id=35)**:
  Returns `self.unlock_duration_sec.load(Ordering::Relaxed)`.
- **Attribute Write (`AutoRelockTime`, id=35)**:
  - Extracts `value: u32` from `ctx.data().u32()?`.
  - Validates `1 <= value && value <= 30`. Returns `ErrorCode::ConstraintError` if invalid.
  - Updates atomic duration and bumps `dataver`.
  - Dispatches `self.config.access.set_unlock_duration(value)`.
  - Calls `ctx.notify_changed()` to notify all subscribed Matter fabrics.
- **Out-of-band updates (`report_unlock_duration`)**:
  When changed via Web UI, updates `self.unlock_duration_sec`, bumps `dataver`, and signals `state_changed` to push subscription reports to Apple Home / Home Assistant.

---

### 3.4 Web Dashboard & REST API (`intercom-adapter-http`)

#### REST API Endpoints (`router.rs` & `server.rs`)
- **`POST /api/action/door-duration?seconds=N`**:
  - Parses query parameter `seconds`.
  - Validates `1..=30`. Returns `400 Bad Request` on invalid input.
  - Calls `access_handle.set_unlock_duration(seconds)`.
  - Responds `200 OK {"status":"ok","door_unlock_duration_sec":N}`.
- **`GET /api/status`**:
  - Zero-allocation JSON streaming writes:
    `,"door_locked":true,"door_unlock_duration_sec":5`.

#### Solid.js Frontend Dashboard
- **Types (`types.ts`)**:
  ```typescript
  export interface IntercomStatus {
    door_locked: boolean;
    door_unlock_duration_sec: number;
  }
  ```
- **API (`api.ts`)**:
  ```typescript
  export async function setDoorUnlockDuration(seconds: number): Promise<void>;
  ```
- **Controls Component (`IntercomControls.tsx`)**:
  - Add interactive duration selector on the Door Strike card:
    - `[ - ]` button, current value badge (e.g. `5s`), `[ + ]` button.
    - Disabled while unlocking is in progress.
    - Card description dynamically displays: `PhotoMOS Relay • Auto-relocks after {duration}s strike pulse`.

---

### 3.5 Persistence & Firmware Composition Root (`intercom-firmware`)

#### `ConfigStore` / `SettingsDriver`
- Manages reading/writing `DOOR_DURATION_KEY = 0x1001` to `system_kvs`.
- Initialized in `main.rs` before services start:
  - Loads initial duration from flash (fallback `5`).
  - Passes duration into `AccessStateMachine`.
  - Connects persistence callback to `AccessController` for runtime changes.

#### Reset Scopes in `reset.rs`
- **`ResetScope::Ecosystem`**:
  - Erases `matter_kvs` only.
  - `system_kvs` is untouched → Wi-Fi credentials and door duration preserved.
- **`ResetScope::Factory`**:
  - Erases `matter_kvs` and `system_kvs`.
  - Clears all credentials and reverts door duration to default 5s.

---

## 4. Verification & Testing Plan

### 4.1 Automated Tests
1. **`intercom-core` Unit Tests**:
   - `test_access_state_machine_default_duration_is_5s()`
   - `test_access_state_machine_set_duration_updates_relock_deadline()`
   - `test_access_state_machine_rejects_out_of_bounds_durations()`
   - `test_access_controller_handles_set_duration_command()`
2. **`intercom-adapter-http` Unit Tests**:
   - Router tests for `POST /api/action/door-duration` (valid values, out-of-bounds, non-numeric).
   - Zero-allocation JSON serialization test checking `door_unlock_duration_sec` formatting.
3. **`intercom-adapter-matter` Unit Tests**:
   - Reading `AutoRelockTime` returns active value.
   - Writing `AutoRelockTime` handles constraints and dispatches to access handle.
4. **Frontend Verification**:
   - `cargo fe-check` (TypeScript typing + Biome linting).
   - `cargo fe-build` (verifies single-file asset generation for firmware bundling).

### 4.2 Hardware Verification
1. **OTA Upgrade from Existing Firmware**:
   - Flash standard current firmware. Commission Wi-Fi and Matter.
   - Deploy new firmware via `scripts/ota.py`.
   - Verify device boots, connects to Wi-Fi, and defaults to 5s without resetting.
2. **Web Control Actuation**:
   - Change duration to 8s in Web Dashboard.
   - Trigger unlock; measure relay actuation time (exactly 8 seconds).
   - Verify `/api/status` returns 8s.
3. **Matter Controller Actuation**:
   - Read / write `AutoRelockTime` from controller; verify bidirectional update.
4. **Reboot & Reset Persistence**:
   - Power-cycle device; verify 8s remains saved.
   - Perform Ecosystem Reset; verify 8s remains saved.
   - Perform Factory Reset; verify duration reverts to 5s.
