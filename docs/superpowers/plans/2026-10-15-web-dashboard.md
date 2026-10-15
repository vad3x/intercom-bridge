# Web UI Dashboard & Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a zero-cost embedded Web UI Dashboard on port 8080 delivering real-time diagnostics (uptime, Wi-Fi RSSI, heap stats, active partition slot, Matter status, in-memory live logs) and interactive access control debugging (emulating rings, triggering unlocks) without compromising the ESP32-C6's strict memory and real-time constraints.

**Architecture:** Strictly follow Hexagonal Architecture: **all business and telemetry aggregation logic belongs in `intercom-core`** (`SystemDiagnostics` domain model, `HardwareInfoDriver` outbound port, and `DiagnosticsService` application service). The adapters (`intercom-adapter-http` and `intercom-adapter-ota`) have **zero dependency on each other** and contain zero business logic. `intercom-adapter-http` is a pure transport adapter that serves the pre-gzipped static asset from flash, serializes `SystemDiagnostics` to JSON, streams logs from `UptimeLogger`, and dispatches commands via `AccessControllerHandle` and `OtaFlashDriver`.

**Tech Stack:** Rust (`no_std` on target, `std` for host tests), Embassy (`embassy-net`, `embassy-time`, `embassy-sync`), `flate2` (build-time compression), `critical_section` (safe logging buffer), Ed25519 firmware verification.

## Global Constraints
- Target platform: ESP32-C6 (`riscv32imac-unknown-none-elf`), bare-metal `no_std` + `alloc`.
- Domain Purity: All business logic, state aggregation, and telemetry snapshot assembly resides exclusively in `intercom-core`. `intercom-adapter-http` must NOT contain telemetry building logic.
- Hexagonal Isolation: Adapters never depend on other adapters. `intercom-adapter-http` depends ONLY on `intercom-core`.
- Host unit tests: Must run on macOS/Linux with `cargo test --target x86_64-apple-darwin` in < 2 seconds.
- Rendering: Zero runtime HTML templating or string concatenation on chip; client-side rendering with compile-time pre-gzipped asset (`build.rs`).
- Concurrency: Strict single-connection TCP model on port 8080 with fixed static buffers (`[u8; 4096]` RX, `[u8; 1024]` TX).
- Logging: In-memory static ring buffer (~6 KiB) with `critical_section` protection, zero heap allocation.

---

### Task 1: In-Memory Live Log Ring Buffer in `intercom-firmware`

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

- [x] **Step 1: Write failing unit test for `LogRingBuffer`**

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

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-firmware --target x86_64-apple-darwin -- test_log_ring_buffer_fifo_overwrite`
Expected: FAIL with compilation error: `cannot find type LogRingBuffer`.

- [x] **Step 3: Implement `LogRingBuffer` and hook into `UptimeLogger`**

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

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-firmware --target x86_64-apple-darwin -- test_log_ring_buffer_fifo_overwrite`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/intercom-firmware/src/utils/uptime_logger.rs
git commit -m "feat(firmware): add in-memory LogRingBuffer to UptimeLogger"
```

---

### Task 2: Core Diagnostics Service, Domain Model & Ports in `intercom-core`

**Files:**
- Create: `crates/intercom-core/src/domain/diagnostics.rs`
- Create: `crates/intercom-core/src/ports/diagnostics.rs`
- Create: `crates/intercom-core/src/ports/ota.rs`
- Create: `crates/intercom-core/src/service/diagnostics.rs`
- Modify: `crates/intercom-core/src/domain/mod.rs`
- Modify: `crates/intercom-core/src/ports/mod.rs`
- Modify: `crates/intercom-core/src/service/mod.rs`
- Modify: `crates/intercom-core/src/lib.rs`
- Test: `crates/intercom-core/tests/diagnostics_service_tests.rs`

**Interfaces:**
- In `intercom_core::domain::diagnostics`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct SystemDiagnostics {
      pub state: crate::events::SystemState,
      pub door_locked: bool,
      pub uptime_sec: u64,
      pub wifi_rssi_dbm: Option<i8>,
      pub ip: Option<[u8; 4]>,
      pub mac: [u8; 6],
      pub active_slot: &'static str,
      pub slot_valid: bool,
      pub free_heap_bytes: usize,
      pub total_heap_bytes: usize,
      pub matter_commissioned: bool,
      pub version: &'static str,
      pub git_commit: &'static str,
      pub build_date: &'static str,
  }
  ```
- In `intercom_core::ports::diagnostics`:
  ```rust
  pub trait HardwareInfoDriver {
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
- In `intercom_core::ports::ota`:
  ```rust
  pub trait OtaStream {
      async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;
      async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()>;
      async fn flush(&mut self) -> Result<(), ()>;
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum OtaDriverError {
      InvalidHeader,
      VerificationFailed,
      FlashWriteFailed,
      TruncatedStream,
      BootloaderCommitFailed,
  }

  pub trait OtaFlashDriver {
      async fn handle_flash_stream(
          &mut self,
          stream: &mut (dyn OtaStream + Unpin),
          content_length: usize,
      ) -> Result<(), OtaDriverError>;
  }
  ```
- In `intercom_core::service::diagnostics`:
  ```rust
  pub struct DiagnosticsService<'a, H: HardwareInfoDriver> {
      hardware: &'a H,
      state_watch: crate::events::SystemStateWatch,
  }

  impl<'a, H: HardwareInfoDriver> DiagnosticsService<'a, H> {
      pub fn new(hardware: &'a H, state_watch: crate::events::SystemStateWatch) -> Self;
      pub fn snapshot(&self) -> SystemDiagnostics;
  }
  ```

- [x] **Step 1: Write failing unit test for `DiagnosticsService`**

In `crates/intercom-core/tests/diagnostics_service_tests.rs`:
- Mock `HardwareInfoDriver` returning known constants.
- Construct `DiagnosticsService` and assert `snapshot()` correctly aggregates domain state (`SystemState::Ready`, `door_locked: true`) and hardware info.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-core --test diagnostics_service_tests --target x86_64-apple-darwin`
Expected: FAIL (modules not found).

- [x] **Step 3: Implement domain, ports, and `DiagnosticsService` in `intercom-core`**

Add domain model, ports, and service logic. Expose them from `intercom_core`.

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-core --target x86_64-apple-darwin`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/intercom-core/
git commit -m "feat(core): add SystemDiagnostics domain model, HardwareInfoDriver, OtaFlashDriver, and DiagnosticsService"
```

---

### Task 3: Implement `OtaFlashDriver` in `intercom-adapter-ota`

**Files:**
- Create: `crates/intercom-adapter-ota/src/driver.rs`
- Modify: `crates/intercom-adapter-ota/src/lib.rs`
- Test: `crates/intercom-adapter-ota/tests/driver_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct FlashOtaDriver {
      public_key: [u8; 32],
  }
  impl FlashOtaDriver {
      pub fn new(public_key: [u8; 32]) -> Self;
  }
  impl intercom_core::ports::ota::OtaFlashDriver for FlashOtaDriver { /* ... */ }
  ```

- [x] **Step 1: Write unit test with mock `OtaStream` for `FlashOtaDriver`**

In `crates/intercom-adapter-ota/tests/driver_tests.rs`:
- Test that `FlashOtaDriver` validates the stream and returns appropriate `OtaDriverError` on bad signature or payload.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-adapter-ota --test driver_tests --target x86_64-apple-darwin`
Expected: FAIL

- [x] **Step 3: Implement `FlashOtaDriver`**

Implement `FlashOtaDriver` in `crates/intercom-adapter-ota/src/driver.rs`, streaming bytes from `OtaStream` into `OtaWriter` and committing state to `OtaBootloader`.

- [x] **Step 4: Run unit tests to verify**

Run: `cargo test -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/intercom-adapter-ota/
git commit -m "feat(ota): implement OtaFlashDriver port in intercom-adapter-ota"
```

---

### Task 4: Scaffold `intercom-adapter-http` with Build-Time Gzip Pipeline

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

- [x] **Step 1: Create `crates/intercom-adapter-http/Cargo.toml` (Zero dependency on `intercom-adapter-ota`)**

```toml
[package]
name = "intercom-adapter-http"
version = "0.1.0"
edition = "2021"

[dependencies]
embassy-net = { version = "0.7", default-features = false, features = ["tcp"] }
embassy-time = { version = "0.4", default-features = false }
intercom-core = { path = "../intercom-core" }
log = "0.4"
serde_json_core = "0.6"

[build-dependencies]
flate2 = "1.0"

[dev-dependencies]
flate2 = "1.0"
```

Add `"crates/intercom-adapter-http"` to root workspace `Cargo.toml`.

- [x] **Step 2: Create `crates/intercom-adapter-http/build.rs`**

Uses `flate2` to gzip `src/assets/dashboard.html` into `$OUT_DIR/dashboard.html.gz` on `cargo build`.

- [x] **Step 3: Create `crates/intercom-adapter-http/src/assets/dashboard.html`**

Vanilla HTML5/CSS/JS document with **Overview** and **System Logs** tabs.
Polls `/api/status` when Overview tab is active; polls `/api/logs` when System Logs tab is active.

- [x] **Step 4: Create `crates/intercom-adapter-http/src/lib.rs` and verify gzip test**

Write decompression test ensuring gzipped asset < 5 KiB and contains required elements.

- [x] **Step 5: Run tests and commit**

Run: `cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: PASS

```bash
git add crates/intercom-adapter-http/ Cargo.toml
git commit -m "feat(http): scaffold intercom-adapter-http with build-time gzip asset pipeline"
```

---

### Task 5: Zero-Alloc Router & Serialization (Pure Transport, No Business Logic)

**Files:**
- Create: `crates/intercom-adapter-http/src/router.rs`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Test: `crates/intercom-adapter-http/tests/router_tests.rs`

**Interfaces:**
- Consumes from `intercom_core`:
  - `SystemDiagnostics`
  - `AccessControllerHandle`
- Produces:
  ```rust
  pub enum Route {
      Dashboard,
      Status,
      Logs,
      ActionRing,
      ActionUnlock,
      OtaFlash,
      NotFound,
  }
  pub fn parse_route(request_line: &str) -> Route;
  pub fn format_status_json(diag: &intercom_core::domain::diagnostics::SystemDiagnostics, out: &mut [u8]) -> Result<usize, ()>;
  ```

- [x] **Step 1: Write unit tests for route parsing and status JSON serialization**

In `crates/intercom-adapter-http/tests/router_tests.rs`:
- Asserts `parse_route` matches all endpoints.
- Asserts `format_status_json` takes a `SystemDiagnostics` from `intercom-core` and serializes valid JSON into a fixed buffer without heap allocation.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
Expected: FAIL

- [x] **Step 3: Implement `router.rs`**

Implement route parsing and JSON formatter. Notice: **zero telemetry building logic here**. It strictly formats the core `SystemDiagnostics` model into bytes.

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-adapter-http --test router_tests --target x86_64-apple-darwin`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/intercom-adapter-http/
git commit -m "feat(http): add zero-alloc HTTP request router and JSON formatter"
```

---

### Task 6: Implement Async `HttpServer` with Injected Core Services

**Files:**
- Create: `crates/intercom-adapter-http/src/server.rs`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Test: `crates/intercom-adapter-http/tests/server_tests.rs`

**Interfaces:**
- Consumes:
  - `embassy_net::Stack<'static>`
  - `AccessControllerHandle`
  - `&'a DiagnosticsService<'a, H>` (from `intercom-core`)
  - `&'a mut O` where `O: OtaFlashDriver` (from `intercom-core::ports::ota`)
- Produces:
  ```rust
  pub struct HttpServer<'a, H: HardwareInfoDriver, O: OtaFlashDriver> { /* ... */ }
  impl<'a, H: HardwareInfoDriver, O: OtaFlashDriver> HttpServer<'a, H, O> {
      pub fn new(
          stack: Stack<'static>,
          access_handle: AccessControllerHandle,
          diagnostics: &'a DiagnosticsService<'a, H>,
          ota_driver: &'a mut O,
      ) -> Self;
      pub async fn run(&mut self) -> !;
  }
  ```

- [x] **Step 1: Write integration test with mock stream and mock core services**

In `crates/intercom-adapter-http/tests/server_tests.rs`:
- Mock `HardwareInfoDriver` and `DiagnosticsService`.
- Mock `OtaFlashDriver`.
- Test `GET /` -> 200 OK + gzipped HTML.
- Test `GET /api/status` -> 200 OK + JSON matching core snapshot.
- Test `GET /api/logs` -> 200 OK + log text.
- Test `POST /api/action/ring` -> dispatches to `AccessControllerHandle`.
- Test `POST /api/v1/ota/flash` -> delegates to `OtaFlashDriver`.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
Expected: FAIL

- [x] **Step 3: Implement `HttpServer`**

Implement single-connection socket listener on port 8080.

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test -p intercom-adapter-http --test server_tests --target x86_64-apple-darwin`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/intercom-adapter-http/
git commit -m "feat(http): implement async HttpServer with injected DiagnosticsService and OtaFlashDriver"
```

---

### Task 7: Firmware Composition Root Wiring

**Files:**
- Modify: `crates/intercom-firmware/Cargo.toml`
- Modify: `crates/intercom-firmware/src/main.rs`
- Delete: Obsolete `crates/intercom-adapter-ota/src/server.rs`

**Interfaces:**
- Connects hardware HAL to `HardwareInfoDriver`, instantiates `DiagnosticsService`, wires `FlashOtaDriver`, and runs `HttpServer`.

- [x] **Step 1: Update `crates/intercom-firmware/Cargo.toml`**

Add `intercom-adapter-http = { path = "../intercom-adapter-http" }`.

- [x] **Step 2: Implement `FirmwareHardwareInfo` in `crates/intercom-firmware/src/main.rs`**

Implement `HardwareInfoDriver` using `embassy_time::Instant::now()`, `esp_alloc::HEAP`, and `intercom_adapter_ota::bootloader`.

- [x] **Step 3: Wire into `main.rs`**

```rust
let hardware_info = FirmwareHardwareInfo;
let diagnostics_service = DiagnosticsService::new(&hardware_info, state_watch);
let mut ota_driver = intercom_adapter_ota::FlashOtaDriver::new(config::OTA_PUBLIC_KEY);

let mut http_server = intercom_adapter_http::HttpServer::new(
    stack,
    access_handle,
    &diagnostics_service,
    &mut ota_driver,
);
http_server.run().await;
```

Delete obsolete `server.rs` from `intercom-adapter-ota`.

- [x] **Step 4: Verify embedded release compilation on RISC-V target**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Successful compilation, binary fits cleanly in `ota_0`.

- [x] **Step 5: Commit**

```bash
git add crates/intercom-firmware/ crates/intercom-adapter-ota/
git commit -m "feat(firmware): wire DiagnosticsService, FlashOtaDriver, and HttpServer in main"
```

---

### Task 8: Full Workspace Verification & OTA Compatibility Check

**Files:**
- Test: All workspace crates

- [x] **Step 1: Run host unit tests across all workspace crates**

Run: `cargo test -p intercom-core -p intercom-adapter-ota -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: All tests PASS in < 2 seconds.

- [x] **Step 2: Run release firmware compilation**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Build succeeds with 0 errors.

- [x] **Step 3: Verify OTA Python script compatibility**

Verify `python3 scripts/ota.py --help` runs and that wire protocol paths (`/api/v1/ota/flash`) remain identical.

- [x] **Step 4: Commit**

```bash
git add -u
git commit -m "chore: complete verification of web UI dashboard and diagnostics integration"
```
