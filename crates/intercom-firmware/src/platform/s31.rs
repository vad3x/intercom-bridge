//! ESP32-S31 platform initialization and centralized pin mapping.

#[cfg(target_arch = "riscv32")]
use super::{PlatformDriver, PlatformDrivers, PlatformPeripherals};

/// S31 board GPIO assignments. Keep these together when aligning firmware to a PCB revision.
pub mod pins {
    pub const RING_INPUT: u8 = 2;
    pub const BUS_LISTEN: u8 = 7;
    pub const BOOT_BUTTON: u8 = 9;
    pub const BUS_TALK: u8 = 10;
    pub const CODEC_RESET_N: u8 = 11;
    pub const I2S_MCLK: u8 = 15;
    pub const BUS_DOOR: u8 = 18;
    pub const INDICATOR_LED: u8 = 19;
    pub const I2S_BCLK: u8 = 20;
    pub const I2S_WS: u8 = 21;
    pub const I2S_DOUT: u8 = 22;
    pub const I2S_DIN: u8 = 23;
    pub const CODEC_I2C_SDA: u8 = 0;
    pub const CODEC_I2C_SCL: u8 = 1;
}

pub struct S31Platform {
    #[cfg(target_arch = "riscv32")]
    cpu_control: Option<esp_hal::peripherals::CPU_CTRL<'static>>,
    #[cfg(target_arch = "riscv32")]
    core1_interrupt: Option<esp_hal::peripherals::FROM_CPU_INTR1<'static>>,
}

#[cfg(target_arch = "riscv32")]
pub fn allocate_webrtc_jitter_buffer()
-> &'static mut intercom_adapter_webrtc::session::WebRtcJitterBuffer {
    let layout = core::alloc::Layout::new::<intercom_adapter_webrtc::session::WebRtcJitterBuffer>();
    let allocation =
        unsafe { esp_alloc::HEAP.alloc_caps(esp_alloc::MemoryCapability::External.into(), layout) };
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

#[cfg(target_arch = "riscv32")]
static APP_CORE_STACK: static_cell::StaticCell<esp_hal::system::Stack<32_768>> =
    static_cell::StaticCell::new();

#[cfg(target_arch = "riscv32")]
static CORE1_EXECUTOR: static_cell::StaticCell<esp_rtos::embassy::Executor> =
    static_cell::StaticCell::new();

#[cfg(target_arch = "riscv32")]
impl PlatformDriver for S31Platform {
    type Drivers = PlatformDrivers<PlatformPeripherals>;

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

        let cpu_control = peripherals.CPU_CTRL;
        let core1_interrupt = peripherals.FROM_CPU_INTR1;
        let esp_hal::peripherals::Peripherals {
            WIFI,
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
            RMT,
            ..
        } = peripherals;

        let drivers = PlatformDrivers {
            peripherals: PlatformPeripherals {
                WIFI,
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
                RMT,
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
