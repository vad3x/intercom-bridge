/// Outbound port for propagating synchronized Unix epoch time to the system clock.
pub trait ClockSyncDriver: Send + Sync {
    /// Sets the current Unix epoch time in milliseconds.
    fn set_epoch_time(&self, unix_ms: u64);
}
