//! Smart Intercom - Matter Access Control Driver
//!
//! Encapsulates all rs-matter SDK details behind a clean interface,
//! mirroring the C++ `MatterAccessControlDriver`.
//!
//! Provides:
//! - Door Lock cluster (endpoint 1) — handles lock/unlock from HomeKit
//! - Occupancy Sensor cluster (endpoint 2) — exposes ring as motion sensor
//! - `MatterController` — builder+runner for the full Matter-over-WiFi stack
//!
//! ## Module layout
//!
//! - `door_lock`   — Door Lock cluster handler (`IntercomDoorLock`)
//! - `occupancy`   — Occupancy Sensing cluster handler (`IntercomOccupancy`)
//! - `controller`  — WiFi Matter controller (`MatterController`)

mod access_control;
mod controller;
mod door_lock;
mod occupancy;

use rs_matter::dm::{AsyncHandler, Cluster, DeviceType};

pub use access_control::MatterAccessControlDriver;
pub use controller::{DeviceInfo, MatterController, MatterControllerConfig, MatterPeripherals};
pub use door_lock::IntercomDoorLock;
pub use occupancy::IntercomOccupancy;

/// Bump allocator size for the Matter protocol stack.
///
/// The rs-matter-embassy example uses 16500 for a single cluster (On-Off).
/// We register two clusters (DoorLock + Occupancy) each with a descriptor
/// handler, which consumes more bump memory during init. After init the
/// allocator also needs headroom for CASE sessions and subscription reports.
/// 20000 gives ~5KB of headroom over observed init usage (~15.6KB).
const BUMP_SIZE: usize = 20000;

// ============================================================================
// Import cluster declarations from Matter spec
// ============================================================================

/// Generate DoorLock and OccupancySensing cluster declarations from the Matter IDL spec.
/// This creates modules `door_lock` and `occupancy_sensing` with:
/// - `FULL_CLUSTER` constant, `AttributeId`, `CommandId` enums
/// - Type enums (DlLockState, OccupancyBitmap, etc.)
pub(super) mod clusters_gen {
    rs_matter::import!(DoorLock, OccupancySensing);
}

// ============================================================================
// MatterCluster trait
// ============================================================================

/// Trait for Matter cluster handlers that can be registered with [`MatterController`].
///
/// Bridges the gap between your cluster handler and `MatterController::register()`.
/// Provides cluster metadata and the device type so `register()` can build
/// both the handler chain **and** the node descriptor automatically,
/// without leaking rs-matter types into `main`.
pub trait MatterCluster: AsyncHandler {
    /// The Matter endpoint ID this cluster is registered on.
    ///
    /// Used by [`MatterController::register()`] to wire the handler chain
    /// and build the node descriptor automatically.
    const ENDPOINT_ID: u16;

    /// The cluster definition (attributes, commands, revision).
    ///
    /// Used for the endpoint's cluster list in the node descriptor
    /// and to derive the cluster ID for handler matching.
    const CLUSTER_DEF: Cluster<'static>;

    /// The Matter device type for the endpoint this cluster belongs to.
    const DEVICE_TYPE: DeviceType;
}
