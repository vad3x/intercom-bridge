//! Smart Intercom - Matter Access Control Driver
//!
//! Encapsulates all rs-matter SDK details behind a clean interface,
//! mirroring the C++ `MatterAccessControlDriver`.
//!
//! Provides:
//! - Door Lock cluster (endpoint 2) — handles lock/unlock from HomeKit
//! - Momentary Switch and Chime clusters (endpoint 1) — expose doorbell ring and chime
//! - `MatterController` — builder+runner for the full Matter-over-WiFi stack

#![no_std]
#![allow(async_fn_in_trait)]

extern crate alloc;

mod access_control;
mod audio_doorbell;
mod chime_handler;
pub mod commissioning;
pub mod config;
mod controller;
mod door_lock;
mod doorbell_switch;
pub mod readiness;

use rs_matter::dm::{AsyncHandler, Cluster, DeviceType};

pub use access_control::MatterAccessControlDriver;
pub use audio_doorbell::{
    ActiveCallGate, IntercomAudioAvCluster, IntercomAudioHooks, IntercomWebRtcHooks,
    IntercomWebRtcProvCluster, MediaSessionPort,
};
pub use chime_handler::IntercomChime;
pub use commissioning::{
    derive_intercom_identity, DeviceIdentity, IdentityError, HOSTNAME_PREFIX, SERIAL_PREFIX,
};
pub use config::{AUDIO_DOORBELL_ENDPOINT_ID, DOOR_LOCK_ENDPOINT_ID, MATTER_ECOSYSTEM};
pub use controller::{DeviceInfo, MatterController, MatterControllerConfig, MatterPeripherals};
pub use door_lock::{IntercomDoorLock, IntercomDoorLockConfig};
pub use doorbell_switch::IntercomDoorbellSwitch;
pub use readiness::ClusterReadinessBarrier;
pub use rs_matter::dm::clusters::app::webrtc_prov::WebRtcError;

/// Bump allocator size for the Matter 1.5 Audio Doorbell + Door Lock protocol stack.
///
/// Sized for Endpoint 0 + Endpoint 1 (`Switch`, `AudioAvStreamHandler`,
/// `WebRtcProvHandler<..., 3, 2048, 1024, 128, 4>`, `Chime`) + Endpoint 2 (`DoorLock`),
/// multi-fabric CASE sessions, event reports, and mDNS service records.
const BUMP_SIZE: usize = 72_000;

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

    /// Optional client cluster IDs advertised in `Descriptor::ClientList` on `ENDPOINT_ID`
    /// (e.g. `WebRTCTransportRequestor` `0x0554` on Endpoint 1).
    const CLIENT_CLUSTERS: &'static [rs_matter::dm::ClusterId] = &[];
}
