# Shared Diagnostics: Restart & Factory Reset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Restart and Factory Reset capabilities to the shared diagnostics page in `@embedded-device-foundation/dashboard-core`, and integrate them end-to-end into both `intercom-matter` and `hvac-bridge`.

**Architecture:** Extend `@embedded-device-foundation/dashboard-core` with `reboot()` and `factoryReset()` API methods and a "System Controls" card on `DiagnosticsTab` with inline confirmation. Expose corresponding `/api/action/reboot` and `/api/action/reset/factory` HTTP endpoints on both device bridges and wire the actions into their respective frontends.

**Tech Stack:** Rust (`no_std`, `picoserve`, `embassy`), TypeScript, Solid.js, Tailwind CSS, DaisyUI, Bun.

## Global Constraints

- Never use unauthenticated or arbitrary execution; reboot and reset strictly trigger hardware reset drivers.
- Preserve backward compatibility: if `onReboot` or `onFactoryReset` props are omitted, those actions are cleanly hidden.
- Adhere strictly to Hexagonal Architecture: HTTP adapters parse and map requests without embedding hardware peripheral code.
- Run tests and lint checks on each repository's affected crates and frontend packages.

---

### Task 1: Shared Dashboard Core API & Component Maintenance Card (`embedded-device-foundation`)

**Files:**
- Modify: `../embedded-device-foundation/packages/dashboard-core/src/api.ts`
- Modify: `../embedded-device-foundation/packages/dashboard-core/src/DiagnosticsTab.tsx`
- Modify: `../embedded-device-foundation/packages/dashboard-core/src/DashboardShell.tsx`

**Interfaces:**
- Produces:
  - `createDashboardApi().reboot(): Promise<void>`
  - `createDashboardApi().factoryReset(): Promise<void>`
  - `DiagnosticsTabProps.onReboot?: () => Promise<void>`
  - `DiagnosticsTabProps.onFactoryReset?: () => Promise<void>`
  - `DashboardShellProps.onReboot?: () => Promise<void>`
  - `DashboardShellProps.onFactoryReset?: () => Promise<void>`

- [x] **Step 1: Add `reboot` and `factoryReset` to `createDashboardApi`**
  In `../embedded-device-foundation/packages/dashboard-core/src/api.ts`:
  Add `reboot` (`POST /api/action/reboot`) and `factoryReset` (`POST /api/action/reset/factory`).

- [x] **Step 2: Add `onReboot` and `onFactoryReset` props to `DiagnosticsTabProps` and `DashboardShellProps`**
  In `../embedded-device-foundation/packages/dashboard-core/src/DashboardShell.tsx`:
  Add props and forward them to `<DiagnosticsTab status={props.status} onReboot={props.onReboot} onFactoryReset={props.onFactoryReset} />`.

- [x] **Step 3: Implement the "System Controls" card with inline confirmation in `DiagnosticsTab.tsx`**
  In `../embedded-device-foundation/packages/dashboard-core/src/DiagnosticsTab.tsx`:
  Add confirmation states for restart (`confirmingRestart`, `pendingRestart`) and factory reset (`confirmingReset`, `pendingReset`). Render buttons and warning confirmation boxes styled with DaisyUI (`btn-warning`, `btn-error`, alert banners, spinners).

- [x] **Step 4: Typecheck and lint in `dashboard-core`**
  Run `bun run check` in `/Users/vadim/personal/projects/embedded-device-foundation/packages/dashboard-core`.

- [x] **Step 5: Commit in `embedded-device-foundation`**
  Commit the changes in `embedded-device-foundation`.

---

### Task 2: `intercom-matter` HTTP Router & Server (`crates/intercom-adapter-http`)

**Files:**
- Modify: `crates/intercom-adapter-http/src/router.rs`
- Modify: `crates/intercom-adapter-http/src/server.rs`
- Modify: `crates/intercom-adapter-http/tests/router_tests.rs`

**Interfaces:**
- Produces:
  - `Route::ActionReboot`
  - `Route::ActionResetFactory`
  - HTTP endpoints `POST /api/action/reboot` and `POST /api/action/reset/factory`

- [x] **Step 1: Write failing router unit tests for reboot and factory reset**
  In `crates/intercom-adapter-http/tests/router_tests.rs`:
  Add unit tests verifying `parse_route("POST /api/action/reboot HTTP/1.1") == Route::ActionReboot` and `parse_route("POST /api/action/reset/factory HTTP/1.1") == Route::ActionResetFactory`.

- [x] **Step 2: Update `Route` and `From<DiagnosticRoute>` in `router.rs`**
  In `crates/intercom-adapter-http/src/router.rs`:
  Add `ActionReboot` and `ActionResetFactory` to `Route`.
  Map `DiagnosticRoute::Reboot => Route::ActionReboot` and `DiagnosticRoute::Reset(ResetScope::Factory) => Route::ActionResetFactory`.

- [x] **Step 3: Run router tests to ensure they pass**
  Run `mise exec -- cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`.

- [x] **Step 4: Register `/api/action/reboot` and `/api/action/reset/factory` in `server.rs`**
  In `crates/intercom-adapter-http/src/server.rs`:
  Add picoserve route handlers for `/api/action/reboot` and `/api/action/reset/factory`.

- [x] **Step 5: Run all `intercom-adapter-http` tests**
  Run `mise exec -- cargo test -p intercom-adapter-http --target x86_64-apple-darwin`.

- [x] **Step 6: Commit in `intercom-matter`**
  Commit the router and server changes.

---

### Task 3: `intercom-matter` Frontend Integration & Asset Rebuild

**Files:**
- Modify: `crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`
- Modify: `crates/intercom-adapter-http/frontend/src/App.tsx`
- Modify: `crates/intercom-adapter-http/assets/dashboard.html`

- [x] **Step 1: Expose `reboot` and `factoryReset` in `intercomStore.ts`**
  In `crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`:
  Add `reboot: () => api.reboot()` and `factoryReset: () => api.factoryReset()`.

- [x] **Step 2: Add handlers and pass props in `App.tsx`**
  In `crates/intercom-adapter-http/frontend/src/App.tsx`:
  Add `handleReboot` and `handleFactoryReset` with informative toast notifications. Pass `onReboot={handleReboot}` and `onFactoryReset={handleFactoryReset}` to `<DashboardShell />`.

- [x] **Step 3: Typecheck and lint intercom frontend**
  Run `cargo fe-check` (or `bun run check` in `crates/intercom-adapter-http/frontend`).

- [x] **Step 4: Build single-file production asset**
  Run `cargo fe-build` to update `crates/intercom-adapter-http/assets/dashboard.html`.

- [x] **Step 5: Verify firmware compilation**
  Run `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`.

- [x] **Step 6: Commit frontend and built asset in `intercom-matter`**
  Commit the changes.

---

### Task 4: `hvac-bridge` HTTP Router & Server (`crates/hvac-adapter-http`)

**Files:**
- Modify: `../hvac-bridge/crates/hvac-adapter-http/src/router.rs`
- Modify: `../hvac-bridge/crates/hvac-adapter-http/src/server.rs`
- Modify: `../hvac-bridge/crates/hvac-adapter-http/tests/router_tests.rs`

- [x] **Step 1: Add failing router tests in `hvac-bridge`**
  In `../hvac-bridge/crates/hvac-adapter-http/tests/router_tests.rs`:
  Add tests for `POST /api/action/reboot` &rarr; `Route::ActionReboot` and `POST /api/action/reset/factory` &rarr; `Route::ActionResetFactory`.

- [x] **Step 2: Update `Route` and route parsing in `hvac-bridge/crates/hvac-adapter-http/src/router.rs`**
  Add `ActionReboot` and `ActionResetFactory` to `Route`, and map from `DiagnosticRoute`.

- [x] **Step 3: Handle `Route::ActionReboot` and `Route::ActionResetFactory` in `server.rs`**
  In `../hvac-bridge/crates/hvac-adapter-http/src/server.rs`:
  Add execution handlers for `Route::ActionReboot` and `Route::ActionResetFactory` with `reboot_delay`, credential erasure, and reboot.

- [x] **Step 4: Run unit tests in `hvac-bridge`**
  Run `cargo test -p hvac-adapter-http --target x86_64-apple-darwin` in `hvac-bridge`.

- [x] **Step 5: Commit changes in `hvac-bridge`**
  Commit the backend changes.

---

### Task 5: `hvac-bridge` Frontend Integration & Asset Rebuild

**Files:**
- Modify: `../hvac-bridge/crates/hvac-adapter-http/frontend/src/api.ts`
- Modify: `../hvac-bridge/crates/hvac-adapter-http/frontend/src/App.tsx`
- Modify: `../hvac-bridge/crates/hvac-adapter-http/assets/dashboard.html`

- [x] **Step 1: Expose `reboot` and `factoryReset` in `api.ts` / store in `hvac-bridge`**
  Call dashboard API helpers in `hvac-bridge` frontend.

- [x] **Step 2: Add handlers and pass props in `App.tsx`**
  In `../hvac-bridge/crates/hvac-adapter-http/frontend/src/App.tsx`:
  Add `handleReboot` and `handleFactoryReset` with toast notifications, and pass them to `<DashboardShell />`.

- [x] **Step 3: Run typecheck & build dashboard asset**
  Run `bun run check` and `bun run build` in `../hvac-bridge/crates/hvac-adapter-http/frontend`.

- [x] **Step 4: Run all verification tests**
  Verify host tests across all three repositories.

- [x] **Step 5: Commit changes in `hvac-bridge`**
  Commit frontend and asset changes.
