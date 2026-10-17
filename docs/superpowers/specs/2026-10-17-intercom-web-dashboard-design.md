# Intercom Bridge Web Dashboard Design Specification

## Overview

This specification establishes the frontend architecture for the **Intercom Bridge Web Dashboard**, adopting 1:1 the architecture and developer workflow established in the `hvac-bridge` project.

The dashboard is built as an ultra-lightweight, zero-virtual-DOM single-page application using **Solid.js**, **DaisyUI**, **Tailwind CSS**, and **Lucide Solid** icons. It compiles via **Bun** and **Vite** (with `vite-plugin-singlefile`) into a single self-contained `assets/dashboard.html` (< 15–20 KB gzipped). The ESP32-C6 firmware serves the pre-gzipped HTML directly from flash memory with zero runtime memory allocation.

---

## 1. Directory Structure

The frontend package lives inside `crates/intercom-adapter-http/frontend/` and compiles into `crates/intercom-adapter-http/assets/dashboard.html`:

```
crates/intercom-adapter-http/
├── assets/
│   └── dashboard.html               # Compiled, self-contained single-file HTML bundle
├── frontend/
│   ├── .mise.toml                   # Local toolchain (bun)
│   ├── biome.json                   # Linter and formatter configuration
│   ├── package.json                 # Bun package definitions, dependencies & scripts
│   ├── postcss.config.js            # PostCSS configuration for Tailwind CSS & Autoprefixer
│   ├── README.md                    # Developer documentation
│   ├── tsconfig.json                # TypeScript compiler configuration
│   ├── vite.config.ts               # Vite bundler config with singlefile plugin & dev proxy
│   └── src/
│       ├── components/
│       │   ├── ui/
│       │   │   ├── Badge.tsx        # DaisyUI badge wrapper
│       │   │   └── Card.tsx         # DaisyUI card wrapper (Card, CardBody, CardTitle)
│       │   ├── DiagnosticsTab.tsx   # System, Network, Memory, Flash partition telemetry
│       │   ├── EcosystemTab.tsx     # Matter fabric status & cluster subscription readiness
│       │   ├── IntercomControls.tsx # Door lock card, unlock action, ring test action
│       │   ├── LogsTab.tsx          # Real-time serial logs viewer with filter & clipboard copy
│       │   ├── Navbar.tsx           # Header with state badge, RSSI & IP pills, theme toggle
│       │   └── Toast.tsx            # Floating action notification popups
│       ├── store/
│       │   └── intercomStore.ts     # Fine-grained reactive state store & polling loop
│       ├── api.ts                   # Fetch client for /api/status, /api/logs, /api/action/*
│       ├── App.tsx                  # Root application with tab switching & store provider
│       ├── index.css                # Tailwind CSS + DaisyUI theme configuration
│       ├── main.tsx                 # Solid.js mount entry point
│       └── types.ts                 # TypeScript types mirroring Rust SystemDiagnostics & API
├── src/
│   ├── config.rs                    # HTTP timeouts and buffer configs
│   ├── lib.rs                       # Crate root, embeds dashboard.html.gz
│   ├── router.rs                    # Route parsing and zero-alloc JSON serialization
│   └── server.rs                    # Async HTTP server (picoserve)
├── build.rs                         # Gzips assets/dashboard.html into $OUT_DIR/dashboard.html.gz
└── Cargo.toml
```

---

## 2. Tech Stack & Dependencies

| Tool / Library | Version | Purpose |
|----------------|---------|---------|
| **Bun** | latest | Fast JS/TS runtime & package manager |
| **Solid.js** | `^1.9.4` | Fine-grained reactive UI framework (zero Virtual DOM overhead) |
| **DaisyUI** | `^4.12.23` | Semantic UI component classes for Tailwind CSS |
| **Tailwind CSS** | `^3.4.17` | Utility-first CSS framework |
| **Lucide Solid** | `^1.16.0` | Feather/Lucide icons ported to Solid.js |
| **Vite** | `^6.0.11` | Build tool and fast HMR dev server |
| **vite-plugin-singlefile** | `^2.1.0` | Inlines all scripts, styles, and assets into one `index.html` |
| **@biomejs/biome** | `^1.9.4` | Formatting and linting |

---

## 3. Component Architecture & User Interface

### 3.1 App Header (`Navbar.tsx`)
* **Title & Icon**: "Intercom Bridge" with ShieldCheck icon.
* **System State Badge**: Color-coded based on `diag.state`:
  * `Operational`: Green badge (`badge-success`)
  * `Commissioning` / `WifiConnecting`: Blue badge (`badge-info`)
  * `Updating`: Orange blinking strobe badge (`badge-warning animate-pulse`)
  * `Error` / `FactoryReset`: Red badge (`badge-error`)
* **Quick Telemetry**: Wi-Fi RSSI badge (with signal icon and dBm value) and current IP address.
* **Theme Controller**: DaisyUI theme switcher supporting `dark`, `night`, `light`, `corporate`, `synthwave`, etc., persisted in `localStorage`.

### 3.2 Controls Tab (`IntercomControls.tsx`)
Replaces the HVAC-specific controls with Intercom access controls:
* **Door Strike / Lock Hero Card**:
  * Visual status indicator:
    * **Locked**: Green shield/padlock icon, label "Door Locked".
    * **Unlocked**: Amber open-padlock icon, label "Door Unlocked".
    * **Unlocking**: Spinner icon, label "Unlocking...".
  * **Unlock Button**: Primary action button ("Unlock Door", `btn-primary`). On click, triggers `POST /api/action/unlock` via `intercomStore.ts`. Provides instant optimistic UI feedback while command executes.
  * **Auto-Relock Notice**: Displays active auto-relock status reminder.
* **Doorbell Ring Card**:
  * Visual status: Indicator displaying bell state (animates when ring is detected).
  * **Test Ring Button**: Secondary action button ("Test Ring", `btn-secondary`). Triggers `POST /api/action/ring` to simulate a chime and verify HomeKit/Matter announcements.

### 3.3 Ecosystem Tab (`EcosystemTab.tsx`)
Direct mirror of `hvac-bridge` ecosystem view adapted for Intercom endpoints:
* **Fabric Commissioning**: Displays active Matter fabric status (`Paired` vs `Unpaired`).
* **Cluster Readiness**:
  * 🔒 `Door Lock Cluster (EP1)`: Subscribed / Waiting / Ready badge (checks `properties.door_lock_subscribed`).
  * 🔔 `Occupancy / Ring Sensor (EP2)`: Subscribed / Waiting / Ready badge (checks `properties.ring_sensor_subscribed`).
* **Ecosystem List**: Displays active Matter over Wi-Fi integration status.

### 3.4 Diagnostics Tab (`DiagnosticsTab.tsx`)
Direct mirror of `hvac-bridge` diagnostics view:
* **Network & System**: System state, formatted uptime (`Xd Xh Xm Xs`), IP address, MAC address, Wi-Fi SSID, Wi-Fi RSSI (dBm with signal bar).
* **Memory & Flash**: Free heap vs Total heap with visual progress bar and exact KB counts, active OTA partition (`ota_0` or `ota_1`), bootloader slot validity flag (`Valid` / `Pending`).
* **Firmware Details**: Software version string, Git commit hash, compilation build date.

### 3.5 Logs Tab (`LogsTab.tsx`)
Direct mirror of `hvac-bridge` live logs view:
* **Terminal View**: Monospace dark viewer fetching `/api/logs` every 2.5s.
* **Toolbar**:
  * Live search input to filter log lines in real time.
  * Pause / Resume auto-scrolling toggle button.
  * "Copy Logs" button copying text to system clipboard.
  * "Clear" button to clear current display.

### 3.6 Toast Notifications (`Toast.tsx`)
* Floating alert popups in the bottom-right corner for success/failure feedback when calling actions (`Unlock sent`, `Test ring triggered`, or network error).

---

## 4. State Management & API Client

### 4.1 API Client (`src/api.ts`)
* `fetchStatus()`: Calls `GET /api/status`, returns `Promise<SystemDiagnostics>`.
* `fetchLogs()`: Calls `GET /api/logs`, returns `Promise<string>`.
* `triggerUnlock()`: Calls `POST /api/action/unlock`, returns `Promise<void>`.
* `triggerRing()`: Calls `POST /api/action/ring`, returns `Promise<void>`.

### 4.2 Reactive Store (`src/store/intercomStore.ts`)
* Implements `createSignal` and `createMemo` for status and optimistic intents.
* Optimistic intent for door unlock: when user clicks "Unlock Door", status immediately reflects unlocking state with a grace period timer before reconciling with physical `/api/status` polling.
* Auto-polling: 2.5s polling loop, paused when document is hidden (`document.visibilityState === 'hidden'`).

---

## 5. Firmware Build & Packaging Workflow

1. **Frontend Compilation**:
   ```bash
   cd crates/intercom-adapter-http/frontend
   bun install
   bun run build
   ```
   Compiles into `crates/intercom-adapter-http/assets/dashboard.html`.

2. **Cargo Build & Gzip Compression**:
   Update `crates/intercom-adapter-http/build.rs` to track `assets/dashboard.html` instead of `src/assets/dashboard.html`:
   ```rust
   println!("cargo:rerun-if-changed=assets/dashboard.html");
   let html_bytes = fs::read("assets/dashboard.html").expect("Failed to read assets/dashboard.html");
   // gzips into $OUT_DIR/dashboard.html.gz
   ```
3. **Picoserve Route**:
   `crates/intercom-adapter-http/src/server.rs` continues to serve `DASHBOARD_HTML_GZ` with `Content-Encoding: gzip` on `GET /`.
4. **Git Tracking**:
   `crates/intercom-adapter-http/assets/dashboard.html` is tracked in git so that pure Rust builds (`cargo build -p intercom-firmware`) succeed without requiring Bun.
