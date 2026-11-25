//! ESP32-S31 board profile, heap init, synthetic audio worker, and peripheral wiring.

use core::mem::MaybeUninit;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_alloc::heap_allocator;
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
    ports::{inbound::AccessControllerHandle, NoopAudioControlDriver},
    service::BusSafetyInterlock,
};
use log::{info, warn};
use static_cell::StaticCell;

pub use super::wireless::BoardWireless;
use super::{allocate_webrtc_jitter_buffer, S31Peripherals, S31Platform};
use crate::config;
use crate::platform::{
    self, ActiveIndicator, ActiveOtaDriver, BoardHardware, PlatformDriver, ReclaimedCell,
};

pub type BoardAudioCtrl = NoopAudioControlDriver;

static OPUS_CHANNEL: intercom_media::OpusChannel = intercom_media::OpusChannel::new();

pub fn init_heap() {
    heap_allocator!(size: config::HEAP_SIZE);
}

#[inline(always)]
pub fn force_bus_relays_low() {
    // SAFETY: ESP32-S31 GPIO.out_w1tc is the aligned write-one-to-clear register
    // at this address. The mask matches the S31 board profile's relay pins.
    unsafe {
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x2058_300C as *mut u32;
        const BUS_RELAYS_GPIO_MASK: u32 = (1 << 7) | (1 << 10) | (1 << 18);
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, BUS_RELAYS_GPIO_MASK);
    }
}

pub fn fill_secure_random(out: &mut [u8]) -> Result<(), SignalingError> {
    esp_hal::rng::Rng::new().read(out);
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

pub fn allocate_matter_stack_storage(
) -> &'static mut MaybeUninit<MatterStackStorage<{ S31Platform::MATTER_BUMP_SIZE }>> {
    static MATTER_STACK_STORAGE: StaticCell<MatterStackStorage<{ S31Platform::MATTER_BUMP_SIZE }>> =
        StaticCell::new();
    MATTER_STACK_STORAGE.uninit()
}

#[inline(never)]
pub fn allocate_enet_resources() -> platform::EnetResources {
    use rs_matter_embassy::matter::utils::init::InitMaybeUninit;

    static STACK_RESOURCES: StaticCell<rs_matter_embassy::enet::EnetMatterStackResources> =
        StaticCell::new();
    static UDP_BUFFERS: StaticCell<rs_matter_embassy::enet::EnetMatterUdpBuffers> =
        StaticCell::new();
    static TCP_BUFFERS: StaticCell<rs_matter_embassy::enet::EnetMatterTcpBuffers> =
        StaticCell::new();
    static MATTER_TCP_BUFFERS: StaticCell<rs_matter_embassy::enet::EnetMatterTcpTransportBuffers> =
        StaticCell::new();
    static TCP_CONFIG: rs_matter::transport::network::tcp::TcpConfig =
        rs_matter::transport::network::tcp::TcpConfig::new();

    platform::EnetResources {
        stack_resources: unsafe { STACK_RESOURCES.uninit().assume_init_mut() },
        buffers: platform::EnetMatterBuffers {
            udp_buffers: UDP_BUFFERS
                .uninit()
                .init_with(rs_matter_embassy::enet::EnetMatterUdpBuffers::init()),
            tcp_buffers: TCP_BUFFERS
                .uninit()
                .init_with(rs_matter_embassy::enet::EnetMatterTcpBuffers::init()),
            matter_tcp_buffers: MATTER_TCP_BUFFERS
                .uninit()
                .init_with(rs_matter_embassy::enet::EnetMatterTcpTransportBuffers::init()),
            tcp_config: &TCP_CONFIG,
        },
    }
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
        Some(&OPUS_CHANNEL),
        webrtc_jitter_buffer,
        webrtc_resources,
        dtls_resource_cell,
    )
}

pub fn log_pre_matter_heap() {}

#[embassy_executor::task]
async fn s31_synthetic_audio_worker(
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    opus_channel: &'static intercom_media::OpusChannel,
    mut call_state: intercom_core::events::CallStateReceiver<'static>,
    mut system_state: intercom_core::events::SystemStateReceiver<'static>,
) {
    static PIPELINE: StaticCell<platform::synthetic_audio::S31SyntheticAudioPipeline> =
        StaticCell::new();
    let pipeline = PIPELINE.init(platform::synthetic_audio::S31SyntheticAudioPipeline::new());
    let rx_sender = rx_audio.sender();
    let tx_receiver = tx_audio.receiver();
    let mut schedule =
        intercom_media::AudioCaptureSchedule::new(embassy_time::Instant::now().as_millis());
    let mut drop_window = intercom_media::AudioFrameDropWindow::new();
    let mut simulated_overruns = 0u64;
    loop {
        let (call_update, system_update) =
            platform::take_audio_state_updates(&mut call_state, &mut system_state);
        if let Some(snapshot) = call_update {
            info!("S31 audio worker observed call state: {:?}", snapshot);
        }
        if let Some(state) = system_update {
            info!("S31 audio worker observed system state: {:?}", state);
        }

        let now_ms = embassy_time::Instant::now().as_millis();
        embassy_time::Timer::after_millis(schedule.delay_until_next_ms(now_ms)).await;
        let batch = schedule.take_due_frames(embassy_time::Instant::now().as_millis());
        simulated_overruns = simulated_overruns.saturating_add(batch.overruns);

        for _ in 0..batch.frames {
            let (source, packet, decoded) = match pipeline.next_frame() {
                Ok(frame) => frame,
                Err(error) => {
                    warn!("S31 synthetic Opus pipeline failed: {:?}", error);
                    super::S31Platform::software_reset();
                }
            };
            let levels = intercom_media::compute_frame_levels_dbfs(&source);
            intercom_core::domain::diagnostics::update_rx_levels_dbfs(
                levels.rms_dbfs,
                levels.peak_dbfs,
            );
            let decoded_levels = intercom_media::compute_frame_levels_dbfs(&decoded);
            let pcm_dropped = rx_sender.try_send(source).is_err();
            let opus_dropped = opus_channel.try_send(packet).is_err();
            let _ = tx_receiver.try_receive();

            if let Some(queue_drops) = drop_window.record(pcm_dropped || opus_dropped) {
                info!(
                    "S31 synthetic audio frames={} simulated_overruns={} queue_drops={} last_decoded_rms_dbfs={}",
                    intercom_media::AUDIO_FRAME_DROP_WINDOW_FRAMES,
                    simulated_overruns,
                    queue_drops,
                    decoded_levels.rms_dbfs,
                );
                simulated_overruns = 0;
            }
        }
    }
}

pub async fn wire_board(
    platform: &mut S31Platform,
    peripherals: S31Peripherals,
    spawner: Spawner,
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    call_state: &'static CallStateWatch,
    system_state: &'static SystemStateWatch,
    _initial_rx_gain: RxPgaGain,
    _initial_tx_gain: TxDacGain,
) -> BoardHardware {
    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    static BUS: StaticCell<BusSafetyInterlock<GpioBusDriver>> = StaticCell::new();
    let bus = &*BUS.init(BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO7, peripherals.GPIO10, peripherals.GPIO18),
    )));

    let core1_call_state_receiver = call_state.receiver().unwrap();
    let core1_system_state_receiver = system_state.receiver().unwrap();
    platform.spawn_audio_worker(spawner, move |audio_spawner| {
        audio_spawner.spawn(
            s31_synthetic_audio_worker(
                rx_audio,
                tx_audio,
                &OPUS_CHANNEL,
                core1_call_state_receiver,
                core1_system_state_receiver,
            )
            .expect("Failed to create S31 synthetic audio worker task"),
        );
    });

    let audio_ctrl = NoopAudioControlDriver;

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
            wifi: Some(peripherals.WIFI),
            wifi_controller: None,
            enet_stack: None,
            enet_buffers: None,
        },
        bus,
        ring,
        audio_ctrl,
        indicator,
        boot_button,
        ota_driver,
        timg0_wdt: peripherals.timg0_wdt,
    }
}
