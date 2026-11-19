//! ESP32-S3 board profile, internal heap init, and peripheral wiring.

use core::mem::MaybeUninit;
use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
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
use log::{info, warn};
use static_cell::StaticCell;

use super::memory::{
    alloc_psram_uninit, allocate_webrtc_jitter_buffer, heap_region_is_present,
    internal_heap_layout, InternalHeapLayout, DCACHE_RECLAIMED_BYTES, INTERNAL_HEAP_BYTES,
};
use super::pins::{BoardPins, GpioClearMasks, RelayPins};
pub use super::wireless::BoardWireless;
use super::{S3Peripherals, S3Platform};
use crate::platform::{self, ActiveIndicator, ActiveOtaDriver, BoardHardware, PlatformDriver};

const S3_BOOT_RECLAIMED_BYTES: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

const S3_INTERNAL_HEAP_LAYOUT: InternalHeapLayout = internal_heap_layout(
    INTERNAL_HEAP_BYTES,
    S3_BOOT_RECLAIMED_BYTES,
    DCACHE_RECLAIMED_BYTES,
    0,
    DCACHE_RECLAIMED_BYTES,
);

pub type BoardAudioCtrl = Aic3204AudioDriver<
    esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>,
    esp_hal::gpio::Output<'static>,
>;

pub fn init_heap() {
    info!(
        "S3 internal heap layout standard={} reclaimed={} dcache_reclaimed={}",
        S3_INTERNAL_HEAP_LAYOUT.standard_bytes,
        S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes,
        S3_INTERNAL_HEAP_LAYOUT.dcache_reclaimed_bytes
    );
    if heap_region_is_present(S3_INTERNAL_HEAP_LAYOUT.standard_bytes) {
        heap_allocator!(size: S3_INTERNAL_HEAP_LAYOUT.standard_bytes);
    }
    if heap_region_is_present(S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes) {
        heap_allocator!(#[ram(reclaimed)] size: S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes);
    }
}

#[inline(always)]
pub fn force_bus_relays_low() {
    // SAFETY: ESP32-S3 GPIO0-31 and GPIO32-53 clear registers are aligned,
    // write-one-to-clear MMIO registers at these addresses. The masks derive
    // from the role-based S3 relay pins and only clear those outputs.
    unsafe {
        let board = BoardPins::SUPPORTED;
        let masks = GpioClearMasks::from(RelayPins {
            listen: board.bus_listen,
            talk: board.bus_talk,
            door: board.bus_door,
        });
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x6000_400C as *mut u32;
        const GPIO_OUT1_W1TC_REG: *mut u32 = 0x6000_4018 as *mut u32;
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, masks.gpio_low);
        core::ptr::write_volatile(GPIO_OUT1_W1TC_REG, masks.gpio_high);
    }
}

pub fn fill_secure_random(out: &mut [u8]) -> Result<(), SignalingError> {
    platform::entropy::fill_global(out).map_err(|_| SignalingError::NotReady)
}

pub fn allocate_dtls_srtp_resources() -> &'static mut DtlsSrtpResources {
    super::memory::allocate_dtls_srtp_resources()
}

pub fn allocate_webrtc_media_resources() -> &'static mut WebRtcMediaResources {
    super::memory::allocate_webrtc_media_resources()
}

pub fn allocate_matter_stack_storage() -> &'static mut MaybeUninit<MatterStackStorage> {
    alloc_psram_uninit::<MatterStackStorage>()
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
    let webrtc_jitter_buffer = allocate_webrtc_jitter_buffer();
    WebRtcMediaEngine::new_with_opus_channel_and_jitter_buffer(
        requests,
        state,
        access_handle,
        rx_audio.receiver(),
        tx_audio.sender(),
        None,
        webrtc_jitter_buffer,
        webrtc_resources,
        dtls_resource_cell,
    )
}

pub fn log_pre_matter_heap() {
    info!(
        "S3 internal heap free before Matter startup: {} bytes",
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into())
    );
}

pub async fn wire_board(
    platform: &mut S3Platform,
    peripherals: S3Peripherals,
    spawner: Spawner,
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    _call_state: &'static CallStateWatch,
    _system_state: &'static SystemStateWatch,
    initial_rx_gain: RxPgaGain,
    initial_tx_gain: TxDacGain,
) -> BoardHardware {
    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO4));
    static BUS: StaticCell<BusSafetyInterlock<GpioBusDriver>> = StaticCell::new();
    let bus = &*BUS.init(BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO17, peripherals.GPIO18, peripherals.GPIO40),
    )));

    // The S3 dev board uses physical I2S playback, but the codec capture path
    // is fed from the embedded test recording while no live intercom is wired.
    {
        static I2S_PIPELINE_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();
        let i2s0 = peripherals.I2S0;
        let dma_ch0 = peripherals.DMA_CH0;
        let gpio10 = peripherals.GPIO10;
        let gpio11 = peripherals.GPIO11;
        let gpio12 = peripherals.GPIO12;
        let gpio13 = peripherals.GPIO13;
        let gpio14 = peripherals.GPIO14;

        platform.spawn_audio_worker(spawner, move |audio_spawner| {
            let i2s_pipeline = intercom_adapter_audio::I2sAudioPipeline::new(
                i2s0, dma_ch0, gpio10, gpio11, gpio12, gpio13, gpio14,
            )
            .expect("S3 I2S0 init");
            audio_spawner.spawn(
                platform::i2s_audio_worker(
                    i2s_pipeline,
                    rx_audio,
                    tx_audio,
                    intercom_adapter_audio::I2sCaptureSource::EmbeddedTestSample,
                )
                .expect("Failed to create S3 I2S audio worker task"),
            );
            I2S_PIPELINE_READY.signal(());
        });

        I2S_PIPELINE_READY.wait().await;
    }

    let audio_ctrl = {
        let i2c_config = esp_hal::i2c::master::Config::default()
            .with_frequency(esp_hal::time::Rate::from_khz(400));
        let i2c0 = esp_hal::i2c::master::I2c::new(peripherals.I2C0, i2c_config)
            .expect("S3 I2C0 init")
            .with_sda(peripherals.GPIO8)
            .with_scl(peripherals.GPIO9);
        let codec_rst = esp_hal::gpio::Output::new(
            peripherals.GPIO21,
            esp_hal::gpio::Level::Low,
            esp_hal::gpio::OutputConfig::default(),
        );
        let mut audio_ctrl = Aic3204AudioDriver::new(i2c0, codec_rst);
        if let Err(error) = audio_ctrl.init(initial_rx_gain, initial_tx_gain).await {
            warn!(
                "S3 TLV320AIC3204 init failed (bench without codec?): {:?}",
                error
            );
        }
        audio_ctrl
    };

    static INDICATOR: StaticCell<ActiveIndicator> = StaticCell::new();
    #[cfg(not(feature = "led-smartled"))]
    let indicator: &'static ActiveIndicator = &*INDICATOR.init(
        intercom_adapter_gpio::led::GpioLedDriver::new(peripherals.GPIO47),
    );
    #[cfg(feature = "led-smartled")]
    let indicator: &'static ActiveIndicator = &*INDICATOR.init(
        crate::devkit_led::DevKitLedDriver::new(peripherals.RMT, peripherals.GPIO47),
    );

    #[cfg(not(feature = "led-smartled"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO16).with_indicator(indicator),
    );
    #[cfg(feature = "led-smartled")]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO16).with_hold_feedback(indicator),
    );

    let ota_driver: ActiveOtaDriver = platform::create_s3_ota_driver();

    BoardHardware {
        wireless: BoardWireless {
            wifi: peripherals.WIFI,
            bt: peripherals.BT,
        },
        bus,
        ring,
        audio_ctrl,
        indicator,
        boot_button,
        ota_driver,
    }
}
