//! Smart Intercom - Matter Access Control Driver
//!
//! Encapsulates all rs-matter SDK details behind a clean interface,
//! mirroring the C++ `MatterAccessControlDriver`.
//!
//! Provides:
//! - Door Lock cluster (endpoint 1) — handles lock/unlock from HomeKit
//! - Occupancy Sensor cluster (endpoint 2) — exposes ring as motion sensor
//! - `MatterController` — builder+runner for the full Matter-over-WiFi stack

#![no_std]
#![allow(async_fn_in_trait)]

extern crate alloc;

mod access_control;
pub mod commissioning;
pub mod config;
mod controller;
mod door_lock;
mod occupancy;
pub mod readiness;

use rs_matter::dm::{AsyncHandler, Cluster, DeviceType};

pub use access_control::MatterAccessControlDriver;
pub use commissioning::{
    derive_intercom_identity, DeviceIdentity, IdentityError, HOSTNAME_PREFIX, SERIAL_PREFIX,
};
pub use config::{DOOR_LOCK_ENDPOINT_ID, MATTER_ECOSYSTEM, OCCUPANCY_SENSOR_ENDPOINT_ID};
pub use controller::{DeviceInfo, MatterController, MatterControllerConfig, MatterPeripherals};
pub use door_lock::{IntercomDoorLock, IntercomDoorLockConfig};
pub use occupancy::{IntercomOccupancy, IntercomOccupancyConfig};
pub use readiness::ClusterReadinessBarrier;

/// Bump allocator size for the Matter protocol stack.
///
/// The rs-matter-embassy example uses 17500–18000 for a single cluster.
/// We register two clusters (DoorLock + Occupancy) each with a descriptor
/// handler, which consumes more bump memory during init. After init the
/// allocator also needs headroom for CASE sessions, subscription reports,
/// multi-fabric commissioning, and mDNS service records.
const BUMP_SIZE: usize = 56000;

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
