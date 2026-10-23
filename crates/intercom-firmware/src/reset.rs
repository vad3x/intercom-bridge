//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::ports::{ResetScope, SystemResetDriver};
use log::info;
#[cfg(target_arch = "riscv32")]
use log::warn;

#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use esp_hal::rom::spiflash;
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use intercom_adapter_gpio::flash::EspFlash;
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// Compatibility name for the foundation partitioned Wi-Fi/Matter store.
pub use edf_matter::store::PartitionedWifiStore as PartitionedKvBlobStore;

/// Outbound reset driver implementing persistent credential erasure and hardware reboot.
pub struct FirmwareResetDriver;

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self, scope: ResetScope) {
        #[cfg(target_arch = "riscv32")]
        {
            #[cfg(not(feature = "dummy-persist"))]
            match scope {
                ResetScope::Ecosystem => {
                    erase_flash_range(
                        &crate::config::MATTER_KVS_RANGE,
                        "Matter KVS (ecosystem only)",
                    );
                }
                ResetScope::Factory => {
                    erase_flash_range(&crate::config::MATTER_KVS_RANGE, "Matter KVS");
                    erase_flash_range(&crate::config::WIFI_KVS_RANGE, "Wi-Fi KVS");
                }
            }
            #[cfg(feature = "dummy-persist")]
            info!("Reset {:?} skipped (dummy-persist)", scope);
        }
        #[cfg(not(target_arch = "riscv32"))]
        info!("Reset {:?} simulated on host", scope);
    }

    fn reboot(&self) -> ! {
        #[cfg(target_arch = "riscv32")]
        {
            esp_hal::system::software_reset();
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            panic!("Host reboot requested");
        }
    }
}

/// Create the partitioned persistence layer spanning Matter KVS and Wi-Fi KVS.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn create_kv_blob_store(
    matter_range: core::ops::Range<u32>,
    wifi_range: core::ops::Range<u32>,
) -> PartitionedKvBlobStore<SeqMapKvBlobStore<EspFlash>, SeqMapKvBlobStore<EspFlash>> {
    let matter_flash = EspFlash::new(matter_range.clone());
    let wifi_flash = EspFlash::new(wifi_range.clone());
    PartitionedKvBlobStore::new(
        SeqMapKvBlobStore::new(matter_flash, matter_range),
        SeqMapKvBlobStore::new(wifi_flash, wifi_range),
    )
}

/// Helper function to erase all 4 KiB sectors in a given flash range.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn erase_flash_range(range: &core::ops::Range<u32>, label: &str) {
    const SECTOR_SIZE: u32 = 4096;

    info!(
        "Erasing {}: 0x{:x}..0x{:x} ({} sectors)",
        label,
        range.start,
        range.end,
        (range.end - range.start) / SECTOR_SIZE
    );

    let ret = unsafe { spiflash::esp_rom_spiflash_unlock() };
    if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
        warn!("Flash unlock failed for {}: {}", label, ret);
        return;
    }

    let mut addr = range.start;
    while addr < range.end {
        let sector = addr / SECTOR_SIZE;
        let ret = unsafe { spiflash::esp_rom_spiflash_erase_sector(sector) };
        if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!(
                "Flash erase sector {} for {} failed: {}",
                sector, label, ret
            );
        }
        addr += SECTOR_SIZE;
    }

    info!("{} erased successfully", label);
}
