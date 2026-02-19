//! Smart Intercom - Application
//!
//! Wires trait-based drivers into actor structs and runs them concurrently
//! via `embassy_futures::join4`. Mirrors the C++ `App` class.
//!
//! All dependencies are injected: event bus, driver instances.
//! No statics are accessed here — fully testable.

use log::info;

use crate::actors::access_control::{AccessControlActor, AccessControlDriver};
use crate::actors::door::{DoorActor, DoorDriver};
use crate::actors::indicator::{IndicatorActor, IndicatorDriver};
use crate::actors::ring::{RingActor, RingDriver};
use crate::events::{Event, EventBus, SystemState};

/// The Smart Intercom application.
///
/// Generic over driver implementations (like C++ template parameters).
/// Owns all actors, runs them concurrently via `join4`.
pub struct App<R, D, I, A>
where
    R: RingDriver,
    D: DoorDriver,
    I: IndicatorDriver,
    A: AccessControlDriver,
{
    ring_actor: RingActor<R>,
    door_actor: DoorActor<D>,
    indicator_actor: IndicatorActor<I>,
    access_control_actor: AccessControlActor<A>,
}

impl<R, D, I, A> App<R, D, I, A>
where
    R: RingDriver,
    D: DoorDriver,
    I: IndicatorDriver,
    A: AccessControlDriver,
{
    /// Create the application from trait-based drivers.
    ///
    /// Each driver is wrapped in its corresponding actor.
    pub fn new(ring: R, door: D, indicator: I, access_control: A) -> Self {
        Self {
            ring_actor: RingActor::new(ring),
            door_actor: DoorActor::new(door),
            indicator_actor: IndicatorActor::new(indicator),
            access_control_actor: AccessControlActor::new(access_control),
        }
    }

    /// Run all actors concurrently (never returns).
    ///
    /// Each actor creates its own publisher/subscriber from the bus.
    pub async fn run(mut self, bus: &EventBus) {
        info!("App: publishing Ready state");

        // Publish Ready state before actors start their loops
        let state_pub = bus.publisher().unwrap();
        state_pub
            .publish(Event::SystemState(SystemState::Ready))
            .await;

        info!("App: starting actors");

        // Run all actors concurrently — each creates its own bus handle
        embassy_futures::join::join4(
            self.ring_actor.run(bus),
            self.door_actor.run(bus),
            self.indicator_actor.run(bus),
            self.access_control_actor.run(bus),
        )
        .await;
    }
}
