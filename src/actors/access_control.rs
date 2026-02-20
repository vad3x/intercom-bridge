//! Smart Intercom - Access Control Actor
//!
//! Bridges ring events to Matter occupancy state, and manages the
//! occupancy hold timer and auto-relock timer.
//!
//! Defines the `AccessControlDriver` trait (like C++ `AccessControlDriver`
//! concept) and is generic over it. The driver is injected at construction.
//!
//! C++ equivalent: `AccessControlActor<Driver>`

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use crate::config;
use crate::events::{Event, EventBus};

// ============================================================================
// AccessControlDriver trait (like C++ concept)
// ============================================================================

/// Trait for access control drivers.
///
/// The actor calls these methods in response to events and timer expirations.
/// The driver implementation encapsulates the actual state management
/// (e.g. Matter cluster atomics).
///
/// C++ equivalent: `AccessControlDriver` concept in `access_control_actor.h`
pub trait AccessControlDriver {
    /// Update occupancy state (ring detected / hold expired).
    fn set_occupancy(&self, occupied: bool);

    /// Lock the door (auto-relock timer expired).
    fn lock(&self);
}

// ============================================================================
// Access Control Actor
// ============================================================================

/// Access control actor — bridges event bus to driver.
///
/// Subscribes to events on the bus and calls driver methods:
/// - `Ringing` → `driver.set_occupancy(true)`, starts hold timer
/// - Hold timer expired → `driver.set_occupancy(false)`
/// - `UnlockDispatched` → starts auto-relock timer
/// - Relock timer expired → `driver.lock()`
pub struct AccessControlActor<D: AccessControlDriver> {
    driver: D,
}

impl<D: AccessControlDriver> AccessControlActor<D> {
    pub fn new(driver: D) -> Self {
        Self { driver }
    }

    /// Run the access control loop (never returns).
    ///
    /// Uses `select` to multiplex event reception with timer deadlines.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        let mut occupancy_deadline = Instant::MAX;
        let mut relock_deadline = Instant::MAX;

        loop {
            // Compute the nearest deadline for either timer
            let nearest = occupancy_deadline.min(relock_deadline);
            let timeout = if nearest == Instant::MAX {
                Duration::from_secs(86400) // No active timers → park
            } else {
                nearest
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::from_millis(0))
            };

            match select(subscriber.next_message_pure(), Timer::after(timeout)).await {
                // ============================================================
                // Event received
                // ============================================================
                Either::First(event) => match event {
                    Event::Ringing => {
                        info!("ring → occupancy ON");
                        self.driver.set_occupancy(true);
                        occupancy_deadline =
                            Instant::now() + Duration::from_millis(config::RING_HOLD_MS);
                    }
                    Event::UnlockDispatched => {
                        info!("unlock → auto-relock timer started");
                        relock_deadline =
                            Instant::now() + Duration::from_secs(config::AUTO_RELOCK_SEC);
                    }
                    _ => {}
                },

                // ============================================================
                // Timer expired — check which deadline(s) passed
                // ============================================================
                Either::Second(()) => {
                    let now = Instant::now();

                    if now >= occupancy_deadline {
                        info!("occupancy hold expired → OFF");
                        self.driver.set_occupancy(false);
                        occupancy_deadline = Instant::MAX;
                    }

                    if now >= relock_deadline {
                        info!("auto-relock");
                        self.driver.lock();
                        relock_deadline = Instant::MAX;
                    }
                }
            }
        }
    }
}
