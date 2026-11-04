//! Intercom Bridge - ESP32-C6 Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
//!
//! Hardware: PCB v2.0 with ESP32-C6
//! Protocol: Matter over WiFi (BLE for commissioning)
#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![recursion_limit = "256"]
#![allow(unexpected_cfgs, async_fn_in_trait)]

#[cfg(not(target_arch = "riscv32"))]
fn main() {}

#[cfg(target_arch = "riscv32")]
extern crate alloc;

#[cfg(target_arch = "riscv32")]
mod config;
#[cfg(target_arch = "riscv32")]
mod hardware_info;
#[cfg(target_arch = "riscv32")]
mod reset;
#[cfg(target_arch = "riscv32")]
mod utils;
#[cfg(target_arch = "riscv32")]
mod version;

#[cfg(target_arch = "riscv32")]
use core::mem::MaybeUninit;
#[cfg(target_arch = "riscv32")]
use core::pin::pin;

#[cfg(target_arch = "riscv32")]
use embassy_executor::Spawner;
#[cfg(target_arch = "riscv32")]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
#[cfg(target_arch = "riscv32")]
use embassy_sync::channel::Channel;
#[cfg(target_arch = "riscv32")]
#[cfg(target_arch = "riscv32")]
use esp_alloc::heap_allocator;
#[cfg(target_arch = "riscv32")]
use esp_backtrace as _;
#[cfg(target_arch = "riscv32")]
use esp_hal::ram;
#[cfg(target_arch = "riscv32")]
use esp_hal::timer::timg::TimerGroup;
#[cfg(target_arch = "riscv32")]
use esp_metadata_generated::memory_range;
#[cfg(target_arch = "riscv32")]
use log::{info, warn};
#[cfg(target_arch = "riscv32")]
use static_cell::StaticCell;

#[cfg(target_arch = "riscv32")]
#[cfg(feature = "dummy-persist")]
use rs_matter_embassy::matter::persist::DummyKvBlobStore;

#[cfg(target_arch = "riscv32")]
use intercom_core::app::App;
#[cfg(target_arch = "riscv32")]
use intercom_core::events::{SystemStateSender, SystemStateWatch};
#[cfg(target_arch = "riscv32")]
use intercom_core::ports::inbound::{AccessControllerHandle, CommandChannel};
#[cfg(target_arch = "riscv32")]
use intercom_core::service::DiagnosticsService;

#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "ota-disabled"))]
use edf_ota::driver::FlashOtaDriver;
#[cfg(target_arch = "riscv32")]
use intercom_adapter_http::{HttpAudioMedia, HttpServer, HttpServerConfig};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_webrtc::dtls::{DtlsSrtpResourceCell, DtlsSrtpResources};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_webrtc::session::{
    WebRtcMediaEngine, WebRtcMediaResources, WebRtcSessionConfig, WebRtcSessionHandle,
};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_webrtc::signaling::{SecureRandom, SignalingError, WebRtcSignalingFacade};
#[cfg(target_arch = "riscv32")]
#[cfg(feature = "ota-disabled")]
use intercom_firmware::disabled_ota::DisabledOtaDriver;

#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::bus::{GpioBusConfig, GpioBusDriver};
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "led-smartled"))]
use intercom_adapter_gpio::led::GpioLedDriver;
#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::ring::{GpioRingConfig, GpioRingDriver};

#[cfg(target_arch = "riscv32")]
use intercom_adapter_matter::{
    ClusterReadinessBarrier, DeviceInfo, IntercomAudioAvCluster, IntercomChime, IntercomDoorLock,
    IntercomDoorLockConfig, IntercomDoorbellSwitch, IntercomWebRtcHooks, IntercomWebRtcProvCluster,
    MatterAccessControlDriver, MatterController, MatterControllerConfig, MatterPeripherals,
};

#[cfg(target_arch = "riscv32")]
use self::hardware_info::FirmwareHardwareInfo;
#[cfg(target_arch = "riscv32")]
use self::reset::{FirmwarePersistenceReporter, FirmwareResetDriver};

#[cfg(target_arch = "riscv32")]
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

#[cfg(target_arch = "riscv32")]
fn media_free_heap_bytes() -> u32 {
    esp_alloc::HEAP.free().min(u32::MAX as usize) as u32
}

#[cfg(target_arch = "riscv32")]
struct HardwareSecureRandom;

#[cfg(target_arch = "riscv32")]
impl SecureRandom for HardwareSecureRandom {
    fn fill(&self, out: &mut [u8]) -> Result<(), SignalingError> {
        let trng = esp_hal::rng::Trng::try_new().map_err(|_| SignalingError::NotReady)?;
        trng.read(out);
        Ok(())
    }
}

#[cfg(target_arch = "riscv32")]
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
#[cfg(target_arch = "riscv32")]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    crate::reset::force_bus_relays_low();
    esp_hal::system::software_reset();
}

// ============================================================================
// Main entry point
// ============================================================================

#[cfg(target_arch = "riscv32")]
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    if let Err(error) = utils::uptime_logger::init(log::LevelFilter::Info) {
        esp_println::println!("Failed to initialize uptime logger: {error:?}");
        custom_halt();
    }

    info!(
        "Smart Intercom v{} (build: {}, date: {}, rev: {})",
        version::SW_VER_STR,
        if version::IS_DIRTY { "dirty" } else { "clean" },
        version::BUILD_DATE,
        version::SW_VER
    );

    heap_allocator!(size: config::HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM - core::mem::size_of::<DtlsSrtpResources>());

    // Initialize ESP-HAL peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize Embassy async runtime via esp-rtos
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!("Hardware initialized");

    // ========================================================================
    // Channels and synchronization primitives
    // ========================================================================

    static STATE_WATCH: SystemStateWatch = SystemStateWatch::new();
    let state_sender = SystemStateSender::new(&STATE_WATCH);
    let state_receiver = STATE_WATCH.receiver().unwrap();

    static CALL_STATE_WATCH: intercom_core::events::CallStateWatch =
        intercom_core::events::CallStateWatch::new();
    static RX_AUDIO_CHANNEL: intercom_media::RxAudioChannel = intercom_media::RxAudioChannel::new();
    static WS_AUDIO_ACTIVE: core::sync::atomic::AtomicBool =
        core::sync::atomic::AtomicBool::new(false);
    static TX_AUDIO_CHANNEL: intercom_media::TxAudioChannel = intercom_media::TxAudioChannel::new();
    static WEBRTC_REQUESTS: Channel<CriticalSectionRawMutex, WebRtcSessionConfig, 1> =
        Channel::new();
    static WEBRTC_STATE: portable_atomic::AtomicU8 = portable_atomic::AtomicU8::new(0);
    static WEBRTC_HANDLE: StaticCell<WebRtcSessionHandle<'static>> = StaticCell::new();
    static mut WEBRTC_RESOURCES: WebRtcMediaResources = WebRtcMediaResources::new();
    #[esp_hal::ram(reclaimed)]
    static mut WEBRTC_DTLS_RESOURCES: MaybeUninit<DtlsSrtpResources> = MaybeUninit::uninit();
    static WEBRTC_DTLS_RESOURCE_CELL: StaticCell<DtlsSrtpResourceCell<'static>> = StaticCell::new();

    static COMMAND_CHANNEL: CommandChannel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(COMMAND_CHANNEL.sender());
    let command_receiver = COMMAND_CHANNEL.receiver();

    static READINESS: StaticCell<ClusterReadinessBarrier> = StaticCell::new();
    let readiness = READINESS.init(ClusterReadinessBarrier::new(state_sender));

    let dtls_resources = unsafe {
        let storage = core::ptr::addr_of_mut!(WEBRTC_DTLS_RESOURCES);
        core::ptr::write_bytes(
            storage.cast::<u8>(),
            0,
            core::mem::size_of::<DtlsSrtpResources>(),
        );
        (&mut *storage).assume_init_mut()
    };
    let dtls_resource_cell =
        WEBRTC_DTLS_RESOURCE_CELL.init(DtlsSrtpResourceCell::new(dtls_resources));
    let webrtc_resources = unsafe { &mut *core::ptr::addr_of_mut!(WEBRTC_RESOURCES) };
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
            .expect("allocate DevKit synthetic audio stream");
        info!("DevKit synthetic audio stream allocated: {}", stream_id);
    }
    let webrtc_prov_cluster = IntercomWebRtcProvCluster::new(IntercomWebRtcHooks::new(
        access_handle,
        &audio_av_cluster,
        &webrtc_signaling,
    ));
    let chime_cluster = IntercomChime::new();
    let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
        access: access_handle,
        readiness,
    });
    door_lock.set_unlock_duration(initial_duration_sec);

    // ========================================================================
    // Matter controller — register cluster handlers via builder
    // ========================================================================

    let matter = MatterController::new(
        MatterControllerConfig {
            peripherals: MatterPeripherals {
                wifi: peripherals.WIFI,
                bt: peripherals.BT,
                rng: peripherals.RNG,
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
        },
        state_sender,
        readiness,
        FirmwareResetDriver,
    )
    .register(&doorbell_switch)
    .register(&audio_av_cluster)
    .register(&webrtc_prov_cluster)
    .register(&chime_cluster)
    .register(&door_lock);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    let bus = intercom_core::service::BusSafetyInterlock::new(GpioBusDriver::new(
        GpioBusConfig::new(peripherals.GPIO7, peripherals.GPIO10, peripherals.GPIO18),
    ));
    // Start MCLK/BCLK/WS before releasing CODEC_RST_N (GPIO11).
    #[cfg(not(feature = "devkit"))]
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
    let i2c_config =
        esp_hal::i2c::master::Config::default().with_frequency(esp_hal::time::Rate::from_khz(400));
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

    #[cfg(not(feature = "led-smartled"))]
    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    #[cfg(not(feature = "led-smartled"))]
    let indicator: &'static GpioLedDriver =
        &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));

    #[cfg(feature = "led-smartled")]
    static DEVKIT_INDICATOR: StaticCell<intercom_firmware::devkit_led::DevKitLedDriver> =
        StaticCell::new();
    #[cfg(feature = "led-smartled")]
    let indicator: &'static intercom_firmware::devkit_led::DevKitLedDriver = &*DEVKIT_INDICATOR
        .init(intercom_firmware::devkit_led::DevKitLedDriver::new(
            peripherals.RMT,
            peripherals.GPIO8,
        ));

    let access_driver = MatterAccessControlDriver::new(&door_lock, &doorbell_switch);
    let call_broadcaster =
        intercom_core::events::CallStateBroadcastReporter::new(&CALL_STATE_WATCH);
    let reporter = (
        access_driver,
        indicator,
        (FirmwarePersistenceReporter, call_broadcaster),
    );

    #[cfg(not(feature = "led-smartled"))]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_indicator(indicator),
    );
    #[cfg(feature = "led-smartled")]
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_hold_feedback(indicator),
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
    #[cfg(not(feature = "ota-disabled"))]
    let mut ota_driver = FlashOtaDriver::new(config::OTA_PUBLIC_KEY);
    #[cfg(feature = "ota-disabled")]
    let mut ota_driver = DisabledOtaDriver;
    // ========================================================================
    // Run App, Matter, LED, HTTP, and I2S services concurrently
    // ========================================================================

    let app_future = app.run(state_sender, &command_receiver, access_handle);
    static CLOCK_SYNC_DRIVER: utils::uptime_logger::FirmwareClockSyncDriver =
        utils::uptime_logger::FirmwareClockSyncDriver;
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
        .with_audio_media(HttpAudioMedia {
            rx_audio: &RX_AUDIO_CHANNEL,
            tx_audio: &TX_AUDIO_CHANNEL,
            call_watch: &CALL_STATE_WATCH,
            state_watch: &STATE_WATCH,
            ws_active: &WS_AUDIO_ACTIVE,
        })
        .with_webrtc_session(webrtc_session_handle);
        http_server.run().await;
    };

    info!("Matter Controller Running");

    #[cfg(not(feature = "devkit"))]
    let audio_input_future = async {
        if let Err(error) = i2s_pipeline
            .run(RX_AUDIO_CHANNEL.sender(), TX_AUDIO_CHANNEL.receiver())
            .await
        {
            warn!("I2S DMA stream failed: {:?}", error);
            #[cfg(not(feature = "devkit"))]
            custom_halt();
            #[cfg(feature = "devkit")]
            core::future::pending::<()>().await;
        }
    };
    #[cfg(feature = "devkit")]
    let audio_input_future = async {
        let mut tone = intercom_media::BenchToneGenerator::new();
        let levels = intercom_media::compute_frame_levels_dbfs(&tone.next_frame());
        intercom_core::domain::diagnostics::update_rx_levels_dbfs(
            levels.rms_dbfs,
            levels.peak_dbfs,
        );
        info!("DevKit audio input: synthetic 1 kHz test tone");
        loop {
            let _ = RX_AUDIO_CHANNEL.try_send(tone.next_frame());
            embassy_time::Timer::after_millis(20).await;
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
        embassy_futures::join::join(webrtc_engine.run(stack, media_free_heap_bytes), refresh_ip)
            .await;
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
