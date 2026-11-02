//! Shared, synchronous relay inhibition for maintenance operations.

use core::cell::RefCell;
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};

use crate::domain::intercom::BusLineState;
use crate::ports::{BusSafetyDriver, BusSwitchDriver};

/// Owns the physical bus driver; both controller and maintenance borrow this interlock.
///
/// Inhibition is permanent until reboot. A failed or cancelled OTA must not revive
/// an old session or allow a queued command to re-energize a relay.
pub struct BusSafetyInterlock<B: BusSwitchDriver> {
    bus: Mutex<CriticalSectionRawMutex, RefCell<GuardedBus<B>>>,
}

impl<B: BusSwitchDriver> BusSafetyInterlock<B> {
    /// Take ownership of the only driver allowed to energize the bus relays.
    pub fn new(driver: B) -> Self {
        Self {
            bus: Mutex::new(RefCell::new(GuardedBus {
                driver,
                inhibited: false,
            })),
        }
    }
}

struct GuardedBus<B> {
    driver: B,
    inhibited: bool,
}

impl<B: BusSwitchDriver> BusSafetyDriver for BusSafetyInterlock<B> {
    fn inhibit(&self) {
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            // Serialize the latch and physical shutdown with every controller write.
            bus.inhibited = true;
            bus.driver.set_bus_state(BusLineState::Idle);
        });
    }
}

impl<B: BusSwitchDriver> BusSwitchDriver for &BusSafetyInterlock<B> {
    fn set_bus_state(&mut self, state: BusLineState) {
        self.bus.lock(|bus| {
            let mut bus = bus.borrow_mut();
            if !bus.inhibited {
                bus.driver.set_bus_state(state);
            }
        });
    }
}
