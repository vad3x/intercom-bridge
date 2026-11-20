//! Host implementation used by firmware integration tests.

use super::{PlatformDriver, PlatformDrivers};

#[derive(Default)]
pub struct MockPlatform;

impl PlatformDriver for MockPlatform {
    type Drivers = PlatformDrivers<()>;

    const MATTER_BUMP_SIZE: usize = 38_000;

    fn init() -> (Self, Self::Drivers) {
        (Self, PlatformDrivers { peripherals: () })
    }

    fn force_bus_relays_low() {}

    fn software_reset() -> ! {
        panic!("Host reboot requested")
    }

    fn spawn_audio_worker(
        &mut self,
        spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    ) {
        worker(spawner);
    }
}
