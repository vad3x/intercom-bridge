//! Inbound driving ports for the Smart Intercom core domain.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use log::warn;

use crate::domain::intercom::{IntercomAction, SessionOwner};

/// Inbound handle for intercom bus, strike, and session commands.
#[derive(Clone, Copy)]
pub struct IntercomControllerHandle<'a> {
    sender: CommandSender<'a>,
}

impl<'a> IntercomControllerHandle<'a> {
    /// Create a handle from the bounded command channel sender.
    pub const fn new(sender: CommandSender<'a>) -> Self {
        Self { sender }
    }

    /// Queue validated microphone and speaker gain settings.
    pub fn set_audio_gain(
        &self,
        rx: crate::domain::policy::RxPgaGain,
        tx: crate::domain::policy::TxDacGain,
    ) {
        self.send(IntercomCommand::SetAudioGain { rx, tx });
    }

    /// Request an entrance door strike.
    pub fn request_unlock(&self) {
        self.send(IntercomCommand::Unlock);
    }

    /// Forward an incoming visitor ring.
    pub fn trigger_ring(&self) {
        self.send(IntercomCommand::Ring);
    }

    /// Queue a strike duration update, returning an error when the queue is full.
    pub fn set_unlock_duration(&self, duration_sec: u32) -> Result<(), ()> {
        self.sender
            .try_send(IntercomCommand::SetUnlockDuration(duration_sec))
            .map_err(|_| ())
    }

    /// Request capture for the given session owner.
    pub fn start_listen(&self, owner: SessionOwner) {
        self.send(IntercomCommand::StartListen(owner));
    }

    /// Request playback for the given session owner.
    pub fn start_talk(&self, owner: SessionOwner) {
        self.send(IntercomCommand::StartTalk(owner));
    }

    /// Return the owning talk session to listening.
    pub fn stop_talk(&self, owner: SessionOwner) {
        self.send(IntercomCommand::StopTalk(owner));
    }

    /// Refresh the owning session heartbeat.
    pub fn session_heartbeat(&self, owner: SessionOwner) {
        self.send(IntercomCommand::SessionHeartbeat(owner));
    }

    /// Release the given owner’s session.
    pub fn end_session(&self, owner: SessionOwner) {
        self.send(IntercomCommand::EndSession(owner));
    }

    /// Wait for command capacity to queue session release without discarding it.
    ///
    /// Async adapters should await this before accepting another session. The
    /// controller must keep draining its command channel while this is pending.
    pub async fn end_session_wait(&self, owner: SessionOwner) {
        self.sender.send(IntercomCommand::EndSession(owner)).await;
    }

    /// Request that the bus and audio return to their safe idle state.
    pub fn emergency_stop(&self) {
        self.send(IntercomCommand::EmergencyStop);
    }

    /// Queue a command without waiting, logging when the queue is full.
    pub fn send(&self, cmd: IntercomCommand) {
        if self.sender.try_send(cmd).is_err() {
            warn!("Intercom command {:?} dropped: command queue full", cmd);
        }
    }
}

/// Capacity of the bounded intercom command queue.
pub const CMD_QUEUE_CAP: usize = 8;

/// Control-plane action consumed by the intercom state machine.
pub type IntercomCommand = IntercomAction;
/// Compatibility command name for existing access adapters.
pub type AccessCommand = IntercomAction;

/// Bounded MPSC channel connecting inbound adapters to the controller.
pub type CommandChannel = Channel<CriticalSectionRawMutex, IntercomCommand, CMD_QUEUE_CAP>;
/// Sender endpoint for queuing intercom commands.
pub type CommandSender<'a> = Sender<'a, CriticalSectionRawMutex, IntercomCommand, CMD_QUEUE_CAP>;
/// Receiver endpoint consumed by the intercom controller.
pub type CommandReceiver<'a> =
    Receiver<'a, CriticalSectionRawMutex, IntercomCommand, CMD_QUEUE_CAP>;

/// Compatibility name for existing access adapters.
pub type AccessControllerHandle<'a> = IntercomControllerHandle<'a>;
