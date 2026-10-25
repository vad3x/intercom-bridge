//! Intercom Bridge - ESP32-C6 Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
//!
//! Hardware: DFRobot Beetle ESP32-C6
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
use core::pin::pin;

#[cfg(target_arch = "riscv32")]
use embassy_executor::Spawner;
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
use intercom_core::ports::DoorDriver;
#[cfg(target_arch = "riscv32")]
use intercom_core::ports::inbound::{AccessControllerHandle, CommandChannel};
#[cfg(target_arch = "riscv32")]
use intercom_core::service::DiagnosticsService;

#[cfg(target_arch = "riscv32")]
use edf_ota::driver::FlashOtaDriver;
#[cfg(target_arch = "riscv32")]
use intercom_adapter_http::{HttpServer, HttpServerConfig};

#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::door::{GpioDoorConfig, GpioDoorDriver};
#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::led::GpioLedDriver;
#[cfg(target_arch = "riscv32")]
use intercom_adapter_gpio::ring::{GpioRingConfig, GpioRingDriver};

#[cfg(target_arch = "riscv32")]
use intercom_adapter_matter::{
    ClusterReadinessBarrier, DeviceInfo, IntercomDoorLock, IntercomDoorLockConfig,
    IntercomOccupancy, IntercomOccupancyConfig, MatterAccessControlDriver, MatterController,
    MatterControllerConfig, MatterPeripherals,
};

#[cfg(target_arch = "riscv32")]
use self::hardware_info::FirmwareHardwareInfo;
#[cfg(target_arch = "riscv32")]
use self::reset::{FirmwarePersistenceReporter, FirmwareResetDriver};

#[cfg(target_arch = "riscv32")]
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

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
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);

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

    static COMMAND_CHANNEL: CommandChannel = CommandChannel::new();
    let access_handle = AccessControllerHandle::new(COMMAND_CHANNEL.sender());
    let command_receiver = COMMAND_CHANNEL.receiver();

    static READINESS: StaticCell<ClusterReadinessBarrier> = StaticCell::new();
    let readiness = READINESS.init(ClusterReadinessBarrier::new(state_sender));

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

    info!("Loaded door unlock duration: {}s", initial_duration_sec);
    intercom_core::domain::diagnostics::update_door_unlock_duration(initial_duration_sec);

    // ========================================================================
    // Matter drivers — own their state internally
    // ========================================================================

    let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
        access: access_handle,
        readiness,
    });
    door_lock.set_unlock_duration(initial_duration_sec);

    let occupancy = IntercomOccupancy::new(IntercomOccupancyConfig { readiness });

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
    .register(&door_lock)
    .register(&occupancy);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    let mut door = GpioDoorDriver::new(GpioDoorConfig::new(peripherals.GPIO18));
    door.set_unlock_duration_ms(initial_duration_sec * 1000);

    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    let indicator: &'static GpioLedDriver =
        &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));

    let access_driver = MatterAccessControlDriver::new(&door_lock, &occupancy);
    let reporter = (access_driver, indicator, FirmwarePersistenceReporter);
    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_indicator(indicator),
    );

    let app = App::with_duration(
        ring,
        door,
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
    let mut ota_driver = FlashOtaDriver::new(config::OTA_PUBLIC_KEY);

    // ========================================================================
    // Run App core services, Matter stack, LED indicator & HTTP server concurrently
    // ========================================================================

    let app_future = app.run(state_sender, &command_receiver, access_handle);
    let matter_future = pin!(matter.run());
    let led_future = indicator.run(state_receiver);
    let http_future = async {
        let stack = rs_matter_embassy::enet::wait_global_stack().await;

        // Mark the running slot valid now that hardware, Wi-Fi, and network stack are operational.
        // If an OTA update failed to boot or crashes before reaching this point, the ESP-IDF
        // 2nd-stage bootloader will automatically roll back to the previously functioning slot.
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
            stack,
            access_handle,
            &diagnostics_service,
            &mut ota_driver,
            &FirmwareResetDriver,
            Some(state_sender),
            utils::uptime_logger::get_buffered_logs,
            http_config,
        );
        http_server.run().await;
    };

    info!("Matter Controller Running");

    // All run forever; join4 waits for all
    embassy_futures::join::join4(app_future, matter_future, led_future, http_future).await;
}
