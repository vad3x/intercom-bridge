//! Smart Intercom - Access Control Actor
//!
//! Bridges ring events to Matter occupancy state, and manages the
//! occupancy hold timer and auto-relock timer.

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use crate::config;
use crate::events::{Event, EventBus, EventBusSubscriber};
use crate::ports::AccessControlDriver;

/// Access control actor — bridges event bus to driver.
///
/// Subscribes to events on the bus and calls driver methods:
/// - `Ringing` → `driver.set_occupancy(true)`, starts hold timer
/// - Hold timer expired → `driver.set_occupancy(false)`
/// - `UnlockDispatched` → starts auto-relock timer
/// - Relock timer expired → `driver.lock()`
pub struct AccessControlActor<D: AccessControlDriver> {
    driver: D,
    occupancy_deadline: Instant,
    relock_deadline: Instant,
}

impl<D: AccessControlDriver> AccessControlActor<D> {
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            occupancy_deadline: Instant::MAX,
            relock_deadline: Instant::MAX,
        }
    }

    /// Run the access control loop (never returns).
    ///
    /// Uses `select` to multiplex event reception with timer deadlines.
    pub async fn run(&mut self, bus: &EventBus) {
        info!("started");

        let mut subscriber = bus.subscriber().unwrap();

        loop {
            self.next(&mut subscriber).await;
        }
    }

    /// Await the next event or timer expiration and handle it.
    ///
    /// Multiplexes event reception with the nearest active deadline.
    pub async fn next(&mut self, subscriber: &mut EventBusSubscriber<'_>) {
        let nearest = self.occupancy_deadline.min(self.relock_deadline);
        let timeout = if nearest == Instant::MAX {
            Duration::from_secs(86400) // No active timers → park
        } else {
            nearest
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::from_millis(0))
        };

        match select(subscriber.next_message_pure(), Timer::after(timeout)).await {
            Either::First(event) => match event {
                Event::Ringing => {
                    info!("ring → occupancy ON");
                    self.driver.set_occupancy(true);
                    self.occupancy_deadline =
                        Instant::now() + Duration::from_millis(config::RING_HOLD_MS);
                }
                Event::UnlockDispatched => {
                    info!("unlock → auto-relock timer started");
                    self.relock_deadline =
                        Instant::now() + Duration::from_secs(config::AUTO_RELOCK_SEC);
                }
                _ => {}
            },
            Either::Second(()) => {
                let now = Instant::now();

                if now >= self.occupancy_deadline {
                    info!("occupancy hold expired → OFF");
                    self.driver.set_occupancy(false);
                    self.occupancy_deadline = Instant::MAX;
                }

                if now >= self.relock_deadline {
                    info!("auto-relock");
                    self.driver.lock();
                    self.relock_deadline = Instant::MAX;
                }
            }
        }
    }
}
