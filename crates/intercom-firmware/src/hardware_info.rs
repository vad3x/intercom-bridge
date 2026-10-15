//! Concrete hardware information provider for ESP32-C6.
//!
//! Queries runtime heap, bootloader active/valid slot, efuse MAC, timers,
//! and Wi-Fi RSSI via ESP-IDF / esp-radio runtime APIs.

use intercom_core::ports::diagnostics::HardwareInfoDriver;

/// Concrete hardware information provider querying runtime heap, bootloader, efuse, and timers.
pub struct FirmwareHardwareInfo;

impl HardwareInfoDriver for FirmwareHardwareInfo {
    fn uptime_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }

    fn free_heap_bytes(&self) -> usize {
        #[cfg(target_arch = "riscv32")]
        {
            esp_alloc::HEAP.free()
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            0
        }
    }

    fn total_heap_bytes(&self) -> usize {
        crate::config::HEAP_SIZE
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
        #[cfg(target_arch = "riscv32")]
        {
            let stack = rs_matter_embassy::enet::global_stack()?;
            let config = stack.config_v4()?;
            Some(config.address.address().octets())
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            None
        }
    }

    fn mac_address(&self) -> [u8; 6] {
        #[cfg(target_arch = "riscv32")]
        {
            let mac = esp_hal::efuse::base_mac_address();
            mac.as_bytes().try_into().unwrap_or([0u8; 6])
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            [0u8; 6]
        }
    }

    fn active_slot(&self) -> &'static str {
        #[cfg(target_arch = "riscv32")]
        {
            match intercom_adapter_ota::OtaBootloader::load() {
                Ok(bl) => match bl.get_running_slot() {
                    intercom_adapter_ota::Slot::Ota0 => "ota_0",
                    intercom_adapter_ota::Slot::Ota1 => "ota_1",
                },
                Err(_) => "ota_0",
            }
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            "ota_0"
        }
    }

    fn is_slot_valid(&self) -> bool {
        #[cfg(target_arch = "riscv32")]
        {
            match intercom_adapter_ota::OtaBootloader::load() {
                Ok(bl) => bl.get_active().state == intercom_adapter_ota::OtaState::Valid,
                Err(_) => true,
            }
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            true
        }
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

#[cfg(target_arch = "riscv32")]
extern "C" {
    fn esp_wifi_sta_get_rssi(rssi: *mut i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_hardware_info_version_and_heap() {
        // Arrange
        let info = FirmwareHardwareInfo;

        // Act
        let version = info.firmware_version();
        let commit = info.git_commit();
        let date = info.build_date();
        let total_heap = info.total_heap_bytes();

        // Assert
        assert!(!version.is_empty());
        assert!(!commit.is_empty());
        assert!(!date.is_empty());
        assert_eq!(total_heap, crate::config::HEAP_SIZE);
    }
}
