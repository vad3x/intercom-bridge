//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::ports::SystemResetDriver;
use log::{info, warn};
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, NETWORKS_KEY};

#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use esp_hal::rom::spiflash;
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use intercom_adapter_gpio::flash::EspFlash;
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// A composite `KvBlobStore` that routes Wi-Fi credentials (`NETWORKS_KEY`) to a dedicated
/// storage partition and all other Matter fabrics/credentials to the Matter KVS partition.
///
/// Automatically migrates legacy credentials from `matter_store` to `wifi_store` on first load.
pub struct PartitionedKvBlobStore<M, W> {
    pub matter_store: M,
    pub wifi_store: W,
}

impl<M, W> PartitionedKvBlobStore<M, W>
where
    M: KvBlobStore,
    W: KvBlobStore,
{
    pub const fn new(matter_store: M, wifi_store: W) -> Self {
        Self {
            matter_store,
            wifi_store,
        }
    }
}

impl<M, W> KvBlobStore for PartitionedKvBlobStore<M, W>
where
    M: KvBlobStore,
    W: KvBlobStore,
{
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        if key == NETWORKS_KEY {
            let mut check_buf = [0u8; 128];
            if self.wifi_store.load(key, &mut check_buf)?.is_some() {
                return self.wifi_store.load(key, buf);
            }

            let mut legacy_buf = [0u8; 128];
            if let Some(legacy_data) = self.matter_store.load(key, &mut legacy_buf)? {
                info!("Migrating Wi-Fi credentials from matter_kvs to dedicated wifi_kvs partition");
                let mut store_buf = [0u8; 128];
                if let Err(e) = self.wifi_store.store(key, legacy_data, &mut store_buf) {
                    warn!("Failed to store migrated Wi-Fi credentials: {:?}", e);
                } else {
                    let mut remove_buf = [0u8; 64];
                    let _ = self.matter_store.remove(key, &mut remove_buf);
                }

                return self.wifi_store.load(key, buf);
            }

            Ok(None)
        } else {
            self.matter_store.load(key, buf)
        }
    }

    fn store(&mut self, key: u16, data: &[u8], buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            self.wifi_store.store(key, data, buf)
        } else {
            self.matter_store.store(key, data, buf)
        }
    }

    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error> {
        if key == NETWORKS_KEY {
            let res = self.wifi_store.remove(key, buf);
            let _ = self.matter_store.remove(key, buf);
            res
        } else {
            self.matter_store.remove(key, buf)
        }
    }
}

/// Outbound reset driver implementing persistent credential erasure and hardware reboot.
pub struct FirmwareResetDriver;

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self, _scope: intercom_core::ports::ResetScope) {
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
