//! Per-core CPU utilization sampler for bare-metal Embassy executors.
//!
//! Uses a lightweight 20 ms timer tick per core and computes busy percentage over
//! a 1-second sliding window from timer scheduling overshoot without busy-waiting.

use intercom_adapter_http::{CpuCoreLoad, CpuSnapshot};
use portable_atomic::{AtomicU8, Ordering};

/// Expected sampler tick interval in microseconds (20 ms = 50 Hz).
pub const SAMPLE_TICK_US: u64 = 20_000;

/// Aggregation window in microseconds (1 second).
pub const WINDOW_US: u64 = 1_000_000;

const MAX_SUPPORTED_CORES: usize = 2;

static CORE_LOAD_PCT: [AtomicU8; MAX_SUPPORTED_CORES] = [AtomicU8::new(0), AtomicU8::new(0)];

/// Pure window accumulator computing CPU busy percentage from tick elapsed durations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuLoadTracker {
    window_busy_us: u64,
    window_total_us: u64,
}

impl CpuLoadTracker {
    pub const fn new() -> Self {
        Self {
            window_busy_us: 0,
            window_total_us: 0,
        }
    }

    /// Record one sampler tick's actual elapsed duration and return a completed
    /// `0..=100` load percentage whenever the 1-second window closes.
    pub fn record_tick(&mut self, actual_elapsed_us: u64) -> Option<u8> {
        let busy_us = actual_elapsed_us.saturating_sub(SAMPLE_TICK_US);
        self.window_busy_us = self.window_busy_us.saturating_add(busy_us);
        self.window_total_us = self.window_total_us.saturating_add(actual_elapsed_us);

        if self.window_total_us >= WINDOW_US {
            let pct = ((u128::from(self.window_busy_us) * 100) / u128::from(self.window_total_us))
                .min(100);
            self.window_busy_us = 0;
            self.window_total_us = 0;
            Some(pct as u8)
        } else {
            None
        }
    }
}

/// Number of active CPU cores on the compiled target.
pub const fn active_core_count() -> usize {
    if cfg!(any(feature = "esp32s3", feature = "esp32s31")) {
        2
    } else {
        1
    }
}

/// Read the latest per-core CPU utilization snapshot.
pub fn cpu_info() -> CpuSnapshot {
    let count = active_core_count();
    let mut cores = [CpuCoreLoad::default(); MAX_SUPPORTED_CORES];
    let mut idx = 0;
    while idx < count {
        cores[idx] = CpuCoreLoad {
            id: idx as u8,
            load_pct: CORE_LOAD_PCT[idx].load(Ordering::Relaxed),
        };
        idx += 1;
    }
    CpuSnapshot::from_cores(&cores[..count])
}

/// Store a core's latest load percentage (clamped to 0..=100).
pub fn set_core_load_pct(core_id: usize, load_pct: u8) {
    if let Some(slot) = CORE_LOAD_PCT.get(core_id) {
        slot.store(load_pct.min(100), Ordering::Relaxed);
    }
}

/// Background Embassy task sampling scheduling contention on `core_id`.
#[cfg(target_os = "none")]
#[embassy_executor::task(pool_size = 2)]
pub async fn sample_core_load(core_id: usize) {
    let mut tracker = CpuLoadTracker::new();
    let mut last = embassy_time::Instant::now();
    loop {
        embassy_time::Timer::after_micros(SAMPLE_TICK_US).await;
        let now = embassy_time::Instant::now();
        let elapsed_us = now.saturating_duration_since(last).as_micros();
        last = now;
        if let Some(pct) = tracker.record_tick(elapsed_us) {
            set_core_load_pct(core_id, pct);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_ticks_report_zero_percent() {
        let mut tracker = CpuLoadTracker::new();
        let mut last = None;
        for _ in 0..50 {
            last = tracker.record_tick(SAMPLE_TICK_US);
        }
        assert_eq!(last, Some(0));
    }

    #[test]
    fn fifty_percent_overshoot_reports_fifty_percent() {
        let mut tracker = CpuLoadTracker::new();
        let mut last = None;
        for _ in 0..25 {
            last = tracker.record_tick(SAMPLE_TICK_US * 2);
        }
        assert_eq!(last, Some(50));
    }

    #[test]
    fn saturated_tick_clamps_to_hundred_percent() {
        let mut tracker = CpuLoadTracker::new();
        assert_eq!(tracker.record_tick(2_000_000), Some(99));
        let mut tracker2 = CpuLoadTracker::new();
        assert_eq!(tracker2.record_tick(u64::MAX), Some(99));
        set_core_load_pct(0, 150);
        assert_eq!(cpu_info().cores()[0].load_pct, 100);
    }

    #[test]
    fn cpu_info_matches_target_core_count() {
        set_core_load_pct(0, 42);
        let snap = cpu_info();
        assert_eq!(snap.cores().len(), active_core_count());
        assert_eq!(snap.cores()[0], CpuCoreLoad { id: 0, load_pct: 42 });
    }
}
