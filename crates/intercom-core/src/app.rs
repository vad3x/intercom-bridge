//! Smart Intercom - Application
//!
//! Wires trait-based drivers into actor structs and runs them concurrently
//! via Embassy async combinators.
//!
//! All dependencies are injected: port drivers, state watch/sender, command receiver.
//! No statics are accessed here — fully testable.

use log::info;

use crate::actors::access_controller::AccessController;
use crate::actors::factory_reset::FactoryResetActor;
use crate::actors::indicator::IndicatorActor;
use crate::actors::ring::RingActor;
use crate::events::{SystemStateReceiver, SystemStateSender};
use crate::ports::inbound::{AccessControllerHandle, CommandReceiver};
use crate::ports::reporter::IntercomReporter;
use crate::ports::{DoorDriver, FactoryResetDriver, IndicatorDriver, RingDriver, SystemResetDriver};

/// The Smart Intercom application.
///
/// Generic over port driver implementations.
/// Owns all actors, runs them concurrently.
pub struct App<R, D, REP, I, F, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    I: IndicatorDriver + Clone,
    F: FactoryResetDriver,
    RST: SystemResetDriver,
{
    ring_actor: RingActor<R>,
    access_controller: AccessController<D, REP, I>,
    indicator_actor: IndicatorActor<I>,
    factory_reset_actor: FactoryResetActor<F, RST>,
}

impl<R, D, REP, I, F, RST> App<R, D, REP, I, F, RST>
where
    R: RingDriver,
    D: DoorDriver,
    REP: IntercomReporter,
    I: IndicatorDriver + Clone,
    F: FactoryResetDriver,
    RST: SystemResetDriver,
{
    /// Create the application from port driver implementations.
    pub fn new(
        ring: R,
        door: D,
        indicator: I,
        reporter: REP,
        factory_reset: F,
        reset: RST,
    ) -> Self {
        Self {
            ring_actor: RingActor::new(ring),
            access_controller: AccessController::new(door, reporter, indicator.clone()),
            indicator_actor: IndicatorActor::new(indicator),
            factory_reset_actor: FactoryResetActor::new(factory_reset, reset),
        }
    }

    /// Run all actors concurrently (never returns).
    pub async fn run(
        mut self,
        state_sender: SystemStateSender<'_>,
        receiver: &CommandReceiver<'_>,
        access_handle: AccessControllerHandle<'_>,
        state_receiver: SystemStateReceiver<'_>,
    ) {
        info!("publishing Initialized state");
        state_sender.initialized();

        info!("starting actors");

        embassy_futures::join::join4(
            self.ring_actor.run(access_handle),
            self.access_controller.run(receiver),
            self.indicator_actor.run(state_receiver),
            self.factory_reset_actor.run(state_sender),
        )
        .await;
    }
}
