//! Smart Intercom - Door Lock Cluster Handler
//!
//! Handles lock/unlock commands from HomeKit via the Matter DoorLock cluster.
//! Calls `on_unlock` callback when an unlock command is received from Matter.
//!
//! Uses `notify_attribute_changed()` to wake the Matter subscription system
//! so HomeKit receives prompt state updates (without it, the app stays on
//! "Unlocking…" until the periodic subscription timer fires).

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use log::info;

use rs_matter::dm::{
    AsyncHandler, Cluster, Dataver, DeviceType, HandlerContext, InvokeContext, InvokeReply,
    ReadContext, ReadReply, Reply, WriteContext,
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
/// Out-of-band state changes (e.g. auto-relock via `lock()`) signal the
/// internal `state_changed` signal, which wakes the `run()` background task.
/// That task then calls `ctx.notify_attribute_changed()` to push a
/// subscription report to HomeKit.
pub struct IntercomDoorLock<OnUnlock: Fn()> {
    dataver: Dataver,
    lock_state: AtomicU8,
    /// Signal used to wake the `run()` background task when an out-of-band
    /// state change (e.g. auto-relock) needs to push a subscription report.
    state_changed: Signal<NoopRawMutex, ()>,
    on_unlock: OnUnlock,
}

impl<OnUnlock: Fn()> IntercomDoorLock<OnUnlock> {
    /// Create a new Door Lock handler.
    ///
    /// `on_unlock` is called (from sync Matter context) when HomeKit sends
    /// an unlock command. Typically wired to publish `Event::UnlockDispatched`
    /// to the event bus.
    pub fn new(on_unlock: OnUnlock) -> Self {
        Self {
            dataver: Dataver::new(1),
            lock_state: AtomicU8::new(DlLockState::Locked as u8),
            state_changed: Signal::new(),
            on_unlock,
        }
    }

    /// Lock the door (update Matter state).
    ///
    /// Called by `MatterAccessControlDriver` when the auto-relock timer fires.
    /// Bumps dataver and signals the `run()` background task to push
    /// a subscription report to HomeKit immediately.
    pub fn lock(&self) {
        info!("lock() called → state=Locked, signalling subscription notify");
        self.lock_state
            .store(DlLockState::Locked as u8, Ordering::Relaxed);
        self.dataver.changed();
        self.state_changed.signal(());
    }
}

impl<OnUnlock: Fn()> IntercomDoorLock<OnUnlock> {
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

impl<OnUnlock: Fn()> MatterCluster for IntercomDoorLock<OnUnlock> {
    const ENDPOINT_ID: u16 = crate::config::DOOR_LOCK_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    /// Door Lock device type (Matter spec 0x000A, rev 3)
    const DEVICE_TYPE: DeviceType = DeviceType {
        dtype: 0x000A,
        drev: 3,
    };
}

impl<OnUnlock: Fn()> AsyncHandler for IntercomDoorLock<OnUnlock> {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
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
                info!("Lock command (no-op, door auto-locks)");
                self.lock_state
                    .store(DlLockState::Locked as u8, Ordering::Relaxed);
                self.dataver.changed();

                // Wake subscription system → push LockState report to HomeKit
                ctx.notify_attribute_changed(
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
                // Without this, the app stays on "Unlocking…" until the periodic
                // subscription timer fires (could be 60+ seconds).
                ctx.notify_attribute_changed(
                    ctx.cmd().endpoint_id,
                    ctx.cmd().cluster_id,
                    door_lock::AttributeId::LockState as _,
                );

                // Notify app layer (sync context — non-blocking)
                (self.on_unlock)();

                Ok(())
            }
            _ => Err(ErrorCode::CommandNotFound.into()),
        }
    }

    /// Background task: waits for out-of-band state changes (e.g. auto-relock)
    /// and notifies the subscription system so HomeKit receives prompt updates.
    fn run(
        &self,
        ctx: impl HandlerContext,
    ) -> impl core::future::Future<Output = Result<(), Error>> {
        async move {
            loop {
                self.state_changed.wait().await;
                info!("run() woke → notifying subscription system");
                ctx.notify_attribute_changed(
                    Self::ENDPOINT_ID,
                    Self::CLUSTER.id,
                    door_lock::AttributeId::LockState as _,
                );
            }
        }
    }
}
