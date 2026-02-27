//! Smart Intercom - Application
//!
//! Wires trait-based drivers into actor structs and runs them concurrently
//! via `embassy_futures::join`. Mirrors the C++ `App` class.
//!
//! All dependencies are injected: event bus, driver instances.
//! No statics are accessed here — fully testable.

use log::info;

use crate::actors::access_control::{AccessControlActor, AccessControlDriver};
use crate::actors::door::{DoorActor, DoorDriver};
use crate::actors::factory_reset::{FactoryResetActor, FactoryResetDriver};
use crate::actors::indicator::{IndicatorActor, IndicatorDriver};
use crate::actors::matter_controller::{MatterControllerActor, MatterControllerActorConfig};
use crate::actors::ring::{RingActor, RingDriver};
use crate::events::{Event, EventBus, SystemState};

/// The Smart Intercom application.
///
/// Generic over driver implementations (like C++ template parameters).
/// Owns all actors, runs them concurrently.
pub struct App<R, D, I, A, F>
where
    R: RingDriver,
    D: DoorDriver,
    I: IndicatorDriver,
    A: AccessControlDriver,
    F: FactoryResetDriver,
{
    ring_actor: RingActor<R>,
    door_actor: DoorActor<D>,
    indicator_actor: IndicatorActor<I>,
    access_control_actor: AccessControlActor<A>,
    factory_reset_actor: FactoryResetActor<F>,
    matter_controller_actor: MatterControllerActor,
}

impl<R, D, I, A, F> App<R, D, I, A, F>
where
    R: RingDriver,
    D: DoorDriver,
    I: IndicatorDriver,
    A: AccessControlDriver,
    F: FactoryResetDriver,
{
    /// Create the application from trait-based drivers.
    ///
    /// Each driver is wrapped in its corresponding actor.
    pub fn new(
        ring: R,
        door: D,
        indicator: I,
        access_control: A,
        factory_reset: F,
        matter_controller_config: MatterControllerActorConfig,
    ) -> Self {
        Self {
            ring_actor: RingActor::new(ring),
            door_actor: DoorActor::new(door),
            indicator_actor: IndicatorActor::new(indicator),
            access_control_actor: AccessControlActor::new(access_control),
            factory_reset_actor: FactoryResetActor::new(factory_reset),
            matter_controller_actor: MatterControllerActor::new(matter_controller_config),
        }
    }

    /// Run all actors concurrently (never returns).
    ///
    /// Each actor creates its own publisher/subscriber from the bus.
    pub async fn run(mut self, bus: &EventBus) {
        info!("publishing Initialized state");

        // Publish Initialized state before actors start their loops
        let state_pub = bus.publisher().unwrap();
        state_pub
            .publish(Event::SystemState(SystemState::Initialized))
            .await;

        info!("starting actors");

        // Run all actors concurrently — each creates its own bus handle
        embassy_futures::join::join3(
            embassy_futures::join::join4(
                self.ring_actor.run(bus),
                self.door_actor.run(bus),
                self.indicator_actor.run(bus),
                self.access_control_actor.run(bus),
            ),
            self.factory_reset_actor.run(bus),
            self.matter_controller_actor.run(bus),
        )
        .await;
    }
}
