//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::ports::SystemResetDriver;
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

/// Outbound reset driver implementing persistent credential erasure and hardware reboot.
pub struct FirmwareResetDriver;

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self) {
        #[cfg(target_arch = "riscv32")]
        {
            #[cfg(not(feature = "dummy-persist"))]
            erase_matter_kvs(&crate::config::MATTER_KVS_RANGE);
            #[cfg(feature = "dummy-persist")]
            info!("Factory reset skipped (dummy-persist)");
        }
        #[cfg(not(target_arch = "riscv32"))]
        info!("Factory reset simulated on host");
    }

    fn reboot(&self) -> ! {
        #[cfg(target_arch = "riscv32")]
        {
            esp_hal::system::software_reset()
        }
        #[cfg(not(target_arch = "riscv32"))]
        {
            panic!("Host reboot requested");
        }
    }
}

/// Create the Matter persistence layer for the given flash range.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn create_kv_blob_store(kvs_range: core::ops::Range<u32>) -> SeqMapKvBlobStore<EspFlash> {
    let flash = EspFlash::new(kvs_range.clone());
    SeqMapKvBlobStore::new(flash, kvs_range)
}

/// Erase the Matter KVS partition (commissioning data) for factory reset.
///
/// Erases all 4 KiB sectors in the given flash range using ROM SPI flash calls.
/// This is a blocking operation (~ms per sector, 6 sectors = ~24 ms for 24 KiB).
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn erase_matter_kvs(range: &core::ops::Range<u32>) {
    const SECTOR_SIZE: u32 = 4096;

    info!(
        "Erasing Matter KVS: 0x{:x}..0x{:x} ({} sectors)",
        range.start,
        range.end,
        (range.end - range.start) / SECTOR_SIZE
    );

    // Unlock flash for erasing
    let ret = unsafe { spiflash::esp_rom_spiflash_unlock() };
    if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
        warn!("Flash unlock failed: {}", ret);
        return;
    }

    let mut addr = range.start;
    while addr < range.end {
        let sector = addr / SECTOR_SIZE;
        let ret = unsafe { spiflash::esp_rom_spiflash_erase_sector(sector) };
        if ret != spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash erase sector {} failed: {}", sector, ret);
        }
        addr += SECTOR_SIZE;
    }

    info!("Matter KVS erased successfully");
}
