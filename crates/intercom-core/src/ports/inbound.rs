//! Smart Intercom - Inbound Driving Ports
//!
//! Defines commands and the `AccessControllerHandle` primary port.
//! Inbound edge adapters (smart home protocols, GPIO interrupts) use this handle to drive the core.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use log::warn;

/// Capacity of the command queue (one active, one queued).
pub const CMD_QUEUE_CAP: usize = 2;

/// Inbound commands that drive the access control domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessCommand {
    /// Request to trigger an entrance door unlock.
    Unlock,
    /// Hardware trigger indicating the intercom buzzer/chime is ringing.
    Ring,
}

/// Channel for queuing access commands.
pub type CommandChannel = Channel<CriticalSectionRawMutex, AccessCommand, CMD_QUEUE_CAP>;
/// Sender endpoint for dispatching access commands.
pub type CommandSender<'a> = Sender<'a, CriticalSectionRawMutex, AccessCommand, CMD_QUEUE_CAP>;
/// Receiver endpoint for consuming access commands.
pub type CommandReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, AccessCommand, CMD_QUEUE_CAP>;

/// Inbound Actor Handle — the primary driving port for the access controller.
///
/// Implements value semantics (`Copy`), owns no heap memory, and provides
/// clean synchronous dispatch methods for edge adapters.
#[derive(Clone, Copy)]
pub struct AccessControllerHandle<'a> {
    sender: CommandSender<'a>,
}

impl<'a> AccessControllerHandle<'a> {
    /// Create a new handle from a command channel sender.
    pub const fn new(sender: CommandSender<'a>) -> Self {
        Self { sender }
    }

    /// External request to unlock the door (from smart home protocols, remote apps, or keypads).
    pub fn request_unlock(&self) {
        if self.sender.try_send(AccessCommand::Unlock).is_err() {
            warn!("Unlock command dropped: command queue full");
        }
    }

    /// Hardware trigger from the intercom ring/buzzer line.
    pub fn trigger_ring(&self) {
        if self.sender.try_send(AccessCommand::Ring).is_err() {
            warn!("Ring trigger dropped: command queue full");
        }
    }
}
