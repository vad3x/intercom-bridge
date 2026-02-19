//! Smart Intercom - Door Lock Cluster Handler
//!
//! Handles lock/unlock commands from HomeKit via the Matter DoorLock cluster.
//! Calls `on_unlock` callback when an unlock command is received from Matter.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use log::info;

use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, InvokeContext, InvokeReply, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;
use rs_matter::with;

use super::clusters_gen::door_lock::{self, DlLockState, DlLockType};
use super::MatterCluster;

// ============================================================================
// Door Lock handler
// ============================================================================

/// Door Lock cluster handler — handles lock/unlock commands from HomeKit.
///
/// Owns its lock state internally (no external atomics).
/// Calls `on_unlock` callback when an unlock command is received,
/// allowing the app layer to bridge to the event bus.
///
/// The `lock_changed` flag is used internally so that out-of-band state
/// changes (e.g. auto-relock via `lock()`) are picked up on the next
/// subscription read and pushed to the Matter controller.
pub struct IntercomDoorLock<'a> {
    dataver: Dataver,
    lock_state: AtomicU8,
    lock_changed: AtomicBool,
    on_unlock: &'a dyn Fn(),
}

impl<'a> IntercomDoorLock<'a> {
    /// Create a new Door Lock handler.
    ///
    /// `on_unlock` is called (from sync Matter context) when HomeKit sends
    /// an unlock command. Typically wired to publish `Event::UnlockDispatched`
    /// to the event bus.
    pub fn new(on_unlock: &'a dyn Fn()) -> Self {
        Self {
            dataver: Dataver::new(1),
            lock_state: AtomicU8::new(DlLockState::Locked as u8),
            lock_changed: AtomicBool::new(false),
            on_unlock,
        }
    }

    /// Lock the door (update Matter state).
    ///
    /// Called by `MatterAccessControlDriver` when the auto-relock timer fires.
    /// Sets `lock_changed` so the next Matter read bumps dataver and triggers
    /// a subscription report.
    pub fn lock(&self) {
        self.lock_state
            .store(DlLockState::Locked as u8, Ordering::Relaxed);
        self.lock_changed.store(true, Ordering::Relaxed);
    }
}

impl IntercomDoorLock<'_> {
    /// The subset of the DoorLock cluster we support.
    ///
    /// We only expose LockState, LockType, ActuatorEnabled attributes
    /// and Lock/Unlock commands.
    pub const CLUSTER: Cluster<'static> = door_lock::FULL_CLUSTER
        .with_revision(7)
        .with_attrs(with!(
            door_lock::AttributeId::LockState
                | door_lock::AttributeId::LockType
                | door_lock::AttributeId::ActuatorEnabled
        ))
        .with_cmds(with!(
            door_lock::CommandId::LockDoor | door_lock::CommandId::UnlockDoor
        ));
}

impl MatterCluster for IntercomDoorLock<'_> {
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    /// Door Lock device type (Matter spec 0x000A, rev 3)
    const DEVICE_TYPE: DeviceType = DeviceType {
        dtype: 0x000A,
        drev: 3,
    };
}

impl AsyncHandler for IntercomDoorLock<'_> {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        // If an external actor (e.g. auto-relock) changed the lock state,
        // bump dataver so the Matter stack sends a subscription report.
        if self.lock_changed.swap(false, Ordering::Relaxed) {
            self.dataver.changed();
        }

        if let Some(writer) = reply.with_dataver(self.dataver.get())? {
            if ctx.attr().is_system() {
                ctx.attr().cluster()?.read(ctx.attr(), writer)
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
                        Reply::set(writer, DlLockType::Other)
                    }
                    // ActuatorEnabled (id=2): bool
                    x if x == door_lock::AttributeId::ActuatorEnabled as u16 => {
                        Reply::set(writer, true)
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
        // None of our DoorLock attributes are writable
        Err(ErrorCode::AttributeNotFound.into())
    }

    async fn invoke(&self, ctx: impl InvokeContext, _reply: impl InvokeReply) -> Result<(), Error> {
        match ctx.cmd().cmd_id as u16 {
            // LockDoor (id=0)
            x if x == door_lock::CommandId::LockDoor as u16 => {
                info!("Matter: Lock command (no-op, door auto-locks)");
                self.lock_state
                    .store(DlLockState::Locked as u8, Ordering::Relaxed);
                self.dataver.changed();
                Ok(())
            }
            // UnlockDoor (id=1)
            x if x == door_lock::CommandId::UnlockDoor as u16 => {
                info!("Matter: Unlock command → calling on_unlock callback");
                self.lock_state
                    .store(DlLockState::Unlocked as u8, Ordering::Relaxed);

                // Notify app layer (sync context — non-blocking)
                (self.on_unlock)();

                self.dataver.changed();
                Ok(())
            }
            _ => Err(ErrorCode::CommandNotFound.into()),
        }
    }
}
