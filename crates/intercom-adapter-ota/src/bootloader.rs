//! ESP-IDF OTA Bootloader State Machine
//!
//! Manages the `otadata` partition containing two redundant `esp_ota_select_entry_t`
//! structures. Tracks sequence numbers, image validation states (`ESP_OTA_IMG_NEW`,
//! `ESP_OTA_IMG_VALID`, `ESP_OTA_IMG_INVALID`), CRC-32 integrity, active slot
//! resolution, and fail-safe rollback logic across `ota_0` and `ota_1` slots.

use log::info;
#[cfg(target_arch = "riscv32")]
use log::warn;

// ============================================================================
// Public Structs & Implementation
// ============================================================================

/// ESP-IDF compatible OTA bootloader coordinator.
///
/// Tracks the active running slot, resolves next target slots, and commits
/// validation / pending verification states into redundant `otadata` sectors.
pub struct OtaBootloader<F = DefaultFlash> {
    flash: F,
    active: ActiveSlot,
}

impl<F: OtaFlashStorage> OtaBootloader<F> {
    /// Initialize the bootloader by reading `otadata` sectors from flash storage.
    pub fn new(mut flash: F) -> Result<Self, BootloaderError> {
        let mut sector0 = [0xffu8; OTA_DATA_SECTOR_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_SECTOR_SIZE];
        flash
            .read_otadata(&mut sector0, &mut sector1)
            .map_err(|_| BootloaderError::Flash)?;

        let active = resolve_active_slot(&sector0, &sector1);
        info!(
            "OTA Bootloader initialized: active slot={:?}, seq={}, state={:?}",
            active.slot, active.seq, active.state
        );

        Ok(Self { flash, active })
    }

    /// Returns the currently running application partition slot.
    pub fn get_running_slot(&self) -> Slot {
        self.active.slot
    }

    /// Returns the current active sequence number in `otadata`.
    pub fn get_running_seq(&self) -> u32 {
        self.active.seq
    }

    /// Returns the target partition slot for the next firmware update.
    pub fn get_target_slot(&self) -> Slot {
        self.active.slot.other()
    }

    /// Returns the sequence number that will be assigned to the next firmware update.
    pub fn get_next_seq(&self) -> u32 {
        compute_next_seq(self.active.seq, self.get_target_slot())
    }

    /// Returns the full active slot metadata.
    pub fn get_active(&self) -> ActiveSlot {
        self.active
    }

    /// Commit a newly flashed image into `otadata` marked as `OtaState::New`.
    ///
    /// The entry is written to the alternate sector with the calculated sequence number,
    /// so the ESP-IDF 2nd-stage bootloader selects it on the next system reset.
    pub fn mark_pending_verify(
        &mut self,
        target_slot: Slot,
        seq: u32,
    ) -> Result<(), BootloaderError> {
        let write_sec = if self.active.seq == 0 {
            0
        } else {
            1 - self.active.sector_index
        };

        let mut sector = [0xffu8; OTA_DATA_SECTOR_SIZE];
        write_ota_entry(&mut sector, seq, OtaState::New);

        self.flash
            .write_otadata_sector(write_sec, &sector)
            .map_err(|_| BootloaderError::Flash)?;

        info!(
            "OTA Bootloader: marked sector {} pending verify (slot={:?}, seq={})",
            write_sec, target_slot, seq
        );

        self.active = ActiveSlot {
            slot: target_slot,
            seq,
            state: OtaState::New,
            sector_index: write_sec,
        };

        Ok(())
    }

    /// Confirm that the currently running firmware has booted and verified successfully.
    ///
    /// If the active entry is not already `OtaState::Valid`, rewrites the active sector
    /// with `OtaState::Valid` to cancel automatic bootloader rollback.
    pub fn mark_valid(&mut self) -> Result<(), BootloaderError> {
        if self.active.state == OtaState::Valid && self.active.seq > 0 {
            return Ok(());
        }

        let write_sec = self.active.sector_index;
        let seq = if self.active.seq == 0 { 1 } else { self.active.seq };

        let mut sector = [0xffu8; OTA_DATA_SECTOR_SIZE];
        write_ota_entry(&mut sector, seq, OtaState::Valid);

        self.flash
            .write_otadata_sector(write_sec, &sector)
            .map_err(|_| BootloaderError::Flash)?;

        info!(
            "OTA Bootloader: confirmed valid slot={:?}, seq={} in sector {}",
            self.active.slot, seq, write_sec
        );

        self.active = ActiveSlot {
            slot: Slot::from_seq(seq),
            seq,
            state: OtaState::Valid,
            sector_index: write_sec,
        };

        Ok(())
    }
}

#[cfg(target_arch = "riscv32")]
impl OtaBootloader<EspOtaFlash> {
    /// Load bootloader state using the ESP32-C6 ROM SPI flash driver.
    pub fn load() -> Result<Self, BootloaderError> {
        Self::new(EspOtaFlash)
    }
}

// ============================================================================
// Public Functions
// ============================================================================

/// Determine the active application slot from two raw `otadata` sectors.
///
/// Evaluates CRC-32 integrity and checks that the image state is not `Invalid`
/// or `Aborted`. Whichever sector contains the highest valid sequence number
/// wins. If both sectors are blank or invalid, defaults to `Slot::Ota0` (seq 0).
pub fn resolve_active_slot(sector0: &[u8], sector1: &[u8]) -> ActiveSlot {
    let entry0 = EspOtaSelectEntry::from_bytes(sector0);
    let entry1 = EspOtaSelectEntry::from_bytes(sector1);

    let valid0 = entry0.as_ref().map_or(false, |e| e.is_valid());
    let valid1 = entry1.as_ref().map_or(false, |e| e.is_valid());

    match (valid0, valid1) {
        (true, true) => {
            let e0 = entry0.unwrap();
            let e1 = entry1.unwrap();
            if e0.ota_seq > e1.ota_seq {
                ActiveSlot {
                    slot: Slot::from_seq(e0.ota_seq),
                    seq: e0.ota_seq,
                    state: OtaState::from_u32(e0.ota_state),
                    sector_index: 0,
                }
            } else {
                ActiveSlot {
                    slot: Slot::from_seq(e1.ota_seq),
                    seq: e1.ota_seq,
                    state: OtaState::from_u32(e1.ota_state),
                    sector_index: 1,
                }
            }
        }
        (true, false) => {
            let e0 = entry0.unwrap();
            ActiveSlot {
                slot: Slot::from_seq(e0.ota_seq),
                seq: e0.ota_seq,
                state: OtaState::from_u32(e0.ota_state),
                sector_index: 0,
            }
        }
        (false, true) => {
            let e1 = entry1.unwrap();
            ActiveSlot {
                slot: Slot::from_seq(e1.ota_seq),
                seq: e1.ota_seq,
                state: OtaState::from_u32(e1.ota_state),
                sector_index: 1,
            }
        }
        (false, false) => ActiveSlot {
            slot: Slot::Ota0,
            seq: 0,
            state: OtaState::Undefined,
            sector_index: 0,
        },
    }
}

/// Calculate the opposite slot for alternate ping-pong flashing.
pub fn compute_target_slot(active_slot: Slot) -> Slot {
    active_slot.other()
}

/// Calculate the smallest sequence number strictly greater than `current_seq`
/// that maps to `target_slot` under the ESP-IDF formula: `(seq - 1) % 2 == slot_idx`.
pub fn compute_next_seq(current_seq: u32, target_slot: Slot) -> u32 {
    let target_idx = target_slot.index();
    let mut next_seq = current_seq.saturating_add(1);
    while (next_seq.saturating_sub(1)) % 2 != target_idx || next_seq == 0 {
        next_seq = next_seq.saturating_add(1);
    }
    next_seq
}

/// Compute the IEEE 802.3 CRC-32 checksum of an OTA sequence number.
pub fn compute_crc(ota_seq: u32) -> u32 {
    crc32fast::hash(&ota_seq.to_le_bytes())
}

/// Serialize an `esp_ota_select_entry_t` into the beginning of a sector buffer.
pub fn write_ota_entry(buffer: &mut [u8], seq: u32, state: OtaState) {
    let entry = EspOtaSelectEntry::new(seq, state);
    let bytes = entry.to_bytes();
    if buffer.len() >= EspOtaSelectEntry::SIZE {
        buffer[..EspOtaSelectEntry::SIZE].copy_from_slice(&bytes);
    }
}

/// Standalone convenience helper called on startup to confirm running firmware health.
#[cfg(target_arch = "riscv32")]
pub fn mark_running_slot_valid() -> Result<(), BootloaderError> {
    let mut bootloader = OtaBootloader::load()?;
    bootloader.mark_valid()
}

/// Standalone convenience helper on host targets (no-op success).
#[cfg(not(target_arch = "riscv32"))]
pub fn mark_running_slot_valid() -> Result<(), BootloaderError> {
    Ok(())
}

// ============================================================================
// Public Supporting Types & Traits
// ============================================================================

/// NOR Flash abstraction for reading and writing `otadata` sectors.
pub trait OtaFlashStorage {
    type Error: core::fmt::Debug;

    /// Read both 4096-byte `otadata` sectors.
    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_SECTOR_SIZE],
        sector1: &mut [u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error>;

    /// Erase and rewrite one `otadata` sector (0 or 1).
    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error>;
}

/// Information describing the currently active boot slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveSlot {
    /// Active partition slot (`Ota0` or `Ota1`).
    pub slot: Slot,
    /// OTA sequence counter.
    pub seq: u32,
    /// Image verification state.
    pub state: OtaState,
    /// Sector index in `otadata` that hosts this entry (0 or 1).
    pub sector_index: usize,
}

/// Dual-bank firmware partition slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Primary slot (`ota_0`).
    Ota0,
    /// Secondary slot (`ota_1`).
    Ota1,
}

impl Slot {
    /// Return the alternate slot.
    pub fn other(&self) -> Self {
        match self {
            Slot::Ota0 => Slot::Ota1,
            Slot::Ota1 => Slot::Ota0,
        }
    }

    /// Return string representation matching `partitions.csv`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Slot::Ota0 => "ota_0",
            Slot::Ota1 => "ota_1",
        }
    }

    /// Partition index (0 for `ota_0`, 1 for `ota_1`).
    pub fn index(&self) -> u32 {
        match self {
            Slot::Ota0 => 0,
            Slot::Ota1 => 1,
        }
    }

    /// Create from zero-based partition index.
    pub fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Slot::Ota0),
            1 => Some(Slot::Ota1),
            _ => None,
        }
    }

    /// Map sequence counter to partition slot using ESP-IDF formula `(seq - 1) % 2`.
    pub fn from_seq(seq: u32) -> Self {
        if seq == 0 || (seq.saturating_sub(1)) % 2 == 0 {
            Slot::Ota0
        } else {
            Slot::Ota1
        }
    }
}

/// ESP-IDF image states stored in `esp_ota_select_entry_t.ota_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum OtaState {
    /// Newly written image awaiting first boot.
    New = 0x00000000,
    /// Running first trial boot, pending confirmation.
    PendingVerify = 0x00000001,
    /// Verified functional; rollback restrictions cancelled.
    Valid = 0x00000002,
    /// Failed diagnostics; bootloader will roll back.
    Invalid = 0x00000003,
    /// Reset during trial before verification; bootloader rolls back.
    Aborted = 0x00000004,
    /// Default erased flash state (all 0xFF).
    Undefined = 0xFFFFFFFF,
}

impl OtaState {
    /// Convert raw `u32` value to `OtaState`.
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => OtaState::New,
            1 => OtaState::PendingVerify,
            2 => OtaState::Valid,
            3 => OtaState::Invalid,
            4 => OtaState::Aborted,
            _ => OtaState::Undefined,
        }
    }
}

/// ESP-IDF `esp_ota_select_entry_t` structure (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EspOtaSelectEntry {
    /// OTA sequence number.
    pub ota_seq: u32,
    /// Optional sequence label / padding.
    pub seq_label: [u8; 20],
    /// Operational state of the slot (`OtaState`).
    pub ota_state: u32,
    /// CRC-32 of `ota_seq`.
    pub crc: u32,
}

impl EspOtaSelectEntry {
    /// Fixed byte length of the entry in flash.
    pub const SIZE: usize = 32;

    /// Construct a new entry with computed CRC-32.
    pub fn new(ota_seq: u32, ota_state: OtaState) -> Self {
        let crc = compute_crc(ota_seq);
        Self {
            ota_seq,
            seq_label: [0u8; 20],
            ota_state: ota_state as u32,
            crc,
        }
    }

    /// Parse structure from a byte slice. Returns `None` if slice is too short.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let ota_seq = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let mut seq_label = [0u8; 20];
        seq_label.copy_from_slice(&bytes[4..24]);
        let ota_state = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
        let crc = u32::from_le_bytes(bytes[28..32].try_into().ok()?);

        Some(Self {
            ota_seq,
            seq_label,
            ota_state,
            crc,
        })
    }

    /// Serialize structure into little-endian 32-byte representation.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0xffu8; Self::SIZE];
        bytes[0..4].copy_from_slice(&self.ota_seq.to_le_bytes());
        bytes[4..24].copy_from_slice(&self.seq_label);
        bytes[24..28].copy_from_slice(&self.ota_state.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.crc.to_le_bytes());
        bytes
    }

    /// Check if the entry is valid for bootloader selection.
    pub fn is_valid(&self) -> bool {
        self.ota_seq != 0xFFFF_FFFF
            && self.ota_seq != 0
            && self.crc == compute_crc(self.ota_seq)
            && self.ota_state != OtaState::Invalid as u32
            && self.ota_state != OtaState::Aborted as u32
    }
}

/// Errors occurring during bootloader state operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootloaderError {
    /// Flash I/O or alignment failure.
    Flash,
    /// Invalid slot specification.
    InvalidSlot,
}

/// In-memory implementation of `OtaFlashStorage` for host testing.
#[derive(Debug, Clone)]
pub struct MemOtaFlash {
    /// Sector 0 buffer.
    pub sector0: [u8; OTA_DATA_SECTOR_SIZE],
    /// Sector 1 buffer.
    pub sector1: [u8; OTA_DATA_SECTOR_SIZE],
}

impl MemOtaFlash {
    /// Create new in-memory flash initialized with given sectors.
    pub fn new(sector0: [u8; OTA_DATA_SECTOR_SIZE], sector1: [u8; OTA_DATA_SECTOR_SIZE]) -> Self {
        Self { sector0, sector1 }
    }

    /// Create blank/unwritten in-memory flash (all 0xFF).
    pub fn blank() -> Self {
        Self {
            sector0: [0xffu8; OTA_DATA_SECTOR_SIZE],
            sector1: [0xffu8; OTA_DATA_SECTOR_SIZE],
        }
    }
}

impl OtaFlashStorage for MemOtaFlash {
    type Error = core::convert::Infallible;

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_SECTOR_SIZE],
        sector1: &mut [u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error> {
        sector0.copy_from_slice(&self.sector0);
        sector1.copy_from_slice(&self.sector1);
        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error> {
        match sector_index {
            0 => self.sector0.copy_from_slice(data),
            1 => self.sector1.copy_from_slice(data),
            _ => panic!("Invalid sector index"),
        }
        Ok(())
    }
}

// ============================================================================
// Constants
// ============================================================================

/// ESP32 SPI flash sector size (4 KiB).
pub const OTA_DATA_SECTOR_SIZE: usize = 4096;

/// Absolute flash offset of `otadata` partition (from partitions.csv).
pub const OTA_DATA_OFFSET: u32 = 0x00f000;

/// Absolute flash offset of sector 0.
pub const OTA_DATA_SECTOR_0_OFFSET: u32 = OTA_DATA_OFFSET;

/// Absolute flash offset of sector 1.
pub const OTA_DATA_SECTOR_1_OFFSET: u32 = OTA_DATA_OFFSET + OTA_DATA_SECTOR_SIZE as u32;

// ============================================================================
// Target Hardware SPI Flash Implementation (RISC-V only)
// ============================================================================

#[cfg(target_arch = "riscv32")]
pub type DefaultFlash = EspOtaFlash;

#[cfg(not(target_arch = "riscv32"))]
pub type DefaultFlash = MemOtaFlash;

#[cfg(target_arch = "riscv32")]
pub struct EspOtaFlash;

#[cfg(target_arch = "riscv32")]
#[derive(Debug, Clone, Copy)]
pub enum EspOtaFlashError {
    Rom(i32),
    InvalidSector,
}

#[cfg(target_arch = "riscv32")]
impl OtaFlashStorage for EspOtaFlash {
    type Error = EspOtaFlashError;

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_SECTOR_SIZE],
        sector1: &mut [u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error> {
        let mut buf0 = [0u32; OTA_DATA_SECTOR_SIZE / 4];
        let ret0 = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_read(
                OTA_DATA_SECTOR_0_OFFSET,
                buf0.as_mut_ptr(),
                OTA_DATA_SECTOR_SIZE as u32,
            )
        };
        if ret0 != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspOtaFlashError::Rom(ret0));
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf0.as_ptr() as *const u8,
                sector0.as_mut_ptr(),
                OTA_DATA_SECTOR_SIZE,
            );
        }

        let mut buf1 = [0u32; OTA_DATA_SECTOR_SIZE / 4];
        let ret1 = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_read(
                OTA_DATA_SECTOR_1_OFFSET,
                buf1.as_mut_ptr(),
                OTA_DATA_SECTOR_SIZE as u32,
            )
        };
        if ret1 != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspOtaFlashError::Rom(ret1));
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf1.as_ptr() as *const u8,
                sector1.as_mut_ptr(),
                OTA_DATA_SECTOR_SIZE,
            );
        }

        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_SECTOR_SIZE],
    ) -> Result<(), Self::Error> {
        let (offset, sector_num) = match sector_index {
            0 => (
                OTA_DATA_SECTOR_0_OFFSET,
                OTA_DATA_SECTOR_0_OFFSET / OTA_DATA_SECTOR_SIZE as u32,
            ),
            1 => (
                OTA_DATA_SECTOR_1_OFFSET,
                OTA_DATA_SECTOR_1_OFFSET / OTA_DATA_SECTOR_SIZE as u32,
            ),
            _ => return Err(EspOtaFlashError::InvalidSector),
        };

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash unlock failed: {}", ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector_num) };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash erase sector {} failed: {}", sector_num, ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        let mut aligned = [0u32; OTA_DATA_SECTOR_SIZE / 4];
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                aligned.as_mut_ptr() as *mut u8,
                OTA_DATA_SECTOR_SIZE,
            );
        }

        let ret = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_write(
                offset,
                aligned.as_ptr(),
                OTA_DATA_SECTOR_SIZE as u32,
            )
        };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash write at 0x{:x} failed: {}", offset, ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_slot_with_higher_valid_sequence() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let mut sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 2);
        assert_eq!(active.slot, Slot::Ota1);
        assert_eq!(active.sector_index, 1);
    }

    #[test]
    fn selects_slot_with_higher_valid_sequence_reversed() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let mut sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 3, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 3);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn ignores_corrupted_crc() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let mut sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 5, OtaState::Valid);
        // Corrupt CRC in sector 0
        sector0[30] ^= 0xff;
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 2);
        assert_eq!(active.slot, Slot::Ota1);
        assert_eq!(active.sector_index, 1);
    }

    #[test]
    fn defaults_to_ota0_when_flash_is_blank() {
        // Arrange
        let sector0 = [0xffu8; 4096];
        let sector1 = [0xffu8; 4096];

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 0);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.state, OtaState::Undefined);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn ignores_invalid_and_aborted_states() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let mut sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Invalid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 1);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn slot_mapping_and_next_sequence_calculation() {
        // Arrange & Act & Assert
        // Slot mapping from sequence
        assert_eq!(Slot::from_seq(0), Slot::Ota0);
        assert_eq!(Slot::from_seq(1), Slot::Ota0);
        assert_eq!(Slot::from_seq(2), Slot::Ota1);
        assert_eq!(Slot::from_seq(3), Slot::Ota0);
        assert_eq!(Slot::from_seq(4), Slot::Ota1);

        // Target slot toggling
        assert_eq!(compute_target_slot(Slot::Ota0), Slot::Ota1);
        assert_eq!(compute_target_slot(Slot::Ota1), Slot::Ota0);
        assert_eq!(Slot::Ota0.other(), Slot::Ota1);
        assert_eq!(Slot::Ota1.other(), Slot::Ota0);

        // Next sequence computation
        assert_eq!(compute_next_seq(0, Slot::Ota1), 2);
        assert_eq!(compute_next_seq(0, Slot::Ota0), 1);
        assert_eq!(compute_next_seq(1, Slot::Ota1), 2);
        assert_eq!(compute_next_seq(2, Slot::Ota0), 3);
        assert_eq!(compute_next_seq(3, Slot::Ota1), 4);
    }

    #[test]
    fn entry_serialization_and_crc_verification() {
        // Arrange
        let entry = EspOtaSelectEntry::new(42, OtaState::PendingVerify);

        // Act
        let bytes = entry.to_bytes();
        let parsed = EspOtaSelectEntry::from_bytes(&bytes).unwrap();

        // Assert
        assert_eq!(parsed.ota_seq, 42);
        assert_eq!(parsed.ota_state, OtaState::PendingVerify as u32);
        assert_eq!(parsed.crc, compute_crc(42));
        assert!(parsed.is_valid());
    }

    #[test]
    fn bootloader_state_transitions_with_in_memory_flash() {
        // Arrange
        let flash = MemOtaFlash::blank();
        let mut bootloader = OtaBootloader::new(flash).unwrap();

        // Act 1: Initial state on blank flash
        assert_eq!(bootloader.get_running_slot(), Slot::Ota0);
        assert_eq!(bootloader.get_running_seq(), 0);
        assert_eq!(bootloader.get_target_slot(), Slot::Ota1);
        assert_eq!(bootloader.get_next_seq(), 2);

        // Act 2: Flash update into target slot (Ota1, seq 2)
        let target = bootloader.get_target_slot();
        let seq = bootloader.get_next_seq();
        bootloader.mark_pending_verify(target, seq).unwrap();

        // Assert 2
        assert_eq!(bootloader.get_running_slot(), Slot::Ota1);
        assert_eq!(bootloader.get_running_seq(), 2);
        assert_eq!(bootloader.get_target_slot(), Slot::Ota0);
        assert_eq!(bootloader.get_next_seq(), 3);

        // Act 3: Confirm valid on boot
        bootloader.mark_valid().unwrap();

        // Assert 3
        assert_eq!(bootloader.get_active().state, OtaState::Valid);
        assert_eq!(bootloader.get_running_slot(), Slot::Ota1);
    }

    #[test]
    fn mark_valid_is_idempotent() {
        // Arrange
        let mut sector0 = [0xffu8; 4096];
        let sector1 = [0xffu8; 4096];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        let flash = MemOtaFlash::new(sector0, sector1);
        let mut bootloader = OtaBootloader::new(flash).unwrap();

        // Act
        let res = bootloader.mark_valid();

        // Assert
        assert!(res.is_ok());
        assert_eq!(bootloader.get_active().state, OtaState::Valid);
    }
}
