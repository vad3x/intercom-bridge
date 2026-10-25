# Shared Diagnostics: Restart & Factory Reset Design

## 1. Overview
This specification details the addition of **Restart** and **Factory Reset** maintenance capabilities to the shared diagnostics page (`DiagnosticsTab`) in `@embedded-device-foundation/dashboard-core`, along with complete end-to-end integration into the consuming firmware bridges: `intercom-matter` and `hvac-bridge`.

---

## 2. Shared Dashboard Core (`embedded-device-foundation`)

### 2.1 API Client (`packages/dashboard-core/src/api.ts`)
Add maintenance API endpoints to `createDashboardApi`:
```ts
async reboot(): Promise<void> {
  await request('/api/action/reboot', { method: 'POST' });
},
async factoryReset(): Promise<void> {
  await request('/api/action/reset/factory', { method: 'POST' });
},
```

### 2.2 Component Props (`packages/dashboard-core/src/DiagnosticsTab.tsx` & `DashboardShell.tsx`)
Extend `DiagnosticsTabProps` and `DashboardShellProps` with optional action callbacks:
```ts
export interface DiagnosticsTabProps {
  status: SystemDiagnosticsBase | null | undefined;
  onReboot?: () => Promise<void>;
  onFactoryReset?: () => Promise<void>;
}

export interface DashboardShellProps {
  // ... existing props
  onReboot?: () => Promise<void>;
  onFactoryReset?: () => Promise<void>;
}
```
`DashboardShell` forwards `props.onReboot` and `props.onFactoryReset` to `<DiagnosticsTab />`. If a callback is undefined, its action button is hidden.

### 2.3 System Controls UI & Inline Confirmation (`packages/dashboard-core/src/DiagnosticsTab.tsx`)
A new "System Controls" card is rendered when `props.onReboot` or `props.onFactoryReset` is provided.

- **Restart Action**:
  - Initial state: Warning button (`btn-warning`) labeled "Restart Device" with a `RotateCw` icon.
  - Confirmation state: Warning card with callout: *"Restarting will temporarily disconnect the device while it reboots. Are you sure?"*
  - Buttons: **"Confirm Restart"** (with spinner when pending) and **"Cancel"**.
- **Factory Reset Action**:
  - Initial state: Danger button (`btn-error`) labeled "Factory Reset" with a `Trash2` / `AlertTriangle` icon.
  - Confirmation state: Error card with callout: *"Factory reset will permanently erase all network configuration (Wi-Fi), paired fabrics/credentials, and restore defaults. Are you sure?"*
  - Buttons: **"Confirm Factory Reset"** (with spinner when pending) and **"Cancel"**.
- Proper mutual exclusion: only one action can be confirming/pending at a time, and controls are disabled while a request is in flight.

---

## 3. `intercom-matter` Integration

### 3.1 HTTP Router & Server (`crates/intercom-adapter-http`)
- **`src/router.rs`**:
  - Add `Route::ActionReboot` and `Route::ActionResetFactory`.
  - Update `From<DiagnosticRoute> for Route`:
    - `DiagnosticRoute::Reboot => Route::ActionReboot`
    - `DiagnosticRoute::Reset(ResetScope::Factory) => Route::ActionResetFactory`
  - Update router tests in `tests/router_tests.rs`.
- **`src/server.rs`**:
  - Add routes:
    - `/api/action/reboot` &rarr; sets `state.reboot_requested.set(true)`, returns `{"status":"ok"}`.
    - `/api/action/reset/factory` &rarr; sets `state.reset_requested.set(Some(ResetScope::Factory))`, returns `{"status":"ok"}`.
  - The server accept loop executes the reset/reboot using `FirmwareResetDriver` after `reboot_delay`.

### 3.2 Frontend (`crates/intercom-adapter-http/frontend`)
- **`store/intercomStore.ts`**:
  - Add `reboot()` and `factoryReset()` methods using `api.reboot()` and `api.factoryReset()`.
- **`src/App.tsx`**:
  - Implement `handleReboot` and `handleFactoryReset` handlers showing toast feedback.
  - Pass `onReboot={handleReboot}` and `onFactoryReset={handleFactoryReset}` to `<DashboardShell />`.
- **Asset Build**:
  - Run `cargo fe build` to compile the single-file asset `crates/intercom-adapter-http/assets/dashboard.html`.

---

## 4. `hvac-bridge` Integration

### 4.1 HTTP Router & Server (`crates/hvac-adapter-http`)
- **`src/router.rs`**:
  - Add `Route::ActionReboot` and `Route::ActionResetFactory`.
  - In `parse_route`:
    - Map `DiagnosticRoute::Reboot => Route::ActionReboot`.
    - Map `DiagnosticRoute::Reset(ResetScope::Factory) => Route::ActionResetFactory`.
  - Update router tests in `tests/router_tests.rs`.
- **`src/server.rs`**:
  - Handle `Route::ActionReboot`:
    - Write `HTTP_200_JSON_OK`, flush, delay by `config.reboot_delay`, and call `reset.reboot()`.
  - Handle `Route::ActionResetFactory`:
    - Notify `state_sender` of reset.
    - Write `HTTP_200_JSON_OK`, flush, delay by `config.reboot_delay`, call `reset.erase_credentials(ResetScope::Factory)`, and call `reset.reboot()`.

### 4.2 Frontend (`crates/hvac-adapter-http/frontend`)
- **`src/api.ts` / store**:
  - Expose `reboot()` and `factoryReset()`.
- **`src/App.tsx`**:
  - Implement `handleReboot` and `handleFactoryReset` handlers with toasts.
  - Pass `onReboot={handleReboot}` and `onFactoryReset={handleFactoryReset}` to `<DashboardShell />`.
- **Asset Build**:
  - Rebuild `assets/dashboard.html` for `hvac-bridge`.

---

## 5. Verification Plan
1. **Frontend Unit Tests & Checks**:
   - In `embedded-device-foundation/packages/dashboard-core`: `bun run check`.
   - In `intercom-matter/crates/intercom-adapter-http/frontend`: `cargo fe check`.
   - In `hvac-bridge/crates/hvac-adapter-http/frontend`: `bun run check`.
2. **Host Rust Unit Tests**:
   - `mise exec -- cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
   - `cargo test -p hvac-adapter-http --target x86_64-apple-darwin` in `hvac-bridge`.
3. **Firmware Build Verification**:
   - `mise exec -- cargo build -p intercom-firmware --target riscv32imac-unknown-none-elf`
   - Check `hvac-bridge` firmware build if applicable.
