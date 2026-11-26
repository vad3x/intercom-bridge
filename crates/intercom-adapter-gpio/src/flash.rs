//! ESP32 internal SPI flash driver.
//!
//! On host (`not(target_os = "none")`), re-exports `edf_gpio::flash::EspFlash`
//! (in-memory mock). On target (`target_os = "none"`), uses `esp_storage::ll`
//! (IRAM-placed ROM/SPI flash primitives) instead of
//! `esp_storage::FlashStorage::new` so `get_flash_size()`'s `SPI1` RDID probe
//! is not executed from XIP flash (which returns `capacity = 0` on ESP32-S3).
//! Dual-core platforms (such as ESP32-S3) wrap [`EspFlash`] in a platform
//! decorator that parks the secondary core around write/erase operations.

#[cfg(not(target_os = "none"))]
pub use edf_gpio::flash::{EspFlash, EspFlashError};

#[cfg(target_os = "none")]
mod target_impl {
    use core::ops::Range;

    use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};
    use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash, ReadNorFlash};

    pub const SECTOR_SIZE: usize = 4096;
    pub const WORD_SIZE: usize = 4;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EspFlashError {
        RomError(i32),
        NotAligned,
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

    pub struct EspFlash {
        range: Range<u32>,
    }

    impl EspFlash {
        pub fn new(range: Range<u32>) -> Self {
            Self { range }
        }

        pub fn range(&self) -> &Range<u32> {
            &self.range
        }

        pub fn erase_range_blocking(&mut self) -> Result<(), EspFlashError> {
            self.erase_blocking(self.range.start, self.range.end)
        }

        pub fn erase_blocking(&mut self, from: u32, to: u32) -> Result<(), EspFlashError> {
            self.check_erase_bounds(from, to)?;
            let start_sector = from / SECTOR_SIZE as u32;
            let end_sector = to / SECTOR_SIZE as u32;
            unsafe { esp_storage::ll::spiflash_unlock() }.map_err(EspFlashError::RomError)?;
            for sector in start_sector..end_sector {
                unsafe { esp_storage::ll::spiflash_erase_sector(sector) }
                    .map_err(EspFlashError::RomError)?;
            }
            Ok(())
        }

        pub fn write_blocking(&mut self, offset: u32, bytes: &[u8]) -> Result<(), EspFlashError> {
            if bytes.is_empty() {
                return Ok(());
            }
            if !(offset as usize).is_multiple_of(WORD_SIZE)
                || !bytes.len().is_multiple_of(WORD_SIZE)
            {
                return Err(EspFlashError::NotAligned);
            }
            self.check_rw_bounds(offset, bytes.len())?;

            let mut aligned = [0u32; 64];
            let mut pos = 0;
            unsafe { esp_storage::ll::spiflash_unlock() }.map_err(EspFlashError::RomError)?;
            while pos < bytes.len() {
                let chunk_bytes = (bytes.len() - pos).min(aligned.len() * WORD_SIZE);
                let word_count = chunk_bytes / WORD_SIZE;
                for (idx, word) in aligned[..word_count].iter_mut().enumerate() {
                    let start = pos + idx * WORD_SIZE;
                    *word = u32::from_le_bytes([
                        bytes[start],
                        bytes[start + 1],
                        bytes[start + 2],
                        bytes[start + 3],
                    ]);
                }
                unsafe {
                    esp_storage::ll::spiflash_write(
                        offset + pos as u32,
                        aligned.as_ptr(),
                        chunk_bytes as u32,
                    )
                }
                .map_err(EspFlashError::RomError)?;
                pos += chunk_bytes;
            }
            Ok(())
        }

        fn check_rw_bounds(&self, offset: u32, len: usize) -> Result<(), EspFlashError> {
            if offset < self.range.start
                || (offset as usize).saturating_add(len) > self.range.end as usize
            {
                return Err(EspFlashError::OutOfBounds);
            }
            Ok(())
        }

        fn check_erase_bounds(&self, from: u32, to: u32) -> Result<(), EspFlashError> {
            if !from.is_multiple_of(SECTOR_SIZE as u32) || !to.is_multiple_of(SECTOR_SIZE as u32) {
                return Err(EspFlashError::NotAligned);
            }
            if from < self.range.start || from > to || to > self.range.end {
                return Err(EspFlashError::OutOfBounds);
            }
            Ok(())
        }

        fn read_aligned_chunk(&self, offset: u32, bytes: &mut [u8]) -> Result<(), EspFlashError> {
            let mut buf = [0u32; 68];
            let mut pos = 0;
            // Leave up to 3 head + 3 tail alignment bytes inside the 272-byte buffer.
            const MAX_CHUNK: usize = 256;
            while pos < bytes.len() {
                let chunk_len = (bytes.len() - pos).min(MAX_CHUNK);
                let cur_offset = offset + pos as u32;
                let aligned_start = cur_offset & !(WORD_SIZE as u32 - 1);
                let head_skip = (cur_offset - aligned_start) as usize;
                let aligned_len = (head_skip + chunk_len + WORD_SIZE - 1) & !(WORD_SIZE - 1);

                unsafe {
                    esp_storage::ll::spiflash_read(
                        aligned_start,
                        buf.as_mut_ptr(),
                        aligned_len as u32,
                    )
                }
                .map_err(EspFlashError::RomError)?;

                let raw =
                    unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u8, aligned_len) };
                bytes[pos..pos + chunk_len].copy_from_slice(&raw[head_skip..head_skip + chunk_len]);
                pos += chunk_len;
            }
            Ok(())
        }
    }

    impl ErrorType for EspFlash {
        type Error = EspFlashError;
    }

    impl ReadNorFlash for EspFlash {
        const READ_SIZE: usize = 1;

        async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
            if bytes.is_empty() {
                return Ok(());
            }
            self.check_rw_bounds(offset, bytes.len())?;
            self.read_aligned_chunk(offset, bytes)
        }

        fn capacity(&self) -> usize {
            (self.range.end.saturating_sub(self.range.start)) as usize
        }
    }

    impl NorFlash for EspFlash {
        const WRITE_SIZE: usize = WORD_SIZE;
        const ERASE_SIZE: usize = SECTOR_SIZE;

        async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
            self.erase_blocking(from, to)
        }

        async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            self.write_blocking(offset, bytes)
        }
    }

    impl MultiwriteNorFlash for EspFlash {}
}

#[cfg(target_os = "none")]
pub use target_impl::{EspFlash, EspFlashError};
