//! ESP32-S31 platform initialization and centralized pin mapping.

#[cfg(target_arch = "riscv32")]
pub mod board;
pub mod pins;
#[cfg(target_arch = "riscv32")]
pub mod wireless;

#[cfg(not(target_arch = "riscv32"))]
pub struct S31Platform;

#[cfg(target_arch = "riscv32")]
mod target_init {
    use crate::platform::{PlatformDriver, PlatformDrivers};

    #[allow(non_snake_case)]
    pub struct S31Peripherals {
        pub WIFI: esp_hal::peripherals::WIFI<'static>,
        pub GPIO2: esp_hal::peripherals::GPIO2<'static>,
        pub GPIO7: esp_hal::peripherals::GPIO7<'static>,
        pub GPIO8: esp_hal::peripherals::GPIO8<'static>,
        pub GPIO9: esp_hal::peripherals::GPIO9<'static>,
        pub GPIO10: esp_hal::peripherals::GPIO10<'static>,
        pub GPIO18: esp_hal::peripherals::GPIO18<'static>,
        pub GPIO19: esp_hal::peripherals::GPIO19<'static>,
        pub RMT: esp_hal::peripherals::RMT<'static>,
        pub timg0_wdt: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
    }

    pub struct S31Platform {
        cpu_control: Option<esp_hal::peripherals::CPU_CTRL<'static>>,
        core1_interrupt: Option<esp_hal::peripherals::FROM_CPU_INTR1<'static>>,
    }

    pub fn allocate_webrtc_jitter_buffer(
    ) -> &'static mut intercom_adapter_webrtc::session::WebRtcJitterBuffer {
        let layout =
            core::alloc::Layout::new::<intercom_adapter_webrtc::session::WebRtcJitterBuffer>();
        let allocation = unsafe {
            esp_alloc::HEAP.alloc_caps(esp_alloc::MemoryCapability::External.into(), layout)
        };
        assert!(
            !allocation.is_null(),
            "S31 PSRAM cannot hold the WebRTC jitter buffer"
        );

        let buffer = allocation.cast::<intercom_adapter_webrtc::session::WebRtcJitterBuffer>();
        unsafe {
            buffer.write(intercom_adapter_webrtc::session::WebRtcJitterBuffer::new(0));
            &mut *buffer
        }
    }

    static APP_CORE_STACK: static_cell::StaticCell<esp_hal::system::Stack<32_768>> =
        static_cell::StaticCell::new();

    static CORE1_EXECUTOR: static_cell::StaticCell<esp_rtos::embassy::Executor> =
        static_cell::StaticCell::new();

    impl PlatformDriver for S31Platform {
        type Drivers = PlatformDrivers<S31Peripherals>;

        const MATTER_BUMP_SIZE: usize = 38_000;

        fn is_wdt_reset() -> bool {
            use esp_hal::rtc_cntl::SocResetReason;
            matches!(
                esp_hal::system::reset_reason(),
                Some(
                    SocResetReason::CoreMwdt0
                        | SocResetReason::CoreMwdt1
                        | SocResetReason::CoreRwdt
                        | SocResetReason::CpuMwdt
                        | SocResetReason::CpuRwdt
                )
            )
        }

        fn init() -> (Self, Self::Drivers) {
            let peripherals = esp_hal::init(
                esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()),
            );
            let psram = esp_hal::psram::Psram::new(peripherals.PSRAM, Default::default());
            let (psram_start, psram_size) = psram.raw_parts();
            assert!(psram_size > 0, "S31 PSRAM initialization failed");
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
                GPIO2,
                GPIO7,
                GPIO8,
                GPIO9,
                GPIO10,
                GPIO18,
                GPIO19,
                RMT,
                ..
            } = peripherals;

            let drivers = PlatformDrivers {
                peripherals: S31Peripherals {
                    WIFI,
                    GPIO2,
                    GPIO7,
                    GPIO8,
                    GPIO9,
                    GPIO10,
                    GPIO18,
                    GPIO19,
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
            let stack = APP_CORE_STACK.init(esp_hal::system::Stack::new());

            // esp-rtos provides the second-core scheduler and routes the Embassy
            // executor's wakeups through Core 1's software interrupt.
            esp_rtos::start_second_core(cpu_control, core1_interrupt, stack, move || {
                let executor = CORE1_EXECUTOR.init(esp_rtos::embassy::Executor::new());
                executor.run(|core1_spawner| worker(core1_spawner));
            });
        }
    }
}

#[cfg(target_arch = "riscv32")]
pub use target_init::{allocate_webrtc_jitter_buffer, S31Peripherals, S31Platform};
