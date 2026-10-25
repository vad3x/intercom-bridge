//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::ports::{IntercomReporter, LockState, ResetScope, SystemResetDriver};
use log::info;
#[cfg(target_arch = "riscv32")]
use log::warn;
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

/// Partitioned KV blob store routing system settings (Wi-Fi networks, door duration)
/// to `system_store` and all Matter fabrics/credentials to `matter_store`.
pub struct PartitionedKvBlobStore<M, S> {
    pub matter_store: M,
    pub system_store: S,
}

impl<M, S> PartitionedKvBlobStore<M, S> {
    pub const fn new(matter_store: M, system_store: S) -> Self {
        Self {
            matter_store,
            system_store,
        }
    }
}

/// Compatibility alias for the partitioned Wi-Fi/system store.
#[allow(dead_code)]
pub type PartitionedWifiStore<M, S> = PartitionedKvBlobStore<M, S>;

fn is_system_key(key: u16) -> bool {
    key == NETWORKS_KEY || key == crate::config::DOOR_DURATION_KEY
}

impl<M: KvBlobStore, S: KvBlobStore> KvBlobStore for PartitionedKvBlobStore<M, S> {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        if is_system_key(key) {
            self.system_store.load(key, buf)
        } else {
            self.matter_store.load(key, buf)
        }
    }

    fn store(&mut self, key: u16, data: &[u8], buf: &mut [u8]) -> Result<(), Error> {
        if is_system_key(key) {
            self.system_store.store(key, data, buf)
        } else {
            self.matter_store.store(key, data, buf)
        }
    }

    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error> {
        if is_system_key(key) {
            self.system_store.remove(key, buf)
        } else {
            self.matter_store.remove(key, buf)
        }
    }
}

/// Load the configured door unlock duration in seconds from the persistent KV store.
///
/// Falls back to [`intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC`] if the key
/// is missing, corrupted, or outside domain valid bounds [1, 30].
pub fn load_door_duration<S: KvBlobStore>(store: &mut S) -> u32 {
    let mut buf = [0u8; 512];
    match store.load(crate::config::DOOR_DURATION_KEY, &mut buf) {
        Ok(Some(slice)) if slice.len() >= 4 => {
            let val = u32::from_le_bytes(slice[..4].try_into().unwrap());
            if intercom_core::config::is_valid_door_unlock_duration(val) {
                val
            } else {
                intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC
            }
        }
        _ => intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC,
    }
}

/// Persist the configured door unlock duration in seconds into the persistent KV store.
///
/// Clamps duration to domain valid bounds [1, 30] before persisting.
pub fn save_door_duration<S: KvBlobStore>(store: &mut S, duration_sec: u32) -> Result<(), ()> {
    let mut buf = [0u8; 512];
    let clamped = intercom_core::config::clamp_door_unlock_duration(duration_sec);
    let bytes = clamped.to_le_bytes();
    store
        .store(crate::config::DOOR_DURATION_KEY, &bytes, &mut buf)
        .map_err(|_| ())
}

/// Persist door duration to the `system_kvs` partition in flash.
pub fn persist_door_duration(duration_sec: u32) {
    #[cfg(target_arch = "riscv32")]
    #[cfg(not(feature = "dummy-persist"))]
    {
        let system_flash = EspFlash::new(crate::config::SYSTEM_KVS_RANGE);
        let mut store = SeqMapKvBlobStore::new(system_flash, crate::config::SYSTEM_KVS_RANGE);
        if let Err(_) = save_door_duration(&mut store, duration_sec) {
            warn!("Failed to persist door duration to system_kvs");
        } else {
            info!("Persisted door duration to system_kvs: {}s", duration_sec);
        }
    }
    #[cfg(any(not(target_arch = "riscv32"), feature = "dummy-persist"))]
    {
        info!("Persisted door duration (simulated): {}s", duration_sec);
    }
}

/// Outbound reporter that synchronizes runtime lock and duration state with telemetry
/// and persists duration changes into system flash.
pub struct FirmwarePersistenceReporter;

impl IntercomReporter for FirmwarePersistenceReporter {
    fn report_ring(&self, _ringing: bool) {}

    fn report_lock_state(&self, state: LockState) {
        intercom_core::domain::diagnostics::update_door_locked(matches!(state, LockState::Locked));
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        intercom_core::domain::diagnostics::update_door_unlock_duration(duration_sec);
        persist_door_duration(duration_sec);
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
                    erase_flash_range(
                        &crate::config::MATTER_KVS_RANGE,
                        "Matter KVS (ecosystem only)",
                    );
                }
                ResetScope::Factory => {
                    erase_flash_range(&crate::config::MATTER_KVS_RANGE, "Matter KVS");
                    erase_flash_range(&crate::config::SYSTEM_KVS_RANGE, "System KVS");
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

/// Create the partitioned persistence layer spanning Matter KVS and System KVS.
#[cfg(target_arch = "riscv32")]
#[cfg(not(feature = "dummy-persist"))]
pub fn create_kv_blob_store(
    matter_range: core::ops::Range<u32>,
    system_range: core::ops::Range<u32>,
) -> PartitionedKvBlobStore<SeqMapKvBlobStore<EspFlash>, SeqMapKvBlobStore<EspFlash>> {
    let matter_flash = EspFlash::new(matter_range.clone());
    let system_flash = EspFlash::new(system_range.clone());
    PartitionedKvBlobStore::new(
        SeqMapKvBlobStore::new(matter_flash, matter_range),
        SeqMapKvBlobStore::new(system_flash, system_range),
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
