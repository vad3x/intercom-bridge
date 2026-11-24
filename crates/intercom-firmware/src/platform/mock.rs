//! Host implementation used by firmware unit tests.

use intercom_core::domain::diagnostics::WifiSsid;

use super::{PlatformDriver, PlatformDrivers};

#[derive(Default)]
pub struct MockPlatform;

impl PlatformDriver for MockPlatform {
    type Drivers = PlatformDrivers<()>;

    fn init() -> (Self, Self::Drivers) {
        (Self, PlatformDrivers { peripherals: () })
    }

    fn software_reset() -> ! {
        panic!("Host reboot requested")
    }
}

pub fn free_heap_bytes() -> usize {
    0
}

pub fn wifi_ssid() -> Option<WifiSsid> {
    None
}

pub fn wifi_rssi() -> Option<i8> {
    None
}

pub fn ip_address() -> Option<[u8; 4]> {
    None
}

pub fn mac_address() -> [u8; 6] {
    [0u8; 6]
}

pub fn active_slot() -> &'static str {
    "ota_0"
}

pub fn is_slot_valid() -> bool {
    true
}
