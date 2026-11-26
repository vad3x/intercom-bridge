//! Intercom Bridge - Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
#![no_std]
#![no_main]
#![recursion_limit = "256"]
#![allow(unexpected_cfgs, async_fn_in_trait)]

extern crate alloc;

use {
    core::pin::pin,
    embassy_executor::Spawner,
    embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel},
    esp_backtrace as _,
    intercom_adapter_http::{config::HttpServerConfig, server::HttpServer, HttpAudioMedia},
    intercom_adapter_matter::{
        ClusterReadinessBarrier, DeviceInfo, IntercomAudioAvCluster, IntercomChime,
        IntercomDoorLock, IntercomDoorLockConfig, IntercomDoorbellSwitch, IntercomWebRtcHooks,
        MatterAccessControlDriver, MatterController, MatterControllerConfig,
    },
    intercom_adapter_webrtc::{
        dtls::{DtlsIdentity, DtlsSrtpResourceCell},
        session::{WebRtcSessionConfig, WebRtcSessionHandle},
        signaling::{SecureRandom as _, WebRtcSignalingFacade},
    },
    intercom_core::{
        app::App,
        events::{SystemStateSender, SystemStateWatch},
        ports::{inbound::CommandChannel, AccessControllerHandle},
        service::DiagnosticsService,
    },
    intercom_firmware::{
        config, cpu_load,
        hardware_info::FirmwareHardwareInfo,
        platform::{self, ActivePlatform, HardwareSecureRandom, PlatformDriver},
        reset::{
            self, clear_persisted_last_failure, read_last_failure_bytes,
            FirmwarePersistenceReporter, FirmwareResetDriver, IntercomFailureStore,
        },
        utils, version,
    },
    log::info,
    static_cell::StaticCell,
};

#[derive(Clone, Copy)]
struct IntercomDiagContext;

impl edf_diag::DiagContext for IntercomDiagContext {
    fn free_heap_bytes(&self) -> usize {
        esp_alloc::HEAP.free()
    }

    fn log_tail(&self, buf: &mut [u8]) -> usize {
        utils::uptime_logger::get_buffered_logs(buf)
    }
}

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

/// Called by `esp-backtrace` after printing the backtrace on panic.
/// Forces bus relays low, persists failure context to flash, and triggers a software reset.
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    ActivePlatform::force_bus_relays_low();
    edf_diag::persist_panic_and_reset(&IntercomFailureStore, &IntercomDiagContext);
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
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

    let wdt_reset = ActivePlatform::is_wdt_reset();
    let (mut platform, platform_drivers) = ActivePlatform::init();
    let peripherals = platform_drivers.peripherals;
    let has_last_failure =
        edf_diag::record_boot_diagnostics(&IntercomFailureStore, &IntercomDiagContext, wdt_reset);

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
    static WEBRTC_DTLS_RESOURCE_CELL: StaticCell<DtlsSrtpResourceCell<'static>> = StaticCell::new();

    static COMMAND_CHANNEL: CommandChannel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(COMMAND_CHANNEL.sender());
    let command_receiver = COMMAND_CHANNEL.receiver();

    static READINESS: StaticCell<ClusterReadinessBarrier> = StaticCell::new();
    let readiness = READINESS.init(ClusterReadinessBarrier::new(state_sender));

    static SECURE_RANDOM: HardwareSecureRandom = HardwareSecureRandom;
    static WEBRTC_DTLS_IDENTITY: StaticCell<DtlsIdentity> = StaticCell::new();

    // ========================================================================
    // Persistence & initial configuration
    // ========================================================================

    let (store, initial_duration_sec, initial_rx_gain, initial_tx_gain) = reset::init_persistence();
    intercom_core::domain::diagnostics::update_audio_gains(initial_rx_gain, initial_tx_gain);
    info!("Loaded door unlock duration: {}s", initial_duration_sec);
    intercom_core::domain::diagnostics::update_door_unlock_duration(initial_duration_sec);

    // ========================================================================
    // Wire board hardware + audio pipeline
    // ========================================================================

    spawner.spawn(cpu_load::sample_core_load(0).expect("Failed to create Core 0 CPU load sampler"));

    let mut hw = platform::wire_board(
        &mut platform,
        peripherals,
        spawner,
        &RX_AUDIO_CHANNEL,
        &TX_AUDIO_CHANNEL,
        &CALL_STATE_WATCH,
        &STATE_WATCH,
        initial_rx_gain,
        initial_tx_gain,
    )
    .await;

    let mut seeds = [0u8; 40];
    SECURE_RANDOM
        .fill(&mut seeds)
        .expect("WebRTC DTLS + Ethernet seed RNG");
    let mut dtls_seed = [0u8; 32];
    dtls_seed.copy_from_slice(&seeds[..32]);
    let enet_seed: [u8; 8] = seeds[32..].try_into().unwrap();
    let dtls_resources = platform::allocate_dtls_srtp_resources();
    let dtls_identity = WEBRTC_DTLS_IDENTITY
        .init(DtlsIdentity::generate(dtls_seed, dtls_resources).expect("WebRTC DTLS identity"));
    let dtls_resource_cell =
        WEBRTC_DTLS_RESOURCE_CELL.init(DtlsSrtpResourceCell::new(dtls_resources));
    let webrtc_resources = platform::allocate_webrtc_media_resources();
    let (webrtc_engine, webrtc_session_handle) = platform::create_webrtc_engine(
        &WEBRTC_REQUESTS,
        &WEBRTC_STATE,
        access_handle,
        &RX_AUDIO_CHANNEL,
        &TX_AUDIO_CHANNEL,
        webrtc_resources,
        dtls_resource_cell,
    );
    let mut webrtc_engine = webrtc_engine.with_opus_encoder_config(config::OPUS_ENCODER_CONFIG);
    let webrtc_session_handle = WEBRTC_HANDLE.init(webrtc_session_handle);
    let webrtc_signaling = WebRtcSignalingFacade::new_with_identity(
        *webrtc_session_handle,
        &SECURE_RANDOM,
        dtls_identity,
    );

    // ========================================================================
    // Matter drivers — own their state internally
    // ========================================================================

    let doorbell_switch = IntercomDoorbellSwitch::new(readiness);
    let audio_av_cluster = IntercomAudioAvCluster::new(access_handle);
    if platform::ENABLE_SYNTHETIC_AUDIO_STREAM {
        let stream_id = audio_av_cluster
            .allocate_synthetic_audio_stream()
            .await
            .expect("allocate synthetic audio stream");
        info!("Synthetic audio stream allocated: {}", stream_id);
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

    let enet_resources = platform::allocate_enet_resources();
    let enet_identity = intercom_adapter_matter::commissioning::derive_intercom_identity(
        &intercom_core::ports::diagnostics::HardwareInfoDriver::mac_address(&FirmwareHardwareInfo),
    )
    .expect("valid Intercom identity prefixes");
    let (stack, mut enet_runner) = hw.wireless.init_network(
        enet_identity.hostname_str(),
        u64::from_le_bytes(enet_seed),
        enet_resources,
    );

    let matter_stack_storage = platform::allocate_matter_stack_storage();
    let matter = MatterController::new(
        MatterControllerConfig {
            wireless: hw.wireless,
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
    // Wire App + Diagnostics
    // ========================================================================

    let access_driver = MatterAccessControlDriver::new(&door_lock, &doorbell_switch);
    let call_broadcaster =
        intercom_core::events::CallStateBroadcastReporter::new(&CALL_STATE_WATCH);
    let reporter = (
        access_driver,
        hw.indicator,
        (FirmwarePersistenceReporter, call_broadcaster),
    );

    let app = App::with_duration(
        hw.ring,
        hw.bus,
        hw.audio_ctrl,
        reporter,
        hw.boot_button,
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

    // ========================================================================
    // Run App, Matter, LED, HTTP, and WebRTC/Audio services concurrently
    // ========================================================================

    let app_future = app.run(state_sender, &command_receiver, access_handle);
    static CLOCK_SYNC_DRIVER: utils::uptime_logger::FirmwareClockSyncDriver =
        utils::uptime_logger::FirmwareClockSyncDriver;
    platform::log_pre_matter_heap();
    let matter_future = pin!(embassy_futures::join::join(
        enet_runner.run(),
        matter.run(&CLOCK_SYNC_DRIVER),
    ));
    let led_future = hw.indicator.run(state_receiver);
    let http_future = async {
        platform::mark_running_slot_valid();

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
            hw.bus,
            &diagnostics_service,
            &mut hw.ota_driver,
            &FirmwareResetDriver,
            Some(state_sender),
            utils::uptime_logger::get_buffered_logs,
            http_config,
        )
        .with_memory_info_provider(FirmwareHardwareInfo::memory_info)
        .with_cpu_info_provider(cpu_load::cpu_info)
        .with_audio_media(HttpAudioMedia {
            rx_audio: &RX_AUDIO_CHANNEL,
            tx_audio: &TX_AUDIO_CHANNEL,
            call_watch: &CALL_STATE_WATCH,
            state_watch: &STATE_WATCH,
            ws_active: &WS_AUDIO_ACTIVE,
        })
        .with_webrtc_session(webrtc_session_handle)
        .with_debug_webrtc_signaling(&webrtc_signaling)
        .with_last_failure_handlers(read_last_failure_bytes, clear_persisted_last_failure);
        http_server.run().await;
    };

    info!("Matter Controller Running");

    let diag_future = edf_diag::run_diag_monitor(
        hw.timg0_wdt,
        stack,
        &IntercomFailureStore,
        &IntercomDiagContext,
        has_last_failure,
    );

    let audio_input_future = platform::run_bench_audio_input(&RX_AUDIO_CHANNEL);

    let webrtc_future = async {
        stack.wait_config_up().await;
        loop {
            if let Some(config) = stack.config_v4() {
                let octets = config.address.address().octets();
                intercom_firmware::hardware_info::set_cached_ip_address(Some(octets));
                webrtc_signaling.set_local_ipv4(octets);
                break;
            }
            embassy_time::Timer::after_millis(100).await;
        }
        let refresh_ip = async {
            loop {
                let ip = stack
                    .config_v4()
                    .map(|config| config.address.address().octets());
                intercom_firmware::hardware_info::set_cached_ip_address(ip);
                webrtc_signaling.set_local_ipv4(ip.unwrap_or([0; 4]));
                embassy_time::Timer::after_secs(2).await;
            }
        };
        let media_future = embassy_futures::join::join(
            webrtc_engine.run(stack, platform::record_and_log_media_free_heap_bytes),
            refresh_ip,
        );
        media_future.await;
    };

    embassy_futures::join::join5(
        app_future,
        matter_future,
        led_future,
        http_future,
        embassy_futures::join::join3(audio_input_future, webrtc_future, diag_future),
    )
    .await;
}
