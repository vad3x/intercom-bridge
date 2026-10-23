//! Smart Intercom - Lifecycle Event System
//!
//! Embassy `Watch` channel for continuous system lifecycle state.

pub use edf_core::events::{
    SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch, STATE_WATCH_RECEIVERS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_sender_updates_consumer_watch() {
        // Arrange
        let watch = SystemStateWatch::new();

        // Act
        edf_core::SystemStateSender::new(&watch).operational();

        // Assert
        assert_eq!(watch.try_get(), Some(SystemState::Operational));
    }
}
