//! ESP32-C6 platform initialization, peripherals, and board wiring.

#[cfg(target_os = "none")]
pub mod board;
#[cfg(target_os = "none")]
pub mod wireless;

pub struct C6Platform;

#[cfg(target_os = "none")]
mod target_init {
    use super::C6Platform;
    use crate::platform::{PlatformDriver, PlatformDrivers};

    #[allow(non_snake_case)]
    pub struct C6Peripherals {
        pub WIFI: esp_hal::peripherals::WIFI<'static>,
        pub BT: esp_hal::peripherals::BT<'static>,
        pub RNG: esp_hal::peripherals::RNG<'static>,
        pub ADC1: esp_hal::peripherals::ADC1<'static>,
        pub GPIO2: esp_hal::peripherals::GPIO2<'static>,
        pub GPIO9: esp_hal::peripherals::GPIO9<'static>,
        pub GPIO18: esp_hal::peripherals::GPIO18<'static>,
        pub GPIO19: esp_hal::peripherals::GPIO19<'static>,
        pub timg0_wdt: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
    }

    impl PlatformDriver for C6Platform {
        type Drivers = PlatformDrivers<C6Peripherals>;

        fn init() -> (Self, Self::Drivers) {
            let peripherals = esp_hal::init(esp_hal::Config::default());
            let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
            esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
            super::board::init_heap();

            let drivers = PlatformDrivers {
                peripherals: C6Peripherals {
                    WIFI: peripherals.WIFI,
                    BT: peripherals.BT,
                    RNG: peripherals.RNG,
                    ADC1: peripherals.ADC1,
                    GPIO2: peripherals.GPIO2,
                    GPIO9: peripherals.GPIO9,
                    GPIO18: peripherals.GPIO18,
                    GPIO19: peripherals.GPIO19,
                    timg0_wdt: timg0.wdt,
                },
            };

            (Self, drivers)
        }

        fn software_reset() -> ! {
            esp_hal::system::software_reset()
        }
    }
}

#[cfg(target_os = "none")]
pub use target_init::C6Peripherals;
