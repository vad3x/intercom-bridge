//! Intercom composition for the shared uptime logger.

use edf_logger::{UptimeLogger, DEFAULT_SUPPRESSED_DEBUG_EXACT, DEFAULT_SUPPRESSED_DEBUG_PREFIXES};

static LOGGER: UptimeLogger<{ crate::config::LOG_BUFFER_CAPACITY }> = UptimeLogger::new(
    uptime_millis,
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
    #[cfg(target_arch = "riscv32")]
    {
        embassy_time::Instant::now().as_millis()
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        0
    }
}

fn write_uart_record(level: log::Level, record: core::fmt::Arguments<'_>) {
    #[cfg(target_arch = "riscv32")]
    {
        let (color, reset) = uart_colors(level);
        esp_println::println!("{color}{record}{reset}");
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        let _ = (level, record);
    }
}

#[cfg(any(target_arch = "riscv32", test))]
fn uart_colors(level: log::Level) -> (&'static str, &'static str) {
    match level {
        log::Level::Error => ("\x1b[31m", "\x1b[0m"),
        log::Level::Warn => ("\x1b[33m", "\x1b[0m"),
        _ => ("", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::{get_buffered_logs, uart_colors, LOGGER};
    use log::{Level, Log, Record};

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

        assert_eq!(count, bytes.len());
        assert!(!retained.contains("retained-record-000"));
        assert!(retained.contains("retained-record-149"));
        let earlier_record = retained.find("retained-record-147").unwrap();
        let latest_record = retained.find("retained-record-149").unwrap();
        assert!(earlier_record < latest_record);
    }
}
