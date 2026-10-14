//! Smart Intercom - Matter Controller Actor
//!
//! Bridges system events to Matter controller actions.
//! Tracks cluster readiness (DoorLock + Occupancy) and publishes
//! `SystemState::Ready` when both are primed by Apple Home.
//! Handles factory reset: erases commissioning data, waits for the
//! indicator to blink, then triggers system reboot via callback.

use embassy_time::{Duration, Timer};
use log::info;

use crate::events::{Event, EventBus, EventBusExt, EventBusSubscriber, SystemState};

/// Configuration for `MatterControllerActor`.
///
/// Groups callbacks passed at construction time, keeping the constructor
/// signature stable and keeping core decoupled from hardware reboot calls.
pub struct MatterControllerActorConfig {
    /// Called to erase Matter KVS on factory reset.
    pub on_factory_reset: fn(),
    /// Called to reboot the system after factory reset delay.
    pub on_reboot: fn(),
}

/// Matter controller actor — handles factory reset and cluster readiness.
///
/// Tracks readiness of both Matter clusters (DoorLock and Occupancy).
/// When both report ready (subscription primed by Apple Home), publishes
/// `SystemState::Ready`.
///
/// When `SystemState::FactoryReset` is received, calls `on_factory_reset`
/// to erase commissioning data, waits for the indicator LED to show the
/// reset pattern, then calls `on_reboot`.
pub struct MatterControllerActor {
    config: MatterControllerActorConfig,
    door_lock_ready: bool,
    ring_ready: bool,
}

impl MatterControllerActor {
    pub fn new(config: MatterControllerActorConfig) -> Self {
        Self {
            config,
            door_lock_ready: false,
            ring_ready: false,
        }
    }

    /// Run the event bridge loop (never returns).
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        loop {
            self.next(&mut subscriber, bus).await;
        }
    }

    /// Await the next event and handle it.
    pub async fn next(&mut self, subscriber: &mut EventBusSubscriber<'_>, bus: &EventBus) {
        match subscriber.next_message_pure().await {
            Event::SystemState(SystemState::FactoryReset) => {
                self.handle_factory_reset().await;
            }
            Event::DoorLockReady => {
                self.door_lock_ready = true;
                self.check_all_ready(bus);
            }
            Event::RingReady => {
                self.ring_ready = true;
                self.check_all_ready(bus);
            }
            Event::SystemState(SystemState::Connecting) => {
                self.door_lock_ready = false;
                self.ring_ready = false;
            }
            _ => {}
        }
    }

    /// Erase commissioning data, wait for indicator, then reboot.
    async fn handle_factory_reset(&self) {
        info!("factory reset event → erasing Matter KVS");
        (self.config.on_factory_reset)();

        Timer::after(Duration::from_millis(
            crate::config::FACTORY_RESET_REBOOT_DELAY_MS,
        ))
        .await;

        info!("Rebooting...");
        (self.config.on_reboot)();
    }

    /// Publish `SystemState::Ready` when both clusters are primed.
    fn check_all_ready(&self, bus: &EventBus) {
        if self.door_lock_ready && self.ring_ready {
            info!("all clusters ready → publishing Ready state");
            bus.ready();
        }
    }
}
