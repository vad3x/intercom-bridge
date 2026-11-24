//! ESP32-C6 board profile, heap initialization, peripheral wiring, and telemetry queries.

use core::sync::atomic::{AtomicU8, Ordering};

use edf_ota::driver::FlashOtaDriver;
use esp_alloc::heap_allocator;
use esp_hal::ram;
use esp_metadata_generated::memory_range;
use intercom_adapter_gpio::boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
use intercom_adapter_gpio::door::{GpioDoorConfig, GpioDoorDriver};
use intercom_adapter_gpio::led::GpioLedDriver;
use intercom_adapter_gpio::ring::{GpioRingConfig, GpioRingDriver};
use intercom_core::domain::diagnostics::WifiSsid;
use intercom_core::ports::DoorDriver;
use static_cell::StaticCell;

use super::wireless::{enet_stack, init_esp_network, BoardWireless, EspWifiDriver};
use super::C6Peripherals;
use crate::config;

const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// Hardware drivers wired for the ESP32-C6 board.
pub struct BoardHardware {
    pub wireless: BoardWireless,
    pub enet_stack: rs_matter_embassy::enet::net::Stack<'static>,
    pub enet_runner: rs_matter_embassy::enet::net::Runner<'static, EspWifiDriver>,
    pub ring: GpioRingDriver,
    pub door: GpioDoorDriver,
    pub indicator: &'static GpioLedDriver,
    pub boot_button: GpioBootButtonDriver,
    pub ota_driver: FlashOtaDriver,
    pub timg0_wdt: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
    _trng_source: esp_hal::rng::TrngSource<'static>,
}

pub fn init_heap() {
    heap_allocator!(size: config::HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);
}

/// Wire all ESP32-C6 peripherals, entropy, network stack, GPIO drivers, and OTA driver.
pub fn wire_board(
    peripherals: C6Peripherals,
    hostname: &str,
    initial_door_duration_sec: u32,
) -> BoardHardware {
    let trng_source = esp_hal::rng::TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let trng = esp_hal::rng::Trng::try_new().expect("TrngSource initialized");
    let mut seed_bytes = [0u8; 8];
    trng.read(&mut seed_bytes);
    let net_seed = u64::from_le_bytes(seed_bytes);

    let (wireless, enet_stack, enet_runner) =
        init_esp_network(peripherals.WIFI, peripherals.BT, hostname, net_seed);

    let ring = GpioRingDriver::new(GpioRingConfig::new(peripherals.GPIO2));
    let mut door = GpioDoorDriver::new(GpioDoorConfig::new(peripherals.GPIO18));
    door.set_unlock_duration_ms(initial_door_duration_sec * 1000);

    static INDICATOR: StaticCell<GpioLedDriver> = StaticCell::new();
    let indicator: &'static GpioLedDriver =
        &*INDICATOR.init(GpioLedDriver::new(peripherals.GPIO19));

    let boot_button = GpioBootButtonDriver::new(
        GpioBootButtonConfig::new(peripherals.GPIO9).with_indicator(indicator),
    );
    let ota_driver = FlashOtaDriver::new(config::OTA_PUBLIC_KEY);

    BoardHardware {
        wireless,
        enet_stack,
        enet_runner,
        ring,
        door,
        indicator,
        boot_button,
        ota_driver,
        timg0_wdt: peripherals.timg0_wdt,
        _trng_source: trng_source,
    }
}

pub fn free_heap_bytes() -> usize {
    esp_alloc::HEAP.free()
}

pub fn wifi_ssid() -> Option<WifiSsid> {
    use esp_wifi_sys_esp32c6::include::{
        esp_wifi_get_config, esp_wifi_sta_get_ap_info, wifi_ap_record_t, wifi_config_t,
        wifi_interface_t_WIFI_IF_STA, ESP_OK,
    };

    let mut record = core::mem::MaybeUninit::<wifi_ap_record_t>::uninit();
    if unsafe { esp_wifi_sta_get_ap_info(record.as_mut_ptr()) } == ESP_OK as i32 {
        let record = unsafe { record.assume_init() };
        let len = record
            .ssid
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(record.ssid.len());
        if len > 0 {
            if let Some(ssid) = WifiSsid::from_bytes(&record.ssid[..len]) {
                return Some(ssid);
            }
        }
    }

    let mut cfg = core::mem::MaybeUninit::<wifi_config_t>::uninit();
    if unsafe { esp_wifi_get_config(wifi_interface_t_WIFI_IF_STA, cfg.as_mut_ptr()) }
        == ESP_OK as i32
    {
        let cfg = unsafe { cfg.assume_init() };
        let ssid_bytes = unsafe { cfg.sta.ssid };
        let len = ssid_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(ssid_bytes.len());
        if len > 0 {
            if let Some(ssid) = WifiSsid::from_bytes(&ssid_bytes[..len]) {
                return Some(ssid);
            }
        }
    }

    None
}

pub fn wifi_rssi() -> Option<i8> {
    let mut rssi: i32 = 0;
    if unsafe { esp_wifi_sta_get_rssi(&mut rssi) } == 0 {
        return Some(rssi as i8);
    }
    None
}

pub fn ip_address() -> Option<[u8; 4]> {
    let stack = enet_stack()?;
    let config = stack.config_v4()?;
    Some(config.address.address().octets())
}

pub fn mac_address() -> [u8; 6] {
    let mac = esp_hal::efuse::base_mac_address();
    mac.as_bytes().try_into().unwrap_or([0u8; 6])
}

static SLOT_CACHE: AtomicU8 = AtomicU8::new(0);
static VALID_CACHE: AtomicU8 = AtomicU8::new(0);

fn ensure_bootloader_cached() {
    if SLOT_CACHE.load(Ordering::Relaxed) != 0 {
        return;
    }
    match edf_ota::OtaBootloader::load() {
        Ok(bl) => {
            let slot_val = match bl.get_running_slot() {
                edf_ota::Slot::Ota0 => 1,
                edf_ota::Slot::Ota1 => 2,
            };
            SLOT_CACHE.store(slot_val, Ordering::Relaxed);
            let valid_val = if bl.get_active().state == edf_ota::OtaState::Valid {
                1
            } else {
                2
            };
            VALID_CACHE.store(valid_val, Ordering::Relaxed);
        }
        Err(_) => {
            SLOT_CACHE.store(1, Ordering::Relaxed);
            VALID_CACHE.store(1, Ordering::Relaxed);
        }
    }
}

pub fn active_slot() -> &'static str {
    ensure_bootloader_cached();
    match SLOT_CACHE.load(Ordering::Relaxed) {
        2 => "ota_1",
        _ => "ota_0",
    }
}

pub fn is_slot_valid() -> bool {
    ensure_bootloader_cached();
    VALID_CACHE.load(Ordering::Relaxed) == 1
}

extern "C" {
    fn esp_wifi_sta_get_rssi(rssi: *mut i32) -> i32;
}
