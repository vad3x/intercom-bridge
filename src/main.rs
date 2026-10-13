//! Smart Intercom - ESP32-C6 Matter Controller
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

mod actors;
mod app;
mod config;
mod drivers;
mod events;
mod utils;

use core::pin::pin;

use embassy_executor::Spawner;

use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::ram;
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;

use log::{info, warn};
#[cfg(not(feature = "dummy-persist"))]
use rs_matter_embassy::persist::SeqMapKvBlobStore;
#[cfg(feature = "dummy-persist")]
use rs_matter_embassy::matter::persist::DummyKvBlobStore;

use crate::actors::matter_controller::MatterControllerActorConfig;
use crate::app::App;
#[cfg(not(feature = "dummy-persist"))]
use crate::drivers::flash::EspFlash;
use crate::drivers::gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
use crate::drivers::gpio::door::{GpioDoorConfig, GpioDoorDriver};
use crate::drivers::gpio::led::GpioLedDriver;
use crate::drivers::gpio::ring::{GpioRingConfig, GpioRingDriver};
use crate::drivers::matter::{
    DeviceInfo, IntercomDoorLock, IntercomDoorLockConfig, IntercomOccupancy,
    IntercomOccupancyConfig, MatterAccessControlDriver, MatterController, MatterControllerConfig,
    MatterPeripherals,
};
use crate::events::{Event, EventBus, EventBusExt};

// ============================================================================
// Memory configuration
// ============================================================================

const HEAP_SIZE: usize = 210 * 1024;

const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

esp_bootloader_esp_idf::esp_app_desc!();

// ============================================================================
// Panic handler: reboot instead of hanging
// ============================================================================

/// Called by `esp-backtrace` after printing the backtrace on panic.
/// Triggers a software reset so the device recovers automatically.
#[unsafe(no_mangle)]
fn custom_halt() -> ! {
    esp_hal::system::software_reset();
}

// ============================================================================
// Matter persistence + factory reset
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

// ============================================================================
// Main entry point
// ============================================================================

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    utils::uptime_logger::init(log::LevelFilter::Info);

    info!("Smart Intercom Starting");

    heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);

    // Initialize ESP-HAL peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize Embassy async runtime via esp-rtos
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!("Hardware initialized");

    // ========================================================================
    // Event bus — local to main, passed by reference
    // ========================================================================

    let event_bus = EventBus::new();

    // ========================================================================
    // Matter drivers — own their state internally
    // ========================================================================

    let door_lock = IntercomDoorLock::new(IntercomDoorLockConfig {
        on_unlock: || {
            event_bus
                .immediate_publisher()
                .publish_immediate(Event::UnlockDispatched);
        },
        on_ready: || event_bus.door_lock_ready(),
    });
    let occupancy = IntercomOccupancy::new(IntercomOccupancyConfig {
        on_ready: || event_bus.ring_ready(),
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
                sw_ver: config::SW_VER,
                sw_ver_str: config::SW_VER_STR,
                serial_no: config::SERIAL_NO,
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
        || event_bus.connecting(),
        || event_bus.connected(),
    )
    .register(&door_lock)
    .register(&occupancy);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    let door = GpioDoorDriver::new(GpioDoorConfig::new(peripherals.GPIO18));
    let indicator = GpioLedDriver::new(peripherals.GPIO19);
    let access_driver = MatterAccessControlDriver::new(&door_lock, &occupancy);
    let boot_button = GpioBootButtonDriver::new(GpioBootButtonConfig::new(peripherals.GPIO9));

    let app = App::new(
        ring,
        door,
        indicator,
        access_driver,
        boot_button,
        MatterControllerActorConfig {
            #[cfg(not(feature = "dummy-persist"))]
            on_factory_reset: || erase_matter_kvs(&config::MATTER_KVS_RANGE),
            #[cfg(feature = "dummy-persist")]
            on_factory_reset: || info!("Factory reset skipped (dummy-persist)"),
        },
    );

    info!("Smart Intercom Ready");

    // ========================================================================
    // Run App actors + Matter stack concurrently
    // ========================================================================

    let app_future = app.run(&event_bus);
    let matter_future = pin!(matter.run());

    info!("Matter Controller Running");

    // Both run forever; join waits for both
    embassy_futures::join::join(app_future, matter_future).await;
}
