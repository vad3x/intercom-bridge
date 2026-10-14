//! Smart Intercom - Event System
//!
//! Embassy `PubSubChannel` event bus with fan-out to all subscribers.
//! Defines system lifecycle states and events flowing through the core.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher, Subscriber};

/// System lifecycle states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemState {
    /// Starting up, hardware init in progress
    Initializing,
    /// App actors running, hardware initialized
    Initialized,
    /// Matter stack is connecting to the hub (WiFi + CASE)
    Connecting,
    /// Connected to Matter hub (WiFi up)
    Connected,
    /// All Matter clusters available in Apple Home
    Ready,
    /// Factory reset triggered (erasing Matter KVS, rebooting)
    FactoryReset,
    /// Something went wrong
    Error,
}

/// All events that flow through the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// System lifecycle state changed
    SystemState(SystemState),
    /// Ring detected from hardware (optocoupler)
    Ringing,
    /// Unlock command dispatched from Matter/HomeKit
    UnlockDispatched,
    /// Door Lock cluster is being actively read (subscription primed)
    DoorLockReady,
    /// Occupancy Sensing cluster is being actively read (subscription primed)
    RingReady,
}

// ============================================================================
// Event bus type aliases
// ============================================================================

/// Buffer capacity for the event bus
pub const BUS_CAP: usize = 4;
/// Maximum concurrent subscribers
pub const BUS_SUBS: usize = 5;
/// Maximum concurrent publishers
pub const BUS_PUBS: usize = 4;

/// The event bus — `PubSubChannel` with fan-out to all subscribers.
pub type EventBus = PubSubChannel<CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;

/// Subscriber type alias matching `EventBus` parameters.
pub type EventBusSubscriber<'a> =
    Subscriber<'a, CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;

/// Publisher type alias matching `EventBus` parameters.
pub type EventBusPublisher<'a> =
    Publisher<'a, CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;

// ============================================================================
// Event bus convenience methods
// ============================================================================

/// Extension trait adding convenience publish methods to `EventBus`.
pub trait EventBusExt {
    /// Publish `SystemState::Initialized` synchronously (immediate).
    fn initialized(&self);

    /// Publish `SystemState::Connecting` synchronously (immediate).
    fn connecting(&self);

    /// Publish `SystemState::Connected` synchronously (immediate).
    fn connected(&self);

    /// Publish `SystemState::Ready` synchronously (immediate).
    fn ready(&self);

    /// Publish `Event::DoorLockReady` synchronously (immediate).
    fn door_lock_ready(&self);

    /// Publish `Event::RingReady` synchronously (immediate).
    fn ring_ready(&self);
}

impl EventBusExt for EventBus {
    fn initialized(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::SystemState(SystemState::Initialized));
    }

    fn connecting(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::SystemState(SystemState::Connecting));
    }

    fn connected(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::SystemState(SystemState::Connected));
    }

    fn ready(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::SystemState(SystemState::Ready));
    }

    fn door_lock_ready(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::DoorLockReady);
    }

    fn ring_ready(&self) {
        self.immediate_publisher()
            .publish_immediate(Event::RingReady);
    }
}
