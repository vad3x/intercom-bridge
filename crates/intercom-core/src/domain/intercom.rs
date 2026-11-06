//! Deterministic 1-hot half-duplex intercom bus and session state machine.

use crate::domain::policy::{IntercomTimingPolicy, RxPgaGain, TxDacGain, UnlockDuration};
use crate::ports::reporter::LockState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntercomStateMachine {
    policy: IntercomTimingPolicy,
    phase: BusPhase,
    bus_state: BusLineState,
    call_state: CallState,
    audio_direction: AudioDirection,
    ring_active: bool,
    ring_deadline_ms: Option<u64>,
}

impl IntercomStateMachine {
    pub const fn new() -> Self {
        Self::with_policy(IntercomTimingPolicy::new())
    }

    pub const fn with_policy(policy: IntercomTimingPolicy) -> Self {
        Self {
            policy,
            phase: BusPhase::Idle,
            bus_state: BusLineState::Idle,
            call_state: CallState::Idle,
            audio_direction: AudioDirection::Muted,
            ring_active: false,
            ring_deadline_ms: None,
        }
    }

    pub const fn with_duration(self, duration_sec: u32) -> Self {
        Self::with_policy(
            self.policy
                .with_unlock_duration(UnlockDuration::clamped(duration_sec)),
        )
    }

    pub const fn unlock_duration_sec(&self) -> u32 {
        self.policy.unlock_duration.as_secs()
    }

    pub const fn bus_state(&self) -> BusLineState {
        self.bus_state
    }

    pub const fn call_state(&self) -> CallState {
        self.call_state
    }

    pub const fn audio_direction(&self) -> AudioDirection {
        self.audio_direction
    }

    pub const fn active_owner(&self) -> Option<SessionOwner> {
        match self.phase {
            BusPhase::Idle => None,
            BusPhase::Listening { owner, .. } | BusPhase::Talking { owner, .. } => Some(owner),
            BusPhase::Unlocking { resume, .. } => resume,
            BusPhase::Settling { target, .. } => target.owner(),
        }
    }

    pub fn handle_action(&mut self, action: IntercomAction, now_ms: u64) -> IntercomEffects {
        match action {
            IntercomAction::SetAudioGain { rx, tx } => IntercomEffects {
                audio_gain: Some((rx, tx)),
                ..Default::default()
            },
            IntercomAction::Ring => self.on_ring(now_ms),
            IntercomAction::SetUnlockDuration(sec) => {
                let dur = UnlockDuration::clamped(sec);
                self.policy.unlock_duration = dur;
                IntercomEffects {
                    report_duration: Some(dur.as_secs()),
                    ..Default::default()
                }
            }
            IntercomAction::Unlock => self.on_unlock(now_ms),
            IntercomAction::StartListen(owner) => self.on_start_listen(owner, now_ms),
            IntercomAction::StartTalk(owner) => self.on_start_talk(owner, now_ms),
            IntercomAction::StopTalk(owner) => self.on_stop_talk(owner, now_ms),
            IntercomAction::SessionHeartbeat(owner) => self.on_heartbeat(owner, now_ms),
            IntercomAction::EndSession(owner) => self.on_end_session(owner),
            IntercomAction::EmergencyStop => self.on_emergency_stop(),
        }
    }

    pub fn poll_deadlines(&mut self, now_ms: u64) -> IntercomEffects {
        let mut fx = IntercomEffects::default();

        if let Some(deadline) = self.ring_deadline_ms {
            if now_ms >= deadline {
                self.ring_active = false;
                self.ring_deadline_ms = None;
                fx.report_ring = Some(false);
            }
        }

        match self.phase {
            BusPhase::Idle => {}
            BusPhase::Listening {
                owner,
                max_deadline_ms,
                heartbeat_deadline_ms,
            } => {
                // A Matter stream has its own ICE consent timeout; a live stream
                // must not be cut off by the local debug-listen duration cap.
                let max_duration_expired =
                    owner != SessionOwner::MatterCall && now_ms >= max_deadline_ms;
                if max_duration_expired || now_ms >= heartbeat_deadline_ms {
                    self.transition_to_idle(&mut fx);
                }
            }
            BusPhase::Talking {
                max_deadline_ms,
                heartbeat_deadline_ms,
                ..
            } => {
                if now_ms >= max_deadline_ms || now_ms >= heartbeat_deadline_ms {
                    self.transition_to_idle(&mut fx);
                }
            }
            BusPhase::Unlocking {
                relock_deadline_ms,
                resume,
            } => {
                if now_ms >= relock_deadline_ms {
                    fx.report_lock = Some(LockState::Locked);
                    if let Some(owner) = resume {
                        self.enter_settling(SettleTarget::Listen(owner), now_ms, &mut fx);
                    } else {
                        self.transition_to_idle(&mut fx);
                    }
                }
            }
            BusPhase::Settling {
                settle_deadline_ms,
                target,
            } => {
                if now_ms >= settle_deadline_ms {
                    self.complete_settling(target, now_ms, &mut fx);
                }
            }
        }

        fx
    }

    pub fn next_deadline(&self) -> Option<u64> {
        let phase_deadline = match self.phase {
            BusPhase::Idle => None,
            BusPhase::Listening {
                owner,
                max_deadline_ms,
                heartbeat_deadline_ms,
            } => Some(if owner == SessionOwner::MatterCall {
                heartbeat_deadline_ms
            } else {
                max_deadline_ms.min(heartbeat_deadline_ms)
            }),
            BusPhase::Talking {
                max_deadline_ms,
                heartbeat_deadline_ms,
                ..
            } => Some(max_deadline_ms.min(heartbeat_deadline_ms)),
            BusPhase::Unlocking {
                relock_deadline_ms, ..
            } => Some(relock_deadline_ms),
            BusPhase::Settling {
                settle_deadline_ms, ..
            } => Some(settle_deadline_ms),
        };

        match (self.ring_deadline_ms, phase_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum BusLineState {
    #[default]
    Idle = 0,
    Listen = 1,
    Talk = 2,
    DoorUnlock = 3,
}

impl BusLineState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listen => "listen",
            Self::Talk => "talk",
            Self::DoorUnlock => "door_unlock",
        }
    }

    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Listen,
            2 => Self::Talk,
            3 => Self::DoorUnlock,
            _ => Self::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum CallState {
    #[default]
    Idle = 0,
    Listening = 1,
    Talking = 2,
    Settling = 3,
    Unlocking = 4,
}

impl CallState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listening => "listening",
            Self::Talking => "talking",
            Self::Settling => "settling",
            Self::Unlocking => "unlocking",
        }
    }

    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Listening,
            2 => Self::Talking,
            3 => Self::Settling,
            4 => Self::Unlocking,
            _ => Self::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioDirection {
    #[default]
    Muted,
    CaptureRx,
    PlaybackTx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SessionOwner {
    BenchTone = 1,
    HttpDebug = 2,
    MatterCall = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntercomAction {
    Ring,
    Unlock,
    SetUnlockDuration(u32),
    SetAudioGain { rx: RxPgaGain, tx: TxDacGain },
    StartListen(SessionOwner),
    StartTalk(SessionOwner),
    StopTalk(SessionOwner),
    SessionHeartbeat(SessionOwner),
    EndSession(SessionOwner),
    EmergencyStop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntercomEffects {
    pub audio_direction: Option<AudioDirection>,
    pub audio_gain: Option<(RxPgaGain, TxDacGain)>,
    pub bus_state: Option<BusLineState>,
    pub report_lock: Option<LockState>,
    pub report_ring: Option<bool>,
    pub report_duration: Option<u32>,
    pub report_call_state: Option<CallState>,
    pub preempted_owner: Option<SessionOwner>,
}

impl IntercomStateMachine {
    fn on_ring(&mut self, now_ms: u64) -> IntercomEffects {
        self.ring_active = true;
        self.ring_deadline_ms = Some(now_ms + self.policy.ring_hold.as_millis());
        IntercomEffects {
            report_ring: Some(true),
            ..Default::default()
        }
    }

    fn on_unlock(&mut self, now_ms: u64) -> IntercomEffects {
        let mut fx = IntercomEffects::default();
        match self.phase {
            BusPhase::Idle => {
                let relock = now_ms + self.policy.unlock_duration.as_millis();
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms: relock,
                    resume: None,
                };
                self.set_bus(BusLineState::DoorUnlock, &mut fx);
                self.set_call(CallState::Unlocking, &mut fx);
                fx.report_lock = Some(LockState::Unlocked);
            }
            BusPhase::Unlocking { resume, .. } => {
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms: now_ms + self.policy.unlock_duration.as_millis(),
                    resume,
                };
                fx.report_lock = Some(LockState::Unlocked);
            }
            BusPhase::Listening { owner, .. } | BusPhase::Talking { owner, .. } => {
                fx.report_lock = Some(LockState::Unlocked);
                self.enter_settling(
                    SettleTarget::DoorUnlock {
                        resume: Some(owner),
                    },
                    now_ms,
                    &mut fx,
                );
            }
            BusPhase::Settling { target, .. } => {
                let resume = target.owner();
                fx.report_lock = Some(LockState::Unlocked);
                self.enter_settling(SettleTarget::DoorUnlock { resume }, now_ms, &mut fx);
            }
        }
        fx
    }

    fn on_start_listen(&mut self, owner: SessionOwner, now_ms: u64) -> IntercomEffects {
        let mut fx = IntercomEffects::default();
        if let Some(current) = self.active_owner() {
            if owner < current {
                return fx;
            }
            if owner > current {
                fx.preempted_owner = Some(current);
            }
        }

        match self.phase {
            BusPhase::Idle => {
                self.activate_listen(owner, now_ms, &mut fx);
            }
            BusPhase::Listening { .. } => {
                self.phase = BusPhase::Listening {
                    owner,
                    max_deadline_ms: now_ms + self.policy.max_listen_duration.as_millis(),
                    heartbeat_deadline_ms: now_ms + self.policy.heartbeat_timeout.as_millis(),
                };
            }
            BusPhase::Settling {
                settle_deadline_ms,
                target: SettleTarget::DoorUnlock { .. },
            } => {
                self.phase = BusPhase::Settling {
                    settle_deadline_ms,
                    target: SettleTarget::DoorUnlock {
                        resume: Some(owner),
                    },
                };
            }
            BusPhase::Talking { .. } | BusPhase::Settling { .. } => {
                self.enter_settling(SettleTarget::Listen(owner), now_ms, &mut fx);
            }
            BusPhase::Unlocking {
                relock_deadline_ms, ..
            } => {
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms,
                    resume: Some(owner),
                };
            }
        }
        fx
    }

    fn on_start_talk(&mut self, owner: SessionOwner, now_ms: u64) -> IntercomEffects {
        let mut fx = IntercomEffects::default();
        if let Some(current) = self.active_owner() {
            if owner < current {
                return fx;
            }
            if owner > current {
                fx.preempted_owner = Some(current);
            }
        }

        match self.phase {
            BusPhase::Idle => {
                self.activate_talk(owner, now_ms, &mut fx);
            }
            BusPhase::Talking {
                max_deadline_ms, ..
            } => {
                self.phase = BusPhase::Talking {
                    owner,
                    max_deadline_ms,
                    heartbeat_deadline_ms: now_ms + self.policy.heartbeat_timeout.as_millis(),
                };
            }
            BusPhase::Settling {
                settle_deadline_ms,
                target: SettleTarget::DoorUnlock { .. },
            } => {
                self.phase = BusPhase::Settling {
                    settle_deadline_ms,
                    target: SettleTarget::DoorUnlock {
                        resume: Some(owner),
                    },
                };
            }
            BusPhase::Listening { .. } | BusPhase::Settling { .. } => {
                self.enter_settling(SettleTarget::Talk(owner), now_ms, &mut fx);
            }
            BusPhase::Unlocking {
                relock_deadline_ms, ..
            } => {
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms,
                    resume: Some(owner),
                };
            }
        }
        fx
    }

    fn on_stop_talk(&mut self, owner: SessionOwner, now_ms: u64) -> IntercomEffects {
        let mut fx = IntercomEffects::default();
        if self.active_owner() != Some(owner) {
            return fx;
        }
        match self.phase {
            BusPhase::Talking { .. } => {
                self.enter_settling(SettleTarget::Listen(owner), now_ms, &mut fx);
            }
            BusPhase::Settling {
                settle_deadline_ms,
                target: SettleTarget::Talk(_),
            } => {
                self.phase = BusPhase::Settling {
                    settle_deadline_ms,
                    target: SettleTarget::Listen(owner),
                };
            }
            _ => {}
        }
        fx
    }

    fn on_heartbeat(&mut self, owner: SessionOwner, now_ms: u64) -> IntercomEffects {
        if self.active_owner() != Some(owner) {
            return IntercomEffects::default();
        }
        match &mut self.phase {
            BusPhase::Listening {
                heartbeat_deadline_ms,
                ..
            }
            | BusPhase::Talking {
                heartbeat_deadline_ms,
                ..
            } => {
                *heartbeat_deadline_ms = now_ms + self.policy.heartbeat_timeout.as_millis();
            }
            _ => {}
        }
        IntercomEffects::default()
    }

    fn on_end_session(&mut self, owner: SessionOwner) -> IntercomEffects {
        let mut fx = IntercomEffects::default();
        if self.active_owner() != Some(owner) {
            return fx;
        }
        match self.phase {
            BusPhase::Unlocking {
                relock_deadline_ms, ..
            } => {
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms,
                    resume: None,
                };
            }
            BusPhase::Settling {
                settle_deadline_ms,
                target: SettleTarget::DoorUnlock { .. },
            } => {
                self.phase = BusPhase::Settling {
                    settle_deadline_ms,
                    target: SettleTarget::DoorUnlock { resume: None },
                };
            }
            BusPhase::Listening { .. } | BusPhase::Talking { .. } | BusPhase::Settling { .. } => {
                self.transition_to_idle(&mut fx);
            }
            BusPhase::Idle => {}
        }
        fx
    }

    fn on_emergency_stop(&mut self) -> IntercomEffects {
        self.phase = BusPhase::Idle;
        self.bus_state = BusLineState::Idle;
        self.call_state = CallState::Idle;
        self.audio_direction = AudioDirection::Muted;
        self.ring_active = false;
        self.ring_deadline_ms = None;
        IntercomEffects {
            audio_direction: Some(AudioDirection::Muted),
            bus_state: Some(BusLineState::Idle),
            report_lock: Some(LockState::Locked),
            report_ring: Some(false),
            report_duration: None,
            report_call_state: Some(CallState::Idle),
            audio_gain: None,
            preempted_owner: None,
        }
    }

    fn enter_settling(&mut self, target: SettleTarget, now_ms: u64, fx: &mut IntercomEffects) {
        self.set_audio(AudioDirection::Muted, fx);
        self.set_bus(BusLineState::Idle, fx);
        self.set_call(CallState::Settling, fx);
        self.phase = BusPhase::Settling {
            settle_deadline_ms: now_ms + self.policy.settling_delay.as_millis(),
            target,
        };
    }

    fn complete_settling(&mut self, target: SettleTarget, now_ms: u64, fx: &mut IntercomEffects) {
        match target {
            SettleTarget::Listen(owner) => self.activate_listen(owner, now_ms, fx),
            SettleTarget::Talk(owner) => self.activate_talk(owner, now_ms, fx),
            SettleTarget::DoorUnlock { resume } => {
                self.phase = BusPhase::Unlocking {
                    relock_deadline_ms: now_ms + self.policy.unlock_duration.as_millis(),
                    resume,
                };
                self.set_bus(BusLineState::DoorUnlock, fx);
                self.set_call(CallState::Unlocking, fx);
            }
        }
    }

    fn activate_listen(&mut self, owner: SessionOwner, now_ms: u64, fx: &mut IntercomEffects) {
        self.phase = BusPhase::Listening {
            owner,
            max_deadline_ms: now_ms + self.policy.max_listen_duration.as_millis(),
            heartbeat_deadline_ms: now_ms + self.policy.heartbeat_timeout.as_millis(),
        };
        self.set_bus(BusLineState::Listen, fx);
        self.set_audio(AudioDirection::CaptureRx, fx);
        self.set_call(CallState::Listening, fx);
    }

    fn activate_talk(&mut self, owner: SessionOwner, now_ms: u64, fx: &mut IntercomEffects) {
        self.phase = BusPhase::Talking {
            owner,
            max_deadline_ms: now_ms + self.policy.max_talk_duration.as_millis(),
            heartbeat_deadline_ms: now_ms + self.policy.heartbeat_timeout.as_millis(),
        };
        self.set_bus(BusLineState::Talk, fx);
        self.set_audio(AudioDirection::PlaybackTx, fx);
        self.set_call(CallState::Talking, fx);
    }

    fn transition_to_idle(&mut self, fx: &mut IntercomEffects) {
        self.phase = BusPhase::Idle;
        self.set_audio(AudioDirection::Muted, fx);
        self.set_bus(BusLineState::Idle, fx);
        self.set_call(CallState::Idle, fx);
    }

    fn set_bus(&mut self, next: BusLineState, fx: &mut IntercomEffects) {
        if self.bus_state != next {
            self.bus_state = next;
            fx.bus_state = Some(next);
        }
    }

    fn set_audio(&mut self, next: AudioDirection, fx: &mut IntercomEffects) {
        if self.audio_direction != next {
            self.audio_direction = next;
            fx.audio_direction = Some(next);
        }
    }

    fn set_call(&mut self, next: CallState, fx: &mut IntercomEffects) {
        if self.call_state != next {
            self.call_state = next;
            fx.report_call_state = Some(next);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BusPhase {
    #[default]
    Idle,
    Listening {
        owner: SessionOwner,
        max_deadline_ms: u64,
        heartbeat_deadline_ms: u64,
    },
    Talking {
        owner: SessionOwner,
        max_deadline_ms: u64,
        heartbeat_deadline_ms: u64,
    },
    Unlocking {
        relock_deadline_ms: u64,
        resume: Option<SessionOwner>,
    },
    Settling {
        settle_deadline_ms: u64,
        target: SettleTarget,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettleTarget {
    Listen(SessionOwner),
    Talk(SessionOwner),
    DoorUnlock { resume: Option<SessionOwner> },
}

impl SettleTarget {
    const fn owner(self) -> Option<SessionOwner> {
        match self {
            Self::Listen(o) | Self::Talk(o) => Some(o),
            Self::DoorUnlock { resume } => resume,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_unlock_takes_zero_ms_fast_path_and_relocks_at_deadline() {
        // Arrange
        let mut sm = IntercomStateMachine::new().with_duration(5);

        // Act
        let unlock_fx = sm.handle_action(IntercomAction::Unlock, 1_000);
        let relock_fx = sm.poll_deadlines(6_000);

        // Assert
        assert_eq!(
            unlock_fx,
            IntercomEffects {
                audio_direction: None,
                bus_state: Some(BusLineState::DoorUnlock),
                report_lock: Some(LockState::Unlocked),
                report_ring: None,
                report_duration: None,
                report_call_state: Some(CallState::Unlocking),
                audio_gain: None,
                preempted_owner: None,
            }
        );
        assert_eq!(
            relock_fx,
            IntercomEffects {
                audio_direction: None,
                bus_state: Some(BusLineState::Idle),
                report_lock: Some(LockState::Locked),
                report_ring: None,
                report_duration: None,
                report_call_state: Some(CallState::Idle),
                audio_gain: None,
                preempted_owner: None,
            }
        );
    }

    #[test]
    fn listen_to_talk_enforces_30ms_break_before_make_and_pre_mute() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let listen_fx =
            sm.handle_action(IntercomAction::StartListen(SessionOwner::MatterCall), 100);

        // Act
        let settle_fx = sm.handle_action(IntercomAction::StartTalk(SessionOwner::MatterCall), 500);
        let early_fx = sm.poll_deadlines(529);
        let talk_fx = sm.poll_deadlines(530);

        // Assert
        assert_eq!(listen_fx.bus_state, Some(BusLineState::Listen));
        assert_eq!(listen_fx.audio_direction, Some(AudioDirection::CaptureRx));
        assert_eq!(
            settle_fx,
            IntercomEffects {
                audio_direction: Some(AudioDirection::Muted),
                bus_state: Some(BusLineState::Idle),
                report_lock: None,
                report_ring: None,
                report_duration: None,
                report_call_state: Some(CallState::Settling),
                audio_gain: None,
                preempted_owner: None,
            }
        );
        assert_eq!(early_fx, IntercomEffects::default());
        assert_eq!(talk_fx.bus_state, Some(BusLineState::Talk));
        assert_eq!(talk_fx.audio_direction, Some(AudioDirection::PlaybackTx));
        assert_eq!(talk_fx.report_call_state, Some(CallState::Talking));
    }

    #[test]
    fn unlock_preempts_active_talk_and_resumes_listen_after_relock() {
        // Arrange
        let mut sm = IntercomStateMachine::new().with_duration(4);
        sm.handle_action(IntercomAction::StartTalk(SessionOwner::MatterCall), 1_000);

        // Act
        let preempt_fx = sm.handle_action(IntercomAction::Unlock, 2_000);
        let strike_fx = sm.poll_deadlines(2_030);
        let relock_fx = sm.poll_deadlines(6_030);
        let resume_fx = sm.poll_deadlines(6_060);

        // Assert
        assert_eq!(preempt_fx.audio_direction, Some(AudioDirection::Muted));
        assert_eq!(preempt_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(preempt_fx.report_lock, Some(LockState::Unlocked));
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(strike_fx.report_call_state, Some(CallState::Unlocking));
        assert_eq!(relock_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(relock_fx.report_lock, Some(LockState::Locked));
        assert_eq!(relock_fx.report_call_state, Some(CallState::Settling));
        assert_eq!(resume_fx.bus_state, Some(BusLineState::Listen));
        assert_eq!(resume_fx.audio_direction, Some(AudioDirection::CaptureRx));
        assert_eq!(resume_fx.report_call_state, Some(CallState::Listening));
    }

    #[test]
    fn higher_priority_owner_preempts_http_debug_while_lower_priority_is_ignored() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartListen(SessionOwner::HttpDebug), 100);

        // Act
        let lower_ignored =
            sm.handle_action(IntercomAction::StartTalk(SessionOwner::BenchTone), 200);
        let matter_preempt =
            sm.handle_action(IntercomAction::StartListen(SessionOwner::MatterCall), 300);
        let http_after = sm.handle_action(IntercomAction::StartTalk(SessionOwner::HttpDebug), 350);

        // Assert
        assert_eq!(lower_ignored, IntercomEffects::default());
        assert_eq!(
            matter_preempt.preempted_owner,
            Some(SessionOwner::HttpDebug)
        );
        assert_eq!(sm.active_owner(), Some(SessionOwner::MatterCall));
        assert_eq!(http_after, IntercomEffects::default());
    }

    #[test]
    fn talk_thermal_cap_expires_despite_continuous_heartbeats() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartTalk(SessionOwner::HttpDebug), 0);

        // Act
        for t in (2_000..60_000).step_by(2_000) {
            sm.handle_action(IntercomAction::SessionHeartbeat(SessionOwner::HttpDebug), t);
            assert_eq!(sm.poll_deadlines(t), IntercomEffects::default());
        }
        let cutoff_fx = sm.poll_deadlines(60_000);

        // Assert
        assert_eq!(cutoff_fx.audio_direction, Some(AudioDirection::Muted));
        assert_eq!(cutoff_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(cutoff_fx.report_call_state, Some(CallState::Idle));
    }

    #[test]
    fn live_matter_listen_survives_listen_cap_while_heartbeats_continue() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let owner = SessionOwner::MatterCall;
        sm.handle_action(IntercomAction::StartListen(owner), 0);

        // Act
        for now_ms in (1_000..=190_000).step_by(1_000) {
            sm.handle_action(IntercomAction::SessionHeartbeat(owner), now_ms);
            assert_eq!(sm.poll_deadlines(now_ms), IntercomEffects::default());
        }

        // Assert
        assert_eq!(sm.bus_state(), BusLineState::Listen);
        assert_eq!(sm.call_state(), CallState::Listening);
        assert_eq!(sm.active_owner(), Some(owner));
        assert_eq!(sm.next_deadline(), Some(195_000));
    }

    #[test]
    fn missing_heartbeat_drops_session_to_idle_after_5_seconds() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartListen(SessionOwner::HttpDebug), 1_000);

        // Act
        let drop_fx = sm.poll_deadlines(6_000);

        // Assert
        assert_eq!(drop_fx.audio_direction, Some(AudioDirection::Muted));
        assert_eq!(drop_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(drop_fx.report_call_state, Some(CallState::Idle));
    }

    #[test]
    fn matter_call_still_expires_when_its_heartbeats_stop() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let owner = SessionOwner::MatterCall;
        sm.handle_action(IntercomAction::StartListen(owner), 1_000);

        // Act
        let drop_fx = sm.poll_deadlines(6_000);

        // Assert
        assert_eq!(drop_fx.audio_direction, Some(AudioDirection::Muted));
        assert_eq!(drop_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(drop_fx.report_call_state, Some(CallState::Idle));
        assert_eq!(sm.active_owner(), None);
    }

    #[test]
    fn ring_and_emergency_stop_behave_deterministically() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartTalk(SessionOwner::MatterCall), 100);
        let ring_fx = sm.handle_action(IntercomAction::Ring, 200);

        // Act
        let stop_fx = sm.handle_action(IntercomAction::EmergencyStop, 300);

        // Assert
        assert_eq!(ring_fx.report_ring, Some(true));
        assert_eq!(ring_fx.bus_state, None);
        assert_eq!(
            stop_fx,
            IntercomEffects {
                audio_direction: Some(AudioDirection::Muted),
                bus_state: Some(BusLineState::Idle),
                report_lock: Some(LockState::Locked),
                report_ring: Some(false),
                report_duration: None,
                report_call_state: Some(CallState::Idle),
                audio_gain: None,
                preempted_owner: None,
            }
        );
    }

    #[test]
    fn repeated_start_talk_cannot_extend_the_original_thermal_deadline() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let owner = SessionOwner::HttpDebug;
        sm.handle_action(IntercomAction::StartTalk(owner), 0);
        for now_ms in (2_000..60_000).step_by(2_000) {
            sm.handle_action(IntercomAction::SessionHeartbeat(owner), now_ms);
        }

        // Act
        sm.handle_action(IntercomAction::StartTalk(owner), 59_000);
        let cutoff_fx = sm.poll_deadlines(60_000);

        // Assert
        assert_eq!(cutoff_fx.audio_direction, Some(AudioDirection::Muted));
        assert_eq!(cutoff_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(cutoff_fx.report_call_state, Some(CallState::Idle));
        assert_eq!(sm.active_owner(), None);
    }

    #[test]
    fn start_talk_cannot_cancel_a_pending_door_unlock() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartTalk(SessionOwner::HttpDebug), 100);
        sm.handle_action(IntercomAction::Unlock, 500);

        // Act
        let start_fx = sm.handle_action(IntercomAction::StartTalk(SessionOwner::MatterCall), 510);
        let next_deadline = sm.next_deadline();
        let strike_fx = sm.poll_deadlines(530);

        // Assert
        assert_eq!(start_fx.preempted_owner, Some(SessionOwner::HttpDebug));
        assert_eq!(next_deadline, Some(530));
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(strike_fx.report_call_state, Some(CallState::Unlocking));
        assert_eq!(sm.audio_direction(), AudioDirection::Muted);
        assert_eq!(sm.active_owner(), Some(SessionOwner::MatterCall));
    }

    #[test]
    fn start_listen_cannot_cancel_a_pending_door_unlock() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        sm.handle_action(IntercomAction::StartTalk(SessionOwner::HttpDebug), 100);
        sm.handle_action(IntercomAction::Unlock, 500);

        // Act
        let start_fx = sm.handle_action(IntercomAction::StartListen(SessionOwner::MatterCall), 510);
        let next_deadline = sm.next_deadline();
        let strike_fx = sm.poll_deadlines(530);

        // Assert
        assert_eq!(start_fx.preempted_owner, Some(SessionOwner::HttpDebug));
        assert_eq!(next_deadline, Some(530));
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(strike_fx.report_call_state, Some(CallState::Unlocking));
        assert_eq!(sm.audio_direction(), AudioDirection::Muted);
        assert_eq!(sm.active_owner(), Some(SessionOwner::MatterCall));
    }

    #[test]
    fn ending_session_during_pre_unlock_settling_preserves_strike_without_resume() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let owner = SessionOwner::MatterCall;
        sm.handle_action(IntercomAction::StartTalk(owner), 100);
        sm.handle_action(IntercomAction::Unlock, 500);

        // Act
        let end_fx = sm.handle_action(IntercomAction::EndSession(owner), 510);
        let remaining_owner = sm.active_owner();
        let strike_fx = sm.poll_deadlines(530);
        let relock_fx = sm.poll_deadlines(5_530);

        // Assert
        assert_eq!(end_fx, IntercomEffects::default());
        assert_eq!(remaining_owner, None);
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(relock_fx.bus_state, Some(BusLineState::Idle));
        assert_eq!(relock_fx.report_lock, Some(LockState::Locked));
        assert_eq!(relock_fx.report_call_state, Some(CallState::Idle));
        assert_eq!(sm.next_deadline(), None);
    }

    #[test]
    fn pending_unlock_survives_start_talk_then_end_session() {
        // Arrange
        let mut sm = IntercomStateMachine::new().with_duration(4);
        let owner = SessionOwner::MatterCall;
        sm.handle_action(IntercomAction::StartTalk(owner), 1_000);
        sm.poll_deadlines(1_030);
        let unlock_fx = sm.handle_action(IntercomAction::Unlock, 2_000);

        // Act
        sm.handle_action(IntercomAction::StartTalk(owner), 2_015);
        sm.handle_action(IntercomAction::EndSession(owner), 2_020);
        let strike_fx = sm.poll_deadlines(2_030);

        // Assert
        assert_eq!(unlock_fx.report_lock, Some(LockState::Unlocked));
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(strike_fx.report_call_state, Some(CallState::Unlocking));
    }

    #[test]
    fn stop_talk_during_pre_unlock_settling_preserves_strike_and_resume_owner() {
        // Arrange
        let mut sm = IntercomStateMachine::new();
        let owner = SessionOwner::MatterCall;
        sm.handle_action(IntercomAction::StartTalk(owner), 100);
        sm.handle_action(IntercomAction::Unlock, 500);

        // Act
        let stop_fx = sm.handle_action(IntercomAction::StopTalk(owner), 510);
        let next_deadline = sm.next_deadline();
        let strike_fx = sm.poll_deadlines(530);

        // Assert
        assert_eq!(stop_fx, IntercomEffects::default());
        assert_eq!(next_deadline, Some(530));
        assert_eq!(strike_fx.bus_state, Some(BusLineState::DoorUnlock));
        assert_eq!(strike_fx.report_call_state, Some(CallState::Unlocking));
        assert_eq!(sm.active_owner(), Some(owner));
    }
}
