//! Intercom Bridge - ESP32-C6 Firmware Composition Root
//!
//! Door lock control and ring detection via Apple Home (HomeKit)
//! using the Matter protocol over WiFi with BLE commissioning.
//!
//! Hardware: DFRobot Beetle ESP32-C6
//! Protocol: Matter over WiFi (BLE for commissioning)
#![no_std]
#![no_main]
#![recursion_limit = "256"]
#![allow(unexpected_cfgs, async_fn_in_trait)]

extern crate alloc;

mod config;
mod utils;
mod version;

use core::pin::pin;

use embassy_executor::Spawner;
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::ram;
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use log::{info, warn};
use static_cell::StaticCell;

#[cfg(not(feature = "dummy-persist"))]
use rs_matter_embassy::persist::SeqMapKvBlobStore;
#[cfg(feature = "dummy-persist")]
use rs_matter_embassy::matter::persist::DummyKvBlobStore;

use intercom_core::app::App;
use intercom_core::events::{SystemStateSender, SystemStateWatch};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::ports::inbound::{AccessControllerHandle, CommandChannel};
use intercom_core::ports::SystemResetDriver;
use intercom_core::service::DiagnosticsService;

use intercom_adapter_http::{HttpServer, HttpServerConfig};
use intercom_adapter_ota::FlashOtaDriver;

use intercom_adapter_gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
use intercom_adapter_gpio::door::{GpioDoorConfig, GpioDoorDriver};
use intercom_adapter_gpio::flash::EspFlash;
use intercom_adapter_gpio::led::GpioLedDriver;
use intercom_adapter_gpio::ring::{GpioRingConfig, GpioRingDriver};

use intercom_adapter_matter::{
    ClusterReadinessBarrier, DeviceInfo, IntercomDoorLock, IntercomDoorLockConfig,
    IntercomOccupancy, IntercomOccupancyConfig, MatterAccessControlDriver, MatterController,
    MatterControllerConfig, MatterPeripherals,
};

// ============================================================================
// Memory configuration
// ============================================================================

const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

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
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    esp_hal::system::software_reset();
}

// ============================================================================
// Matter persistence + factory reset driver
// ============================================================================

/// Create the Matter persistence layer for the given flash range.
#[cfg(not(feature = "dummy-persist"))]
fn create_kv_blob_store(kvs_range: core::ops::Range<u32>) -> SeqMapKvBlobStore<EspFlash> {
    let flash = EspFlash::new(kvs_range.clone());
    SeqMapKvBlobStore::new(flash, kvs_range)
}

/// Erase the Matter KVS partition (commissioning data) for factory reset.
///
/// Erases all 4 KiB sectors in the given flash range using ROM SPI flash calls.
/// This is a blocking operation (~ms per sector, 6 sectors = ~24 ms for 24 KiB).
#[cfg(not(feature = "dummy-persist"))]
fn erase_matter_kvs(range: &core::ops::Range<u32>) {
    const SECTOR_SIZE: u32 = 4096;

    info!(
        "Erasing Matter KVS: 0x{:x}..0x{:x} ({} sectors)",
        range.start,
        range.end,
        (range.end - range.start) / SECTOR_SIZE
    );

    // Unlock flash for erasing
    let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
    if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
        warn!("Flash unlock failed: {}", ret);
        return;
    }

    let mut addr = range.start;
    while addr < range.end {
        let sector = addr / SECTOR_SIZE;
        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector) };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash erase sector {} failed: {}", sector, ret);
        }
        addr += SECTOR_SIZE;
    }

    info!("Matter KVS erased successfully");
}

/// Outbound reset driver implementing persistent credential erasure and hardware reboot.
struct FirmwareResetDriver;

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self) {
        #[cfg(not(feature = "dummy-persist"))]
        erase_matter_kvs(&config::MATTER_KVS_RANGE);
        #[cfg(feature = "dummy-persist")]
        info!("Factory reset skipped (dummy-persist)");
    }

    fn reboot(&self) -> ! {
        esp_hal::system::software_reset()
    }
}

/// Concrete hardware information provider querying runtime heap, bootloader, efuse, and timers.
struct FirmwareHardwareInfo;

#[cfg(target_arch = "riscv32")]
extern "C" {
    fn esp_wifi_sta_get_rssi(rssi: *mut i32) -> i32;
}

impl HardwareInfoDriver for FirmwareHardwareInfo {
    fn uptime_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }

    fn free_heap_bytes(&self) -> usize {
        esp_alloc::HEAP.free()
    }

    fn total_heap_bytes(&self) -> usize {
        config::HEAP_SIZE
    }

    fn wifi_rssi(&self) -> Option<i8> {
        #[cfg(target_arch = "riscv32")]
        {
            let mut rssi: i32 = 0;
            if unsafe { esp_wifi_sta_get_rssi(&mut rssi) } == 0 {
                return Some(rssi as i8);
            }
        }
        None
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        let stack = rs_matter_embassy::enet::global_stack()?;
        let config = stack.config_v4()?;
        Some(config.address.address().octets())
    }

    fn mac_address(&self) -> [u8; 6] {
        let mac = esp_hal::efuse::base_mac_address();
        mac.as_bytes().try_into().unwrap_or([0u8; 6])
    }

    fn active_slot(&self) -> &'static str {
        match intercom_adapter_ota::OtaBootloader::load() {
            Ok(bl) => match bl.get_running_slot() {
                intercom_adapter_ota::Slot::Ota0 => "ota_0",
                intercom_adapter_ota::Slot::Ota1 => "ota_1",
            },
            Err(_) => "ota_0",
        }
    }

    fn is_slot_valid(&self) -> bool {
        match intercom_adapter_ota::OtaBootloader::load() {
            Ok(bl) => bl.get_active().state == intercom_adapter_ota::OtaState::Valid,
            Err(_) => true,
        }
    }

    fn firmware_version(&self) -> &'static str {
        version::SW_VER_STR
    }

    fn git_commit(&self) -> &'static str {
        version::GIT_HASH
    }

    fn build_date(&self) -> &'static str {
        version::BUILD_DATE
    }
}

/// Concrete smart home ecosystem status provider for Matter protocol.
struct FirmwareEcosystemInfo;

impl EcosystemInfoDriver for FirmwareEcosystemInfo {
    fn is_paired(&self) -> bool {
        true
    }

    fn protocol_name(&self) -> &'static str {
        "Matter"
    }
}

// ============================================================================
// Main entry point
// ============================================================================

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    utils::uptime_logger::init(log::LevelFilter::Debug);

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
    // Matter drivers — own their state internally
    // ========================================================================

    let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
        access: access_handle,
        readiness,
    });
    let occupancy = IntercomOccupancy::new(IntercomOccupancyConfig {
        readiness,
    });

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
                vendor_name: config::VENDOR_NAME,
                product_name: config::PRODUCT_NAME,
                device_name: config::DEVICE_NAME,
            },
            #[cfg(not(feature = "dummy-persist"))]
            store: create_kv_blob_store(config::MATTER_KVS_RANGE),
            #[cfg(feature = "dummy-persist")]
            store: DummyKvBlobStore,
            comm_data: None,
        },
        state_sender,
        FirmwareResetDriver,
    )
    .register(&door_lock)
    .register(&occupancy);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    let door = GpioDoorDriver::new(GpioDoorConfig::new(peripherals.GPIO18));

    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    let indicator: &'static GpioLedDriver = &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));

    let access_driver = MatterAccessControlDriver::new(&door_lock, &occupancy);
    let reporter = (access_driver, indicator);
    let boot_button = GpioBootButtonDriver::new(GpioBootButtonConfig::new(peripherals.GPIO9));

    let app = App::new(
        ring,
        door,
        reporter,
        boot_button,
        FirmwareResetDriver,
    );

    info!("Smart Intercom Ready");

    let hardware_info = FirmwareHardwareInfo;
    let ecosystem_info = FirmwareEcosystemInfo;
    let diagnostics_service = DiagnosticsService::with_ecosystem(
        &hardware_info,
        &ecosystem_info,
        &STATE_WATCH,
    );
    let mut ota_driver = FlashOtaDriver::new(config::OTA_PUBLIC_KEY);

    // ========================================================================
    // Run App core services, Matter stack, LED indicator & HTTP server concurrently
    // ========================================================================

    let app_future = app.run(state_sender, &command_receiver, access_handle);
    let matter_future = pin!(matter.run());
    let led_future = indicator.run(state_receiver);
    let ota_future = async {
        let stack = rs_matter_embassy::enet::wait_global_stack().await;

        // Mark the running slot valid now that hardware, Wi-Fi, and network stack are operational.
        // If an OTA update failed to boot or crashes before reaching this point, the ESP-IDF
        // 2nd-stage bootloader will automatically roll back to the previously functioning slot.
        if let Err(e) = intercom_adapter_ota::bootloader::mark_running_slot_valid() {
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
    embassy_futures::join::join4(app_future, matter_future, led_future, ota_future).await;
}
