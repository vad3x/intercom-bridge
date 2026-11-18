//! Intercom Bridge - ESP32-C6 Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
//!
//! Hardware: PCB v2.0 with ESP32-C6
//! Protocol: Matter over WiFi (BLE for commissioning)
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![recursion_limit = "256"]
#![allow(unexpected_cfgs, async_fn_in_trait)]

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod config;
#[cfg(target_os = "none")]
mod reset;
#[cfg(target_os = "none")]
mod utils;
#[cfg(target_os = "none")]
mod version;

#[cfg(target_os = "none")]
use {
    self::reset::{FirmwarePersistenceReporter, FirmwareResetDriver},
    core::pin::pin,
    embassy_executor::Spawner,
    embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel},
    esp_alloc::heap_allocator,
    esp_backtrace as _,
    intercom_adapter_gpio::{
        boot_button::{GpioBootButtonConfig, GpioBootButtonDriver},
        bus::{GpioBusConfig, GpioBusDriver},
        ring::{GpioRingConfig, GpioRingDriver},
    },
    intercom_adapter_http::{HttpAudioMedia, HttpServer, HttpServerConfig},
    intercom_adapter_matter::{
        ClusterReadinessBarrier, DeviceInfo, IntercomAudioAvCluster, IntercomChime,
        IntercomDoorLock, IntercomDoorLockConfig, IntercomDoorbellSwitch, IntercomWebRtcHooks,
        MatterAccessControlDriver, MatterController, MatterControllerConfig, MatterPeripherals,
        MatterStackStorage,
    },
    intercom_adapter_webrtc::{
        dtls::{DtlsSrtpResourceCell, DtlsSrtpResources},
        session::{WebRtcMediaEngine, WebRtcSessionConfig, WebRtcSessionHandle},
        signaling::{SecureRandom, SignalingError, WebRtcSignalingFacade},
    },
    intercom_core::{
        app::App,
        events::{SystemStateSender, SystemStateWatch},
        ports::inbound::{AccessControllerHandle, CommandChannel},
        service::DiagnosticsService,
    },
    intercom_firmware::{
        hardware_info::{self, FirmwareHardwareInfo},
        platform::{ActivePlatform, PlatformDriver},
    },
    log::{info, warn},
    static_cell::StaticCell,
};

#[cfg(all(
    target_os = "none",
    any(all(feature = "esp32c6", not(feature = "devkit")), feature = "esp32s3")
))]
use embassy_sync::signal::Signal;

#[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
use esp_hal::ram;
#[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
use esp_metadata_generated::memory_range;

#[cfg(all(target_os = "none", not(feature = "ota-disabled")))]
use edf_ota::driver::FlashOtaDriver;
#[cfg(all(target_os = "none", not(feature = "led-smartled")))]
use intercom_adapter_gpio::led::GpioLedDriver;
#[cfg(all(target_os = "none", not(feature = "esp32s3")))]
use intercom_adapter_webrtc::session::WebRtcMediaResources;
#[cfg(all(target_os = "none", feature = "ota-disabled"))]
use intercom_firmware::disabled_ota::DisabledOtaDriver;
#[cfg(all(target_os = "none", feature = "esp32s3"))]
use intercom_firmware::platform;
#[cfg(all(target_os = "none", feature = "dummy-persist"))]
use rs_matter_embassy::matter::persist::DummyKvBlobStore;

#[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

#[cfg(all(target_os = "none", feature = "esp32s3"))]
const S3_INTERNAL_HEAP_LAYOUT: platform::s3::InternalHeapLayout =
    platform::s3::internal_heap_layout(
        config::HEAP_SIZE,
        RECLAIMED_RAM,
        platform::s3::DCACHE_RECLAIMED_BYTES,
        0,
        core::mem::size_of::<esp_hal::system::Stack<32_768>>(),
    );

#[cfg(target_os = "none")]
fn media_free_heap_bytes() -> u32 {
    let low_water = hardware_info::FirmwareHardwareInfo::record_media_resource_sample();
    #[cfg(feature = "esp32s3")]
    {
        static SAMPLE_COUNT: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
        let sample = SAMPLE_COUNT.fetch_add(1, portable_atomic::Ordering::Relaxed);
        if sample % 50 == 0 {
            info!(
                "WebRTC media resource low water min_internal_free_bytes={} min_audio_stack_free_bytes={} latest_psram_free_bytes={}",
                low_water.min_internal_free_bytes,
                low_water.min_audio_stack_free_bytes,
                low_water.latest_psram_free_bytes,
            );
        }
    }
    low_water.min_internal_free_bytes.min(u32::MAX as usize) as u32
}

#[cfg(all(target_os = "none", feature = "esp32s3"))]
fn fill_s3_secure_random(output: &mut [u8]) -> bool {
    platform::entropy::fill_global(output).is_ok()
}

#[cfg(all(target_os = "none", feature = "esp32s3"))]
fn start_s3_radio_entropy() -> bool {
    platform::entropy::mark_radio_starting().is_ok()
}

#[cfg(target_os = "none")]
struct HardwareSecureRandom;

#[cfg(all(
    target_os = "none",
    any(all(feature = "esp32c6", not(feature = "devkit")), feature = "esp32s3")
))]
#[embassy_executor::task]
async fn i2s_audio_worker(
    pipeline: intercom_adapter_audio::I2sAudioPipeline,
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    capture_source: intercom_adapter_audio::I2sCaptureSource,
) {
    if let Err(error) = pipeline
        .run_with_capture_source(rx_audio.sender(), tx_audio.receiver(), capture_source)
        .await
    {
        warn!("I2S DMA stream failed: {:?}", error);
        custom_halt();
    }
}

#[cfg(all(target_os = "none", feature = "esp32s31"))]
#[embassy_executor::task]
async fn s31_synthetic_audio_worker(
    rx_audio: &'static intercom_media::RxAudioChannel,
    tx_audio: &'static intercom_media::TxAudioChannel,
    opus_channel: &'static intercom_media::OpusChannel,
    mut call_state: intercom_core::events::CallStateReceiver<'static>,
    mut system_state: intercom_core::events::SystemStateReceiver<'static>,
) {
    static PIPELINE: StaticCell<
        intercom_firmware::platform::synthetic_audio::S31SyntheticAudioPipeline,
    > = StaticCell::new();
    let pipeline = PIPELINE
        .init(intercom_firmware::platform::synthetic_audio::S31SyntheticAudioPipeline::new());
    let rx_sender = rx_audio.sender();
    let tx_receiver = tx_audio.receiver();
    let mut schedule =
        intercom_media::AudioCaptureSchedule::new(embassy_time::Instant::now().as_millis());
    let mut drop_window = intercom_media::AudioFrameDropWindow::new();
    let mut simulated_overruns = 0u64;
    loop {
        let (call_update, system_update) = intercom_firmware::platform::take_audio_state_updates(
            &mut call_state,
            &mut system_state,
        );
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
                    custom_halt();
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

#[cfg(target_os = "none")]
impl SecureRandom for HardwareSecureRandom {
    fn fill(&self, out: &mut [u8]) -> Result<(), SignalingError> {
        #[cfg(feature = "esp32c6")]
        {
            let trng = esp_hal::rng::Trng::try_new().map_err(|_| SignalingError::NotReady)?;
            trng.read(out);
        }
        #[cfg(feature = "esp32s3")]
        if !fill_s3_secure_random(out) {
            return Err(SignalingError::NotReady);
        }
        #[cfg(feature = "esp32s31")]
        esp_hal::rng::Rng::new().read(out);
        Ok(())
    }
}

#[cfg(target_os = "none")]
esp_bootloader_esp_idf::esp_app_desc!(
    version::SW_VER_STR,
    "intercom-bridge",
    esp_bootloader_esp_idf::BUILD_TIME,
    version::BUILD_DATE,
    esp_bootloader_esp_idf::ESP_IDF_COMPATIBLE_VERSION,
    esp_bootloader_esp_idf::MMU_PAGE_SIZE,
    0,
    u16::MAX,
    esp_bootloader_esp_idf::SECURE_VERSION
);

// ============================================================================
// Panic handler: reboot instead of hanging
// ============================================================================

/// Called by `esp-backtrace` after printing the backtrace on panic.
/// Triggers a software reset so the device recovers automatically.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    crate::reset::force_bus_relays_low();
    esp_hal::system::software_reset();
}

// ============================================================================
// Main entry point
// ============================================================================

#[cfg(target_os = "none")]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    if let Err(error) = utils::uptime_logger::init(log::LevelFilter::Info) {
        esp_println::println!("Failed to initialize uptime logger: {error:?}");
        custom_halt();
    }

    reset::force_bus_relays_low();

    info!(
        "Smart Intercom v{} (build: {}, date: {}, rev: {})",
        version::SW_VER_STR,
        if version::IS_DIRTY { "dirty" } else { "clean" },
        version::BUILD_DATE,
        version::SW_VER
    );

    #[cfg(feature = "esp32c6")]
    {
        heap_allocator!(size: config::HEAP_SIZE - RECLAIMED_RAM);
        heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM - core::mem::size_of::<intercom_firmware::platform::ReclaimedCell<DtlsSrtpResources>>());
    }
    #[cfg(feature = "esp32s31")]
    heap_allocator!(size: config::HEAP_SIZE);
    #[cfg(feature = "esp32s3")]
    {
        info!(
            "S3 internal heap layout standard={} reclaimed={} dcache_reclaimed={}",
            S3_INTERNAL_HEAP_LAYOUT.standard_bytes,
            S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes,
            S3_INTERNAL_HEAP_LAYOUT.dcache_reclaimed_bytes
        );
        if platform::s3::heap_region_is_present(S3_INTERNAL_HEAP_LAYOUT.standard_bytes) {
            heap_allocator!(size: S3_INTERNAL_HEAP_LAYOUT.standard_bytes);
        }
        if platform::s3::heap_region_is_present(S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes) {
            heap_allocator!(#[ram(reclaimed)] size: S3_INTERNAL_HEAP_LAYOUT.boot_reclaimed_bytes);
        }
    }

    let (mut platform, platform_drivers) = ActivePlatform::init();
    let peripherals = platform_drivers.peripherals;

    info!("Hardware initialized");

    // ========================================================================
    // Channels and synchronization primitives
    // ========================================================================

    static STATE_WATCH: SystemStateWatch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&STATE_WATCH);
    let state_receiver = STATE_WATCH.receiver().unwrap();

    static CALL_STATE_WATCH: intercom_core::events::CallStateWatch =
        intercom_core::events::CallStateWatch::new();
    #[cfg(feature = "esp32s31")]
    let core1_call_state_receiver = CALL_STATE_WATCH.receiver().unwrap();
    #[cfg(feature = "esp32s31")]
    let core1_system_state_receiver = STATE_WATCH.receiver().unwrap();
    static RX_AUDIO_CHANNEL: intercom_media::RxAudioChannel = intercom_media::RxAudioChannel::new();
    static WS_AUDIO_ACTIVE: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    static TX_AUDIO_CHANNEL: intercom_media::TxAudioChannel = intercom_media::TxAudioChannel::new();
    #[cfg(feature = "esp32s31")]
    static OPUS_CHANNEL: intercom_media::OpusChannel = intercom_media::OpusChannel::new();
    static WEBRTC_REQUESTS: Channel<CriticalSectionRawMutex, WebRtcSessionConfig, 1> =
        Channel::new();
    static WEBRTC_STATE: portable_atomic::AtomicU8 = portable_atomic::AtomicU8::new(0);
    static WEBRTC_HANDLE: StaticCell<WebRtcSessionHandle<'static>> = StaticCell::new();
    #[cfg(not(feature = "esp32s3"))]
    static WEBRTC_RESOURCES: StaticCell<WebRtcMediaResources> = StaticCell::new();
    #[cfg(not(feature = "esp32s3"))]
    #[esp_hal::ram(reclaimed)]
    static WEBRTC_DTLS_RESOURCES: intercom_firmware::platform::ReclaimedCell<DtlsSrtpResources> =
        intercom_firmware::platform::ReclaimedCell::new();
    #[cfg(feature = "esp32s3")]
    let dtls_resources = platform::s3::allocate_dtls_srtp_resources();
    #[cfg(not(feature = "esp32s3"))]
    let dtls_resources = WEBRTC_DTLS_RESOURCES.init(DtlsSrtpResources::new());
    static WEBRTC_DTLS_RESOURCE_CELL: StaticCell<DtlsSrtpResourceCell<'static>> = StaticCell::new();

    static COMMAND_CHANNEL: CommandChannel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(COMMAND_CHANNEL.sender());
    let command_receiver = COMMAND_CHANNEL.receiver();

    static READINESS: StaticCell<ClusterReadinessBarrier> = StaticCell::new();
    let readiness = READINESS.init(ClusterReadinessBarrier::new(state_sender));

    let dtls_resource_cell =
        WEBRTC_DTLS_RESOURCE_CELL.init(DtlsSrtpResourceCell::new(dtls_resources));
    #[cfg(not(feature = "esp32s3"))]
    let webrtc_resources = WEBRTC_RESOURCES.init(WebRtcMediaResources::new());
    #[cfg(feature = "esp32s3")]
    let webrtc_resources = intercom_firmware::platform::s3::allocate_webrtc_media_resources();
    #[cfg(feature = "esp32s31")]
    let webrtc_jitter_buffer = intercom_firmware::platform::s31::allocate_webrtc_jitter_buffer();
    #[cfg(feature = "esp32s3")]
    let webrtc_jitter_buffer = intercom_firmware::platform::s3::allocate_webrtc_jitter_buffer();
    #[cfg(feature = "esp32s31")]
    let opus_channel = Some(&OPUS_CHANNEL);
    #[cfg(feature = "esp32s3")]
    let opus_channel = None;
    #[cfg(any(feature = "esp32s31", feature = "esp32s3"))]
    let (mut webrtc_engine, webrtc_session_handle) =
        WebRtcMediaEngine::new_with_opus_channel_and_jitter_buffer(
            &WEBRTC_REQUESTS,
            &WEBRTC_STATE,
            access_handle,
            RX_AUDIO_CHANNEL.receiver(),
            TX_AUDIO_CHANNEL.sender(),
            opus_channel,
            webrtc_jitter_buffer,
            webrtc_resources,
            dtls_resource_cell,
        );
    #[cfg(not(any(feature = "esp32s31", feature = "esp32s3")))]
    let (mut webrtc_engine, webrtc_session_handle) = WebRtcMediaEngine::new(
        &WEBRTC_REQUESTS,
        &WEBRTC_STATE,
        access_handle,
        RX_AUDIO_CHANNEL.receiver(),
        TX_AUDIO_CHANNEL.sender(),
        webrtc_resources,
        dtls_resource_cell,
    );
    let webrtc_session_handle = WEBRTC_HANDLE.init(webrtc_session_handle);
    static SECURE_RANDOM: HardwareSecureRandom = HardwareSecureRandom;
    let webrtc_signaling = WebRtcSignalingFacade::new_with_resources(
        *webrtc_session_handle,
        &SECURE_RANDOM,
        dtls_resource_cell,
    );

    // ========================================================================
    // Persistence & initial configuration
    // ========================================================================

    #[cfg(not(feature = "dummy-persist"))]
    let mut store = reset::create_kv_blob_store(config::MATTER_KVS_RANGE, config::SYSTEM_KVS_RANGE);
    #[cfg(not(feature = "dummy-persist"))]
    let initial_duration_sec = reset::load_door_duration(&mut store);
    #[cfg(feature = "dummy-persist")]
    let store = DummyKvBlobStore;
    #[cfg(feature = "dummy-persist")]
    let initial_duration_sec = intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC;

    #[cfg(not(feature = "dummy-persist"))]
    let (initial_rx_gain, initial_tx_gain) = reset::load_audio_gains(&mut store);
    #[cfg(feature = "dummy-persist")]
    let (initial_rx_gain, initial_tx_gain) = (
        intercom_core::domain::policy::RxPgaGain::default(),
        intercom_core::domain::policy::TxDacGain::default(),
    );
    intercom_core::domain::diagnostics::update_audio_gains(initial_rx_gain, initial_tx_gain);

    info!("Loaded door unlock duration: {}s", initial_duration_sec);
    intercom_core::domain::diagnostics::update_door_unlock_duration(initial_duration_sec);

    // ========================================================================
    // Matter drivers — own their state internally
    // ========================================================================

    let doorbell_switch = IntercomDoorbellSwitch::new(readiness);
    let audio_av_cluster = IntercomAudioAvCluster::new(access_handle);
    #[cfg(feature = "devkit")]
    {
        let stream_id = audio_av_cluster
            .allocate_synthetic_audio_stream()
            .await
            .expect("allocate DevKit synthetic audio stream");
        info!("DevKit synthetic audio stream allocated: {}", stream_id);
    }
    let chime_cluster = IntercomChime::new();
    let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
        access: access_handle,
        readiness,
    });
    door_lock.set_unlock_duration(initial_duration_sec);

    // ========================================================================
    // Matter controller — register cluster handlers via builder
    // ========================================================================

    #[cfg(not(feature = "esp32s3"))]
    static MATTER_STACK_STORAGE: StaticCell<MatterStackStorage> = StaticCell::new();
    #[cfg(not(feature = "esp32s3"))]
    let matter_stack_storage = MATTER_STACK_STORAGE.uninit();
    #[cfg(feature = "esp32s3")]
    let matter_stack_storage = platform::s3::alloc_psram_uninit::<MatterStackStorage>();
    let matter = MatterController::new(
        MatterControllerConfig {
            peripherals: MatterPeripherals {
                wifi: peripherals.WIFI,
                #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
                bt: peripherals.BT,
                #[cfg(not(feature = "esp32s3"))]
                rng: peripherals.RNG,
                #[cfg(not(feature = "esp32s3"))]
                adc1: peripherals.ADC1,
            },
            device_info: DeviceInfo {
                hw_ver: config::HW_VER,
                hw_ver_str: config::HW_VER_STR,
                sw_ver: version::SW_VER,
                sw_ver_str: version::SW_VER_STR,
                serial_no: None,
                hostname: None,
                vendor_name: config::VENDOR_NAME,
                product_name: config::PRODUCT_NAME,
                device_name: config::DEVICE_NAME,
            },
            store,
            comm_data: None,
            #[cfg(feature = "esp32s3")]
            secure_random: fill_s3_secure_random,
            #[cfg(feature = "esp32s3")]
            start_radio_entropy: start_s3_radio_entropy,
        },
        state_sender,
        readiness,
        FirmwareResetDriver,
        matter_stack_storage,
    );
    let webrtc_prov_cluster = matter.webrtc_prov_cluster(IntercomWebRtcHooks::new(
        access_handle,
        &audio_av_cluster,
        &webrtc_signaling,
    ));
    let matter = matter
        .register(&doorbell_switch)
        .register(&audio_av_cluster)
        .register(&webrtc_prov_cluster)
        .register(&chime_cluster)
        .register(&door_lock);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    #[cfg(not(feature = "esp32s3"))]
    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    #[cfg(feature = "esp32s3")]
    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO4));
    #[cfg(not(feature = "esp32s3"))]
    let bus = intercom_core::service::BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO7, peripherals.GPIO10, peripherals.GPIO18),
    ));
    #[cfg(feature = "esp32s3")]
    let bus = intercom_core::service::BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO17, peripherals.GPIO18, peripherals.GPIO40),
    ));
    // Start I2S clocks before releasing the C6 codec reset.
    #[cfg(all(feature = "esp32c6", not(feature = "devkit")))]
    {
        static I2S_PIPELINE_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

        platform.spawn_audio_worker(spawner, move |audio_spawner| {
            let i2s_pipeline = intercom_adapter_audio::I2sAudioPipeline::new(
                peripherals.I2S0,
                peripherals.DMA_CH0,
                peripherals.GPIO22,
                peripherals.GPIO23,
                peripherals.GPIO15,
                peripherals.GPIO20,
                peripherals.GPIO21,
            )
            .expect("I2S0 init");
            audio_spawner.spawn(
                i2s_audio_worker(
                    i2s_pipeline,
                    &RX_AUDIO_CHANNEL,
                    &TX_AUDIO_CHANNEL,
                    intercom_adapter_audio::I2sCaptureSource::Codec,
                )
                .expect("Failed to create I2S audio worker task"),
            );
            I2S_PIPELINE_READY.signal(());
        });

        I2S_PIPELINE_READY.wait().await;
    }

    // The S3 dev board uses physical I2S playback, but the codec capture path
    // is fed from the embedded test recording while no live intercom is wired.
    #[cfg(feature = "esp32s3")]
    {
        static I2S_PIPELINE_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

        platform.spawn_audio_worker(spawner, move |audio_spawner| {
            let i2s_pipeline = intercom_adapter_audio::I2sAudioPipeline::new(
                peripherals.I2S0,
                peripherals.DMA_CH0,
                peripherals.GPIO10,
                peripherals.GPIO11,
                peripherals.GPIO12,
                peripherals.GPIO13,
                peripherals.GPIO14,
            )
            .expect("S3 I2S0 init");
            audio_spawner.spawn(
                i2s_audio_worker(
                    i2s_pipeline,
                    &RX_AUDIO_CHANNEL,
                    &TX_AUDIO_CHANNEL,
                    intercom_adapter_audio::I2sCaptureSource::EmbeddedTestSample,
                )
                .expect("Failed to create S3 I2S audio worker task"),
            );
            I2S_PIPELINE_READY.signal(());
        });

        I2S_PIPELINE_READY.wait().await;
    }

    #[cfg(feature = "esp32s31")]
    platform.spawn_audio_worker(spawner, move |audio_spawner| {
        audio_spawner.spawn(
            s31_synthetic_audio_worker(
                &RX_AUDIO_CHANNEL,
                &TX_AUDIO_CHANNEL,
                &OPUS_CHANNEL,
                core1_call_state_receiver,
                core1_system_state_receiver,
            )
            .expect("Failed to create S31 synthetic audio worker task"),
        );
    });

    #[cfg(feature = "esp32c6")]
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
        let mut audio_ctrl = intercom_adapter_audio::Aic3204AudioDriver::new(i2c0, codec_rst);
        if let Err(error) = audio_ctrl.init(initial_rx_gain, initial_tx_gain).await {
            warn!(
                "TLV320AIC3204 init failed (bench without codec?): {:?}",
                error
            );
        }
        audio_ctrl
    };
    #[cfg(feature = "esp32s3")]
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
        let mut audio_ctrl = intercom_adapter_audio::Aic3204AudioDriver::new(i2c0, codec_rst);
        if let Err(error) = audio_ctrl.init(initial_rx_gain, initial_tx_gain).await {
            warn!(
                "S3 TLV320AIC3204 init failed (bench without codec?): {:?}",
                error
            );
        }
        audio_ctrl
    };
    #[cfg(feature = "esp32s31")]
    let audio_ctrl = intercom_core::ports::NoopAudioControlDriver;

    #[cfg(not(feature = "led-smartled"))]
    #[cfg(not(feature = "esp32s3"))]
    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    #[cfg(not(feature = "led-smartled"))]
    #[cfg(not(feature = "esp32s3"))]
    let indicator: &'static GpioLedDriver =
        &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));
    #[cfg(all(not(feature = "led-smartled"), feature = "esp32s3"))]
    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    #[cfg(all(not(feature = "led-smartled"), feature = "esp32s3"))]
    let indicator: &'static GpioLedDriver =
        &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO47));

    #[cfg(all(feature = "led-smartled", not(feature = "esp32s3")))]
    static DEVKIT_INDICATOR: StaticCell<intercom_firmware::devkit_led::DevKitLedDriver> =
        StaticCell::new();
    #[cfg(all(feature = "led-smartled", not(feature = "esp32s3")))]
    let indicator: &'static intercom_firmware::devkit_led::DevKitLedDriver = &*DEVKIT_INDICATOR
        .init(intercom_firmware::devkit_led::DevKitLedDriver::new(
            peripherals.RMT,
            peripherals.GPIO8,
        ));
    #[cfg(all(feature = "led-smartled", feature = "esp32s3"))]
    static S3_INDICATOR: StaticCell<intercom_firmware::devkit_led::DevKitLedDriver> =
        StaticCell::new();
    #[cfg(all(feature = "led-smartled", feature = "esp32s3"))]
    let indicator: &'static intercom_firmware::devkit_led::DevKitLedDriver = &*S3_INDICATOR.init(
        intercom_firmware::devkit_led::DevKitLedDriver::new(peripherals.RMT, peripherals.GPIO47),
    );

    let access_driver = MatterAccessControlDriver::new(&door_lock, &doorbell_switch);
    let call_broadcaster =
        intercom_core::events::CallStateBroadcastReporter::new(&CALL_STATE_WATCH);
    let reporter = (
        access_driver,
        indicator,
        (FirmwarePersistenceReporter, call_broadcaster),
    );

    #[cfg(not(feature = "led-smartled"))]
    #[cfg(not(feature = "esp32s3"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_indicator(indicator),
    );
    #[cfg(all(not(feature = "led-smartled"), feature = "esp32s3"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO16).with_indicator(indicator),
    );
    #[cfg(all(feature = "led-smartled", not(feature = "esp32s3")))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_hold_feedback(indicator),
    );
    #[cfg(all(feature = "led-smartled", feature = "esp32s3"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO16).with_hold_feedback(indicator),
    );

    let app = App::with_duration(
        ring,
        &bus,
        audio_ctrl,
        reporter,
        boot_button,
        FirmwareResetDriver,
        initial_duration_sec,
    );

    info!("Smart Intercom Ready");

    let hardware_info = FirmwareHardwareInfo;
    let diagnostics_service = DiagnosticsService::with_ecosystem(
        &hardware_info,
        readiness,
        config::AVAILABLE_ECOSYSTEMS,
        &STATE_WATCH,
    );
    #[cfg(all(not(feature = "ota-disabled"), not(feature = "esp32s3")))]
    let mut ota_driver = FlashOtaDriver::new(config::OTA_PUBLIC_KEY);
    #[cfg(all(not(feature = "ota-disabled"), feature = "esp32s3"))]
    let mut ota_driver = FlashOtaDriver::new_with_profile(
        config::OTA_PUBLIC_KEY,
        edf_ota::OtaImageProfile {
            chip_id: edf_ota::ESP32_S3_CHIP_ID,
            slot_size: config::S3_ACTIVE_LAYOUT.app_partitions[0]
                .expect("S3 OTA profile must define app slot 0")
                .size as usize,
            ota_0_offset: config::S3_ACTIVE_LAYOUT.app_partitions[0]
                .expect("S3 OTA profile must define app slot 0")
                .offset,
            ota_1_offset: config::S3_ACTIVE_LAYOUT.app_partitions[1]
                .expect("S3 OTA profile must define app slot 1")
                .offset,
        },
    );
    #[cfg(feature = "ota-disabled")]
    let mut ota_driver = DisabledOtaDriver;
    // ========================================================================
    // Run App, Matter, LED, HTTP, and I2S services concurrently
    // ========================================================================

    let app_future = app.run(state_sender, &command_receiver, access_handle);
    static CLOCK_SYNC_DRIVER: utils::uptime_logger::FirmwareClockSyncDriver =
        utils::uptime_logger::FirmwareClockSyncDriver;
    #[cfg(feature = "esp32s3")]
    info!(
        "S3 internal heap free before Matter startup: {} bytes",
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into())
    );
    let matter_future = pin!(matter.run(&CLOCK_SYNC_DRIVER));
    let led_future = indicator.run(state_receiver);
    let http_future = async {
        let stack = rs_matter_embassy::enet::wait_global_stack().await;

        #[cfg(not(feature = "ota-disabled"))]
        {
            // Mark the running slot valid now that hardware, Wi-Fi, and network stack are operational.
            // If an OTA update failed to boot or crashes before reaching this point, the ESP-IDF
            // 2nd-stage bootloader will automatically roll back to the previously functioning slot.
            if let Err(e) = edf_ota::bootloader::mark_running_slot_valid() {
                warn!("Failed to mark running slot valid: {:?}", e);
            } else {
                info!("Running firmware slot confirmed valid (rollback watchdog cancelled)");
            }
        }
        #[cfg(feature = "ota-disabled")]
        {
            info!("OTA rollback watchdog check skipped (OTA updates disabled on this profile)");
        }

        let http_config = HttpServerConfig {
            port: config::HTTP_PORT,
            tarpit_delay: config::HTTP_TARPIT_DELAY,
            start_read_timeout: config::HTTP_START_READ_TIMEOUT,
            persistent_start_read_timeout: config::HTTP_PERSISTENT_START_READ_TIMEOUT,
            read_request_timeout: config::HTTP_READ_REQUEST_TIMEOUT,
            write_timeout: config::HTTP_WRITE_TIMEOUT,
            socket_timeout: config::HTTP_SOCKET_TIMEOUT,
            reboot_delay: config::HTTP_REBOOT_DELAY,
            accept_retry_delay: config::HTTP_ACCEPT_RETRY_DELAY,
        };

        let mut http_server = HttpServer::new(
            stack,
            access_handle,
            &bus,
            &diagnostics_service,
            &mut ota_driver,
            &FirmwareResetDriver,
            Some(state_sender),
            utils::uptime_logger::get_buffered_logs,
            http_config,
        )
        .with_memory_info_provider(FirmwareHardwareInfo::memory_info)
        .with_audio_media(HttpAudioMedia {
            rx_audio: &RX_AUDIO_CHANNEL,
            tx_audio: &TX_AUDIO_CHANNEL,
            call_watch: &CALL_STATE_WATCH,
            state_watch: &STATE_WATCH,
            ws_active: &WS_AUDIO_ACTIVE,
        })
        .with_webrtc_session(webrtc_session_handle)
        .with_debug_webrtc_signaling(&webrtc_signaling);
        http_server.run().await;
    };

    info!("Matter Controller Running");

    #[cfg(not(any(feature = "devkit", feature = "esp32s3")))]
    let audio_input_future = core::future::pending::<()>();
    #[cfg(any(feature = "devkit", feature = "esp32s3"))]
    let audio_input_future = async {
        let mut speech = intercom_media::Pcm16LeLoopingSource::new(
            include_bytes!("../assets/devkit_test_speech_short.pcm"),
            intercom_media::AUDIO_SAMPLE_RATE_HZ as usize,
        )
        .expect("embedded DevKit speech sample must contain complete PCM samples");
        let mut schedule =
            intercom_media::AudioCaptureSchedule::new(embassy_time::Instant::now().as_millis());
        let mut drop_window = intercom_media::AudioFrameDropWindow::new();
        let mut simulated_dma_overruns = 0u64;
        let levels = intercom_media::compute_frame_levels_dbfs(&speech.next_frame());
        intercom_core::domain::diagnostics::update_rx_levels_dbfs(
            levels.rms_dbfs,
            levels.peak_dbfs,
        );
        info!("Test audio input: embedded 16 kHz mono speech sample, 1 s repeat pause");
        loop {
            let now_ms = embassy_time::Instant::now().as_millis();
            embassy_time::Timer::after_millis(schedule.delay_until_next_ms(now_ms)).await;
            let batch = schedule.take_due_frames(embassy_time::Instant::now().as_millis());
            simulated_dma_overruns = simulated_dma_overruns.saturating_add(batch.overruns);

            for _ in 0..batch.frames {
                let queue_dropped = RX_AUDIO_CHANNEL.try_send(speech.next_frame()).is_err();
                if let Some(queue_drops) = drop_window.record(queue_dropped) {
                    info!(
                        "Test audio frames=50 simulated_overruns={} queue_drops={}",
                        simulated_dma_overruns, queue_drops,
                    );
                    simulated_dma_overruns = 0;
                }
            }
        }
    };

    let webrtc_future = async {
        let stack = rs_matter_embassy::enet::wait_global_stack().await;
        stack.wait_config_up().await;
        loop {
            if let Some(config) = stack.config_v4() {
                webrtc_signaling.set_local_ipv4(config.address.address().octets());
                break;
            }
            embassy_time::Timer::after_millis(100).await;
        }
        let refresh_ip = async {
            loop {
                let address = stack
                    .config_v4()
                    .map(|config| config.address.address().octets())
                    .unwrap_or([0; 4]);
                webrtc_signaling.set_local_ipv4(address);
                embassy_time::Timer::after_secs(2).await;
            }
        };
        let media_future = embassy_futures::join::join(
            webrtc_engine.run(stack, media_free_heap_bytes),
            refresh_ip,
        );
        media_future.await;
    };

    embassy_futures::join::join5(
        app_future,
        matter_future,
        led_future,
        http_future,
        embassy_futures::join::join(audio_input_future, webrtc_future),
    )
    .await;
}
