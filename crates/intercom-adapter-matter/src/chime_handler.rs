//! Endpoint 1 Chime cluster handler (0x0556) for Matter 1.5 Audio Doorbell conformance.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use rs_matter::dm::clusters::decl::chime::{
    self, ChimeSoundStructArrayBuilder, ChimeSoundStructBuilder, ClusterAsyncHandler,
    HandlerAsyncAdaptor, PlayChimeSoundRequest,
};
use rs_matter::dm::devices::DEV_TYPE_AUDIO_DOORBELL;
use rs_matter::dm::{
    ArrayAttributeRead, AsyncHandler, Cluster, Dataver, DeviceType, HandlerContext, InvokeContext,
    InvokeReply, LifecycleOp, MatchContext, ReadContext, ReadReply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVBuilderParent;
use rs_matter::with;

use crate::config::AUDIO_DOORBELL_ENDPOINT_ID;
use crate::MatterCluster;

const DEFAULT_CHIME_ID: u8 = 0;
const DEFAULT_CHIME_NAME: &str = "Intercom Buzzer";

pub struct IntercomChime(HandlerAsyncAdaptor<IntercomChimeInner>);

impl IntercomChime {
    pub const CLUSTER: Cluster<'static> = IntercomChimeInner::CLUSTER;

    pub const fn new() -> Self {
        Self(HandlerAsyncAdaptor(IntercomChimeInner::new()))
    }

    pub fn is_enabled(&self) -> bool {
        self.0 .0.enabled.load(Ordering::Acquire)
    }

    pub fn selected_chime_id(&self) -> u8 {
        self.0 .0.selected_chime.load(Ordering::Acquire)
    }
}

impl Default for IntercomChime {
    fn default() -> Self {
        Self::new()
    }
}

impl MatterCluster for IntercomChime {
    const ENDPOINT_ID: u16 = AUDIO_DOORBELL_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    const DEVICE_TYPE: DeviceType = DEV_TYPE_AUDIO_DOORBELL;
}

impl AsyncHandler for IntercomChime {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.0.read(ctx, reply).await
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.0.write(ctx).await
    }

    async fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.0.invoke(ctx, reply).await
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        self.0.bump_dataver(ctx);
    }

    fn lifecycle(&self, ctx: impl HandlerContext, op: LifecycleOp) -> Result<(), Error> {
        self.0.lifecycle(ctx, op)
    }

    fn run(
        &self,
        ctx: impl HandlerContext,
    ) -> impl core::future::Future<Output = Result<(), Error>> {
        self.0.run(ctx)
    }
}

struct IntercomChimeInner {
    dataver: Dataver,
    selected_chime: AtomicU8,
    enabled: AtomicBool,
}

impl IntercomChimeInner {
    const CLUSTER: Cluster<'static> = chime::FULL_CLUSTER
        .with_revision(1)
        .with_attrs(with!(system;
            chime::AttributeId::InstalledChimeSounds
                | chime::AttributeId::SelectedChime
                | chime::AttributeId::Enabled
        ))
        .with_cmds(with!(chime::CommandId::PlayChimeSound));

    const fn new() -> Self {
        Self {
            dataver: Dataver::new(20),
            selected_chime: AtomicU8::new(DEFAULT_CHIME_ID),
            enabled: AtomicBool::new(true),
        }
    }
}

impl ClusterAsyncHandler for IntercomChimeInner {
    const CLUSTER: Cluster<'static> = Self::CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    async fn installed_chime_sounds<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: ArrayAttributeRead<ChimeSoundStructArrayBuilder<P>, ChimeSoundStructBuilder<P>>,
    ) -> Result<P, Error> {
        match builder {
            ArrayAttributeRead::ReadAll(b) => b
                .push()?
                .chime_id(DEFAULT_CHIME_ID)?
                .name(DEFAULT_CHIME_NAME)?
                .end()?
                .end(),
            ArrayAttributeRead::ReadOne(0, b) => b
                .chime_id(DEFAULT_CHIME_ID)?
                .name(DEFAULT_CHIME_NAME)?
                .end(),
            ArrayAttributeRead::ReadOne(_, _) => Err(ErrorCode::ConstraintError.into()),
            ArrayAttributeRead::ReadNone(b) => b.end(),
        }
    }

    async fn selected_chime(&self, _ctx: impl ReadContext) -> Result<u8, Error> {
        Ok(self.selected_chime.load(Ordering::Acquire))
    }

    async fn enabled(&self, _ctx: impl ReadContext) -> Result<bool, Error> {
        Ok(self.enabled.load(Ordering::Acquire))
    }

    async fn set_selected_chime(&self, ctx: impl WriteContext, value: u8) -> Result<(), Error> {
        if value != DEFAULT_CHIME_ID {
            return Err(ErrorCode::NotFound.into());
        }
        self.selected_chime.store(value, Ordering::Release);
        self.dataver.changed();
        ctx.notify_changed();
        Ok(())
    }

    async fn set_enabled(&self, ctx: impl WriteContext, value: bool) -> Result<(), Error> {
        self.enabled.store(value, Ordering::Release);
        self.dataver.changed();
        ctx.notify_changed();
        Ok(())
    }

    async fn handle_play_chime_sound(
        &self,
        _ctx: impl InvokeContext,
        request: PlayChimeSoundRequest<'_>,
    ) -> Result<(), Error> {
        if let Some(id) = request.chime_id()? {
            if id != DEFAULT_CHIME_ID {
                return Err(ErrorCode::NotFound.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chime_cluster_preserves_system_attributes_and_defaults_enabled() {
        // Arrange
        let chime = IntercomChime::new();
        let cluster = IntercomChime::CLUSTER;

        // Act
        let has_feature_map = cluster.attribute(0xFFFC).is_some();
        let has_play_cmd = cluster
            .command(chime::CommandId::PlayChimeSound as _)
            .is_some();

        // Assert
        assert!(has_feature_map);
        assert!(has_play_cmd);
        assert!(chime.is_enabled());
        assert_eq!(chime.selected_chime_id(), 0);
    }
}
