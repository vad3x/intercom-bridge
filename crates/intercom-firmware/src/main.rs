//! Intercom Bridge - ESP32-C6 Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
//!
//! Hardware: DFRobot Beetle ESP32-C6
//! Protocol: Matter over WiFi (BLE for commissioning)
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![recursion_limit = "256"]
#![allow(unexpected_cfgs, async_fn_in_trait)]

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(target_os = "none")]
mod firmware {
    extern crate alloc;

    use core::pin::pin;

    use embassy_executor::Spawner;
    use esp_backtrace as _;
    use log::{info, warn};
    use static_cell::StaticCell;

    #[cfg(feature = "dummy-persist")]
    use rs_matter_embassy::matter::persist::DummyKvBlobStore;

    use intercom_core::app::App;
    use intercom_core::events::{SystemStateSender, SystemStateWatch};
    use intercom_core::ports::inbound::{AccessControllerHandle, CommandChannel};
    use intercom_core::service::DiagnosticsService;

    use intercom_adapter_http::{HttpServer, HttpServerConfig};

    use intercom_adapter_matter::{
        derive_intercom_identity, ClusterReadinessBarrier, DeviceInfo, IntercomDoorLock,
        IntercomDoorLockConfig, IntercomOccupancy, IntercomOccupancyConfig,
        MatterAccessControlDriver, MatterController, MatterControllerConfig,
    };

    use intercom_firmware::hardware_info::FirmwareHardwareInfo;
    use intercom_firmware::platform::{self, ActivePlatform, PlatformDriver};
    use intercom_firmware::reset::{self, FirmwarePersistenceReporter, FirmwareResetDriver};
    use intercom_firmware::{config, diag, utils, version};

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
    /// Triggers a software reset so the device recovers automatically.
    #[unsafe(no_mangle)]
    pub fn custom_halt() -> ! {
        diag::persist_panic_and_reset();
    }

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

        let (_platform, drivers) = ActivePlatform::init();

        let has_last_failure = diag::record_boot_diagnostics();
        info!("Hardware initialized");

        // ====================================================================
        // Channels and synchronization primitives
        // ====================================================================

        static STATE_WATCH: SystemStateWatch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&STATE_WATCH);
        let state_receiver = STATE_WATCH.receiver().unwrap();

        static COMMAND_CHANNEL: CommandChannel = CommandChannel::new();
        let access_handle = AccessControllerHandle::new(COMMAND_CHANNEL.sender());
        let command_receiver = COMMAND_CHANNEL.receiver();

        static READINESS: StaticCell<ClusterReadinessBarrier> = StaticCell::new();
        let readiness = READINESS.init(ClusterReadinessBarrier::new(state_sender));

        // ====================================================================
        // Persistence & initial configuration
        // ====================================================================

        #[cfg(not(feature = "dummy-persist"))]
        let mut store =
            reset::create_kv_blob_store(config::MATTER_KVS_RANGE, config::SYSTEM_KVS_RANGE);
        #[cfg(not(feature = "dummy-persist"))]
        let initial_duration_sec = reset::load_door_duration(&mut store);
        #[cfg(feature = "dummy-persist")]
        let store = DummyKvBlobStore;
        #[cfg(feature = "dummy-persist")]
        let initial_duration_sec = intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC;

        info!("Loaded door unlock duration: {}s", initial_duration_sec);
        intercom_core::domain::diagnostics::update_door_unlock_duration(initial_duration_sec);

        // ====================================================================
        // Board hardware & Matter drivers
        // ====================================================================

        let mac_bytes = platform::mac_address();
        static IDENTITY_CELL: StaticCell<intercom_adapter_matter::DeviceIdentity> =
            StaticCell::new();
        let identity = &*IDENTITY_CELL.init(
            derive_intercom_identity(&mac_bytes).expect("valid Intercom identity prefixes"),
        );
        let hostname = identity.hostname_str();
        info!("Generated DHCP hostname: {}", hostname);

        let mut hw = platform::wire_board(drivers.peripherals, hostname, initial_duration_sec);

        let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
            access: access_handle,
            readiness,
        });
        door_lock.set_unlock_duration(initial_duration_sec);

        let occupancy = IntercomOccupancy::new(IntercomOccupancyConfig { readiness });

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
        )
        .register(&door_lock)
        .register(&occupancy);

        let access_driver = MatterAccessControlDriver::new(&door_lock, &occupancy);
        let reporter = (access_driver, hw.indicator, FirmwarePersistenceReporter);

        let app = App::with_duration(
            hw.ring,
            hw.door,
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

        // ====================================================================
        // Run App core services, Matter stack, LED indicator & HTTP server concurrently
        // ====================================================================

        let app_future = app.run(state_sender, &command_receiver, access_handle);
        let matter_future = pin!(embassy_futures::join::join(
            hw.enet_runner.run(),
            matter.run(),
        ));
        let led_future = hw.indicator.run(state_receiver);
        let http_future = async {
            if let Err(e) = edf_ota::bootloader::mark_running_slot_valid() {
                warn!("Failed to mark running slot valid: {:?}", e);
            } else {
                info!("Running firmware slot confirmed valid (rollback watchdog cancelled)");
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
                hw.enet_stack,
                access_handle,
                &diagnostics_service,
                &mut hw.ota_driver,
                &FirmwareResetDriver,
                Some(state_sender),
                utils::uptime_logger::get_buffered_logs,
                http_config,
            )
            .with_last_failure_handlers(
                diag::read_last_failure_bytes,
                reset::clear_persisted_last_failure,
            );
            http_server.run().await;
        };

        let diag_future = diag::run_diag_monitor(hw.timg0_wdt, has_last_failure);

        info!("Matter Controller Running");

        embassy_futures::join::join5(
            app_future,
            matter_future,
            led_future,
            http_future,
            diag_future,
        )
        .await;
    }
}
