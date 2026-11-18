//! Concrete hardware information provider for ESP32-C6.
//!
//! Queries runtime heap, bootloader active/valid slot, efuse MAC, timers,
//! and Wi-Fi RSSI via ESP-IDF / esp-radio runtime APIs.

#[cfg(all(target_os = "none", feature = "esp32s3"))]
use crate::platform::s3;
use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use intercom_core::domain::diagnostics::WifiSsid;
use intercom_core::ports::diagnostics::HardwareInfoDriver;

static MEDIA_RESOURCE_LOW_WATER: Mutex<CriticalSectionRawMutex, RefCell<MediaResourceLowWater>> =
    Mutex::new(RefCell::new(MediaResourceLowWater {
        min_internal_free_bytes: 0,
        min_audio_stack_free_bytes: 0,
        latest_psram_free_bytes: 0,
        sampled: false,
    }));

/// Low-water measurements for media resources that are sampled independently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaResourceLowWater {
    /// Minimum free bytes in internal heap since the first observation.
    pub min_internal_free_bytes: usize,
    /// Minimum free bytes on the audio worker's stack since the first observation.
    pub min_audio_stack_free_bytes: usize,
    /// Most recently observed free PSRAM, which can fluctuate independently.
    pub latest_psram_free_bytes: usize,
    sampled: bool,
}

impl MediaResourceLowWater {
    /// Incorporate one runtime sample without allowing transient PSRAM changes
    /// to alter the internal-heap or worker-stack low-water marks.
    pub fn observe(
        &mut self,
        internal_free_bytes: usize,
        audio_stack_free_bytes: usize,
        psram_free_bytes: usize,
    ) {
        if self.sampled {
            self.min_internal_free_bytes = self.min_internal_free_bytes.min(internal_free_bytes);
            self.min_audio_stack_free_bytes =
                self.min_audio_stack_free_bytes.min(audio_stack_free_bytes);
        } else {
            self.min_internal_free_bytes = internal_free_bytes;
            self.min_audio_stack_free_bytes = audio_stack_free_bytes;
            self.sampled = true;
        }
        self.latest_psram_free_bytes = psram_free_bytes;
    }
}

/// Concrete hardware information provider querying runtime heap, bootloader, efuse, and timers.
pub struct FirmwareHardwareInfo;

impl FirmwareHardwareInfo {
    /// Return live allocator usage split by internal RAM and external PSRAM.
    pub fn memory_info() -> intercom_adapter_http::MemorySnapshot {
        #[cfg(target_os = "none")]
        {
            let mut internal = intercom_adapter_http::MemoryRegion {
                free_bytes: 0,
                total_bytes: 0,
            };
            let mut psram = None;
            for region in esp_alloc::HEAP.stats().region_stats.into_iter().flatten() {
                let stats = intercom_adapter_http::MemoryRegion {
                    free_bytes: region.free,
                    total_bytes: region.size,
                };
                if region
                    .capabilities
                    .contains(esp_alloc::MemoryCapability::External)
                {
                    psram = Some(stats);
                } else if region
                    .capabilities
                    .contains(esp_alloc::MemoryCapability::Internal)
                {
                    internal.free_bytes += stats.free_bytes;
                    internal.total_bytes += stats.total_bytes;
                }
            }

            intercom_adapter_http::MemorySnapshot { internal, psram }
        }
        #[cfg(not(target_os = "none"))]
        intercom_adapter_http::MemorySnapshot {
            internal: intercom_adapter_http::MemoryRegion {
                free_bytes: 0,
                total_bytes: 0,
            },
            psram: None,
        }
    }

    /// Record one split-heap sample and the S3 Core 1 stack watermark.
    pub fn record_media_resource_sample() -> MediaResourceLowWater {
        let memory = Self::memory_info();
        #[cfg(all(target_os = "none", feature = "esp32s3"))]
        let audio_stack_free = s3::audio_stack_free_low_water_bytes();
        #[cfg(not(all(target_os = "none", feature = "esp32s3")))]
        let audio_stack_free = 0;

        MEDIA_RESOURCE_LOW_WATER.lock(|measurement| {
            let mut measurement = measurement.borrow_mut();
            measurement.observe(
                memory.internal.free_bytes,
                audio_stack_free,
                memory.psram.map_or(0, |psram| psram.free_bytes),
            );
            *measurement
        })
    }
}

impl HardwareInfoDriver for FirmwareHardwareInfo {
    fn device_type(&self) -> &'static str {
        crate::config::DEVICE_TYPE
    }

    fn uptime_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }

    fn free_heap_bytes(&self) -> usize {
        #[cfg(target_os = "none")]
        {
            esp_alloc::HEAP.free()
        }
        #[cfg(not(target_os = "none"))]
        {
            0
        }
    }

    fn total_heap_bytes(&self) -> usize {
        crate::config::HEAP_SIZE
    }

    fn wifi_ssid(&self) -> Option<WifiSsid> {
        #[cfg(all(target_os = "none", feature = "esp32c6"))]
        {
            use esp_wifi_sys_esp32c6::include::{
                esp_wifi_get_config, esp_wifi_sta_get_ap_info, wifi_ap_record_t, wifi_config_t,
                wifi_interface_t_WIFI_IF_STA, ESP_OK,
            };

            // Query active AP record first
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

            // Fallback to configured station SSID
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
        }
        #[cfg(all(target_os = "none", feature = "esp32s3"))]
        {
            use esp_wifi_sys_esp32s3::include::{
                esp_wifi_get_config, esp_wifi_sta_get_ap_info, wifi_ap_record_t, wifi_config_t,
                wifi_interface_t_WIFI_IF_STA, ESP_OK,
            };

            let mut record = core::mem::MaybeUninit::<wifi_ap_record_t>::uninit();
            if unsafe { esp_wifi_sta_get_ap_info(record.as_mut_ptr()) } == ESP_OK as i32 {
                let record = unsafe { record.assume_init() };
                let len = record
                    .ssid
                    .iter()
                    .position(|&byte| byte == 0)
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
                    .position(|&byte| byte == 0)
                    .unwrap_or(ssid_bytes.len());
                if len > 0 {
                    if let Some(ssid) = WifiSsid::from_bytes(&ssid_bytes[..len]) {
                        return Some(ssid);
                    }
                }
            }
        }
        None
    }

    fn wifi_rssi(&self) -> Option<i8> {
        #[cfg(all(target_os = "none", feature = "esp32c6"))]
        {
            let mut rssi: i32 = 0;
            if unsafe { esp_wifi_sta_get_rssi(&mut rssi) } == 0 {
                return Some(rssi as i8);
            }
        }
        #[cfg(all(target_os = "none", feature = "esp32s3"))]
        {
            let mut rssi: i32 = 0;
            if unsafe { esp_wifi_sta_get_rssi_s3(&mut rssi) } == 0 {
                return Some(rssi as i8);
            }
        }
        None
    }

    fn ip_address(&self) -> Option<[u8; 4]> {
        #[cfg(target_os = "none")]
        {
            let stack = rs_matter_embassy::enet::global_stack()?;
            let config = stack.config_v4()?;
            Some(config.address.address().octets())
        }
        #[cfg(not(target_os = "none"))]
        {
            None
        }
    }

    fn mac_address(&self) -> [u8; 6] {
        #[cfg(target_os = "none")]
        {
            let mac = esp_hal::efuse::base_mac_address();
            mac.as_bytes().try_into().unwrap_or([0u8; 6])
        }
        #[cfg(not(target_os = "none"))]
        {
            [0u8; 6]
        }
    }

    fn active_slot(&self) -> &'static str {
        #[cfg(target_os = "none")]
        {
            ensure_bootloader_cached();
            match SLOT_CACHE.load(core::sync::atomic::Ordering::Relaxed) {
                2 => "ota_1",
                _ => "ota_0",
            }
        }
        #[cfg(not(target_os = "none"))]
        {
            "ota_0"
        }
    }

    fn is_slot_valid(&self) -> bool {
        #[cfg(target_os = "none")]
        {
            ensure_bootloader_cached();
            VALID_CACHE.load(core::sync::atomic::Ordering::Relaxed) == 1
        }
        #[cfg(not(target_os = "none"))]
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

#[cfg(target_os = "none")]
static SLOT_CACHE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
#[cfg(target_os = "none")]
static VALID_CACHE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[cfg(target_os = "none")]
fn ensure_bootloader_cached() {
    if SLOT_CACHE.load(core::sync::atomic::Ordering::Relaxed) != 0 {
        return;
    }
    match edf_ota::OtaBootloader::load() {
        Ok(bl) => {
            let slot_val = match bl.get_running_slot() {
                edf_ota::Slot::Ota0 => 1,
                edf_ota::Slot::Ota1 => 2,
            };
            SLOT_CACHE.store(slot_val, core::sync::atomic::Ordering::Relaxed);
            let valid_val = if bl.get_active().state == edf_ota::OtaState::Valid {
                1
            } else {
                2
            };
            VALID_CACHE.store(valid_val, core::sync::atomic::Ordering::Relaxed);
        }
        Err(_) => {
            SLOT_CACHE.store(1, core::sync::atomic::Ordering::Relaxed);
            VALID_CACHE.store(1, core::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(all(target_os = "none", feature = "esp32c6"))]
extern "C" {
    fn esp_wifi_sta_get_rssi(rssi: *mut i32) -> i32;
}

#[cfg(all(target_os = "none", feature = "esp32s3"))]
extern "C" {
    #[link_name = "esp_wifi_sta_get_rssi"]
    fn esp_wifi_sta_get_rssi_s3(rssi: *mut i32) -> i32;
}

/// Helper to update the active door unlock duration reflected in telemetry snapshots.
#[inline]
#[allow(dead_code)]
pub fn set_door_unlock_duration(duration_sec: u32) {
    intercom_core::domain::diagnostics::update_door_unlock_duration(duration_sec);
}

/// Helper to update the active door lock state reflected in telemetry snapshots.
#[inline]
#[allow(dead_code)]
pub fn set_door_locked(locked: bool) {
    intercom_core::domain::diagnostics::update_door_locked(locked);
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

    #[test]
    fn test_telemetry_snapshot_door_unlock_duration() {
        set_door_unlock_duration(12);
        let state = intercom_core::domain::diagnostics::current_intercom_state();
        assert_eq!(state.door_unlock_duration_sec, 12);
        set_door_unlock_duration(intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC);
    }

    #[test]
    fn tracks_internal_heap_and_audio_stack_low_water() {
        // Arrange
        let mut measurement = MediaResourceLowWater::default();

        // Act
        measurement.observe(40_000, 20_000, 100_000);
        measurement.observe(32_000, 18_000, 500_000);
        measurement.observe(38_000, 24_000, 200_000);

        // Assert
        assert_eq!(measurement.min_internal_free_bytes, 32_000);
        assert_eq!(measurement.min_audio_stack_free_bytes, 18_000);
        assert_eq!(measurement.latest_psram_free_bytes, 200_000);
    }
}
