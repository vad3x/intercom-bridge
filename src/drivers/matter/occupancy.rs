//! Smart Intercom - Occupancy Sensing Cluster Handler
//!
//! Exposes ring detection as a Motion Sensor in HomeKit via the
//! Matter OccupancySensing cluster.

use core::sync::atomic::{AtomicBool, Ordering};

use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::with;

use super::clusters_gen::occupancy_sensing::{
    self, OccupancyBitmap, OccupancySensorTypeBitmap, OccupancySensorTypeEnum,
};
use super::MatterCluster;

// ============================================================================
// Occupancy Sensing handler
// ============================================================================

/// Occupancy Sensing cluster handler — exposes ring detection as Motion Sensor in HomeKit.
///
/// Owns its occupancy state internally (no external atomics).
pub struct IntercomOccupancy {
    dataver: Dataver,
    occupancy: AtomicBool,
}

impl IntercomOccupancy {
    /// Create a new Occupancy Sensing handler.
    ///
    /// Occupancy starts as `false` (unoccupied).
    pub fn new() -> Self {
        Self {
            dataver: Dataver::new(2),
            occupancy: AtomicBool::new(false),
        }
    }

    /// Update the occupancy state.
    ///
    /// Called by `MatterAccessControlDriver` when ring is detected or hold timer expires.
    pub fn set_occupancy(&self, occupied: bool) {
        self.occupancy.store(occupied, Ordering::Relaxed);
    }
}

impl IntercomOccupancy {
    /// The subset of OccupancySensing we support.
    ///
    /// We expose the three mandatory attributes; no commands.
    pub const CLUSTER: Cluster<'static> = occupancy_sensing::FULL_CLUSTER
        .with_revision(5)
        .with_attrs(with!(
            occupancy_sensing::AttributeId::Occupancy
                | occupancy_sensing::AttributeId::OccupancySensorType
                | occupancy_sensing::AttributeId::OccupancySensorTypeBitmap
        ))
        .with_cmds(with!());
}

impl MatterCluster for IntercomOccupancy {
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    /// Occupancy Sensor device type (Matter spec 0x0107, rev 3)
    const DEVICE_TYPE: DeviceType = DeviceType {
        dtype: 0x0107,
        drev: 3,
    };
}

impl AsyncHandler for IntercomOccupancy {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        if let Some(writer) = reply.with_dataver(self.dataver.get())? {
            if ctx.attr().is_system() {
                ctx.attr().cluster()?.read(ctx.attr(), writer)
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
}
