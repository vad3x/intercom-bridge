//! Uptime logger — `log` implementation with HH:MM:SS.mmm timestamps.
//!
//! Uses Embassy's monotonic clock to prefix every log line with the
//! device uptime in a human-readable format. Supports per-module
//! filtering to allow Debug level for interesting modules while
//! suppressing noisy ones (mDNS, BUMP allocator).
//!
//! ```text
//! 00:00:00.012 INFO  intercom_matter - === Smart Intercom Starting ===
//! 00:20:34.567 WARN  intercom_core::service::ring - ring detected
//! ```

struct UptimeLogger;

/// Modules suppressed at Debug/Trace level (only Info+ passes).
/// These produce high-volume output that blocks the UART and starves the CPU.
///
/// Prefix match — any target starting with these strings is suppressed.
const SUPPRESSED_DEBUG_PREFIXES: &[&str] = &[
    "rs_matter::transport::network::mdns", // mDNS broadcast flood
    "trouble_host",                        // BLE host internals
];

/// Exact-match targets suppressed at Debug/Trace level.
/// The `rs_matter` root module logs mDNS service enumeration at debug!
/// level (~20/sec), but sub-modules like `rs_matter::transport` and
/// `rs_matter::sc` contain the useful CASE/PASE diagnostics we need.
const SUPPRESSED_DEBUG_EXACT: &[&str] = &[];

impl log::Log for UptimeLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // For Debug/Trace, suppress noisy modules
        if metadata.level() > log::Level::Info {
            let target = metadata.target();
            for exact in SUPPRESSED_DEBUG_EXACT {
                if target == *exact {
                    return false;
                }
            }
            for prefix in SUPPRESSED_DEBUG_PREFIXES {
                if target.starts_with(prefix) {
                    return false;
                }
            }
        }
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
