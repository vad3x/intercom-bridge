use intercom_core::ports::ClockSyncDriver;
use rs_matter::dm::clusters::time_sync::UtcTime;
use rs_matter::Matter;

/// Polls Matter's RTC without exposing its state or synchronization primitives outside this adapter.
pub(super) fn poll_time_sync(matter: &Matter<'_>, clock: &dyn ClockSyncDriver) {
    let utc_time = matter.with_state(|state| state.rtc.utc_time());
    if let Some(unix_ms) = reliable_matter_time_to_unix_ms(utc_time) {
        clock.set_epoch_time(unix_ms);
    }
}

/// Converts reliable Matter-epoch microseconds to Unix epoch milliseconds.
fn reliable_matter_time_to_unix_ms(utc_time: UtcTime) -> Option<u64> {
    match utc_time {
        UtcTime::Reliable(matter_us) => Some(
            (matter_us / 1_000)
                .saturating_add(rs_matter::utils::epoch::MATTER_EPOCH_SECS.saturating_mul(1_000)),
        ),
        UtcTime::LastKnown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::reliable_matter_time_to_unix_ms;
    use rs_matter::dm::clusters::time_sync::UtcTime;

    #[test]
    fn converts_reliable_matter_time_and_ignores_last_known_time() {
        assert_eq!(
            reliable_matter_time_to_unix_ms(UtcTime::Reliable(0)),
            Some(946_684_800_000)
        );
        assert_eq!(
            reliable_matter_time_to_unix_ms(UtcTime::LastKnown(1_000_000)),
            None
        );
    }
}
