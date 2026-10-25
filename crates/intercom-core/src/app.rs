//! Smart Intercom - Application Coordinator
//!
//! Wires trait-based drivers into application services and runs them concurrently
//! via Embassy async combinators.
//!
//! All dependencies are injected: port drivers, state sender, command receiver.
//! No statics are accessed here — fully testable.

use log::info;

use crate::events::SystemStateSender;
use crate::ports::inbound::{AccessControllerHandle, CommandReceiver};
use crate::ports::reporter::IntercomReporter;
use crate::ports::{DoorDriver, ResetTriggerDriver, RingDriver, SystemResetDriver};
use crate::service::access::AccessController;
use crate::service::reset::ResetService;
use crate::service::ring::RingWatcher;

/// The Smart Intercom application.
///
/// Generic over port driver implementations.
/// Owns all core domain services, runs them concurrently.
pub struct App<R, D, REP, T, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    T: ResetTriggerDriver,
    RST: SystemResetDriver,
{
    ring_watcher: RingWatcher<R>,
    access_controller: AccessController<D, REP>,
    reset_service: ResetService<T, RST>,
}

impl<R, D, REP, T, RST> App<R, D, REP, T, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    T: ResetTriggerDriver,
    RST: SystemResetDriver,
{
    /// Create the application from port driver implementations.
    pub fn new(
        ring: R,
        door: D,
        reporter: REP,
        reset_trigger: T,
        reset: RST,
    ) -> Self {
        Self {
            ring_watcher: RingWatcher::new(ring),
            access_controller: AccessController::new(door, reporter),
            reset_service: ResetService::new(reset_trigger, reset),
        }
    }

    /// Create the application from port driver implementations with a specific initial unlock duration.
    pub fn with_duration(
        ring: R,
        door: D,
        reporter: REP,
        reset_trigger: T,
        reset: RST,
        unlock_duration_sec: u32,
    ) -> Self {
        Self {
            ring_watcher: RingWatcher::new(ring),
            access_controller: AccessController::with_duration(door, reporter, unlock_duration_sec),
            reset_service: ResetService::new(reset_trigger, reset),
        }
    }

    /// Run all core domain services concurrently (never returns).
    pub async fn run(
        mut self,
        state_sender: SystemStateSender<'_>,
        receiver: &CommandReceiver<'_>,
        access_handle: AccessControllerHandle<'_>,
    ) {
        info!("publishing Starting state");
        state_sender.starting();

        info!("starting core services");

        embassy_futures::join::join3(
            self.ring_watcher.run(access_handle),
            self.access_controller.run(receiver),
            self.reset_service.run(state_sender),
        )
        .await;
    }
}
