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

use log::info;

use crate::app::App;
use crate::drivers::gpio::door::{GpioDoorConfig, GpioDoorDriver};
use crate::drivers::gpio::led::{GpioLedConfig, GpioLedDriver};
use crate::drivers::gpio::ring::{GpioRingConfig, GpioRingDriver};
use crate::drivers::matter::{
    IntercomDoorLock, IntercomOccupancy, MatterAccessControlDriver, MatterController,
    MatterControllerOptions,
};
use crate::events::{Event, EventBus};

// ============================================================================
// Memory configuration
// ============================================================================

const HEAP_SIZE: usize = 100 * 1024;

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
// Main entry point
// ============================================================================

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    utils::uptime_logger::init(log::LevelFilter::Info);

    info!("=== Smart Intercom Starting ===");

    heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);

    // Initialize ESP-HAL peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize Embassy async runtime via esp-rtos
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(
        timg0.timer0,
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT)
            .software_interrupt0,
    );

    info!("Hardware initialized");

    // ========================================================================
    // Event bus — local to main, passed by reference
    // ========================================================================

    let event_bus = EventBus::new();

    // ========================================================================
    // Matter drivers — own their state internally
    // ========================================================================

    let on_unlock = || {
        event_bus
            .immediate_publisher()
            .publish_immediate(Event::UnlockDispatched);
    };
    let door_lock = IntercomDoorLock::new(&on_unlock);
    let occupancy = IntercomOccupancy::new();

    // ========================================================================
    // Matter controller — register cluster handlers via builder
    // ========================================================================

    let matter = MatterController::new(MatterControllerOptions {
        wifi: peripherals.WIFI,
        bt: peripherals.BT,
        rng: peripherals.RNG,
        adc1: peripherals.ADC1,
    })
    .register(config::DOOR_LOCK_ENDPOINT_ID, &door_lock)
    .register(config::OCCUPANCY_SENSOR_ENDPOINT_ID, &occupancy);

    // ========================================================================
    // Create GPIO + access control drivers → wire into App
    // ========================================================================

    let ring = GpioRingDriver::new(peripherals.GPIO2, GpioRingConfig::default());
    let door = GpioDoorDriver::new(peripherals.GPIO18, GpioDoorConfig::default());
    let indicator = GpioLedDriver::new(peripherals.GPIO19, GpioLedConfig::default());
    let access_driver = MatterAccessControlDriver::new(&door_lock, &occupancy);

    let app = App::new(ring, door, indicator, access_driver);

    info!("=== Smart Intercom Ready ===");

    // ========================================================================
    // Run App actors + Matter stack concurrently
    // ========================================================================

    let app_future = app.run(&event_bus);
    let matter_future = pin!(matter.run());

    info!("=== Matter Controller Running ===");

    // Both run forever; join waits for both
    embassy_futures::join::join(app_future, matter_future).await;
}
