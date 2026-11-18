//! Smart Intercom - Firmware Composition Root Configuration
//!
//! Target hardware version, product identification, and flash memory layout
//! for the Intercom Bridge firmware.

use core::ops::Range;

// ============================================================================
// Device identity (Basic Information cluster)
// ============================================================================

/// Hardware version string.
#[cfg(feature = "devkit")]
pub const HW_VER_STR: &str = "2.0-devkit";
#[cfg(not(feature = "devkit"))]
pub const HW_VER_STR: &str = "2.0";

/// Numeric hardware version derived from `HW_VER_STR`.
pub const HW_VER: u16 = parse_version(HW_VER_STR) as u16;

/// Vendor name shown in smart home ecosystem controllers.
pub const VENDOR_NAME: &str = "Smart Intercom";

/// Product name shown in smart home ecosystem controllers.
pub const PRODUCT_NAME: &str = "Smart Intercom";

/// Device name shown in smart home ecosystem controllers.
pub const DEVICE_NAME: &str = "Smart Intercom";

/// Canonical device type identifier used for telemetry and OTA target validation.
pub const DEVICE_TYPE: &str = "intercom_bridge";

/// Target-specific Opus encoder profile for outbound WebRTC Listen audio.
#[cfg(feature = "esp32c6")]
pub const OPUS_ENCODER_CONFIG: intercom_adapter_webrtc::opus::OpusEncoderConfig =
    intercom_adapter_webrtc::opus::OpusEncoderConfig::C6_DEFAULT;
#[cfg(any(feature = "esp32s3", feature = "esp32s31"))]
pub const OPUS_ENCODER_CONFIG: intercom_adapter_webrtc::opus::OpusEncoderConfig =
    intercom_adapter_webrtc::opus::OpusEncoderConfig::WIDEBAND_16KHZ;

// ============================================================================
// Flash persistence & Partition Layout (from partitions.csv)
// ============================================================================

/// Start offset of the `otadata` partition in flash.
#[allow(dead_code)]
pub const OTA_DATA_OFFSET: u32 = 0x00f000;
/// Size of the `otadata` partition (8 KiB = 2 sectors).
#[allow(dead_code)]
pub const OTA_DATA_SIZE: u32 = 0x002000;

/// Start offset of `ota_0` app partition (Slot A).
#[allow(dead_code)]
pub const OTA_0_OFFSET: u32 = 0x020000;
/// Start offset of `ota_1` app partition (Slot B).
#[allow(dead_code)]
pub const OTA_1_OFFSET: u32 = 0x370000;
/// Size of each app partition (3.4 MiB = 896 sectors × 4 KiB).
#[allow(dead_code)]
pub const OTA_SLOT_SIZE: u32 = 0x350000;

/// Start offset of the `system_kvs` partition in flash (from partitions.csv / partitions-4mb.csv).
#[cfg(feature = "flash-4mb")]
pub const SYSTEM_KVS_OFFSET: u32 = 0x3d0000;
#[cfg(not(feature = "flash-4mb"))]
pub const SYSTEM_KVS_OFFSET: u32 = 0x6c0000;

/// Size of the `system_kvs` partition (8 KiB = 2 sectors × 4 KiB).
pub const SYSTEM_KVS_SIZE: u32 = 0x2000;

/// Flash range for the system settings and Wi-Fi credentials store.
pub const SYSTEM_KVS_RANGE: Range<u32> = SYSTEM_KVS_OFFSET..SYSTEM_KVS_OFFSET + SYSTEM_KVS_SIZE;

// Backward compatibility aliases:
#[allow(dead_code)]
pub const WIFI_KVS_OFFSET: u32 = SYSTEM_KVS_OFFSET;
#[allow(dead_code)]
pub const WIFI_KVS_SIZE: u32 = SYSTEM_KVS_SIZE;
#[allow(dead_code)]
pub const WIFI_KVS_RANGE: Range<u32> = SYSTEM_KVS_RANGE;

/// Start offset of the `matter_kvs` partition in flash (from partitions.csv / partitions-4mb.csv).
#[cfg(feature = "flash-4mb")]
pub const MATTER_KVS_OFFSET: u32 = 0x3d2000;
#[cfg(not(feature = "flash-4mb"))]
pub const MATTER_KVS_OFFSET: u32 = 0x6c2000;

/// Size of the `matter_kvs` partition (24 KiB = 6 sectors × 4 KiB).
pub const MATTER_KVS_SIZE: u32 = 0x6000;

/// Flash range for the Matter KV blob store.
pub const MATTER_KVS_RANGE: Range<u32> = MATTER_KVS_OFFSET..MATTER_KVS_OFFSET + MATTER_KVS_SIZE;

/// Persistent KV storage key for door unlock duration in seconds (stored in `system_kvs`).
pub const DOOR_DURATION_KEY: u16 = 0x1001;

/// Default door unlock duration in seconds.
#[allow(dead_code)]
pub const DEFAULT_DOOR_DURATION_SEC: u32 = intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC;

// ============================================================================
// Memory & Logging Configuration
// ============================================================================

/// Internal heap budget for the S3 profile; PSRAM supplies a separate external heap region.
#[cfg(feature = "esp32s3")]
pub const HEAP_SIZE: usize = crate::platform::s3::INTERNAL_HEAP_BYTES;

/// C6 internal heap budget, balancing runtime allocations with the main task stack.
///
/// Peak heap usage with WebRTC DTLS+SRTP and Matter TCP is ~108 KiB.
/// 120 KiB leaves ~12 KiB free heap margin while giving the main task stack ~48 KiB
/// of headroom to safely run Opus Silk fixed-point pitch analysis.
#[cfg(not(feature = "esp32s3"))]
pub const HEAP_SIZE: usize = 120 * 1024;

/// Diagnostic log history (2 KiB), leaving room for two HTTP sockets and audio.
pub const LOG_BUFFER_CAPACITY: usize = 2048;

// ============================================================================
// HTTP Server & Web Dashboard Configuration
// ============================================================================

use embassy_time::Duration;

/// TCP port on which the HTTP web dashboard and REST API listen.
pub const HTTP_PORT: u16 = 8080;

/// Delay on failed OTA transfer (tarpit to prevent brute-force DoS).
pub const HTTP_TARPIT_DELAY: Duration = Duration::from_secs(5);

/// Timeout for waiting for the first byte of a new request.
pub const HTTP_START_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for waiting for the first byte of a persistent keep-alive request.
pub const HTTP_PERSISTENT_START_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum duration permitted for an entire HTTP request lifecycle (including OTA firmware flash).
pub const HTTP_READ_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for writing HTTP responses.
pub const HTTP_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Socket-level timeout on accepted TCP connection.
pub const HTTP_SOCKET_TIMEOUT: Duration = Duration::from_secs(60);

/// Delay before initiating hardware reboot after OTA commit to allow HTTP response flush.
pub const HTTP_REBOOT_DELAY: Duration = Duration::from_millis(500);

/// Backoff delay after a socket accept failure before re-listening.
pub const HTTP_ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

// ============================================================================
// OTA Firmware Update Security
// ============================================================================

/// Ed25519 public key used to verify incoming OTA firmware images.
/// Loaded at compile time from the gitignored `ota_public_key.bin` file.
/// If missing, generate using: `python3 scripts/generate_keys.py`
#[allow(dead_code)]
pub const OTA_PUBLIC_KEY: [u8; 32] = *include_bytes!("../ota_public_key.bin");

// ============================================================================
// Smart Home Ecosystems
// ============================================================================

#[cfg(target_os = "none")]
use intercom_adapter_matter::MATTER_ECOSYSTEM;
#[cfg(not(target_os = "none"))]
const MATTER_ECOSYSTEM: EcosystemId = EcosystemId::new("Matter");

use intercom_core::domain::{EcosystemId, EcosystemType};

/// Smart home ecosystems supported by this product configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedEcosystem {
    Matter,
    HomeAssistant,
}

impl EcosystemType for SupportedEcosystem {
    fn id(&self) -> EcosystemId {
        match self {
            Self::Matter => MATTER_ECOSYSTEM,
            Self::HomeAssistant => EcosystemId::new("HomeAssistant"),
        }
    }
}

/// Ecosystems exposed in system diagnostics and the web dashboard.
pub const AVAILABLE_ECOSYSTEMS: &[SupportedEcosystem] = &[
    SupportedEcosystem::Matter,
    SupportedEcosystem::HomeAssistant,
];

// ============================================================================
// Private helpers
// ============================================================================

/// Parse a dotted version string (e.g. `"1.2.3"`, `"1.1"`, `"2.0-devkit"`) into a numeric
/// value at compile time: `major * 10000 + minor * 100 + patch`.
const fn parse_version(version: &str) -> u32 {
    let bytes = version.as_bytes();
    let mut i = 0;
    let mut parts = [0u32; 3];
    let mut part = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            part += 1;
            if part >= 3 {
                break;
            }
        } else if bytes[i] >= b'0' && bytes[i] <= b'9' {
            parts[part] = parts[part] * 10 + (bytes[i] - b'0') as u32;
        } else {
            // Stop at any non-numeric/non-dot separator (e.g. "-devkit")
            break;
        }
        i += 1;
    }
    parts[0] * 10000 + parts[1] * 100 + parts[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_ecosystem_ids() {
        // Arrange
        let matter = SupportedEcosystem::Matter;
        let ha = SupportedEcosystem::HomeAssistant;

        // Act
        let matter_id = matter.id();
        let ha_id = ha.id();

        // Assert
        assert_eq!(matter_id.as_str(), "Matter");
        assert_eq!(ha_id.as_str(), "HomeAssistant");
    }

    #[test]
    fn test_available_ecosystems_list() {
        // Arrange
        let expected_count = 2;

        // Act
        let ecosystems = AVAILABLE_ECOSYSTEMS;

        // Assert
        assert_eq!(ecosystems.len(), expected_count);
        assert_eq!(ecosystems[0], SupportedEcosystem::Matter);
        assert_eq!(ecosystems[1], SupportedEcosystem::HomeAssistant);
    }

    #[test]
    fn test_flash_partitions_are_sector_aligned_and_non_overlapping() {
        // Arrange
        const SECTOR_SIZE: u32 = 4096;
        #[cfg(feature = "flash-4mb")]
        const FLASH_TOTAL_SIZE: u32 = 0x400000; // 4 MB
        #[cfg(not(feature = "flash-4mb"))]
        const FLASH_TOTAL_SIZE: u32 = 0x800000; // 8 MB

        // Act
        let system_offset_aligned = SYSTEM_KVS_OFFSET % SECTOR_SIZE == 0;
        let system_size_aligned = SYSTEM_KVS_SIZE % SECTOR_SIZE == 0;
        let matter_offset_aligned = MATTER_KVS_OFFSET % SECTOR_SIZE == 0;
        let matter_size_aligned = MATTER_KVS_SIZE % SECTOR_SIZE == 0;
        let partitions_adjacent = SYSTEM_KVS_OFFSET + SYSTEM_KVS_SIZE == MATTER_KVS_OFFSET;
        let non_overlapping = SYSTEM_KVS_RANGE.end <= MATTER_KVS_RANGE.start;
        let fits_in_flash = MATTER_KVS_RANGE.end <= FLASH_TOTAL_SIZE;

        // Assert
        assert!(system_offset_aligned);
        assert!(system_size_aligned);
        assert!(matter_offset_aligned);
        assert!(matter_size_aligned);
        assert!(partitions_adjacent);
        assert!(non_overlapping);
        assert!(fits_in_flash);
    }
}

#[cfg(test)]
mod pcb_v2_tests {
    #[test]
    fn firmware_identifies_pcb_v2() {
        // Arrange
        #[cfg(feature = "devkit")]
        let expected_version = "2.0-devkit";
        #[cfg(not(feature = "devkit"))]
        let expected_version = "2.0";

        // Act
        let actual_version = super::HW_VER_STR;

        // Assert
        assert_eq!(actual_version, expected_version);
    }
}

#[cfg(all(test, feature = "esp32s3"))]
mod s3_layout_tests {
    use super::{S3FlashLayout, S3Partition};

    #[test]
    fn s3_layout_matches_partition_table() {
        // Arrange: fixtures mirror partitions.csv and partitions-4mb.csv.
        let dual_bank = S3FlashLayout::dual_bank_8mb();
        let factory_only = S3FlashLayout::factory_only_4mb();

        // Act
        let dual_valid = dual_bank.validate();
        let factory_valid = factory_only.validate();

        // Assert
        assert!(dual_valid);
        assert!(factory_valid);
        assert!(dual_bank.ota_capable());
        assert!(!factory_only.ota_capable());
        assert_eq!(dual_bank.flash_bytes, 0x800000);
        assert_eq!(
            dual_bank.app_partitions[0],
            Some(S3Partition {
                offset: 0x020000,
                size: 0x350000
            })
        );
        assert_eq!(
            dual_bank.app_partitions[1],
            Some(S3Partition {
                offset: 0x370000,
                size: 0x350000
            })
        );
        assert_eq!(
            dual_bank.system_kvs,
            S3Partition {
                offset: 0x6c0000,
                size: 0x2000
            }
        );
        assert_eq!(
            dual_bank.matter_kvs,
            S3Partition {
                offset: 0x6c2000,
                size: 0x6000
            }
        );
        assert_eq!(factory_only.flash_bytes, 0x400000);
        assert_eq!(
            factory_only.app_partitions[0],
            Some(S3Partition {
                offset: 0x020000,
                size: 0x3b0000
            })
        );
        assert_eq!(
            factory_only.system_kvs,
            S3Partition {
                offset: 0x3d0000,
                size: 0x2000
            }
        );
        assert_eq!(
            factory_only.matter_kvs,
            S3Partition {
                offset: 0x3d2000,
                size: 0x6000
            }
        );
    }
}

pub const AUDIO_RX_GAIN_KEY: u16 = 0x1002;
pub const AUDIO_TX_GAIN_KEY: u16 = 0x1003;

/// A flash partition's start and length in bytes.
#[cfg(feature = "esp32s3")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S3Partition {
    pub offset: u32,
    pub size: u32,
}

/// S3 flash geometry represented by the checked-in partition CSV profiles.
#[cfg(feature = "esp32s3")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S3FlashLayout {
    pub flash_bytes: u32,
    pub nvs: S3Partition,
    pub ota_data: Option<S3Partition>,
    pub app_partitions: [Option<S3Partition>; 2],
    pub system_kvs: S3Partition,
    pub matter_kvs: S3Partition,
}

#[cfg(feature = "esp32s3")]
impl S3FlashLayout {
    /// The current 8 MiB rollback-capable partition table.
    pub const fn dual_bank_8mb() -> Self {
        Self {
            flash_bytes: 0x800000,
            nvs: S3Partition {
                offset: 0x9000,
                size: 0x6000,
            },
            ota_data: Some(S3Partition {
                offset: 0xf000,
                size: 0x2000,
            }),
            app_partitions: [
                Some(S3Partition {
                    offset: 0x020000,
                    size: 0x350000,
                }),
                Some(S3Partition {
                    offset: 0x370000,
                    size: 0x350000,
                }),
            ],
            system_kvs: S3Partition {
                offset: 0x6c0000,
                size: 0x2000,
            },
            matter_kvs: S3Partition {
                offset: 0x6c2000,
                size: 0x6000,
            },
        }
    }

    /// The current 4 MiB factory-only recovery layout.
    pub const fn factory_only_4mb() -> Self {
        Self {
            flash_bytes: 0x400000,
            nvs: S3Partition {
                offset: 0x9000,
                size: 0x6000,
            },
            ota_data: None,
            app_partitions: [
                Some(S3Partition {
                    offset: 0x020000,
                    size: 0x3b0000,
                }),
                None,
            ],
            system_kvs: S3Partition {
                offset: 0x3d0000,
                size: 0x2000,
            },
            matter_kvs: S3Partition {
                offset: 0x3d2000,
                size: 0x6000,
            },
        }
    }

    /// Whether the profile has both OTA slots and an OTA metadata partition.
    #[cfg(test)]
    pub const fn ota_capable(&self) -> bool {
        self.app_partitions[0].is_some()
            && self.app_partitions[1].is_some()
            && self.ota_data.is_some()
    }

    /// Ensure all defined partitions fit and do not overlap.
    #[cfg(test)]
    pub fn validate(&self) -> bool {
        let mut partitions = [None; 6];
        partitions[0] = Some(self.nvs);
        partitions[1] = self.ota_data;
        partitions[2] = self.app_partitions[0];
        partitions[3] = self.app_partitions[1];
        partitions[4] = Some(self.system_kvs);
        partitions[5] = Some(self.matter_kvs);

        for (index, partition) in partitions.iter().enumerate() {
            let Some(partition) = partition else { continue };
            let Some(end) = partition.offset.checked_add(partition.size) else {
                return false;
            };
            if partition.size == 0 || end > self.flash_bytes {
                return false;
            }
            for other in partitions.iter().skip(index + 1).flatten() {
                let Some(other_end) = other.offset.checked_add(other.size) else {
                    return false;
                };
                if partition.offset < other_end && other.offset < end {
                    return false;
                }
            }
        }
        true
    }
}

/// Active S3 partition geometry selected by the release feature set.
#[cfg(all(feature = "esp32s3", feature = "flash-4mb"))]
pub const S3_ACTIVE_LAYOUT: S3FlashLayout = S3FlashLayout::factory_only_4mb();
#[cfg(all(feature = "esp32s3", not(feature = "flash-4mb")))]
pub const S3_ACTIVE_LAYOUT: S3FlashLayout = S3FlashLayout::dual_bank_8mb();
