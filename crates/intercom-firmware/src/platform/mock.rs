//! Host implementation used by firmware integration tests.

use super::{PlatformDriver, PlatformDrivers};

#[derive(Default)]
pub struct MockPlatform;

impl PlatformDriver for MockPlatform {
    type Drivers = PlatformDrivers<()>;

    fn init() -> (Self, Self::Drivers) {
        (Self, PlatformDrivers { peripherals: () })
    }

    fn spawn_audio_worker(
        &mut self,
        spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    ) {
        worker(spawner);
    }
}
