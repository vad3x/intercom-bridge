//! Smart Intercom - Event System
//!
//! Replaces the C++ `EventBus` + `Listener` pattern with Embassy `PubSubChannel`.
//!
//! `PubSubChannel` provides fan-out: one published event is delivered to ALL
//! active subscribers. This mirrors the C++ observer pattern exactly.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::pubsub::Publisher;
use embassy_sync::pubsub::Subscriber;

/// System lifecycle states
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SystemState {
    /// Starting up, hardware init in progress
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    Error,
}

/// All events that flow through the system.
///
/// C++ equivalent: `RingingEvent`, `UnlockDispatchedEvent`, `SystemStateEvent`
#[derive(Clone, Copy, Debug)]
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
const BUS_CAP: usize = 4;
/// Maximum concurrent subscribers
const BUS_SUBS: usize = 5;
/// Maximum concurrent publishers
const BUS_PUBS: usize = 4;

/// The event bus — `PubSubChannel` with fan-out to all subscribers.
///
/// C++ equivalent: `EventBus` class
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
