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
        pub I2C0: esp_hal::peripherals::I2C0<'static>,
        pub GPIO0: esp_hal::peripherals::GPIO0<'static>,
        pub GPIO1: esp_hal::peripherals::GPIO1<'static>,
        pub GPIO2: esp_hal::peripherals::GPIO2<'static>,
        pub GPIO7: esp_hal::peripherals::GPIO7<'static>,
        pub GPIO8: esp_hal::peripherals::GPIO8<'static>,
        pub GPIO9: esp_hal::peripherals::GPIO9<'static>,
        pub GPIO10: esp_hal::peripherals::GPIO10<'static>,
        pub GPIO11: esp_hal::peripherals::GPIO11<'static>,
        pub GPIO15: esp_hal::peripherals::GPIO15<'static>,
        pub GPIO18: esp_hal::peripherals::GPIO18<'static>,
        pub GPIO19: esp_hal::peripherals::GPIO19<'static>,
        pub GPIO20: esp_hal::peripherals::GPIO20<'static>,
        pub GPIO21: esp_hal::peripherals::GPIO21<'static>,
        pub GPIO22: esp_hal::peripherals::GPIO22<'static>,
        pub GPIO23: esp_hal::peripherals::GPIO23<'static>,
        pub I2S0: esp_hal::peripherals::I2S0<'static>,
        pub DMA_CH0: esp_hal::peripherals::DMA_CH0<'static>,
        pub RMT: esp_hal::peripherals::RMT<'static>,
        pub timg0_wdt: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
    }

    impl PlatformDriver for C6Platform {
        type Drivers = PlatformDrivers<C6Peripherals>;

        // With PreexistingWireless, peak Matter bump usage is about 32,144 bytes.
        // This leaves about 1.8 KiB of headroom and reserves SRAM for the main stack.
        const MATTER_BUMP_SIZE: usize = 34_000;

        fn init() -> (Self, Self::Drivers) {
            let peripherals = esp_hal::init(
                esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()),
            );
            let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
            esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
            Self::force_bus_relays_low();
            super::board::init_heap();

            let esp_hal::peripherals::Peripherals {
                WIFI,
                BT,
                RNG,
                ADC1,
                I2C0,
                GPIO0,
                GPIO1,
                GPIO2,
                GPIO7,
                GPIO8,
                GPIO9,
                GPIO10,
                GPIO11,
                GPIO15,
                GPIO18,
                GPIO19,
                GPIO20,
                GPIO21,
                GPIO22,
                GPIO23,
                I2S0,
                DMA_CH0,
                RMT,
                ..
            } = peripherals;

            let drivers = PlatformDrivers {
                peripherals: C6Peripherals {
                    WIFI,
                    BT,
                    RNG,
                    ADC1,
                    I2C0,
                    GPIO0,
                    GPIO1,
                    GPIO2,
                    GPIO7,
                    GPIO8,
                    GPIO9,
                    GPIO10,
                    GPIO11,
                    GPIO15,
                    GPIO18,
                    GPIO19,
                    GPIO20,
                    GPIO21,
                    GPIO22,
                    GPIO23,
                    I2S0,
                    DMA_CH0,
                    RMT,
                    timg0_wdt: timg0.wdt,
                },
            };

            (Self, drivers)
        }

        fn force_bus_relays_low() {
            super::board::force_bus_relays_low();
        }

        fn software_reset() -> ! {
            esp_hal::system::software_reset();
        }

        fn spawn_audio_worker(
            &mut self,
            spawner: embassy_executor::Spawner,
            worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
        ) {
            worker(spawner);
        }
    }
}

#[cfg(target_os = "none")]
pub use target_init::C6Peripherals;
