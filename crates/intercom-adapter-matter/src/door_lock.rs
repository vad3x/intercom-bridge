//! Smart Intercom - Matter Door Lock Cluster Handler
//!
//! Exposes a Matter DoorLock cluster on Endpoint 1 (HomeKit lock accessory).
//! Handles lock/unlock commands, pushes subscription reports when state changes,
//! and notifies readiness via the `ClusterReadinessBarrier`.

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use log::info;

use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, HandlerContext, InvokeContext, InvokeReply,
    MatchContext, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;
use rs_matter::with;

use rs_matter::dm::clusters::decl::door_lock::{
    self, DlLockState, DlLockType, DlSupportedOperatingModes, OperatingModeEnum,
};
use super::readiness::ClusterReadinessBarrier;
use super::MatterCluster;
use intercom_core::ports::AccessControllerHandle;

// ============================================================================
// Door Lock handler
// ============================================================================

/// Door Lock cluster handler — handles lock/unlock commands from HomeKit.
///
/// Owns its lock state internally (no external atomics).
/// Dispatches unlock requests to `AccessControllerHandle`.
/// Coordinates cluster subscription readiness via `ClusterReadinessBarrier`.
///
/// Out-of-band state changes (e.g. auto-relock via `lock()`) signal the
/// internal `state_changed` signal, which wakes the `run()` background task.
/// That task then calls `ctx.notify_attr_changed()` to push a
/// subscription report to HomeKit.
pub struct IntercomDoorLock {
    config: IntercomDoorLockConfig,
    dataver: Dataver,
    lock_state: AtomicU8,
    unlock_duration_sec: AtomicU32,
    /// Signal used to wake the `run()` background task when an out-of-band
    /// state change (e.g. auto-relock) needs to push a subscription report.
    state_changed: Signal<NoopRawMutex, ()>,
}

impl IntercomDoorLock {
    /// Create a new Door Lock handler.
    pub fn new(config: IntercomDoorLockConfig) -> Self {
        Self {
            config,
            dataver: Dataver::new(1),
            lock_state: AtomicU8::new(DlLockState::Locked as u8),
            unlock_duration_sec: AtomicU32::new(5),
            state_changed: Signal::new(),
        }
    }

    /// Lock the door (update Matter state).
    ///
    /// Called by `MatterAccessControlDriver` when the auto-relock timer fires.
    /// Bumps dataver and signals the `run()` background task to push
    /// a subscription report to HomeKit immediately.
    pub fn lock(&self) {
        let prev = self.lock_state.swap(DlLockState::Locked as u8, Ordering::SeqCst);
        if prev != DlLockState::Locked as u8 {
            info!("lock() called → state=Locked, signalling subscription notify");
            self.dataver.changed();
            self.state_changed.signal(());
        }
    }

    /// Unlock the door (update Matter state).
    ///
    /// Called by `MatterAccessControlDriver` when the door is unlocked out-of-band (e.g. web dashboard).
    /// Bumps dataver and signals the `run()` background task to push
    /// a subscription report to HomeKit immediately.
    pub fn unlock(&self) {
        let prev = self.lock_state.swap(DlLockState::Unlocked as u8, Ordering::SeqCst);
        if prev != DlLockState::Unlocked as u8 {
            info!("unlock() called → state=Unlocked, signalling subscription notify");
            self.dataver.changed();
            self.state_changed.signal(());
        }
    }

    /// Update the door unlock duration in seconds (Matter AutoRelockTime).
    ///
    /// Called by `MatterAccessControlDriver` when the unlock duration changes out-of-band (e.g. web dashboard).
    /// Bumps dataver and signals the `run()` background task to push
    /// a subscription report to HomeKit immediately.
    pub fn set_unlock_duration(&self, duration_sec: u32) {
        let clamped = intercom_core::config::clamp_door_unlock_duration(duration_sec);
        let prev = self.unlock_duration_sec.swap(clamped, Ordering::SeqCst);
        if prev != clamped {
            info!("Door lock auto-relock duration updated: {}s -> {}s", prev, clamped);
            self.dataver.changed();
            self.state_changed.signal(());
        }
    }

    /// The subset of the DoorLock cluster we support.
    ///
    /// Mandatory attributes: LockState, LockType, ActuatorEnabled,
    /// OperatingMode, SupportedOperatingModes.
    /// Commands: LockDoor, UnlockDoor.
    pub const CLUSTER: Cluster<'static> = door_lock::FULL_CLUSTER
        .with_revision(9)
        .with_attrs(with!(system;
            door_lock::AttributeId::LockState
                | door_lock::AttributeId::LockType
                | door_lock::AttributeId::ActuatorEnabled
                | door_lock::AttributeId::AutoRelockTime
                | door_lock::AttributeId::OperatingMode
                | door_lock::AttributeId::SupportedOperatingModes
                | door_lock::AttributeId::RequirePINforRemoteOperation
        ))
        .with_cmds(with!(
            door_lock::CommandId::LockDoor | door_lock::CommandId::UnlockDoor
        ));
}

/// Configuration for `IntercomDoorLock`.
pub struct IntercomDoorLockConfig {
    /// Inbound driving port handle to dispatch access requests.
    pub access: AccessControllerHandle<'static>,
    /// Cluster readiness barrier to signal Apple Home subscription priming.
    pub readiness: &'static ClusterReadinessBarrier,
}

impl MatterCluster for IntercomDoorLock {
    const ENDPOINT_ID: u16 = crate::config::DOOR_LOCK_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    /// Door Lock device type (Matter spec 0x000A, rev 3)
    const DEVICE_TYPE: DeviceType = DeviceType {
        dtype: 0x000A,
        drev: 3,
    };
}

impl AsyncHandler for IntercomDoorLock {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.config.readiness.mark_door_lock_ready();

        if let Some(writer) = reply.with_dataver(self.dataver.get())? {
            if ctx.attr().is_system() {
                Self::CLUSTER.read(ctx.attr(), writer)
            } else {
                match ctx.attr().attr_id as u16 {
                    // LockState (id=0): Nullable<DlLockState>
                    x if x == door_lock::AttributeId::LockState as u16 => {
                        let state = match self.lock_state.load(Ordering::Relaxed) {
                            x if x == DlLockState::Unlocked as u8 => DlLockState::Unlocked,
                            _ => DlLockState::Locked,
                        };
                        Reply::set(writer, Nullable::some(state))
                    }
                    // LockType (id=1): DlLockType
                    x if x == door_lock::AttributeId::LockType as u16 => {
                        Reply::set(writer, DlLockType::DeadBolt)
                    }
                    // ActuatorEnabled (id=2): bool
                    x if x == door_lock::AttributeId::ActuatorEnabled as u16 => {
                        Reply::set(writer, true)
                    }
                    // AutoRelockTime (id=35): u32
                    x if x == door_lock::AttributeId::AutoRelockTime as u16 => {
                        Reply::set(writer, self.unlock_duration_sec.load(Ordering::Relaxed))
                    }
                    // OperatingMode (id=37): OperatingModeEnum
                    x if x == door_lock::AttributeId::OperatingMode as u16 => {
                        Reply::set(writer, OperatingModeEnum::Normal)
                    }
                    // SupportedOperatingModes (id=38): DlSupportedOperatingModes (0xFFF6 = Normal mode supported)
                    x if x == door_lock::AttributeId::SupportedOperatingModes as u16 => {
                        Reply::set(
                            writer,
                            DlSupportedOperatingModes::from_bits_retain(0xFFF6),
                        )
                    }
                    // RequirePINforRemoteOperation (id=51): bool (false = remote unlock allowed without PIN)
                    x if x == door_lock::AttributeId::RequirePINforRemoteOperation as u16 => {
                        Reply::set(writer, false)
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
        match ctx.attr().attr_id as u16 {
            // AutoRelockTime (id=35) - allow controllers to configure auto-relock
            x if x == door_lock::AttributeId::AutoRelockTime as u16 => {
                let val = ctx.data().u32().map_err(|_| ErrorCode::InvalidDataType)?;
                if !intercom_core::config::is_valid_door_unlock_duration(val) {
                    return Err(ErrorCode::ConstraintError.into());
                }
                self.unlock_duration_sec.store(val, Ordering::Relaxed);
                self.dataver.changed();
                ctx.notify_changed();
                let _ = self.config.access.set_unlock_duration(val);
                Ok(())
            }
            // RequirePINforRemoteOperation (id=51) - allow controllers to configure PIN requirement
            x if x == door_lock::AttributeId::RequirePINforRemoteOperation as u16 => Ok(()),
            _ => Err(ErrorCode::AttributeNotFound.into()),
        }
    }

    async fn invoke(&self, ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        match ctx.cmd().cmd_id as u16 {
            // LockDoor (id=0)
            x if x == door_lock::CommandId::LockDoor as u16 => {
                info!("Lock command (no-op, door auto-locks)");
                self.lock_state
                    .store(DlLockState::Locked as u8, Ordering::Relaxed);
                self.dataver.changed();

                // Wake subscription system → push LockState report to HomeKit
                ctx.notify_attr_changed(
                    ctx.cmd().endpoint_id,
                    ctx.cmd().cluster_id,
                    door_lock::AttributeId::LockState as _,
                );

                Ok(())
            }
            // UnlockDoor (id=1)
            x if x == door_lock::CommandId::UnlockDoor as u16 => {
                info!("Unlock command → state=Unlocked, notifying subscriptions");
                self.lock_state
                    .store(DlLockState::Unlocked as u8, Ordering::Relaxed);
                self.dataver.changed();

                // Wake subscription system → push LockState report to HomeKit
                ctx.notify_attr_changed(
                    ctx.cmd().endpoint_id,
                    ctx.cmd().cluster_id,
                    door_lock::AttributeId::LockState as _,
                );

                // Dispatch to inbound driving port handle
                self.config.access.request_unlock();

                Ok(())
            }
            _ => Err(ErrorCode::CommandNotFound.into()),
        }
    }

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
                    door_lock::AttributeId::LockState as _,
                );
                ctx.notify_attr_changed(
                    Self::ENDPOINT_ID,
                    Self::CLUSTER.id,
                    door_lock::AttributeId::AutoRelockTime as _,
                );
            }
        }
    }
}
