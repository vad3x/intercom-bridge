//! Outbound port traits for Smart Intercom core.

pub mod diagnostics;
pub mod ecosystem;
pub mod inbound;
pub mod ota;
pub mod reporter;
pub mod reset;

use crate::domain::intercom::{AudioDirection, BusLineState};

pub use diagnostics::HardwareInfoDriver;
pub use ecosystem::EcosystemInfoDriver;
pub use edf_core::ports::ota as ota_edf;
pub use inbound::{
    AccessCommand, AccessControllerHandle, CommandChannel, CommandReceiver, CommandSender,
    IntercomCommand, IntercomControllerHandle,
};
pub use ota::{OtaDriverError, OtaFlashDriver, OtaStream};
pub use reporter::{IntercomReporter, LockState};
pub use reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};

/// Synchronously selects one intercom bus line state.
pub trait BusSwitchDriver {
    fn set_bus_state(&mut self, state: BusLineState);
}

/// Synchronously mutes audio or selects capture or playback.
pub trait AudioControlDriver {
    fn set_audio_direction(&mut self, direction: AudioDirection);
}

/// Waits for a valid incoming visitor ring signal.
pub trait RingDriver {
    async fn wait_for_ring(&mut self);
}

/// Audio control placeholder for adapters without an audio pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopAudioControlDriver;

impl AudioControlDriver for NoopAudioControlDriver {
    fn set_audio_direction(&mut self, _direction: AudioDirection) {}
}
