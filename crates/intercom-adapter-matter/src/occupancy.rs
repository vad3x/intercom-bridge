//! Smart Intercom - Occupancy Sensing Cluster Handler
//!
//! Exposes ring detection as a Motion Sensor in HomeKit via the
//! Matter OccupancySensing cluster.
//!
//! Uses `notify_attribute_changed()` to wake the Matter subscription system
//! so HomeKit receives prompt occupancy updates (without it, ring detection
//! wouldn't show in the app until the periodic subscription timer fires).

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use log::info;

use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, HandlerContext, MatchContext, ReadContext,
    ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::with;

use rs_matter::dm::clusters::decl::occupancy_sensing::{
    self, OccupancyBitmap, OccupancySensorTypeBitmap, OccupancySensorTypeEnum,
};
use super::readiness::ClusterReadinessBarrier;
use super::MatterCluster;

// ============================================================================
// Occupancy Sensing handler
// ============================================================================

/// Occupancy Sensing cluster handler — exposes ring detection as Motion Sensor in HomeKit.
///
/// Owns its occupancy state internally (no external atomics).
/// Coordinates cluster subscription readiness via `ClusterReadinessBarrier`.
///
/// Out-of-band state changes (via `set_occupancy()`) signal the internal
/// `state_changed` signal, which wakes the `run()` background task.
/// That task then calls `ctx.notify_attr_changed()` to push a
/// subscription report to HomeKit.
pub struct IntercomOccupancy {
    config: IntercomOccupancyConfig,
    dataver: Dataver,
    occupancy: AtomicBool,
    /// Signal used to wake the `run()` background task when occupancy changes
    /// and a subscription report needs to be pushed to HomeKit.
    state_changed: Signal<NoopRawMutex, ()>,
}

impl IntercomOccupancy {
    /// Create a new Occupancy Sensing handler.
    ///
    /// Occupancy starts as `false` (unoccupied).
    pub fn new(config: IntercomOccupancyConfig) -> Self {
        Self {
            config,
            dataver: Dataver::new(2),
            occupancy: AtomicBool::new(false),
            state_changed: Signal::new(),
        }
    }

    /// Update the occupancy state.
    ///
    /// Called by `MatterAccessControlDriver` when ring is detected or hold timer expires.
    /// Bumps dataver and signals the `run()` background task to push
    /// a subscription report to HomeKit immediately.
    pub fn set_occupancy(&self, occupied: bool) {
        info!("set_occupancy({occupied}) → signalling subscription notify");
        self.occupancy.store(occupied, Ordering::Relaxed);
        self.dataver.changed();
        self.state_changed.signal(());
    }

    /// The subset of OccupancySensing we support.
    ///
    /// We expose the three mandatory attributes and advertise PIR sensing capabilities; no commands.
    pub const CLUSTER: Cluster<'static> = occupancy_sensing::FULL_CLUSTER
        .with_revision(5)
        .with_features(occupancy_sensing::Feature::PASSIVE_INFRARED.bits())
        .with_attrs(with!(system;
            occupancy_sensing::AttributeId::Occupancy
                | occupancy_sensing::AttributeId::OccupancySensorType
                | occupancy_sensing::AttributeId::OccupancySensorTypeBitmap
        ))
        .with_cmds(with!());
}

/// Configuration for `IntercomOccupancy`.
pub struct IntercomOccupancyConfig {
    /// Cluster readiness barrier to signal Apple Home subscription priming.
    pub readiness: &'static ClusterReadinessBarrier,
}

impl MatterCluster for IntercomOccupancy {
    const ENDPOINT_ID: u16 = crate::config::OCCUPANCY_SENSOR_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    /// Occupancy Sensor device type (Matter spec 0x0107, rev 4)
    const DEVICE_TYPE: DeviceType = DeviceType {
        dtype: 0x0107,
        drev: 4,
    };
}

impl AsyncHandler for IntercomOccupancy {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.config.readiness.mark_ring_ready();

        if let Some(writer) = reply.with_dataver(self.dataver.get())? {
            if ctx.attr().is_system() {
                Self::CLUSTER.read(ctx.attr(), writer)
            } else {
                match ctx.attr().attr_id as u16 {
                    // Occupancy (id=0): OccupancyBitmap
                    x if x == occupancy_sensing::AttributeId::Occupancy as u16 => {
                        let value = if self.occupancy.load(Ordering::Relaxed) {
                            OccupancyBitmap::OCCUPIED
                        } else {
                            OccupancyBitmap::empty()
                        };
                        Reply::set(writer, value)
                    }
                    // OccupancySensorType (id=1): OccupancySensorTypeEnum
                    x if x == occupancy_sensing::AttributeId::OccupancySensorType as u16 => {
                        Reply::set(writer, OccupancySensorTypeEnum::PIR)
                    }
                    // OccupancySensorTypeBitmap (id=2): OccupancySensorTypeBitmap
                    x if x == occupancy_sensing::AttributeId::OccupancySensorTypeBitmap as u16 => {
                        Reply::set(writer, OccupancySensorTypeBitmap::PIR)
                    }
                    _ => Err(ErrorCode::AttributeNotFound.into()),
                }
            }
        } else {
            Ok(())
        }
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        ctx.attr().check_dataver(self.dataver.get())?;
        // None of our OccupancySensing attributes are writable
        Err(ErrorCode::AttributeNotFound.into())
    }

    // OccupancySensing has no commands

    fn bump_dataver(&self, ctx: impl MatchContext) {
        if ctx.endpt() == Some(Self::ENDPOINT_ID) && ctx.cluster() == Some(Self::CLUSTER.id) {
            self.dataver.changed();
        }
    }

    fn run(
        &self,
        ctx: impl HandlerContext,
    ) -> impl core::future::Future<Output = Result<(), Error>> {
        async move {
            loop {
                self.state_changed.wait().await;
                info!("run() woke → notifying subscription system");
                ctx.notify_attr_changed(
                    Self::ENDPOINT_ID,
                    Self::CLUSTER.id,
                    occupancy_sensing::AttributeId::Occupancy as _,
                );
            }
        }
    }
}
