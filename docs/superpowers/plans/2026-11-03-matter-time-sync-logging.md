# Real-Time Logging via Matter Time Synchronization Implementation Plan

- **Date:** 2026-11-03
- **Specification:** [`docs/superpowers/specs/2026-11-03-matter-time-sync-logging-design.md`](file:///Users/vadim/personal/projects/intercom-matter/docs/superpowers/specs/2026-11-03-matter-time-sync-logging-design.md)
- **Architecture:** Hexagonal Architecture (Ports & Adapters)
- **Goal:** Display real-time UTC calendar timestamps (`YYYY-MM-DD HH:MM:SS.mmm`) in firmware logs (UART and in-memory HTTP ring buffer) when synchronized via Matter Time Synchronization Cluster (`0x0038`), while displaying monotonic uptime (`HH:MM:SS.mmm`) during boot before synchronization.

---

## User Review Checkpoint

> [!NOTE]
> Review this implementation plan before code changes begin. Each task follows Hexagonal Architecture boundaries and TDD:
> 1. Foundation: `edf-logger` (`LogTimestamp` with Gregorian math).
> 2. Domain: `intercom-core` (`ClockSyncDriver` outbound port).
> 3. Adapter: `intercom-adapter-matter` (Root Endpoint 0 `time_sync` cluster advertisement and time extraction).
> 4. Composition Root: `intercom-firmware` (`FirmwareClockSyncDriver`, atomic offset, wire into `App` & `Matter`).
> 5. Full system verification.

---

## Proposed Changes

### `embedded-device-foundation` (`crates/edf-logger`)
- Add `src/timestamp.rs` defining `LogTimestamp`:
  - `Uptime { ms: u64 }`
  - `DateTime { year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8, millis: u16 }`
  - `from_epoch_millis(ms: u64) -> Self`
  - `Display` implementation for formatting.
- Update `UptimeLogger<const N: usize>` to accept `fn() -> LogTimestamp` (or support both timestamp and legacy clock ms).
- Unit tests for epoch conversions, leap years, and format strings.

### `intercom-core` (`crates/intercom-core`)
- Add `src/ports/clock.rs` declaring outbound port `ClockSyncDriver`:
  ```rust
  pub trait ClockSyncDriver: Send + Sync {
      fn set_epoch_time(&self, unix_ms: u64);
  }
  ```
- Export `ClockSyncDriver` in `crates/intercom-core/src/ports/mod.rs`.

### `intercom-adapter-matter` (`crates/intercom-adapter-matter`)
- Update `controller.rs` to build Endpoint 0 descriptor with `time_sync`:
  ```rust
  root_endpoint!(wifi, time_sync())
  ```
- Add `TimeSyncMonitor` or hook in `MatterController` to observe `Matter::utc_time()` and invoke `ClockSyncDriver::set_epoch_time`.

### `intercom-firmware` (`crates/intercom-firmware`)
- Implement `FirmwareClockSyncDriver` in `src/utils/uptime_logger.rs` (or `src/time_sync.rs`).
- Update `uptime_logger.rs` to maintain lock-free atomic `CLOCK_OFFSET_MS` and supply `log_timestamp() -> LogTimestamp` to `edf_logger::UptimeLogger`.
- Wire `FirmwareClockSyncDriver` in `main.rs` and spawn/join the time sync monitor task.

---

## Task Breakdown

### Task 1: Extend `edf-logger` with `LogTimestamp` & Gregorian Calendar Conversion
- **Files:**
  - `embedded-device-foundation/crates/edf-logger/src/timestamp.rs`
  - `embedded-device-foundation/crates/edf-logger/src/lib.rs`
  - `embedded-device-foundation/crates/edf-logger/tests/timestamp_tests.rs`
- **Steps:**
  1. Write failing tests for `LogTimestamp::from_epoch_millis` (Unix epoch 0, leap years 2000/2024, century non-leap 2100, known date 2026-11-03) and `Display` formatting.
  2. Implement `LogTimestamp` and integer Gregorian date math in `timestamp.rs`.
  3. Update `UptimeLogger` in `lib.rs` to accept `timestamp_fn: fn() -> LogTimestamp`.
  4. Run host tests: `mise exec -- cargo test -p edf-logger`.
  5. Commit changes.

### Task 2: Define `ClockSyncDriver` Outbound Port in `intercom-core`
- **Files:**
  - `crates/intercom-core/src/ports/clock.rs`
  - `crates/intercom-core/src/ports/mod.rs`
  - `crates/intercom-core/tests/clock_port_tests.rs`
- **Steps:**
  1. Write unit test with a mock `ClockSyncDriver`.
  2. Implement `ClockSyncDriver` trait in `crates/intercom-core/src/ports/clock.rs`.
  3. Re-export in `crates/intercom-core/src/ports/mod.rs` and `lib.rs`.
  4. Run tests: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`.
  5. Commit changes.

### Task 3: Root Endpoint Descriptor & Matter Time Sync in `intercom-adapter-matter`
- **Files:**
  - `crates/intercom-adapter-matter/src/controller.rs`
  - `crates/intercom-adapter-matter/src/time_sync.rs`
  - `crates/intercom-adapter-matter/src/lib.rs`
- **Steps:**
  1. Update Endpoint 0 construction in `controller.rs` to include `time_sync()`.
  2. Add `poll_time_sync(matter: &Matter, clock: &dyn ClockSyncDriver)` helper or integration runner.
  3. Verify `intercom-adapter-matter` compiles cleanly and passes host checks.
  4. Commit changes.

### Task 4: Concrete Clock Adapter & Composition Root Wiring in `intercom-firmware`
- **Files:**
  - `crates/intercom-firmware/src/utils/uptime_logger.rs`
  - `crates/intercom-firmware/src/main.rs`
  - `crates/intercom-firmware/src/time_sync.rs`
- **Steps:**
  1. Add `CLOCK_OFFSET_MS: AtomicI64` and `FirmwareClockSyncDriver` to `uptime_logger.rs`.
  2. Connect `log_timestamp` callback to `edf_logger::UptimeLogger`.
  3. In `main.rs`, spawn/join the Matter time sync task passing `&FirmwareClockSyncDriver`.
  4. Run host library tests: `mise exec -- cargo test -p intercom-firmware --lib --target x86_64-apple-darwin`.
  5. Commit changes.

### Task 5: End-to-End Verification & MCU Build
- **Steps:**
  1. Run all workspace host tests:
     `mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`
  2. Build release firmware binary for RISC-V target:
     `mise exec -- cargo build -p intercom-firmware --release`
  3. Build devkit firmware binary:
     `mise exec -- cargo build-devkit`
  4. Verify clean git diff and formatted code.
