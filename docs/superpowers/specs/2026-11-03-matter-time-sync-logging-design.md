# Technical Design: Real-Time Logging via Matter Time Synchronization

- **Date:** 2026-11-03
- **Status:** Approved
- **Scope:** `crates/intercom-firmware`, `crates/intercom-adapter-matter`, `../embedded-device-foundation/crates/edf-logger`
- **Primary Goal:** Display real-time UTC calendar timestamps (`YYYY-MM-DD HH:MM:SS.mmm`) in firmware logs (UART and in-memory HTTP ring buffer) using the Matter Time Synchronization Cluster (`0x0038`), while maintaining monotonic uptime formatting (`HH:MM:SS.mmm`) during boot prior to clock synchronization.

---

## 1. Problem Statement & Constraints

Currently, `edf-logger` formats all log records using monotonic uptime from device start:
```
00:01:23.456 INFO  intercom_firmware - Smart Intercom Ready
```

While useful for initial boot diagnostics, uptime makes it difficult to correlate log entries with external smart home events (e.g., Apple Home doorbell ring notifications, door unlock commands, WebRTC calls) or diagnose issues days after boot.

### Key Constraints:
1. **No Battery-Backed RTC:** The ESP32-C6 hardware has no coin-cell battery or persistent real-time clock. On power cycle or reset, it only has monotonic uptime starting from zero.
2. **Zero Deadlock Risk:** `log::*` macros can be called from any execution context, including interrupt handlers, critical sections, and internal callbacks inside `rs-matter`. The logger must NEVER synchronously acquire `Matter` state or locks during log formatting.
3. **Boot-Time Fallback:** During the initial boot phase (hardware init, Wi-Fi association, fabric discovery), wall-clock time is not yet known. The logger must display `HH:MM:SS.mmm` until time is synchronized, then switch to `YYYY-MM-DD HH:MM:SS.mmm`.
4. **Zero Allocation & Embedded Performance:** Log formatting must be pure `no_std`, zero-allocation, and execute in under a microsecond to avoid stalling async tasks or UART buffers.

---

## 2. Architecture & Design

### 2.1 Endpoint 0 Cluster Descriptor (`intercom-adapter-matter`)

In `rs-matter`, the root endpoint handler chain (`root_handler_chained`) already includes `TimeSyncHandler`, which handles the mandatory `UTCTime`, `Granularity`, `TimeSource`, and `SetUTCTime` commands against `Matter::state.rtc`.

However, the Root Node descriptor on Endpoint 0 must explicitly advertise the cluster so Matter fabric administrators (e.g. Apple Home, Home Assistant) discover it:

```rust
// crates/intercom-adapter-matter/src/controller.rs
// Replace default root_endpoint() with root_endpoint!(wifi, time_sync())
endpoints.push(root_endpoint!(wifi, time_sync()));
```

### 2.2 Shared `edf-logger` Timestamp Abstraction (`embedded-device-foundation`)

Update `edf-logger` to support structured timestamps while preserving its zero-allocation and ring-buffer retention guarantees:

```rust
// embedded-device-foundation/crates/edf-logger/src/timestamp.rs

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogTimestamp {
    /// Monotonic uptime in milliseconds since boot.
    Uptime { ms: u64 },
    /// Civil calendar date and UTC time.
    DateTime {
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        millis: u16,
    },
}

impl LogTimestamp {
    /// Converts Unix epoch milliseconds (seconds since 1970-01-01 00:00:00 UTC)
    /// to a civil date/time representation using pure integer arithmetic (no_std).
    pub const fn from_epoch_millis(epoch_ms: u64) -> Self {
        // Deterministic Gregorian calendar conversion
        let total_secs = epoch_ms / 1000;
        let millis = (epoch_ms % 1000) as u16;
        let second = (total_secs % 60) as u8;
        let total_mins = total_secs / 60;
        let minute = (total_mins % 60) as u8;
        let total_hours = total_mins / 60;
        let hour = (total_hours % 24) as u8;
        let days = (total_hours / 24) as u32;

        // Days since 1970-01-01 to (year, month, day)
        let (year, month, day) = days_to_civil(days);

        Self::DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millis,
        }
    }
}

impl core::fmt::Display for LogTimestamp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Uptime { ms } => {
                let total_secs = ms / 1000;
                let hours = total_secs / 3600;
                let minutes = (total_secs % 3600) / 60;
                let seconds = total_secs % 60;
                let millis = ms % 1000;
                write!(f, "{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
            }
            Self::DateTime { year, month, day, hour, minute, second, millis } => {
                write!(
                    f,
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{millis:03}"
                )
            }
        }
    }
}
```

In `UptimeLogger<const N: usize>`:
```rust
pub struct UptimeLogger<const N: usize> {
    timestamp_fn: fn() -> LogTimestamp,
    output: for<'a> fn(log::Level, fmt::Arguments<'a>),
    suppressed_debug_exact: &'static [&'static str],
    suppressed_debug_prefixes: &'static [&'static str],
    ring: Mutex<RefCell<LogRingBuffer<N>>>,
}
```
`log(&self, record: &Record)` formats `timestamp_fn()` directly into both the in-memory ring and the UART output callback using `Display`.

### 2.3 Lock-Free Clock Tracking in Firmware (`intercom-firmware`)

In `crates/intercom-firmware/src/utils/uptime_logger.rs`:
```rust
const UNSYNCED: i64 = i64::MIN;
static CLOCK_OFFSET_MS: portable_atomic::AtomicI64 = portable_atomic::AtomicI64::new(UNSYNCED);

pub fn update_clock_offset_ms(offset_ms: i64) {
    CLOCK_OFFSET_MS.store(offset_ms, core::sync::atomic::Ordering::Release);
}

pub fn is_clock_synchronized() -> bool {
    CLOCK_OFFSET_MS.load(core::sync::atomic::Ordering::Relaxed) != UNSYNCED
}

fn log_timestamp() -> edf_logger::LogTimestamp {
    let uptime_ms = embassy_time::Instant::now().as_millis();
    let offset = CLOCK_OFFSET_MS.load(core::sync::atomic::Ordering::Relaxed);
    if offset == UNSYNCED {
        edf_logger::LogTimestamp::Uptime { ms: uptime_ms }
    } else {
        let unix_ms = (uptime_ms as i64).saturating_add(offset) as u64;
        edf_logger::LogTimestamp::from_epoch_millis(unix_ms)
    }
}
```

### 2.4 Time Synchronization Background Task (`intercom-firmware`)

A lightweight task monitors Matter's `state.rtc` without blocking any critical paths:
1. When `readiness.matter` is attached, the task polls `matter.with_state(|s| s.rtc.utc_time())`.
2. When `UtcTime::Reliable(matter_epoch_us)` is reported:
   - Converts Matter epoch microseconds to Unix epoch milliseconds:
     $$\text{unix\_ms} = \left(\frac{\text{matter\_us}}{1000}\right) + (946,684,800 \times 1000)$$
   - Computes offset:
     $$\text{offset\_ms} = \text{unix\_ms} - \text{Instant::now().as\_millis()}$$
   - If this is the initial transition, emits:
     `INFO - Real-time clock synchronized via Matter to YYYY-MM-DD HH:MM:SS UTC`
   - Atomically updates `CLOCK_OFFSET_MS`.
3. Periodically re-checks (e.g. every 60 seconds) to apply any adjustment if the controller sends an updated `SetUTCTime`.

---

## 3. Verification & Testing

### 3.1 Unit Testing (`edf-logger`)
- **Gregorian Conversion:** Test `LogTimestamp::from_epoch_millis`:
  - Epoch 0 (1970-01-01 00:00:00.000)
  - Matter Epoch (2000-01-01 00:00:00.000)
  - Leap year edge cases (2024-02-29, 2000-02-29, 2100-02-28)
  - Arbitrary recent timestamps (e.g., 2026-11-03 18:30:00.123)
- **Formatting Parity:** Verify `Display` outputs exact character counts and match across `ring` and `output`.

### 3.2 Unit Testing (`intercom-firmware`)
- Verify `log_timestamp` returns `Uptime` when `CLOCK_OFFSET_MS == UNSYNCED`.
- Verify `log_timestamp` returns `DateTime` with exact computed timestamp when offset is populated.

### 3.3 Target Compilation & Release Verification
- Build release binary: `mise exec -- cargo build -p intercom-firmware --release`
- Verify flash space and clean compilation for both standard and `devkit` feature flags.
