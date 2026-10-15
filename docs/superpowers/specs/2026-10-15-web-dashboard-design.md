# Web UI Dashboard & Diagnostics Design Specification

- **Date:** 2026-10-15
- **Status:** Approved
- **Target:** ESP32-C6 (Bare-metal `no_std`, RISC-V, Embassy async runtime)
- **Port:** TCP 8080 (Unified single-connection listener)

---

## 1. Executive Summary

This specification defines the architecture, protocol, and interface for an embedded **Web UI Dashboard** on the Intercom Bridge. The dashboard enables zero-tooling diagnostics (uptime, Wi-Fi RSSI, heap memory, active partition slot, Matter status, live system logs) and interactive access control debugging (emulating rings, triggering unlocks) directly from any web browser on the local network.

To preserve strict hardware constraints and guarantee zero risk of memory exhaustion or real-time jitter:
1. **Zero Dynamic HTML Rendering on Chip**: The ESP32-C6 serves a pre-compressed, single-file static asset (`dashboard.html.gz`) embedded into flash at compile time. 100% of DOM rendering is performed client-side by the browser.
2. **Unified Single-Connection HTTP Listener**: Reuses the existing port 8080 TCP listener and static buffers (`[u8; 4096]` RX, `[u8; 1024]` TX), incurring **zero additional socket RAM allocations**.
3. **Hexagonal Architecture (Ports & Adapters)**: Core domain logic remains completely decoupled. Inbound actions drive the domain through [`AccessControllerHandle`](file:///Users/vadim/personal/projects/intercom-matter/crates/intercom-core/src/ports/inbound.rs), while outbound telemetry flows through an abstract trait.
4. **In-Memory Live Log Ring Buffer**: An in-memory circular ring buffer (~4–6 KiB) captures recent system logs for inspection in the dashboard console without UART cables.

---

## 2. System Architecture & Crate Boundaries

The codebase is structured into clear hexagonal boundaries:

```
┌────────────────────────────────────────────────────────────────────────┐
│                          intercom-firmware                             │
│                  (Composition Root & HAL Wiring)                       │
└───────────┬──────────────────────┬──────────────────────┬──────────────┘
            │                      │                      │
            ▼                      ▼                      ▼
┌───────────────────────┐┌───────────────────┐┌──────────────────────────┐
│ intercom-adapter-http ││intercom-adapter-ota││  intercom-adapter-gpio   │
│   (HTTP Edge Adapter) ││(Flash/Crypto Lib) ││(Physical Hardware Relay) │
└───────────┬───────────┘└─────────┬─────────┘└───────────┬──────────────┘
            │                      │                      │
            │                      │                      │
            ▼                      ▼                      ▼
┌────────────────────────────────────────────────────────────────────────┐
│                             intercom-core                              │
│                      (Pure Domain Logic & Ports)                       │
│                                                                        │
│   • AccessControllerHandle (Inbound: Unlock, Ring)                     │
│   • SystemStateWatch       (Lifecycle State Observation)               │
└────────────────────────────────────────────────────────────────────────┘
```

### Crate Separation:
1. **`crates/intercom-adapter-ota` (Library)**:
   * Retains dual-bank `otadata` state machine, A/B partition logic, rollback protection, streaming sector writer, and Ed25519 signature verification.
   * Completely decoupled from network transport (no direct HTTP server or socket loop).
2. **`crates/intercom-adapter-http` (New Edge Adapter)**:
   * Binds the TCP socket on port 8080.
   * Routes incoming HTTP requests (`/`, `/api/status`, `/api/logs`, `/api/action/*`, `/api/v1/ota/flash`).
   * Imports and drives `intercom-adapter-ota` for the `/api/v1/ota/flash` route.
   * Drives core domain via `AccessControllerHandle` and reads telemetry via `SystemTelemetry` port.
3. **`crates/intercom-core`**:
   * Pure domain logic unchanged. Holds inbound driving ports and event lifecycle channels.

---

## 3. Network Protocol & REST Endpoints (Port 8080)

All routes are served over HTTP/1.1 on TCP port 8080:

| Route | Method | Content-Type | Encoding | Description |
|---|---|---|---|---|
| `/` | `GET` | `text/html; charset=utf-8` | `gzip` | Serves embedded dashboard UI |
| `/api/status` | `GET` | `application/json` | identity | Returns live system telemetry |
| `/api/logs` | `GET` | `text/plain; charset=utf-8`| identity | Dumps recent ring buffer logs |
| `/api/action/ring` | `POST` | `application/json` | identity | Dispatches `AccessCommand::Ring` |
| `/api/action/unlock`| `POST` | `application/json` | identity | Dispatches `AccessCommand::Unlock` |
| `/api/v1/ota/flash` | `POST` | `application/json` | identity | Streams firmware update into OTA writer |
| *any other* | `*` | `application/json` | identity | Returns `404 Not Found` |

### Security Model:
* **Unauthenticated LAN-Only**: Read and action routes require no authentication for frictionless local development and testing.
* **OTA Integrity**: Firmware flashing (`/api/v1/ota/flash`) continues to require offline cryptographic Ed25519 signatures verified against `config::OTA_PUBLIC_KEY`.

---

## 4. Telemetry Port & Data Contracts

### Telemetry Outbound Port (`intercom-adapter-http`)

```rust
pub trait SystemTelemetry {
    fn uptime_secs(&self) -> u64;
    fn free_heap_bytes(&self) -> usize;
    fn total_heap_bytes(&self) -> usize;
    fn wifi_rssi(&self) -> Option<i8>;
    fn ip_address(&self) -> Option<[u8; 4]>;
    fn mac_address(&self) -> [u8; 6];
    fn active_slot(&self) -> &'static str;
    fn is_slot_valid(&self) -> bool;
    fn is_matter_commissioned(&self) -> bool;
    fn firmware_version(&self) -> &'static str;
    fn git_commit(&self) -> &'static str;
    fn build_date(&self) -> &'static str;
}
```

### JSON Schema (`GET /api/status`):

```json
{
  "state": "Ready",
  "door_locked": true,
  "uptime_sec": 14250,
  "wifi_rssi_dbm": -58,
  "ip": "192.168.1.142",
  "mac": "68:B6:B3:21:40:8C",
  "active_slot": "ota_0",
  "slot_valid": true,
  "free_heap_bytes": 151552,
  "total_heap_bytes": 215040,
  "matter_commissioned": true,
  "version": "0.1.0",
  "git_commit": "a3f89b",
  "build_date": "2026-10-15"
}
```

---

## 5. In-Memory Live Log Ring Buffer

To capture real-time diagnostic output without allocating on the heap:

1. **Storage Structure**:
   * A fixed static ring buffer `LogRingBuffer<const N: usize>` (size: 6144 bytes) defined in `crates/intercom-firmware/src/utils/uptime_logger.rs`.
   * Protected by `critical_section` for concurrency-safe writing from any task or interrupt.
2. **Log Ingestion**:
   * On every call to `log::Log::log()`, the formatted line (timestamp + level + target + message, stripped of ANSI escape sequences) is appended to the ring buffer.
   * When full, old bytes are overwritten (FIFO ring semantics).
3. **Log Retrieval (`GET /api/logs`)**:
   * The HTTP server reads the contiguous slices from the ring buffer and writes them out directly to the client socket as plain text.

---

## 6. Build-Time Static Asset Pipeline

To ensure the developer experience is seamless:
1. **Source File**: `crates/intercom-adapter-http/src/assets/dashboard.html`
   * Plain, human-readable HTML5 with inline CSS and vanilla JavaScript.
   * Can be previewed and tested standalone in any browser.
2. **Compile-Time Compression (`build.rs`)**:
   * `build.rs` uses `flate2` to gzip `dashboard.html` into `$OUT_DIR/dashboard.html.gz`.
   * Triggers automatic recompilation via `println!("cargo:rerun-if-changed=src/assets/dashboard.html")`.
3. **Embedded Binary**:
   * `const DASHBOARD_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.html.gz"));`
   * Flash footprint: ~3.0 KiB compressed.
   * Zero runtime compression overhead on the ESP32.

---

## 7. Frontend User Interface Design (Option A: Modern Card Grid)

The web dashboard is delivered as a single self-contained document:

```
┌────────────────────────────────────────────────────────┐
│  ⚡ Intercom Bridge                          ● Ready    │
├────────────────────────────────────────────────────────┤
│  [ 🔔 Emulate Ring ]          [ 🔓 Trigger Unlock ]    │
├──────────────────────────┬─────────────────────────────┤
│  SYSTEM UPTIME           │  WI-FI SIGNAL               │
│  03:57:30                │  -58 dBm (Strong)           │
├──────────────────────────┼─────────────────────────────┤
│  ACTIVE PARTITION        │  HEAP MEMORY                │
│  ota_0 (Valid)           │  148 KB / 210 KB (70% Free) │
├──────────────────────────┼─────────────────────────────┤
│  MATTER STATUS           │  FIRMWARE VERSION           │
│  Commissioned            │  v0.1.0 (git: a3f89b)       │
├──────────────────────────┴─────────────────────────────┤
│  RECENT SYSTEM LOGS               [ Auto-refresh: ☑ ]  │
│  ┌──────────────────────────────────────────────────┐  │
│  │ 00:00:01.020 INFO  intercom_matter - Wi-Fi up    │  │
│  │ 00:00:03.142 INFO  intercom_core - System Ready  │  │
│  │ 03:57:12.809 INFO  intercom_core - Ring Disptch  │  │
│  └──────────────────────────────────────────────────┘  │
│  [ 📋 Copy Logs ]                                      │
└────────────────────────────────────────────────────────┘
```

### UI Behavior:
* **Background Awareness**: Telemetry and logs poll every 3 seconds, but pause completely when the tab is hidden (`document.visibilityState === 'hidden'`).
* **Debounced Actions**: Clicking an action button triggers an immediate visual loading state, dispatches the POST request, displays a toast alert, and disables the button for 2 seconds to prevent spamming.
* **Offline Indication**: If the ESP32 reboots or Wi-Fi drops, a persistent connection warning badge appears until connectivity is restored.

---

## 8. Verification & Test Strategy

### 1. Host Unit Tests (`cargo test -p intercom-adapter-http --target x86_64-apple-darwin`)
* In-memory mock TCP stream tests verifying:
  * `GET /` serves gzip-encoded HTML with correct Content-Type and Content-Length.
  * `GET /api/status` returns valid JSON matching mock telemetry values.
  * `GET /api/logs` produces expected ring buffer plain-text output.
  * `POST /api/action/ring` triggers `AccessCommand::Ring`.
  * `POST /api/action/unlock` triggers `AccessCommand::Unlock`.
  * `POST /api/v1/ota/flash` routes payload to `OtaWriter`.
  * Unknown routes return `404 Not Found`.

### 2. Embedded Target Build (`mise exec -- cargo build -p intercom-firmware --release`)
* Verifies zero linker errors on `riscv32imac-unknown-none-elf`.
* Validates static RAM consumption and binary size fit cleanly within the 3.4 MB app partitions.

### 3. End-to-End OTA Compatibility
* Executes `python3 scripts/ota.py --target <IP>` to ensure wire-protocol backwards compatibility with the existing Python updater.
