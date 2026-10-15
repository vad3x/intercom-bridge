# Web UI Dashboard & Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a zero-cost embedded Web UI Dashboard on port 8080 delivering real-time diagnostics (uptime, Wi-Fi RSSI, heap stats, active partition slot, Matter status, in-memory live logs) and interactive access control debugging (emulating rings, triggering unlocks) without compromising the ESP32-C6's strict memory and real-time constraints.

**Architecture:** Split HTTP serving and low-level dual-bank flash logic into separate crates (`intercom-adapter-http` and `intercom-adapter-ota`) adhering strictly to Hexagonal Architecture. The HTTP server reuses the single-connection TCP socket on port 8080, serving a pre-gzipped single-file HTML/CSS/JS frontend embedded in flash, a zero-alloc JSON status endpoint, an in-memory log streaming endpoint, and action routes that drive the core domain via `AccessControllerHandle`.

**Tech Stack:** Rust (`no_std` on target, `std` for host tests), Embassy (`embassy-net`, `embassy-time`, `embassy-sync`), `flate2` (build-time compression), `critical_section` (safe logging buffer), Ed25519 firmware verification.

## Global Constraints
- Target platform: ESP32-C6 (`riscv32imac-unknown-none-elf`), bare-metal `no_std` + `alloc`.
- Host unit tests: Must run on macOS/Linux with `cargo test --target x86_64-apple-darwin` in < 2 seconds.
- Rendering: Zero runtime HTML templating or string concatenation on chip; client-side rendering with compile-time pre-gzipped asset (`build.rs`).
- Concurrency: Strict single-connection TCP model on port 8080 with fixed static buffers (`[u8; 4096]` RX, `[u8; 1024]` TX).
- Logging: In-memory static ring buffer (~6 KiB) with `critical_section` protection, zero heap allocation.

---

### Task 1: In-Memory Live Log Ring Buffer

**Files:**
- Modify: `crates/intercom-firmware/src/utils/uptime_logger.rs`
- Test: `crates/intercom-firmware/src/utils/uptime_logger.rs` (inline test module with `#[cfg(test)]`)

**Interfaces:**
- Produces:
  ```rust
  pub struct LogRingBuffer<const N: usize> { /* ... */ }
  impl<const N: usize> LogRingBuffer<N> {
      pub const fn new() -> Self;
      pub fn write_str(&mut self, s: &str);
      pub fn read_into(&self, buf: &mut [u8]) -> usize;
      pub fn clear(&mut self);
  }
  pub fn get_buffered_logs(buf: &mut [u8]) -> usize;
  ```

- [ ] **Step 1: Write failing unit test for `LogRingBuffer`**

Add unit tests to `crates/intercom-firmware/src/utils/uptime_logger.rs` covering circular overwrite, chronological read-back, and empty buffer behavior:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_ring_buffer_fifo_overwrite() {
        // Arrange
        let mut rb = LogRingBuffer::<16>::new();
        let mut out = [0u8; 32];

        // Act
        rb.write_str("12345678");
        let n1 = rb.read_into(&mut out);
        assert_eq!(&out[..n1], b"12345678");

        rb.write_str("abcdefghij"); // Total 18 chars, overflows 16
        let n2 = rb.read_into(&mut out);

        // Assert: Oldest characters dropped, newest 16 preserved
        assert_eq!(n2, 16);
        assert_eq!(&out[..n2], b"345678abcdefghij");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-firmware --target x86_64-apple-darwin -- test_log_ring_buffer_fifo_overwrite`
Expected: FAIL with compilation error: `cannot find type LogRingBuffer`.

- [ ] **Step 3: Implement `LogRingBuffer` and hook into `UptimeLogger`**

Implement `LogRingBuffer<const N: usize>` in `crates/intercom-firmware/src/utils/uptime_logger.rs`:

```rust
pub struct LogRingBuffer<const N: usize> {
    storage: [u8; N],
    write_pos: usize,
    count: usize,
}

impl<const N: usize> LogRingBuffer<N> {
    pub const fn new() -> Self {
        Self {
            storage: [0u8; N],
            write_pos: 0,
            count: 0,
        }
    }

    pub fn write_str(&mut self, s: &str) {
        for &byte in s.as_bytes() {
            self.storage[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % N;
            if self.count < N {
                self.count += 1;
            }
        }
    }

    pub fn read_into(&self, buf: &mut [u8]) -> usize {
        let to_copy = self.count.min(buf.len());
        let start = if self.count < N {
            0
        } else {
            self.write_pos
        };
        for i in 0..to_copy {
            let idx = (start + i) % N;
            buf[i] = self.storage[idx];
        }
        to_copy
    }

    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.count = 0;
    }
}

pub static LOG_BUFFER: critical_section::Mutex<core::cell::RefCell<LogRingBuffer<6144>>> =
    critical_section::Mutex::new(core::cell::RefCell::new(LogRingBuffer::new()));

pub fn get_buffered_logs(buf: &mut [u8]) -> usize {
    critical_section::with(|cs| LOG_BUFFER.borrow(cs).borrow().read_into(buf))
}
```

In `UptimeLogger::log()`, format the clean message (no ANSI codes) and write it to `LOG_BUFFER`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-firmware --target x86_64-apple-darwin -- test_log_ring_buffer_fifo_overwrite`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-firmware/src/utils/uptime_logger.rs
git commit -m "feat(firmware): add in-memory LogRingBuffer to UptimeLogger"
```

---

### Task 2: Refactor `intercom-adapter-ota` into Pure Library

**Files:**
- Modify: `crates/intercom-adapter-ota/src/lib.rs`
- Modify: `crates/intercom-adapter-ota/Cargo.toml`
- Keep: `crates/intercom-adapter-ota/src/bootloader.rs`
- Keep: `crates/intercom-adapter-ota/src/writer.rs`
- Deprecate/Extract: `crates/intercom-adapter-ota/src/server.rs` (logic moves to `intercom-adapter-http` in Task 5)

**Interfaces:**
- Consumes: Flash storage, ROM SPI flash.
- Produces:
  ```rust
  pub use bootloader::{OtaBootloader, BootloaderError, OtaFlashStorage, RunningSlotStatus};
  pub use writer::{OtaWriter, OtaError, OtaTargetFlash, MAX_APP_SLOT_SIZE, OTA_HEADER_LEN};
  ```

- [ ] **Step 1: Write unit test verifying public exports of `intercom-adapter-ota`**

In `crates/intercom-adapter-ota/tests/library_exports.rs`:

```rust
use intercom_adapter_ota::{OtaBootloader, OtaWriter, MAX_APP_SLOT_SIZE};

#[test]
fn test_ota_library_types_exported() {
    assert_eq!(MAX_APP_SLOT_SIZE, 3473408);
}
```

- [ ] **Step 2: Run test to verify exports**

Run: `cargo test -p intercom-adapter-ota --test library_exports --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 3: Clean up exports in `crates/intercom-adapter-ota/src/lib.rs`**

Ensure `bootloader` and `writer` are public modules, clean up unnecessary server exports from `lib.rs` while keeping backward compatibility for `server.rs` until Task 5 completes:

```rust
pub mod bootloader;
pub mod writer;
#[doc(hidden)]
pub mod server;

pub use bootloader::{mark_running_slot_valid, BootloaderError, OtaBootloader, OtaFlashStorage};
pub use writer::{OtaError, OtaTargetFlash, OtaWriter, MAX_APP_SLOT_SIZE, OTA_HEADER_LEN};
```

- [ ] **Step 4: Run all tests in `intercom-adapter-ota`**

Run: `cargo test -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: PASS (all bootloader, writer, and server tests pass).

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-adapter-ota/
git commit -m "refactor(ota): prepare intercom-adapter-ota as a pure flash library"
```

---

### Task 3: Scaffold `intercom-adapter-http` with Build-Time Gzip Pipeline

**Files:**
- Create: `crates/intercom-adapter-http/Cargo.toml`
- Create: `crates/intercom-adapter-http/build.rs`
- Create: `crates/intercom-adapter-http/src/assets/dashboard.html`
- Create: `crates/intercom-adapter-http/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

**Interfaces:**
- Produces:
  ```rust
  pub const DASHBOARD_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.html.gz"));
  ```

- [ ] **Step 1: Create `crates/intercom-adapter-http/Cargo.toml`**

```toml
[package]
name = "intercom-adapter-http"
version = "0.1.0"
edition = "2021"

[dependencies]
embassy-net = { version = "0.7", default-features = false, features = ["tcp"] }
embassy-time = { version = "0.4", default-features = false }
intercom-core = { path = "../intercom-core" }
intercom-adapter-ota = { path = "../intercom-adapter-ota" }
log = "0.4"
serde_json_core = "0.6"

[build-dependencies]
flate2 = "1.0"

[dev-dependencies]
flate2 = "1.0"
```

Update root `Cargo.toml` workspace members to include `"crates/intercom-adapter-http"`.

- [ ] **Step 2: Create `crates/intercom-adapter-http/build.rs`**

```rust
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use flate2::write::GzEncoder;
use flate2::Compression;

fn main() {
    println!("cargo:rerun-if-changed=src/assets/dashboard.html");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("dashboard.html.gz");

    let mut src = File::open("src/assets/dashboard.html")
        .expect("Failed to open src/assets/dashboard.html");
    let mut content = Vec::new();
    src.read_to_end(&mut content).unwrap();

    let dest = File::create(&dest_path).expect("Failed to create dashboard.html.gz");
    let mut encoder = GzEncoder::new(dest, Compression::best());
    encoder.write_all(&content).unwrap();
    encoder.finish().unwrap();
}
```

- [ ] **Step 3: Create `crates/intercom-adapter-http/src/assets/dashboard.html`**

Create the complete, vanilla HTML5/CSS/JS document with the **Overview** and **System Logs** tabs:
- Header: Intercom Bridge + Live Status Badge (`#status-badge`)
- Tab Switcher: `[Overview]` and `[System Logs]`
- Tab 1 Overview: Action buttons (`#btn-ring`, `#btn-unlock`), Telemetry cards (`#val-uptime`, `#val-rssi`, `#val-slot`, `#val-heap`, `#val-matter`, `#val-version`).
- Tab 2 System Logs: Full-height `<pre id="logs-pre">`, `[📋 Copy Logs]` button, auto-refresh checkbox.
- Script: `fetch('/api/status')` every 3s on Overview, `fetch('/api/logs')` every 3s on Logs, pauses on `document.hidden`, handles button clicks via `fetch('/api/action/...')`.

- [ ] **Step 4: Create `crates/intercom-adapter-http/src/lib.rs` and verify gzip decompression in unit test**

```rust
#![no_std]
#[cfg(test)]
extern crate std;

pub const DASHBOARD_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.html.gz"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use flate2::read::GzDecoder;

    #[test]
    fn test_dashboard_gzip_decompresses_to_valid_html() {
        let mut decoder = GzDecoder::new(DASHBOARD_HTML_GZ);
        let mut html = std::string::String::new();
        decoder.read_to_string(&mut html).expect("Decompression failed");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Intercom Bridge"));
        assert!(html.contains("btn-ring"));
        assert!(DASHBOARD_HTML_GZ.len() < 5120, "Gzip asset exceeds 5 KiB limit");
    }
}
```

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: PASS (decompression test passes, gzipped asset < 5 KiB).

```bash
git add crates/intercom-adapter-http/ Cargo.toml
git commit -m "feat(http): scaffold intercom-adapter-http with build-time gzip asset pipeline"
```

---

### Task 4: Telemetry Port & Zero-Alloc Router Endpoints

**Files:**
- Create: `crates/intercom-adapter-http/src/telemetry.rs`
- Create: `crates/intercom-adapter-http/src/router.rs`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Test: `crates/intercom-adapter-http/tests/router_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub trait SystemTelemetry {
      fn uptime_secs(&self) -> u64;
      fn free_heap_bytes(&self) -> usize;
      fn total_heap_bytes(&self) -> usize;
      fn wifi_rssi(&self) -> Option<i8>;
      fn ip_address(&self) -> Option<[u8; 4]>;
      fn active_slot(&self) -> &'static str;
      fn is_slot_valid(&self) -> bool;
      fn is_matter_commissioned(&self) -> bool;
      fn firmware_version(&self) -> &'static str;
  }
  pub enum Route<'a> {
      Dashboard,
      Status,
      Logs,
      ActionRing,
      ActionUnlock,
      OtaFlash,
      NotFound,
  }
  pub fn parse_route(request_line: &str) -> Route;
  ```

- [ ] **Step 1: Write unit tests for route parsing and status JSON serialization**

In `crates/intercom-adapter-http/tests/router_tests.rs`:

```rust
use intercom_adapter_http::router::{parse_route, Route};

#[test]
fn test_parse_routes() {
    assert_eq!(parse_route("GET / HTTP/1.1"), Route::Dashboard);
    assert_eq!(parse_route("GET /api/status HTTP/1.1"), Route::Status);
    assert_eq!(parse_route("GET /api/logs HTTP/1.1"), Route::Logs);
    assert_eq!(parse_route("POST /api/action/ring HTTP/1.1"), Route::ActionRing);
    assert_eq!(parse_route("POST /api/action/unlock HTTP/1.1"), Route::ActionUnlock);
    assert_eq!(parse_route("POST /api/v1/ota/flash HTTP/1.1"), Route::OtaFlash);
    assert_eq!(parse_route("GET /random HTTP/1.1"), Route::NotFound);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
Expected: FAIL (modules not found).

- [ ] **Step 3: Implement `telemetry.rs` and `router.rs`**

Define `SystemTelemetry` trait in `crates/intercom-adapter-http/src/telemetry.rs`.
Implement `parse_route` and formatting functions in `crates/intercom-adapter-http/src/router.rs`:
- Status JSON formatter: uses `core::fmt::write` to format into fixed TX buffer without dynamic allocation.
- Action handlers: invoke `access_handle.send(AccessCommand::Ring)` and `access_handle.send(AccessCommand::Unlock)`.
- 404 handler: writes standard HTTP 404 response.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-adapter-http/
git commit -m "feat(http): add SystemTelemetry trait and zero-alloc HTTP request router"
```

---

### Task 5: Implement Async `HttpServer` with Unified OTA Handling

**Files:**
- Create: `crates/intercom-adapter-http/src/server.rs`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Test: `crates/intercom-adapter-http/tests/server_tests.rs`

**Interfaces:**
- Consumes:
  - `embassy_net::Stack<'static>`
  - `AccessControllerHandle`
  - `SystemStateSender<'a>`
  - `&'a impl SystemTelemetry`
  - Ed25519 Public Key `[u8; 32]`
- Produces:
  ```rust
  pub struct HttpServer<'a, T: SystemTelemetry> { /* ... */ }
  impl<'a, T: SystemTelemetry> HttpServer<'a, T> {
      pub fn new(...) -> Self;
      pub async fn run(&self) -> !;
  }
  ```

- [ ] **Step 1: Write integration test with mock stream for all endpoints**

In `crates/intercom-adapter-http/tests/server_tests.rs`:
- Mock stream providing `GET /` -> verifies response header contains `Content-Encoding: gzip` and payload matches `DASHBOARD_HTML_GZ`.
- Mock stream providing `GET /api/status` -> verifies JSON parses into expected fields.
- Mock stream providing `POST /api/action/ring` -> verifies `AccessControllerHandle` receives `Ring`.
- Mock stream providing invalid OTA header -> verifies 400 Bad Request.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
Expected: FAIL (`HttpServer` not implemented).

- [ ] **Step 3: Implement `HttpServer` in `crates/intercom-adapter-http/src/server.rs`**

Port the robust socket accept loop from `intercom-adapter-ota/src/server.rs` into `intercom-adapter-http/src/server.rs`:
- Reuses fixed `[u8; 4096]` RX buffer and `[u8; 1024]` TX buffer.
- Dispatches parsed route:
  - `Route::Dashboard`: Streams `DASHBOARD_HTML_GZ` directly with `Content-Encoding: gzip`.
  - `Route::Status`: Formats telemetry JSON into TX buffer and writes out.
  - `Route::Logs`: Calls `get_buffered_logs()` and writes out as `text/plain`.
  - `Route::ActionRing`: Calls `access_handle.send(AccessCommand::Ring)` and returns `{"status":"ok"}`.
  - `Route::ActionUnlock`: Calls `access_handle.send(AccessCommand::Unlock)` and returns `{"status":"ok"}`.
  - `Route::OtaFlash`: Calls `intercom_adapter_ota::writer::OtaWriter` and updates `OtaBootloader`.
- Enforces single-connection concurrency and graceful connection closing.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-adapter-http/
git commit -m "feat(http): implement async HttpServer supporting dashboard, status, logs, and OTA"
```

---

### Task 6: Firmware Composition Root Wiring

**Files:**
- Modify: `crates/intercom-firmware/Cargo.toml`
- Modify: `crates/intercom-firmware/src/main.rs`
- Delete: Obsolete `server.rs` in `intercom-adapter-ota`

**Interfaces:**
- Connects `HttpServer` with `AccessControllerHandle`, `UptimeLogger::get_buffered_logs`, and concrete `SystemTelemetry` implementation.

- [ ] **Step 1: Add `intercom-adapter-http` dependency to `crates/intercom-firmware/Cargo.toml`**

Add `intercom-adapter-http = { path = "../intercom-adapter-http" }`.

- [ ] **Step 2: Implement `FirmwareTelemetry` in `crates/intercom-firmware/src/main.rs`**

```rust
struct FirmwareTelemetry;

impl SystemTelemetry for FirmwareTelemetry {
    fn uptime_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }
    fn free_heap_bytes(&self) -> usize {
        esp_alloc::HEAP.free()
    }
    fn total_heap_bytes(&self) -> usize {
        HEAP_SIZE
    }
    fn wifi_rssi(&self) -> Option<i8> {
        // Query from active Wi-Fi controller
        None
    }
    fn ip_address(&self) -> Option<[u8; 4]> {
        // Query from global Embassy net stack
        None
    }
    fn active_slot(&self) -> &'static str {
        match intercom_adapter_ota::OtaBootloader::load() {
            Ok(b) => match b.running_slot() {
                intercom_adapter_ota::bootloader::Slot::Slot0 => "ota_0",
                intercom_adapter_ota::bootloader::Slot::Slot1 => "ota_1",
            },
            Err(_) => "unknown",
        }
    }
    fn is_slot_valid(&self) -> bool {
        true
    }
    fn is_matter_commissioned(&self) -> bool {
        // Query from MatterController or store
        true
    }
    fn firmware_version(&self) -> &'static str {
        version::SW_VER_STR
    }
}
```

- [ ] **Step 3: Replace `OtaServer` with `HttpServer` in `main.rs`**

Replace:
```rust
let ota_server = intercom_adapter_ota::OtaServer::new(...);
ota_server.run().await;
```
With:
```rust
let http_server = intercom_adapter_http::HttpServer::new(
    stack,
    state_sender,
    access_handle,
    FirmwareTelemetry,
    config::OTA_PUBLIC_KEY,
);
http_server.run().await;
```

- [ ] **Step 4: Verify embedded release compilation on RISC-V target**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Successful compilation, binary fits cleanly in `ota_0`.

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-firmware/
git commit -m "feat(firmware): wire HttpServer into main composition root"
```

---

### Task 7: Full Workspace Verification & OTA Compatibility Check

**Files:**
- Test: All workspace crates

- [ ] **Step 1: Run host unit tests across all workspace crates**

Run: `cargo test -p intercom-core -p intercom-adapter-ota -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: All tests PASS.

- [ ] **Step 2: Run release firmware compilation**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Build succeeds with 0 errors.

- [ ] **Step 3: Verify OTA Python script compatibility**

Verify `python3 scripts/ota.py --help` runs and that wire protocol paths (`/api/v1/ota/flash`) remain identical.

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -m "chore: complete verification of web UI dashboard and diagnostics integration"
```
