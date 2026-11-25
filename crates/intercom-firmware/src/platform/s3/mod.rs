//! ESP32-S3 board profile and platform initialization.

#[cfg(target_os = "none")]
pub mod board;
pub mod memory;
pub mod pins;
#[cfg(target_os = "none")]
pub mod wireless;

pub use memory::{
    heap_region_is_present, heap_region_sizes, internal_heap_layout, reclaimed_heap_size,
    InternalHeapLayout, DCACHE_RECLAIMED_BYTES, INTERNAL_HEAP_BYTES,
};
pub use pins::{BoardPins, GpioClearMasks, RelayPins};

/// S3 platform driver owning Core 1 resources until the audio worker is started.
#[cfg(not(target_os = "none"))]
pub struct S3Platform;

#[cfg(target_os = "none")]
mod target_init {
    use crate::platform::{PlatformDriver, PlatformDrivers};

    const AUDIO_STACK_BYTES: usize = 32_768;
    const STACK_WATERMARK: u8 = 0xA5;

    #[repr(transparent)]
    struct ReclaimedCell<T>(core::cell::UnsafeCell<core::mem::MaybeUninit<T>>);

    // SAFETY: Access is serialized by the one-time initialization guard below.
    unsafe impl<T> esp_hal::Uninit for ReclaimedCell<T> {}
    unsafe impl<T> Sync for ReclaimedCell<T> {}

    static APP_CORE_STACK_INITIALIZED: portable_atomic::AtomicBool =
        portable_atomic::AtomicBool::new(false);

    impl<T> ReclaimedCell<T> {
        const fn new() -> Self {
            Self(core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()))
        }

        fn init(&'static self, value: T) -> &'static mut T {
            assert!(
                APP_CORE_STACK_INITIALIZED
                    .compare_exchange(
                        false,
                        true,
                        portable_atomic::Ordering::AcqRel,
                        portable_atomic::Ordering::Relaxed,
                    )
                    .is_ok(),
                "S3 Core 1 stack can only be initialized once"
            );
            unsafe { (*self.0.get()).write(value) }
        }
    }

    static AUDIO_STACK_LOW_WATER: portable_atomic::AtomicUsize =
        portable_atomic::AtomicUsize::new(usize::MAX);

    static AUDIO_STACK_BOTTOM: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

    fn fill_radio_entropy(output: &mut [u8]) -> bool {
        esp_hal::rng::Rng::new().read(output);
        true
    }

    #[esp_hal::ram(unstable(dcache_reclaimed))]
    static APP_CORE_STACK: ReclaimedCell<esp_hal::system::Stack<32_768>> = ReclaimedCell::new();

    static CORE1_EXECUTOR: static_cell::StaticCell<esp_rtos::embassy::Executor> =
        static_cell::StaticCell::new();

    #[embassy_executor::task]
    async fn sample_audio_stack_watermark() {
        loop {
            let bottom = AUDIO_STACK_BOTTOM.load(portable_atomic::Ordering::Acquire) as *const u8;
            let untouched = (0..AUDIO_STACK_BYTES)
                .take_while(|offset| {
                    (unsafe { core::ptr::read_volatile(bottom.add(*offset)) }) == STACK_WATERMARK
                })
                .count();
            AUDIO_STACK_LOW_WATER.fetch_min(untouched, portable_atomic::Ordering::Relaxed);
            embassy_time::Timer::after_millis(250).await;
        }
    }

    /// Minimum untouched bytes observed at the bottom of the Core 1 audio stack.
    pub fn audio_stack_free_low_water_bytes() -> usize {
        AUDIO_STACK_LOW_WATER.load(portable_atomic::Ordering::Relaxed)
    }

    #[allow(non_snake_case)]
    pub struct S3Peripherals {
        pub WIFI: esp_hal::peripherals::WIFI<'static>,
        pub BT: esp_hal::peripherals::BT<'static>,
        pub I2C0: esp_hal::peripherals::I2C0<'static>,
        pub GPIO4: esp_hal::peripherals::GPIO4<'static>,
        pub GPIO8: esp_hal::peripherals::GPIO8<'static>,
        pub GPIO9: esp_hal::peripherals::GPIO9<'static>,
        pub GPIO10: esp_hal::peripherals::GPIO10<'static>,
        pub GPIO11: esp_hal::peripherals::GPIO11<'static>,
        pub GPIO12: esp_hal::peripherals::GPIO12<'static>,
        pub GPIO13: esp_hal::peripherals::GPIO13<'static>,
        pub GPIO14: esp_hal::peripherals::GPIO14<'static>,
        pub GPIO16: esp_hal::peripherals::GPIO16<'static>,
        pub GPIO17: esp_hal::peripherals::GPIO17<'static>,
        pub GPIO18: esp_hal::peripherals::GPIO18<'static>,
        pub GPIO21: esp_hal::peripherals::GPIO21<'static>,
        pub GPIO40: esp_hal::peripherals::GPIO40<'static>,
        pub GPIO47: esp_hal::peripherals::GPIO47<'static>,
        pub I2S0: esp_hal::peripherals::I2S0<'static>,
        pub DMA_CH0: esp_hal::peripherals::DMA_CH0<'static>,
        pub RMT: esp_hal::peripherals::RMT<'static>,
        pub timg0_wdt: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
    }

    pub struct S3Platform {
        cpu_control: Option<esp_hal::peripherals::CPU_CTRL<'static>>,
        core1_interrupt: Option<esp_hal::peripherals::FROM_CPU_INTR1<'static>>,
    }

    impl PlatformDriver for S3Platform {
        type Drivers = PlatformDrivers<S3Peripherals>;

        const MATTER_BUMP_SIZE: usize = 38_000;

        fn init() -> (Self, Self::Drivers) {
            let peripherals = esp_hal::init(
                esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()),
            );
            let mut startup_seed = [0; 32];
            {
                let entropy_source =
                    esp_hal::rng::TrngSource::new(peripherals.RNG, peripherals.ADC1);
                let trng = esp_hal::rng::Trng::try_new()
                    .expect("S3 SAR ADC entropy source must seed startup randomness");
                trng.read(&mut startup_seed);
                let _ = trng.downgrade();
                drop(entropy_source);
            }
            crate::platform::entropy::initialize_global(startup_seed);
            crate::platform::entropy::set_radio_entropy_source(fill_radio_entropy);

            let psram = esp_hal::psram::Psram::new(
                peripherals.PSRAM,
                esp_hal::psram::PsramConfig {
                    mode: esp_hal::psram::PsramMode::OctalSpi,
                    size: esp_hal::psram::PsramSize::Size(8 * 1024 * 1024),
                    ..Default::default()
                },
            );
            let (psram_start, psram_size) = psram.raw_parts();
            assert_eq!(
                psram_size,
                8 * 1024 * 1024,
                "ESP32-S3-N16R8 requires 8 MiB Octal SPI PSRAM"
            );
            unsafe {
                esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                    psram_start,
                    psram_size,
                    esp_alloc::MemoryCapability::External.into(),
                ));
            }

            let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
            esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
            Self::force_bus_relays_low();
            super::board::init_heap();
            let cpu_control = peripherals.CPU_CTRL;
            let core1_interrupt = peripherals.FROM_CPU_INTR1;

            let esp_hal::peripherals::Peripherals {
                WIFI,
                BT,
                I2C0,
                GPIO4,
                GPIO8,
                GPIO9,
                GPIO10,
                GPIO11,
                GPIO12,
                GPIO13,
                GPIO14,
                GPIO16,
                GPIO17,
                GPIO18,
                GPIO21,
                GPIO40,
                GPIO47,
                I2S0,
                DMA_CH0,
                RMT,
                ..
            } = peripherals;

            let drivers = PlatformDrivers {
                peripherals: S3Peripherals {
                    WIFI,
                    BT,
                    I2C0,
                    GPIO4,
                    GPIO8,
                    GPIO9,
                    GPIO10,
                    GPIO11,
                    GPIO12,
                    GPIO13,
                    GPIO14,
                    GPIO16,
                    GPIO17,
                    GPIO18,
                    GPIO21,
                    GPIO40,
                    GPIO47,
                    I2S0,
                    DMA_CH0,
                    RMT,
                    timg0_wdt: timg0.wdt,
                },
            };

            (
                Self {
                    cpu_control: Some(cpu_control),
                    core1_interrupt: Some(core1_interrupt),
                },
                drivers,
            )
        }

        fn force_bus_relays_low() {
            super::board::force_bus_relays_low();
        }

        fn software_reset() -> ! {
            esp_hal::system::software_reset();
        }

        fn spawn_audio_worker(
            &mut self,
            _spawner: embassy_executor::Spawner,
            worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
        ) {
            let cpu_control = self
                .cpu_control
                .take()
                .expect("Core 1 can only be started once");
            let core1_interrupt = self
                .core1_interrupt
                .take()
                .expect("Core 1 interrupt can only be consumed once");
            let stack = unsafe {
                let stack = APP_CORE_STACK.init(esp_hal::system::Stack::new());
                AUDIO_STACK_BOTTOM
                    .store(stack.bottom() as usize, portable_atomic::Ordering::Release);
                core::ptr::write_bytes(
                    stack.bottom().cast::<u8>(),
                    STACK_WATERMARK,
                    AUDIO_STACK_BYTES,
                );
                stack
            };
            esp_rtos::start_second_core(cpu_control, core1_interrupt, stack, move || {
                let executor = CORE1_EXECUTOR.init(esp_rtos::embassy::Executor::new());
                executor.run(|core1_spawner| {
                    core1_spawner.spawn(
                        sample_audio_stack_watermark().expect("Failed to create stack sampler"),
                    );
                    core1_spawner.spawn(
                        crate::cpu_load::sample_core_load(1)
                            .expect("Failed to create Core 1 CPU load sampler"),
                    );
                    worker(core1_spawner);
                });
            });
        }
    }
}

#[cfg(target_os = "none")]
pub use target_init::{audio_stack_free_low_water_bytes, S3Peripherals, S3Platform};

#[cfg(all(test, feature = "esp32s3"))]
mod tests {
    use super::pins::{EXPOSED_PINS, RESERVED_PINS};
    use super::{
        heap_region_is_present, heap_region_sizes, internal_heap_layout, reclaimed_heap_size,
        BoardPins, GpioClearMasks, RelayPins,
    };

    const STACK_WATERMARK: u8 = 0xA5;

    fn stack_watermark_bytes(stack: &[u8]) -> usize {
        stack
            .iter()
            .take_while(|byte| **byte == STACK_WATERMARK)
            .count()
    }

    #[test]
    fn validates_pin_roles_and_relay_clear_masks() {
        let board = BoardPins::SUPPORTED;
        let pins = board.all_signal_pins();

        let are_unique = pins
            .iter()
            .enumerate()
            .all(|(index, pin)| !pins[..index].contains(pin));
        let are_exposed = pins.iter().all(|pin| EXPOSED_PINS.contains(pin));
        let avoid_reserved = pins.iter().all(|pin| !RESERVED_PINS.contains(pin));
        let relay_masks = GpioClearMasks::from(RelayPins {
            listen: board.bus_listen,
            talk: board.bus_talk,
            door: board.bus_door,
        });

        assert!(are_unique, "board signal pins must not overlap");
        assert!(are_exposed, "every board signal pin must be exposed");
        assert!(
            avoid_reserved,
            "board signals must avoid flash, PSRAM, and console pins"
        );
        assert_eq!(relay_masks.gpio_low, (1 << 17) | (1 << 18));
        assert_eq!(relay_masks.gpio_high, 1 << (40 - 32));
        assert!(RESERVED_PINS.contains(&38));
    }

    #[test]
    fn devkit_rgb_led_uses_gpio47_rmt_driver() {
        assert!(cfg!(feature = "led-smartled"));
        assert_eq!(BoardPins::SUPPORTED.indicator_led, 47);
    }

    #[test]
    fn reclaims_bootloader_ram_without_changing_heap_budget() {
        let total_heap = 120 * 1024;
        let reclaimed_region = 73_744;
        let dtls_reservation = 32 * 1024;

        let reclaimed_heap = reclaimed_heap_size(total_heap, reclaimed_region, dtls_reservation);

        assert_eq!(reclaimed_heap, reclaimed_region - dtls_reservation);
        assert_eq!(total_heap - reclaimed_heap, 120 * 1024 - reclaimed_heap);
        assert_eq!(reclaimed_heap + (total_heap - reclaimed_heap), total_heap);
    }

    #[test]
    fn distributes_heap_across_boot_and_dcache_reclaimed_ram() {
        let layout = heap_region_sizes(120 * 1024, 73_744, 32 * 1024, 32 * 1024, 0);

        assert_eq!(layout.boot_reclaimed_bytes, 73_744 - 32 * 1024);
        assert_eq!(layout.dcache_reclaimed_bytes, 32 * 1024);
        assert_eq!(layout.standard_bytes, 120 * 1024 - 73_744);
        assert_eq!(
            layout.boot_reclaimed_bytes + layout.dcache_reclaimed_bytes + layout.standard_bytes,
            120 * 1024
        );
    }

    #[test]
    fn s3_release_heap_preserves_the_linker_main_stack_reserve() {
        let layout = internal_heap_layout(120 * 1024, 73_744, 32 * 1024, 0, 32 * 1024);

        assert_eq!(
            layout.standard_bytes + layout.boot_reclaimed_bytes + layout.dcache_reclaimed_bytes,
            crate::config::HEAP_SIZE
        );
        assert_eq!(crate::config::HEAP_SIZE, 120 * 1024);
        assert_eq!(layout.boot_reclaimed_bytes, 73_744);
        assert_eq!(layout.dcache_reclaimed_bytes, 0);
        assert_eq!(layout.standard_bytes, 49_136);
    }

    #[test]
    fn empty_heap_regions_are_not_registered() {
        assert!(!heap_region_is_present(0));
        assert!(heap_region_is_present(1));
    }

    #[test]
    fn reserves_dcache_reclaimed_ram_for_the_audio_worker_stack() {
        let layout = heap_region_sizes(120 * 1024, 73_744, 32 * 1024, 0, 32 * 1024);

        assert_eq!(layout.boot_reclaimed_bytes, 73_744);
        assert_eq!(layout.dcache_reclaimed_bytes, 0);
        assert_eq!(layout.standard_bytes, 49_136);
    }

    #[test]
    fn stack_watermark_counts_only_the_untouched_low_address_prefix() {
        let mut stack = [STACK_WATERMARK; 16];
        stack[12..].fill(0x00);

        let unused = stack_watermark_bytes(&stack);

        assert_eq!(unused, 12);
    }
}
