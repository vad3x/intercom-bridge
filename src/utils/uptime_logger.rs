//! Uptime logger — `log` implementation with HH:MM:SS.mmm timestamps.
//!
//! Uses Embassy's monotonic clock to prefix every log line with the
//! device uptime in a human-readable format.
//!
//! ```text
//! 00:00:00.012 INFO  intercom_matter - === Smart Intercom Starting ===
//! 00:20:34.567 WARN  intercom_matter::actors::ring - ring detected
//! ```

struct UptimeLogger;

impl log::Log for UptimeLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let ms = embassy_time::Instant::now().as_millis();
            let total_secs = ms / 1000;
            let h = total_secs / 3600;
            let m = (total_secs % 3600) / 60;
            let s = total_secs % 60;
            let frac = ms % 1000;
            let (color, reset) = match record.level() {
                log::Level::Error => ("\x1b[31m", "\x1b[0m"), // red
                log::Level::Warn => ("\x1b[33m", "\x1b[0m"),  // yellow
                _ => ("", ""),
            };
            esp_println::println!(
                "{}{:02}:{:02}:{:02}.{:03} {:<5} {} - {}{}",
                color,
                h,
                m,
                s,
                frac,
                record.level(),
                record.target(),
                record.args(),
                reset
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: UptimeLogger = UptimeLogger;

/// Initialize the uptime logger at the given level.
pub fn init(level: log::LevelFilter) {
    unsafe {
        log::set_logger_racy(&LOGGER).unwrap();
        log::set_max_level_racy(level);
    }
}
