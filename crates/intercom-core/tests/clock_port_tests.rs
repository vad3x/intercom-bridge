use intercom_core::ports::ClockSyncDriver;
use std::sync::atomic::{AtomicU64, Ordering};

struct FakeClockSyncDriver {
    last_epoch_ms: AtomicU64,
}

impl ClockSyncDriver for FakeClockSyncDriver {
    fn set_epoch_time(&self, unix_ms: u64) {
        self.last_epoch_ms.store(unix_ms, Ordering::Relaxed);
    }
}

#[test]
fn clock_sync_port_accepts_epoch_milliseconds() {
    let driver = FakeClockSyncDriver {
        last_epoch_ms: AtomicU64::new(0),
    };

    driver.set_epoch_time(1_793_730_600_123);

    assert_eq!(
        driver.last_epoch_ms.load(Ordering::Relaxed),
        1_793_730_600_123
    );
}
