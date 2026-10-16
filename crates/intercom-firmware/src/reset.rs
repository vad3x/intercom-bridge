//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::ports::{ResetScope, SystemResetDriver};
use log::{info, warn};
use rs_matter::error::{Error, ErrorCode};
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

/// Maximum expected length of serialized Wi-Fi credentials in bytes.
pub const MAX_WIFI_CREDENTIALS_LEN: usize = 128;

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
            let mut check_buf = [0u8; MAX_WIFI_CREDENTIALS_LEN];
            if let Some(data) = self.wifi_store.load(key, &mut check_buf)? {
                if data.len() > buf.len() {
                    return Err(ErrorCode::BufferTooSmall.into());
                }
                buf[..data.len()].copy_from_slice(data);
                return Ok(Some(&buf[..data.len()]));
            }

            let mut legacy_buf = [0u8; MAX_WIFI_CREDENTIALS_LEN];
            if let Some(legacy_data) = self.matter_store.load(key, &mut legacy_buf)? {
                let len = legacy_data.len();
                if len > buf.len() {
                    return Err(ErrorCode::BufferTooSmall.into());
                }
                info!("Migrating Wi-Fi credentials from matter_kvs to dedicated wifi_kvs partition");
                let mut store_buf = [0u8; MAX_WIFI_CREDENTIALS_LEN];
                if let Err(e) = self.wifi_store.store(key, legacy_data, &mut store_buf) {
                    warn!("Failed to store migrated Wi-Fi credentials: {:?}", e);
                } else {
                    let mut remove_buf = [0u8; 64];
                    let _ = self.matter_store.remove(key, &mut remove_buf);
                }

                buf[..len].copy_from_slice(&legacy_buf[..len]);
                return Ok(Some(&buf[..len]));
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
    fn erase_credentials(&self, scope: ResetScope) {
        #[cfg(target_arch = "riscv32")]
        {
            #[cfg(not(feature = "dummy-persist"))]
            match scope {
                ResetScope::Ecosystem => {
                    erase_flash_range(&crate::config::MATTER_KVS_RANGE, "Matter KVS (ecosystem only)");
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
            warn!("Flash erase sector {} for {} failed: {}", sector, label, ret);
        }
        addr += SECTOR_SIZE;
    }

    info!("{} erased successfully", label);
}
