//! Uptime logger — `log` implementation with HH:MM:SS.mmm timestamps.
//!
//! Uses Embassy's monotonic clock to prefix every log line with the
//! device uptime in a human-readable format.
//!
//! ```text
//! 00:00:00.012 INFO - === Smart Intercom Starting ===
//! 00:20:34.567 INFO - DoorActor: unlock event
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
            esp_println::println!(
                "{:02}:{:02}:{:02}.{:03} {} - {}",
                h,
                m,
                s,
                frac,
                record.level(),
                record.args()
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
