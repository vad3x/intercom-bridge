//! Smart Intercom - Application Coordinator

use log::info;

use crate::events::SystemStateSender;
use crate::ports::inbound::{CommandReceiver, IntercomControllerHandle};
use crate::ports::reporter::IntercomReporter;
use crate::ports::{
    AudioControlDriver, BusSwitchDriver, ResetTriggerDriver, RingDriver, SystemResetDriver,
};
use crate::service::intercom::IntercomController;
use crate::service::reset::ResetService;
use crate::service::ring::RingWatcher;

/// Runs ring detection, non-blocking intercom control, and system reset concurrently.
pub struct App<R, B, A, REP, T, RST>
where
    R: RingDriver,
    B: BusSwitchDriver,
    A: AudioControlDriver,
    REP: IntercomReporter,
    T: ResetTriggerDriver,
    RST: SystemResetDriver,
{
    ring_watcher: RingWatcher<R>,
    intercom_controller: IntercomController<B, A, REP>,
    reset_service: ResetService<T, RST>,
}

impl<R, B, A, REP, T, RST> App<R, B, A, REP, T, RST>
where
    R: RingDriver,
    B: BusSwitchDriver,
    A: AudioControlDriver,
    REP: IntercomReporter,
    T: ResetTriggerDriver,
    RST: SystemResetDriver,
{
    /// Create the application from its outbound port drivers.
    pub fn new(ring: R, bus: B, audio: A, reporter: REP, reset_trigger: T, reset: RST) -> Self {
        Self {
            ring_watcher: RingWatcher::new(ring),
            intercom_controller: IntercomController::new(bus, audio, reporter),
            reset_service: ResetService::new(reset_trigger, reset),
        }
    }

    /// Create the application with a clamped initial strike duration in seconds.
    pub fn with_duration(
        ring: R,
        bus: B,
        audio: A,
        reporter: REP,
        reset_trigger: T,
        reset: RST,
        unlock_duration_sec: u32,
    ) -> Self {
        Self {
            ring_watcher: RingWatcher::new(ring),
            intercom_controller: IntercomController::with_duration(
                bus,
                audio,
                reporter,
                unlock_duration_sec,
            ),
            reset_service: ResetService::new(reset_trigger, reset),
        }
    }

    /// Publish startup and run the core services indefinitely.
    pub async fn run(
        mut self,
        state_sender: SystemStateSender<'_>,
        receiver: &CommandReceiver<'_>,
        access_handle: IntercomControllerHandle<'_>,
    ) {
        info!("publishing Starting state");
        state_sender.starting();

        info!("starting core services");

        embassy_futures::join::join3(
            self.ring_watcher.run(access_handle),
            self.intercom_controller.run(receiver),
            self.reset_service.run(state_sender),
        )
        .await;
    }
}
