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
use crate::ports::{DoorDriver, FactoryResetDriver, RingDriver, SystemResetDriver};
use crate::service::access::AccessController;
use crate::service::reset::FactoryResetService;
use crate::service::ring::RingWatcher;

/// The Smart Intercom application.
///
/// Generic over port driver implementations.
/// Owns all core domain services, runs them concurrently.
pub struct App<R, D, REP, F, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    F: FactoryResetDriver,
    RST: SystemResetDriver,
{
    ring_watcher: RingWatcher<R>,
    access_controller: AccessController<D, REP>,
    reset_service: FactoryResetService<F, RST>,
}

impl<R, D, REP, F, RST> App<R, D, REP, F, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    F: FactoryResetDriver,
    RST: SystemResetDriver,
{
    /// Create the application from port driver implementations.
    pub fn new(
        ring: R,
        door: D,
        reporter: REP,
        factory_reset: F,
        reset: RST,
    ) -> Self {
        Self {
            ring_watcher: RingWatcher::new(ring),
            access_controller: AccessController::new(door, reporter),
            reset_service: FactoryResetService::new(factory_reset, reset),
        }
    }

    /// Run all core domain services concurrently (never returns).
    pub async fn run(
        mut self,
        state_sender: SystemStateSender<'_>,
        receiver: &CommandReceiver<'_>,
        access_handle: AccessControllerHandle<'_>,
    ) {
        info!("publishing Initialized state");
        state_sender.initialized();

        info!("starting core services");

        embassy_futures::join::join3(
            self.ring_watcher.run(access_handle),
            self.access_controller.run(receiver),
            self.reset_service.run(state_sender),
        )
        .await;
    }
}
