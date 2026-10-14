//! Smart Intercom - Ring Watcher Service
//!
//! Listens to hardware ring detection via the `RingDriver` port
//! and forwards ring triggers to `AccessControllerHandle`.

use log::info;

use crate::ports::inbound::AccessControllerHandle;
use crate::ports::RingDriver;

/// Ring detection watcher service.
///
/// Generic over any `RingDriver` port implementation.
/// Dispatches ring events to `AccessControllerHandle`.
pub struct RingWatcher<R: RingDriver> {
    driver: R,
}

impl<R: RingDriver> RingWatcher<R> {
    /// Create a new ring watcher with a ring driver.
    pub fn new(driver: R) -> Self {
        Self { driver }
    }

    /// Run the ring detection loop (never returns).
    pub async fn run(&mut self, access: AccessControllerHandle<'_>) {
        loop {
            self.driver.wait_for_ring().await;
            info!("ring detected → triggering access controller");
            access.trigger_ring();
        }
    }
}
