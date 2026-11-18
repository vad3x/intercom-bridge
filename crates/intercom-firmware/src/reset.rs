//! Concrete system reset driver and Matter persistence management.
//!
//! Handles SPI flash Matter KVS sector erasure and software system reboot.

use intercom_core::domain::intercom::{BusLineState, CallState, SessionOwner};
use intercom_core::domain::policy::{RxPgaGain, TxDacGain};

use intercom_core::ports::{IntercomReporter, LockState, ResetScope, SystemResetDriver};
use log::info;
#[cfg(target_os = "none")]
use log::warn;
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, NETWORKS_KEY};

#[cfg(target_os = "none")]
#[cfg(not(feature = "dummy-persist"))]
use intercom_adapter_gpio::flash::EspFlash;
#[cfg(target_os = "none")]
#[cfg(not(feature = "dummy-persist"))]
use rs_matter_embassy::persist::SeqMapKvBlobStore;

/// Partitioned KV blob store routing system settings (Wi-Fi networks, door duration, audio gains)
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

/// Load half-dB gain settings, defaulting each missing or invalid value independently.
pub fn load_audio_gains<S: KvBlobStore>(store: &mut S) -> (RxPgaGain, TxDacGain) {
    let mut buf = [0u8; 64];
    let rx = match store.load(crate::config::AUDIO_RX_GAIN_KEY, &mut buf) {
        Ok(Some(slice)) if !slice.is_empty() => RxPgaGain::try_new(slice[0]).unwrap_or_default(),
        _ => RxPgaGain::default(),
    };
    let tx = match store.load(crate::config::AUDIO_TX_GAIN_KEY, &mut buf) {
        Ok(Some(slice)) if !slice.is_empty() => {
            TxDacGain::try_new(slice[0] as i8).unwrap_or_default()
        }
        _ => TxDacGain::default(),
    };
    (rx, tx)
}

/// Store validated half-dB settings as one byte per gain in the system partition.
pub fn save_audio_gains<S: KvBlobStore>(
    store: &mut S,
    rx: RxPgaGain,
    tx: TxDacGain,
) -> Result<(), ()> {
    let mut buf = [0u8; 64];
    store
        .store(crate::config::AUDIO_RX_GAIN_KEY, &[rx.half_db()], &mut buf)
        .map_err(|_| ())?;
    store
        .store(
            crate::config::AUDIO_TX_GAIN_KEY,
            &[tx.half_db() as u8],
            &mut buf,
        )
        .map_err(|_| ())
}

/// Persist gain changes to system flash (simulated for host and dummy persistence).
pub fn persist_audio_gains(rx: RxPgaGain, tx: TxDacGain) {
    #[cfg(target_os = "none")]
    #[cfg(not(feature = "dummy-persist"))]
    {
        let system_flash = EspFlash::new(crate::config::SYSTEM_KVS_RANGE);
        let mut store = SeqMapKvBlobStore::new(system_flash, crate::config::SYSTEM_KVS_RANGE);
        if save_audio_gains(&mut store, rx, tx).is_err() {
            warn!("Failed to persist audio gains to system_kvs");
        } else {
            info!(
                "Persisted audio gains to system_kvs: rx_half_db={}, tx_half_db={}",
                rx.half_db(),
                tx.half_db()
            );
        }
    }
    #[cfg(any(not(target_os = "none"), feature = "dummy-persist"))]
    {
        info!(
            "Persisted audio gains (simulated): rx_half_db={}, tx_half_db={}",
            rx.half_db(),
            tx.half_db()
        );
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
    #[cfg(target_os = "none")]
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
    #[cfg(any(not(target_os = "none"), feature = "dummy-persist"))]
    {
        info!("Persisted door duration (simulated): {}s", duration_sec);
    }
}

/// Outbound reporter that synchronizes runtime lock and duration state with telemetry
/// and persists duration changes into system flash.
pub struct FirmwarePersistenceReporter;

impl IntercomReporter for FirmwarePersistenceReporter {
    fn report_audio_gain(&self, rx: RxPgaGain, tx: TxDacGain) {
        intercom_core::domain::diagnostics::update_audio_gains(rx, tx);
        persist_audio_gains(rx, tx);
    }

    fn report_session_state(&self, call_state: CallState, active_owner: Option<SessionOwner>) {
        let bench_tone = active_owner == Some(SessionOwner::BenchTone)
            && matches!(call_state, CallState::Talking | CallState::Settling);
        intercom_core::domain::diagnostics::update_tone_active(bench_tone);
    }

    fn report_ring(&self, _ringing: bool) {}

    fn report_lock_state(&self, state: LockState) {
        intercom_core::domain::diagnostics::update_door_locked(matches!(state, LockState::Locked));
    }

    fn report_unlock_duration(&self, duration_sec: u32) {
        intercom_core::domain::diagnostics::update_door_unlock_duration(duration_sec);
        persist_door_duration(duration_sec);
    }

    fn report_bus_state(&self, state: BusLineState) {
        intercom_core::domain::diagnostics::update_bus_state(state);
    }

    fn report_call_state(&self, state: CallState) {
        intercom_core::domain::diagnostics::update_call_state(state);
    }
}

/// Immediately clear every relay output for the selected chip's board profile.
///
/// Keep this independent of HAL ownership so it remains available during panic
/// and reset handling. `target_os = "none"` excludes the MMIO accesses from host
/// tests; the chip feature selects the matching GPIO register block and pins.
#[inline(always)]
pub fn force_bus_relays_low() {
    #[cfg(all(target_os = "none", feature = "esp32c6"))]
    // SAFETY: ESP32-C6 GPIO.out_w1tc is the aligned write-one-to-clear register
    // at this address. The mask matches the C6 PCB's relay output pins.
    unsafe {
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x6009_100C as *mut u32;
        const BUS_RELAYS_GPIO_MASK: u32 = (1 << 7) | (1 << 10) | (1 << 18);
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, BUS_RELAYS_GPIO_MASK);
    }

    #[cfg(all(target_os = "none", feature = "esp32s31"))]
    // SAFETY: ESP32-S31 GPIO.out_w1tc is the aligned write-one-to-clear register
    // at this address. The mask matches the S31 board profile's relay pins.
    unsafe {
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x2058_300C as *mut u32;
        const BUS_RELAYS_GPIO_MASK: u32 = (1 << 7) | (1 << 10) | (1 << 18);
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, BUS_RELAYS_GPIO_MASK);
    }

    #[cfg(all(target_os = "none", feature = "esp32s3"))]
    // SAFETY: ESP32-S3 GPIO0-31 and GPIO32-53 clear registers are aligned,
    // write-one-to-clear MMIO registers at these addresses. The masks derive
    // from the role-based S3 relay pins and only clear those outputs.
    unsafe {
        let board = crate::platform::s3::BoardPins::SUPPORTED;
        let masks = crate::platform::s3::GpioClearMasks::from(crate::platform::s3::RelayPins {
            listen: board.bus_listen,
            talk: board.bus_talk,
            door: board.bus_door,
        });
        const GPIO_OUT_W1TC_REG: *mut u32 = 0x6000_400C as *mut u32;
        const GPIO_OUT1_W1TC_REG: *mut u32 = 0x6000_4018 as *mut u32;
        core::ptr::write_volatile(GPIO_OUT_W1TC_REG, masks.gpio_low);
        core::ptr::write_volatile(GPIO_OUT1_W1TC_REG, masks.gpio_high);
    }
}

/// Outbound reset driver implementing persistent credential erasure and hardware reboot.
pub struct FirmwareResetDriver;

impl SystemResetDriver for FirmwareResetDriver {
    fn erase_credentials(&self, scope: ResetScope) {
        force_bus_relays_low();
        #[cfg(target_os = "none")]
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
        #[cfg(not(target_os = "none"))]
        info!("Reset {:?} simulated on host", scope);
    }

    fn reboot(&self) -> ! {
        force_bus_relays_low();
        #[cfg(target_os = "none")]
        {
            esp_hal::system::software_reset();
        }
        #[cfg(not(target_os = "none"))]
        {
            panic!("Host reboot requested");
        }
    }
}

/// Create the partitioned persistence layer spanning Matter KVS and System KVS.
#[cfg(target_os = "none")]
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
#[cfg(target_os = "none")]
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

    let mut flash = EspFlash::new(range.clone());
    if let Err(err) = flash.erase_range_blocking() {
        warn!("Flash erase for {} failed: {:?}", label, err);
        return;
    }

    info!("{} erased successfully", label);
}

fn is_system_key(key: u16) -> bool {
    key == NETWORKS_KEY
        || key == crate::config::DOOR_DURATION_KEY
        || key == crate::config::AUDIO_RX_GAIN_KEY
        || key == crate::config::AUDIO_TX_GAIN_KEY
}

#[cfg(test)]
mod tests {
    use super::FirmwarePersistenceReporter;
    use intercom_core::domain::{
        diagnostics,
        intercom::{BusLineState, CallState},
    };
    use intercom_core::ports::IntercomReporter;

    #[test]
    fn persistence_reporter_forwards_bus_and_call_states_to_telemetry() {
        // Arrange
        let reporter = FirmwarePersistenceReporter;
        diagnostics::update_bus_state(BusLineState::Idle);
        diagnostics::update_call_state(CallState::Idle);

        // Act
        reporter.report_bus_state(BusLineState::Talk);
        reporter.report_call_state(CallState::Talking);
        let snapshot = diagnostics::current_intercom_state();
        diagnostics::update_bus_state(BusLineState::Idle);
        diagnostics::update_call_state(CallState::Idle);

        // Assert
        assert_eq!(snapshot.bus_state, BusLineState::Talk);
        assert_eq!(snapshot.call_state, CallState::Talking);
    }
}
