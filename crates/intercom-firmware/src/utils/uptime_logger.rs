//! Uptime logger — `log` implementation with HH:MM:SS.mmm timestamps.
//!
//! Uses Embassy's monotonic clock to prefix every log line with the
//! device uptime in a human-readable format. Supports per-module
//! filtering to allow Debug level for interesting modules while
//! suppressing noisy ones (mDNS, BUMP allocator). Also retains
//! the latest log records in an in-memory ring buffer for remote diagnostics.
//!
//! ```text
//! 00:00:00.012 INFO  intercom_matter - === Smart Intercom Starting ===
//! 00:20:34.567 WARN  intercom_core::service::ring - ring detected
//! ```

// ============================================================================
// Public API
// ============================================================================

/// Initialize the uptime logger at the given level.
pub fn init(level: log::LevelFilter) {
    unsafe {
        log::set_logger_racy(&LOGGER).unwrap();
        log::set_max_level_racy(level);
    }
}

/// Copies up to `buf.len()` bytes of the oldest-to-newest buffered logs.
///
/// Returns the number of bytes written into `buf`.
#[allow(dead_code)]
pub fn get_buffered_logs(buf: &mut [u8]) -> usize {
    critical_section::with(|cs| LOG_BUFFER.borrow(cs).borrow().read_into(buf))
}

/// Global in-memory log ring buffer storing the latest 6 KiB of log output.
#[allow(dead_code)]
pub static LOG_BUFFER: critical_section::Mutex<core::cell::RefCell<LogRingBuffer<6144>>> =
    critical_section::Mutex::new(core::cell::RefCell::new(LogRingBuffer::new()));

/// Fixed-capacity in-memory byte ring buffer.
///
/// Overwrites the oldest bytes when full, preserving chronological ordering
/// when read back. Zero heap allocation.
#[allow(dead_code)]
pub struct LogRingBuffer<const N: usize> {
    storage: [u8; N],
    write_pos: usize,
    count: usize,
}

#[allow(dead_code)]
impl<const N: usize> LogRingBuffer<N> {
    /// Creates an empty ring buffer.
    pub const fn new() -> Self {
        Self {
            storage: [0u8; N],
            write_pos: 0,
            count: 0,
        }
    }

    /// Appends the string slice to the ring buffer, overwriting oldest bytes on overflow.
    pub fn write_str(&mut self, s: &str) {
        if N == 0 {
            return;
        }
        for &byte in s.as_bytes() {
            self.storage[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % N;
            if self.count < N {
                self.count += 1;
            }
        }
    }

    /// Reads up to `buf.len()` bytes chronologically into `buf`.
    ///
    /// Returns the number of bytes copied.
    pub fn read_into(&self, buf: &mut [u8]) -> usize {
        if N == 0 {
            return 0;
        }
        let to_copy = self.count.min(buf.len());
        let start = (self.write_pos + N - to_copy) % N;
        for i in 0..to_copy {
            let idx = (start + i) % N;
            buf[i] = self.storage[idx];
        }
        to_copy
    }

    /// Clears all buffered logs.
    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.count = 0;
    }
}

impl<const N: usize> core::fmt::Write for LogRingBuffer<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

// ============================================================================
// Internal Logger Implementation
// ============================================================================

struct UptimeLogger;

static LOGGER: UptimeLogger = UptimeLogger;

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
            #[cfg(target_arch = "riscv32")]
            let ms = embassy_time::Instant::now().as_millis();
            #[cfg(not(target_arch = "riscv32"))]
            let ms = 0u64;

            let total_secs = ms / 1000;
            let h = total_secs / 3600;
            let m = (total_secs % 3600) / 60;
            let s = total_secs % 60;
            let frac = ms % 1000;

            // Write clean record into in-memory ring buffer (no ANSI codes)
            critical_section::with(|cs| {
                use core::fmt::Write;
                let mut buffer = LOG_BUFFER.borrow(cs).borrow_mut();
                let _ = core::write!(
                    &mut *buffer,
                    "{:02}:{:02}:{:02}.{:03} {:<5} {} - {}\n",
                    h,
                    m,
                    s,
                    frac,
                    record.level(),
                    record.target(),
                    record.args()
                );
            });

            let (color, reset) = match record.level() {
                log::Level::Error => ("\x1b[31m", "\x1b[0m"), // red
                log::Level::Warn => ("\x1b[33m", "\x1b[0m"),  // yellow
                _ => ("", ""),
            };
            #[cfg(target_arch = "riscv32")]
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
            #[cfg(not(target_arch = "riscv32"))]
            let _ = (color, reset);
        }
    }

    fn flush(&self) {}
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

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

    #[test]
    fn test_log_ring_buffer_empty_and_clear() {
        // Arrange
        let mut rb = LogRingBuffer::<16>::new();
        let mut out = [0u8; 16];

        // Act & Assert: initially empty
        let n = rb.read_into(&mut out);
        assert_eq!(n, 0);

        // Act & Assert: write and clear
        rb.write_str("hello");
        assert_eq!(rb.read_into(&mut out), 5);
        rb.clear();
        assert_eq!(rb.read_into(&mut out), 0);
    }

    #[test]
    fn test_log_ring_buffer_partial_read() {
        // Arrange
        let mut rb = LogRingBuffer::<16>::new();
        let mut out = [0u8; 4];

        // Act
        rb.write_str("12345678");
        let n = rb.read_into(&mut out);

        // Assert: buffer smaller than stored log, returns latest 4 bytes chronologically
        assert_eq!(n, 4);
        assert_eq!(&out[..n], b"5678");
    }

    #[test]
    fn test_log_ring_buffer_partial_read_after_overflow() {
        // Arrange
        let mut rb = LogRingBuffer::<16>::new();
        let mut out = [0u8; 4];

        // Act: write 18 bytes into 16-byte buffer
        rb.write_str("abcdefghijklmnopqr");
        let n = rb.read_into(&mut out);

        // Assert: returns latest 4 bytes of 18 ("opqr")
        assert_eq!(n, 4);
        assert_eq!(&out[..n], b"opqr");
    }

    #[test]
    fn test_get_buffered_logs_via_logger() {
        // Arrange
        critical_section::with(|cs| {
            LOG_BUFFER.borrow(cs).borrow_mut().clear();
        });
        let logger = UptimeLogger;

        // Act
        logger.log(
            &log::Record::builder()
                .args(format_args!("test event"))
                .level(log::Level::Info)
                .target("test_module")
                .build(),
        );

        let mut buf = [0u8; 128];
        let n = get_buffered_logs(&mut buf);

        // Assert
        let log_str = core::str::from_utf8(&buf[..n]).unwrap();
        assert!(log_str.ends_with("INFO  test_module - test event\n"));
    }
}
