//! ESP32 internal SPI flash driver implementing `embedded-storage-async` traits.
//!
//! Wraps the ESP32 ROM SPI flash functions (`esp_rom_spiflash_{read,write,erase_sector}`)
//! to provide an async `MultiwriteNorFlash` implementation compatible with
//! `sequential-storage` and `EmbassyKvBlobStore`.
//!
//! The flash operations are synchronous (ROM calls) wrapped in async signatures.
//! This is fine because:
//! - Flash writes/erases are fast (~ms) and non-preemptible on ESP32 anyway
//! - The Matter persist loop only writes when commissioning state changes (rare)

use core::ops::Range;

use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};
use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash, ReadNorFlash};

use log::{debug, warn};

/// ESP32 SPI flash sector size (4 KiB).
const SECTOR_SIZE: usize = 4096;

/// ESP32 SPI flash write/read alignment (4 bytes = u32).
const WORD_SIZE: usize = 4;

// ============================================================================
// Error type
// ============================================================================

/// Errors from the ESP32 ROM SPI flash operations.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum EspFlashError {
    /// ROM function returned a non-zero error code.
    RomError(i32),
    /// Arguments are not properly aligned.
    NotAligned,
    /// Arguments are out of bounds.
    OutOfBounds,
}

impl NorFlashError for EspFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            EspFlashError::RomError(_) => NorFlashErrorKind::Other,
            EspFlashError::NotAligned => NorFlashErrorKind::NotAligned,
            EspFlashError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
        }
    }
}

// ============================================================================
// EspFlash driver
// ============================================================================

/// Async NOR flash driver for the ESP32's internal SPI flash.
///
/// Delegates to ROM functions which operate on absolute flash offsets.
/// Use with `EmbassyKvBlobStore::new(EspFlash::new(range), range)`.
///
/// # Flash characteristics (ESP32-C6)
/// - Sector size: 4 KiB (erase granularity)
/// - Write granularity: 4 bytes (u32 aligned)
/// - Read granularity: 1 byte (handled via aligned buffer internally)
/// - Multiwrite: supported (bits can go 1→0 without erase)
pub struct EspFlash {
    /// The flash range this driver covers (absolute offsets).
    range: Range<u32>,
}

impl EspFlash {
    /// Create a new flash driver covering the given absolute flash range.
    ///
    /// The range should correspond to a dedicated flash partition
    /// (e.g., the `matter_kvs` partition from `partitions.csv`).
    pub fn new(range: Range<u32>) -> Self {
        Self { range }
    }

    /// Read from flash with 4-byte alignment handling.
    ///
    /// The ROM `esp_rom_spiflash_read` requires both address and length to be
    /// 4-byte aligned. This method handles unaligned reads by reading into
    /// a word-aligned temporary buffer and copying the relevant bytes.
    fn read_aligned(&self, offset: u32, bytes: &mut [u8]) -> Result<(), EspFlashError> {
        if bytes.is_empty() {
            return Ok(());
        }

        // Align down to 4-byte boundary
        let aligned_start = offset & !(WORD_SIZE as u32 - 1);
        let head_skip = (offset - aligned_start) as usize;
        let total_len = head_skip + bytes.len();
        let aligned_len = (total_len + WORD_SIZE - 1) & !(WORD_SIZE - 1);

        // Stack buffer for small reads (covers typical sequential_storage access)
        let mut buf = [0u32; 64]; // 256 bytes
        if aligned_len > buf.len() * WORD_SIZE {
            // Fallback: read in chunks
            return self.read_chunked(offset, bytes);
        }

        let word_count = aligned_len / WORD_SIZE;
        let ret = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_read(
                aligned_start,
                buf.as_mut_ptr(),
                aligned_len as u32,
            )
        };

        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspFlashError::RomError(ret));
        }

        // Copy from aligned buffer to output, handling head offset
        let raw = unsafe {
            core::slice::from_raw_parts(buf.as_ptr() as *const u8, word_count * WORD_SIZE)
        };
        bytes.copy_from_slice(&raw[head_skip..head_skip + bytes.len()]);

        Ok(())
    }

    /// Fallback chunked read for buffers larger than 256 bytes.
    fn read_chunked(&self, offset: u32, bytes: &mut [u8]) -> Result<(), EspFlashError> {
        const CHUNK: usize = 256;
        let mut pos = 0;
        while pos < bytes.len() {
            let chunk_len = (bytes.len() - pos).min(CHUNK);
            // Recurse with smaller chunk (will use the stack buffer path)
            self.read_aligned(offset + pos as u32, &mut bytes[pos..pos + chunk_len])?;
            pos += chunk_len;
        }
        Ok(())
    }

    /// Write to flash with 4-byte alignment handling.
    ///
    /// The ROM `esp_rom_spiflash_write` requires both address and length to be
    /// 4-byte aligned. Callers should ensure proper alignment via `WRITE_SIZE = 4`.
    fn write_aligned(&self, offset: u32, bytes: &[u8]) -> Result<(), EspFlashError> {
        if bytes.is_empty() {
            return Ok(());
        }

        if offset as usize % WORD_SIZE != 0 || bytes.len() % WORD_SIZE != 0 {
            return Err(EspFlashError::NotAligned);
        }

        // Unlock flash for writing (idempotent, fast)
        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash unlock failed: {}", ret);
            return Err(EspFlashError::RomError(ret));
        }

        let ret = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_write(
                offset,
                bytes.as_ptr() as *const u32,
                bytes.len() as u32,
            )
        };

        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash write failed at 0x{:x}: {}", offset, ret);
            return Err(EspFlashError::RomError(ret));
        }

        Ok(())
    }
}

// ============================================================================
// embedded-storage-async trait implementations
// ============================================================================

impl ErrorType for EspFlash {
    type Error = EspFlashError;
}

impl ReadNorFlash for EspFlash {
    /// Read granularity: 1 byte (alignment handled internally).
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.read_aligned(offset, bytes)
    }

    fn capacity(&self) -> usize {
        (self.range.end - self.range.start) as usize
    }
}

impl NorFlash for EspFlash {
    /// Write granularity: 4 bytes (ROM requirement).
    const WRITE_SIZE: usize = WORD_SIZE;

    /// Erase granularity: 4 KiB sectors.
    const ERASE_SIZE: usize = SECTOR_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        if from % SECTOR_SIZE as u32 != 0 || to % SECTOR_SIZE as u32 != 0 {
            return Err(EspFlashError::NotAligned);
        }
        if from > to || to > self.range.end {
            return Err(EspFlashError::OutOfBounds);
        }

        // Unlock flash for erasing
        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash unlock failed: {}", ret);
            return Err(EspFlashError::RomError(ret));
        }

        let mut addr = from;
        while addr < to {
            let sector = addr / SECTOR_SIZE as u32;
            debug!("Erasing flash sector {} (0x{:x})", sector, addr);

            let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector) };

            if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
                warn!("Flash erase sector {} failed: {}", sector, ret);
                return Err(EspFlashError::RomError(ret));
            }

            addr += SECTOR_SIZE as u32;
        }

        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.write_aligned(offset, bytes)
    }
}

/// ESP32 SPI flash supports multiwrite (writing to the same word again
/// performs a bitwise AND — bits go 1→0 but not 0→1).
impl MultiwriteNorFlash for EspFlash {}
