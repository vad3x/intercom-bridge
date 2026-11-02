//! Validated domain timing value objects for half-duplex intercom bus control.

use crate::config::{clamp_door_unlock_duration, DEFAULT_DOOR_UNLOCK_DURATION_SEC, RING_HOLD_MS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntercomTimingPolicy {
    pub unlock_duration: UnlockDuration,
    pub settling_delay: SettlingDelay,
    pub max_talk_duration: MaxTalkDuration,
    pub max_listen_duration: MaxListenDuration,
    pub heartbeat_timeout: SessionHeartbeatTimeout,
    pub ring_hold: RingHoldDuration,
}

impl IntercomTimingPolicy {
    pub const fn new() -> Self {
        Self {
            unlock_duration: UnlockDuration::default_duration(),
            settling_delay: SettlingDelay::new(DEFAULT_SETTLING_DELAY_MS),
            max_talk_duration: MaxTalkDuration::new(DEFAULT_MAX_TALK_DURATION_MS),
            max_listen_duration: MaxListenDuration::new(DEFAULT_MAX_LISTEN_DURATION_MS),
            heartbeat_timeout: SessionHeartbeatTimeout::new(DEFAULT_SESSION_HEARTBEAT_TIMEOUT_MS),
            ring_hold: RingHoldDuration::new(RING_HOLD_MS),
        }
    }

    pub const fn with_unlock_duration(mut self, duration: UnlockDuration) -> Self {
        self.unlock_duration = duration;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnlockDuration(u32);

impl UnlockDuration {
    pub const fn clamped(seconds: u32) -> Self {
        Self(clamp_door_unlock_duration(seconds))
    }

    pub const fn default_duration() -> Self {
        Self(DEFAULT_DOOR_UNLOCK_DURATION_SEC)
    }

    pub const fn as_secs(self) -> u32 {
        self.0
    }

    pub const fn as_millis(self) -> u64 {
        (self.0 as u64) * 1_000
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettlingDelay(u64);

impl SettlingDelay {
    pub const fn new(ms: u64) -> Self {
        Self(if ms == 0 { 1 } else { ms })
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxTalkDuration(u64);

impl MaxTalkDuration {
    pub const fn new(ms: u64) -> Self {
        Self(ms)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxListenDuration(u64);

impl MaxListenDuration {
    pub const fn new(ms: u64) -> Self {
        Self(ms)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionHeartbeatTimeout(u64);

impl SessionHeartbeatTimeout {
    pub const fn new(ms: u64) -> Self {
        Self(ms)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingHoldDuration(u64);

impl RingHoldDuration {
    pub const fn new(ms: u64) -> Self {
        Self(ms)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

pub const DEFAULT_SETTLING_DELAY_MS: u64 = 30;
pub const DEFAULT_MAX_TALK_DURATION_MS: u64 = 60_000;
pub const DEFAULT_MAX_LISTEN_DURATION_MS: u64 = 180_000;
pub const DEFAULT_SESSION_HEARTBEAT_TIMEOUT_MS: u64 = 5_000;

impl Default for IntercomTimingPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for UnlockDuration {
    fn default() -> Self {
        Self::default_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_duration_clamps_to_domain_bounds() {
        // Arrange
        let low = UnlockDuration::clamped(0);
        let mid = UnlockDuration::clamped(7);
        let high = UnlockDuration::clamped(99);

        // Act
        let actual = (low.as_secs(), mid.as_millis(), high.as_secs());

        // Assert
        assert_eq!(actual, (1, 7_000, 30));
    }
}
