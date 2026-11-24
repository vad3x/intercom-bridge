//! Concrete hardware information provider for Intercom Bridge.
//!
//! Delegates platform-specific queries (heap, bootloader slot, efuse MAC,
//! network IP, and Wi-Fi SSID/RSSI) to `crate::platform`.

use intercom_core::domain::diagnostics::WifiSsid;
use intercom_core::ports::diagnostics::HardwareInfoDriver;

/// Concrete hardware information provider querying runtime heap, bootloader, efuse, and timers.
pub struct FirmwareHardwareInfo;

impl HardwareInfoDriver for FirmwareHardwareInfo {
    fn device_type(&self) -> &'static str {
        crate::config::DEVICE_TYPE
    }

    fn uptime_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }

    fn free_heap_bytes(&self) -> usize {
        crate::platform::free_heap_bytes()
    }

    fn total_heap_bytes(&self) -> usize {
        crate::config::HEAP_SIZE
    }

    fn wifi_ssid(&self) -> Option<WifiSsid> {
        crate::platform::wifi_ssid()
    }

    fn wifi_rssi(&self) -> Option<i8> {
        crate::platform::wifi_rssi()
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        crate::platform::ip_address()
    }

    fn mac_address(&self) -> [u8; 6] {
        crate::platform::mac_address()
    }

    fn active_slot(&self) -> &'static str {
        crate::platform::active_slot()
    }

    fn is_slot_valid(&self) -> bool {
        crate::platform::is_slot_valid()
    }

    fn firmware_version(&self) -> &'static str {
        crate::version::SW_VER_STR
    }

    fn git_commit(&self) -> &'static str {
        crate::version::GIT_HASH
    }

    fn build_date(&self) -> &'static str {
        crate::version::BUILD_DATE
    }
}

/// Helper to update the active door unlock duration reflected in telemetry snapshots.
#[inline]
#[allow(dead_code)]
pub fn update_door_unlock_duration(duration_sec: u32) {
    intercom_core::domain::diagnostics::update_door_unlock_duration(duration_sec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_hardware_info_version_and_heap() {
        let info = FirmwareHardwareInfo;
        assert!(!info.firmware_version().is_empty());
        assert!(!info.git_commit().is_empty());
        assert!(!info.build_date().is_empty());
        assert_eq!(info.total_heap_bytes(), crate::config::HEAP_SIZE);
    }

    #[test]
    fn test_telemetry_snapshot_door_unlock_duration() {
        update_door_unlock_duration(12);
        let state = intercom_core::domain::diagnostics::current_intercom_state();
        assert_eq!(state.door_unlock_duration_sec, 12);
        update_door_unlock_duration(intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC);
    }
}
