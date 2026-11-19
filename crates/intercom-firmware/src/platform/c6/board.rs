//! ESP32-C6 board profile, heap initialization, and peripheral wiring.

use core::mem::MaybeUninit;
use embassy_executor::Spawner;
#[cfg(not(feature = "devkit"))]
use embassy_sync::signal::Signal;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_alloc::heap_allocator;
use esp_hal::ram;
use esp_metadata_generated::memory_range;
use intercom_adapter_audio::Aic3204AudioDriver;
use intercom_adapter_gpio::{
    boot_button::{GpioBootButtonConfig, GpioBootButtonDriver},
    bus::{GpioBusConfig, GpioBusDriver},
    ring::{GpioRingConfig, GpioRingDriver},
};
use intercom_adapter_matter::MatterStackStorage;
use intercom_adapter_webrtc::{
    dtls::{DtlsSrtpResourceCell, DtlsSrtpResources},
    session::{WebRtcMediaEngine, WebRtcMediaResources, WebRtcSessionConfig, WebRtcSessionHandle},
    signaling::SignalingError,
};
use intercom_core::{
    domain::policy::{RxPgaGain, TxDacGain},
    events::{CallStateWatch, SystemStateWatch},
    ports::inbound::AccessControllerHandle,
    service::BusSafetyInterlock,
};
use log::warn;
use static_cell::StaticCell;

pub use super::wireless::BoardWireless;
use super::{C6Peripherals, C6Platform};
use crate::config;
#[cfg(not(feature = "devkit"))]
use crate::platform::PlatformDriver;
use crate::platform::{self, ActiveIndicator, ActiveOtaDriver, BoardHardware, ReclaimedCell};

const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

pub type BoardAudioCtrl = Aic3204AudioDriver<
    esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>,
    esp_hal::gpio::Output<'static>,
>;

pub fn init_heap() {
    heap_allocator!(size: config::HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM - core::mem::size_of::<ReclaimedCell<DtlsSrtpResources>>());
}

#[inline(always)]
pub fn force_bus_relays_low() {
    // SAFETY: ESP32-C6 GPIO.out_w1tc is the aligned write-one-to-clear register
    // at this address. The mask matches the C6 PCB's relay output pins.
    unsafe {
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x6009_100C as *mut u32;
        const BUS_RELAYS_GPIO_MASK: u32 = (1 << 7) | (1 << 10) | (1 << 18);
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, BUS_RELAYS_GPIO_MASK);
    }
}

pub fn fill_secure_random(out: &mut [u8]) -> Result<(), SignalingError> {
    let trng = esp_hal::rng::Trng::try_new().map_err(|_| SignalingError::NotReady)?;
    trng.read(out);
    Ok(())
}

pub fn allocate_dtls_srtp_resources() -> &'static mut DtlsSrtpResources {
    #[esp_hal::ram(reclaimed)]
    static WEBRTC_DTLS_RESOURCES: ReclaimedCell<DtlsSrtpResources> = ReclaimedCell::new();
    WEBRTC_DTLS_RESOURCES.init(DtlsSrtpResources::new())
}

pub fn allocate_webrtc_media_resources() -> &'static mut WebRtcMediaResources {
    static WEBRTC_RESOURCES: StaticCell<WebRtcMediaResources> = StaticCell::new();
    WEBRTC_RESOURCES.init(WebRtcMediaResources::new())
}

pub fn allocate_matter_stack_storage() -> &'static mut MaybeUninit<MatterStackStorage> {
    static MATTER_STACK_STORAGE: StaticCell<MatterStackStorage> = StaticCell::new();
    MATTER_STACK_STORAGE.uninit()
}

pub fn create_webrtc_engine(
    requests: &'static Channel<CriticalSectionRawMutex, WebRtcSessionConfig, 1>,
    state: &'static portable_atomic::AtomicU8,
    access_handle: AccessControllerHandle<'static>,
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    webrtc_resources: &'static mut WebRtcMediaResources,
    dtls_resource_cell: &'static DtlsSrtpResourceCell<'static>,
) -> (WebRtcMediaEngine<'static>, WebRtcSessionHandle<'static>) {
    WebRtcMediaEngine::new(
        requests,
        state,
        access_handle,
        rx_audio.receiver(),
        tx_audio.sender(),
        webrtc_resources,
        dtls_resource_cell,
    )
}

pub fn log_pre_matter_heap() {}

pub async fn wire_board(
    platform: &mut C6Platform,
    peripherals: C6Peripherals,
    spawner: Spawner,
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    _call_state: &'static CallStateWatch,
    _system_state: &'static SystemStateWatch,
    initial_rx_gain: RxPgaGain,
    initial_tx_gain: TxDacGain,
) -> BoardHardware {
    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    static BUS: StaticCell<BusSafetyInterlock<GpioBusDriver>> = StaticCell::new();
    let bus = &*BUS.init(BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO7, peripherals.GPIO10, peripherals.GPIO18),
    )));

    // Start I2S clocks before releasing the C6 codec reset (disabled on DevKit).
    #[cfg(not(feature = "devkit"))]
    {
        static I2S_PIPELINE_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();
        let i2s0 = peripherals.I2S0;
        let dma_ch0 = peripherals.DMA_CH0;
        let gpio22 = peripherals.GPIO22;
        let gpio23 = peripherals.GPIO23;
        let gpio15 = peripherals.GPIO15;
        let gpio20 = peripherals.GPIO20;
        let gpio21 = peripherals.GPIO21;

        platform.spawn_audio_worker(spawner, move |audio_spawner| {
            let i2s_pipeline = intercom_adapter_audio::I2sAudioPipeline::new(
                i2s0, dma_ch0, gpio22, gpio23, gpio15, gpio20, gpio21,
            )
            .expect("I2S0 init");
            audio_spawner.spawn(
                platform::i2s_audio_worker(
                    i2s_pipeline,
                    rx_audio,
                    tx_audio,
                    intercom_adapter_audio::I2sCaptureSource::Codec,
                )
                .expect("Failed to create I2S audio worker task"),
            );
            I2S_PIPELINE_READY.signal(());
        });

        I2S_PIPELINE_READY.wait().await;
    }
    #[cfg(feature = "devkit")]
    let _ = (platform, spawner, rx_audio, tx_audio);

    let audio_ctrl = {
        let i2c_config = esp_hal::i2c::master::Config::default()
            .with_frequency(esp_hal::time::Rate::from_khz(400));
        let i2c0 = esp_hal::i2c::master::I2c::new(peripherals.I2C0, i2c_config)
            .expect("I2C0 init")
            .with_sda(peripherals.GPIO0)
            .with_scl(peripherals.GPIO1);
        let codec_rst = esp_hal::gpio::Output::new(
            peripherals.GPIO11,
            esp_hal::gpio::Level::Low,
            esp_hal::gpio::OutputConfig::default(),
        );
        let mut audio_ctrl = Aic3204AudioDriver::new(i2c0, codec_rst);
        if let Err(error) = audio_ctrl.init(initial_rx_gain, initial_tx_gain).await {
            warn!(
                "TLV320AIC3204 init failed (bench without codec?): {:?}",
                error
            );
        }
        audio_ctrl
    };

    static INDICATOR: StaticCell<ActiveIndicator> = StaticCell::new();
    #[cfg(not(feature = "led-smartled"))]
    let indicator: &'static ActiveIndicator = &*INDICATOR.init(
        intercom_adapter_gpio::led::GpioLedDriver::new(peripherals.GPIO19),
    );
    #[cfg(feature = "led-smartled")]
    let indicator: &'static ActiveIndicator = &*INDICATOR.init(
        crate::devkit_led::DevKitLedDriver::new(peripherals.RMT, peripherals.GPIO8),
    );

    #[cfg(not(feature = "led-smartled"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_indicator(indicator),
    );
    #[cfg(feature = "led-smartled")]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_hold_feedback(indicator),
    );

    let ota_driver: ActiveOtaDriver = platform::create_default_ota_driver();

    BoardHardware {
        wireless: BoardWireless {
            wifi: peripherals.WIFI,
            bt: peripherals.BT,
            rng: Some(peripherals.RNG),
            adc1: Some(peripherals.ADC1),
            _trng_source: None,
        },
        bus,
        ring,
        audio_ctrl,
        indicator,
        boot_button,
        ota_driver,
    }
}
