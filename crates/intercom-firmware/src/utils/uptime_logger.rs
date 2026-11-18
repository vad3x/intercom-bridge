//! Intercom composition for the shared uptime logger.

use core::sync::atomic::{fence, AtomicU32, Ordering};
use edf_logger::{
    LogTimestamp, UptimeLogger, DEFAULT_SUPPRESSED_DEBUG_EXACT, DEFAULT_SUPPRESSED_DEBUG_PREFIXES,
};
use intercom_core::ports::ClockSyncDriver;

const UNSYNCED: i64 = i64::MIN;

/// A signed 64-bit offset published through native 32-bit atomics.
///
/// The Matter monitor is the single writer. Readers take a bounded seqlock
/// snapshot; if a write is in progress they use uptime for that log record.
struct ClockOffset {
    sequence: AtomicU32,
    low: AtomicU32,
    high: AtomicU32,
}

impl ClockOffset {
    const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            low: AtomicU32::new(UNSYNCED as u32),
            high: AtomicU32::new((UNSYNCED as u64 >> 32) as u32),
        }
    }

    fn store(&self, offset_ms: i64) {
        // There is one writer: the Matter connection monitor task.
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.low.store(offset_ms as u32, Ordering::Relaxed);
        self.high
            .store((offset_ms as u64 >> 32) as u32, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }

    fn load(&self) -> Option<i64> {
        // Bound retries so a log call interrupted during publication cannot
        // spin and prevent the writer from resuming.
        for _ in 0..2 {
            let before = self.sequence.load(Ordering::Acquire);
            if before & 1 != 0 {
                continue;
            }

            let low = self.low.load(Ordering::Relaxed);
            let high = self.high.load(Ordering::Relaxed);
            fence(Ordering::Acquire);
            let after = self.sequence.load(Ordering::Relaxed);

            if before == after {
                let offset_ms = (((high as u64) << 32) | low as u64) as i64;
                return (offset_ms != UNSYNCED).then_some(offset_ms);
            }
        }

        None
    }
}

static CLOCK_OFFSET_MS: ClockOffset = ClockOffset::new();

/// Clock adapter owned by the firmware composition root.
pub struct FirmwareClockSyncDriver;

impl ClockSyncDriver for FirmwareClockSyncDriver {
    fn set_epoch_time(&self, unix_ms: u64) {
        let epoch_ms = unix_ms.min(i64::MAX as u64) as i64;
        let uptime_ms = uptime_millis().min(i64::MAX as u64) as i64;
        let initial_sync = !is_clock_synchronized();
        update_clock_offset_ms(epoch_ms.saturating_sub(uptime_ms));

        if initial_sync {
            log::info!(
                "Real-time clock synchronized via Matter to {} UTC",
                LogTimestamp::from_epoch_millis(unix_ms)
            );
        }
    }
}

static LOGGER: UptimeLogger<{ crate::config::LOG_BUFFER_CAPACITY }> = UptimeLogger::new(
    log_timestamp,
    write_uart_record,
    DEFAULT_SUPPRESSED_DEBUG_EXACT,
    DEFAULT_SUPPRESSED_DEBUG_PREFIXES,
);

/// Installs the Intercom logger and returns any global registration failure.
pub fn init(level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    LOGGER.init(level)
}

/// Copies the newest retained log bytes in chronological order.
pub fn get_buffered_logs(buf: &mut [u8]) -> usize {
    LOGGER.get_buffered_logs(buf)
}

fn uptime_millis() -> u64 {
    #[cfg(target_os = "none")]
    {
        embassy_time::Instant::now().as_millis()
    }
    #[cfg(not(target_os = "none"))]
    {
        0
    }
}

fn log_timestamp() -> LogTimestamp {
    let uptime_ms = uptime_millis();
    if let Some(offset_ms) = CLOCK_OFFSET_MS.load() {
        let unix_ms = (uptime_ms.min(i64::MAX as u64) as i64)
            .saturating_add(offset_ms)
            .max(0) as u64;
        LogTimestamp::from_epoch_millis(unix_ms)
    } else {
        LogTimestamp::Uptime { ms: uptime_ms }
    }
}

/// Replaces the wall-clock offset used by log formatting.
pub fn update_clock_offset_ms(offset_ms: i64) {
    CLOCK_OFFSET_MS.store(offset_ms);
}

/// Reports whether the Matter time synchronization adapter supplied reliable time.
pub fn is_clock_synchronized() -> bool {
    CLOCK_OFFSET_MS.load().is_some()
}

fn write_uart_record(level: log::Level, record: core::fmt::Arguments<'_>) {
    #[cfg(target_os = "none")]
    {
        let (color, reset) = uart_colors(level);
        esp_println::println!("{color}{record}{reset}");
    }
    #[cfg(not(target_os = "none"))]
    {
        let _ = (level, record);
    }
}

#[cfg(any(target_os = "none", test))]
fn uart_colors(level: log::Level) -> (&'static str, &'static str) {
    match level {
        log::Level::Error => ("\x1b[31m", "\x1b[0m"),
        log::Level::Warn => ("\x1b[33m", "\x1b[0m"),
        _ => ("", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_buffered_logs, is_clock_synchronized, log_timestamp, uart_colors, ClockOffset,
        FirmwareClockSyncDriver, LOGGER,
    };
    use edf_logger::{LogTimestamp, UptimeLogger};
    use intercom_core::ports::ClockSyncDriver;
    use log::{Level, Log, Record};

    #[test]
    fn clock_offset_snapshot_preserves_signed_64_bit_values() {
        let offset = ClockOffset::new();

        assert_eq!(offset.load(), None);

        offset.store(1_793_730_600_123);
        assert_eq!(offset.load(), Some(1_793_730_600_123));

        offset.store(-4_294_967_301);
        assert_eq!(offset.load(), Some(-4_294_967_301));
    }

    #[test]
    fn log_timestamp_uses_uptime_until_clock_sync() {
        super::CLOCK_OFFSET_MS.store(super::UNSYNCED);

        assert!(!is_clock_synchronized());
        assert_eq!(log_timestamp(), LogTimestamp::Uptime { ms: 0 });
    }

    #[test]
    fn clock_driver_switches_logger_to_synchronized_utc() {
        super::CLOCK_OFFSET_MS.store(super::UNSYNCED);
        FirmwareClockSyncDriver.set_epoch_time(1_793_730_600_123);

        assert!(is_clock_synchronized());
        assert_eq!(
            log_timestamp(),
            LogTimestamp::DateTime {
                year: 2026,
                month: 11,
                day: 3,
                hour: 18,
                minute: 30,
                second: 0,
                millis: 123,
            }
        );

        super::CLOCK_OFFSET_MS.store(super::UNSYNCED);
    }

    #[test]
    fn logger_retains_the_synchronized_utc_timestamp() {
        super::CLOCK_OFFSET_MS.store(super::UNSYNCED);
        FirmwareClockSyncDriver.set_epoch_time(1_793_730_600_123);
        let logger = UptimeLogger::<128>::new(log_timestamp, discard_uart_record, &[], &[]);
        let record = Record::builder()
            .args(format_args!("synchronized"))
            .level(Level::Info)
            .target("clock_test")
            .build();

        logger.log(&record);

        let mut retained = [0; 128];
        let count = logger.get_buffered_logs(&mut retained);
        assert_eq!(
            &retained[..count],
            b"2026-11-03 18:30:00.123 INFO  clock_test - synchronized\n"
        );
        super::CLOCK_OFFSET_MS.store(super::UNSYNCED);
    }

    fn discard_uart_record(_level: Level, _record: core::fmt::Arguments<'_>) {}

    #[test]
    fn uart_colors_match_log_severity_without_coloring_retained_records() {
        assert_eq!(uart_colors(Level::Error), ("\x1b[31m", "\x1b[0m"));
        assert_eq!(uart_colors(Level::Warn), ("\x1b[33m", "\x1b[0m"));
        assert_eq!(uart_colors(Level::Info), ("", ""));
    }

    #[test]
    fn get_buffered_logs_returns_newest_records_within_http_read_window() {
        for index in 0..150 {
            let args = format_args!("retained-record-{index:03}");
            let record = Record::builder()
                .args(args)
                .level(Level::Info)
                .target("uptime_logger_test")
                .build();
            LOGGER.log(&record);
        }

        let mut bytes = [0; intercom_adapter_http::config::LOG_BUFFER_SIZE];
        let count = get_buffered_logs(&mut bytes);
        let retained = core::str::from_utf8(&bytes[..count]).unwrap();

        // The two HTTP workers leave a bounded 2 KiB diagnostic history.
        assert_eq!(count, 2 * 1024);
        assert!(!retained.contains("retained-record-000"));
        assert!(retained.contains("retained-record-149"));
        let earlier_record = retained.find("retained-record-147").unwrap();
        let latest_record = retained.find("retained-record-149").unwrap();
        assert!(earlier_record < latest_record);
    }
}
