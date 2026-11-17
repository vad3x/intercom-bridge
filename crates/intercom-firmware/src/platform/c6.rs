//! ESP32-C6 platform initialization.

#[cfg(target_arch = "riscv32")]
use super::{PlatformDriver, PlatformDrivers, PlatformPeripherals};

pub struct C6Platform;

#[cfg(target_arch = "riscv32")]
impl PlatformDriver for C6Platform {
    type Drivers = PlatformDrivers<PlatformPeripherals>;

    fn init() -> (Self, Self::Drivers) {
        let peripherals = esp_hal::init(
            esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()),
        );
        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

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
            peripherals: PlatformPeripherals {
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
            },
        };

        (Self, drivers)
    }

    fn spawn_audio_worker(
        &mut self,
        spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    ) {
        worker(spawner);
    }
}
