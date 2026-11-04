//! Endpoint 1 Momentary Switch cluster handler (0x003B) for Audio Doorbell ring press events.

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use rs_matter::dm::clusters::decl::switch;
use rs_matter::dm::devices::DEV_TYPE_AUDIO_DOORBELL;
use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, HandlerContext, InvokeContext, InvokeReply,
    MatchContext, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::with;

use crate::config::AUDIO_DOORBELL_ENDPOINT_ID;
use crate::{ClusterReadinessBarrier, MatterCluster};

const PENDING_INITIAL_PRESS: u8 = 1 << 0;
const PENDING_SHORT_RELEASE: u8 = 1 << 1;

pub struct IntercomDoorbellSwitch {
    readiness: &'static ClusterReadinessBarrier,
    dataver: Dataver,
    current_position: AtomicU8,
    pending_events: AtomicU8,
    state_changed: Signal<NoopRawMutex, ()>,
}

impl IntercomDoorbellSwitch {
    pub const CLUSTER: Cluster<'static> = switch::FULL_CLUSTER
        .with_revision(2)
        .with_features(
            switch::Feature::MOMENTARY_SWITCH.bits()
                | switch::Feature::MOMENTARY_SWITCH_RELEASE.bits(),
        )
        .with_attrs(with!(system;
            switch::AttributeId::NumberOfPositions | switch::AttributeId::CurrentPosition
        ))
        .with_cmds(with!())
        .with_events(with!(
            switch::EventId::InitialPress | switch::EventId::ShortRelease
        ));

    pub const fn new(readiness: &'static ClusterReadinessBarrier) -> Self {
        Self {
            readiness,
            dataver: Dataver::new(10),
            current_position: AtomicU8::new(0),
            pending_events: AtomicU8::new(0),
            state_changed: Signal::new(),
        }
    }

    pub fn set_ringing(&self, ringing: bool) {
        let next = if ringing { 1 } else { 0 };
        let prev = self.current_position.swap(next, Ordering::AcqRel);
        if prev != next {
            let bit = if ringing {
                PENDING_INITIAL_PRESS
            } else {
                PENDING_SHORT_RELEASE
            };
            self.pending_events.fetch_or(bit, Ordering::AcqRel);
            self.dataver.changed();
            self.state_changed.signal(());
        }
    }

    pub fn take_pending_events(&self) -> u8 {
        self.pending_events.swap(0, Ordering::AcqRel)
    }
}

impl MatterCluster for IntercomDoorbellSwitch {
    const ENDPOINT_ID: u16 = AUDIO_DOORBELL_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    const DEVICE_TYPE: DeviceType = DEV_TYPE_AUDIO_DOORBELL;
}

impl AsyncHandler for IntercomDoorbellSwitch {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let fab_idx = ctx
            .accessor()
            .ok()
            .and_then(|a| a.fab_idx().ok().map(|idx| idx.get()));
        self.readiness.mark_ring_ready(fab_idx);
        if let Some(writer) = reply.with_dataver(self.dataver.get())? {
            if ctx.attr().is_system() {
                Self::CLUSTER.read(ctx.attr(), writer)
            } else {
                match ctx.attr().attr_id as u16 {
                    x if x == switch::AttributeId::NumberOfPositions as u16 => {
                        Reply::set(writer, 2u8)
                    }
                    x if x == switch::AttributeId::CurrentPosition as u16 => {
                        Reply::set(writer, self.current_position.load(Ordering::Acquire))
                    }
                    _ => Err(ErrorCode::AttributeNotFound.into()),
                }
            }
        } else {
            Ok(())
        }
    }

    async fn write(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        Err(ErrorCode::InvalidAction.into())
    }

    async fn invoke(
        &self,
        _ctx: impl InvokeContext,
        _reply: impl InvokeReply,
    ) -> Result<(), Error> {
        Err(ErrorCode::InvalidCommand.into())
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        if ctx.endpt() == Some(Self::ENDPOINT_ID) && ctx.cluster() == Some(Self::CLUSTER.id) {
            self.dataver.changed();
        }
    }

    async fn run(&self, ctx: impl HandlerContext) -> Result<(), Error> {
        loop {
            self.state_changed.wait().await;
            let events = self.take_pending_events();
            if (events & PENDING_INITIAL_PRESS) != 0 {
                let _ = switch::InitialPress::emit_for(&ctx, AUDIO_DOORBELL_ENDPOINT_ID, |b| {
                    b.new_position(1)?.end()
                });
            }
            if (events & PENDING_SHORT_RELEASE) != 0 {
                let _ = switch::ShortRelease::emit_for(&ctx, AUDIO_DOORBELL_ENDPOINT_ID, |b| {
                    b.previous_position(1)?.end()
                });
            }
            ctx.notify_attr_changed(
                AUDIO_DOORBELL_ENDPOINT_ID,
                Self::CLUSTER.id,
                switch::AttributeId::CurrentPosition as _,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intercom_core::events::{SystemStateSender, SystemStateWatch};

    #[test]
    fn switch_cluster_exposes_ms_msr_features_events_and_system_attrs() {
        // Arrange
        let cluster = IntercomDoorbellSwitch::CLUSTER;

        // Act
        let has_feature_map = cluster.attribute(0xFFFC).is_some();
        let has_initial_press = cluster.event(switch::EventId::InitialPress as _).is_some();
        let has_short_release = cluster.event(switch::EventId::ShortRelease as _).is_some();

        // Assert
        assert!(has_feature_map);
        assert!(has_initial_press);
        assert!(has_short_release);
        assert_eq!(
            cluster.feature_map,
            (switch::Feature::MOMENTARY_SWITCH | switch::Feature::MOMENTARY_SWITCH_RELEASE).bits()
        );
    }

    #[test]
    fn rapid_press_and_release_latches_both_events_without_overwriting() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        static BARRIER: ClusterReadinessBarrier =
            ClusterReadinessBarrier::new(SystemStateSender::new(&WATCH));
        let switch = IntercomDoorbellSwitch::new(&BARRIER);

        // Act
        switch.set_ringing(true);
        switch.set_ringing(false);
        let latched = switch.take_pending_events();

        // Assert
        assert_eq!(latched, PENDING_INITIAL_PRESS | PENDING_SHORT_RELEASE);
        assert_eq!(switch.take_pending_events(), 0);
    }
    #[test]
    fn unchanged_ring_state_does_not_latch_events() {
        // Arrange
        static WATCH: SystemStateWatch = SystemStateWatch::new();
        static BARRIER: ClusterReadinessBarrier =
            ClusterReadinessBarrier::new(SystemStateSender::new(&WATCH));
        let switch = IntercomDoorbellSwitch::new(&BARRIER);

        // Act
        switch.set_ringing(false);
        let idle = switch.take_pending_events();
        switch.set_ringing(true);
        let press = switch.take_pending_events();
        switch.set_ringing(true);
        let repeated = switch.take_pending_events();

        // Assert
        assert_eq!(idle, 0);
        assert_eq!(press, PENDING_INITIAL_PRESS);
        assert_eq!(repeated, 0);
    }
}
