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
    Initializing,
    /// App actors running, ready to operate
    Ready,
    /// Matter stack is connecting to the hub (WiFi + CASE)
    Connecting,
    /// Connected to Matter hub, fully operational
    Connected,
    /// Something went wrong
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
}

// ============================================================================
// Event bus type aliases
// ============================================================================

/// Buffer capacity for the event bus
const BUS_CAP: usize = 4;
/// Maximum concurrent subscribers
const BUS_SUBS: usize = 4;
/// Maximum concurrent publishers
const BUS_PUBS: usize = 4;

/// The event bus — `PubSubChannel` with fan-out to all subscribers.
///
/// C++ equivalent: `EventBus` class
pub type EventBus = PubSubChannel<CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;

/// Subscriber type alias matching `EventBus` parameters.
pub type EventBusSubscriber<'a> =
    Subscriber<'a, CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;

pub type EventBusPublisher<'a> =
    Publisher<'a, CriticalSectionRawMutex, Event, BUS_CAP, BUS_SUBS, BUS_PUBS>;
