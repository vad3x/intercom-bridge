//! ESP32 internal SPI flash driver.
//!
//! On host (`not(target_os = "none")`), re-exports `edf_gpio::flash::EspFlash`
//! (in-memory mock). On target (`target_os = "none"`), wraps
//! `esp_storage::FlashStorage` so all ROM SPI flash calls run from IRAM and, on
//! dual-core ESP32-S3, automatically park Core 1 during write/erase.

#[cfg(not(target_os = "none"))]
pub use edf_gpio::flash::{EspFlash, EspFlashError};

#[cfg(target_os = "none")]
mod target_impl {
    use core::ops::Range;

    use embedded_storage::nor_flash::{ErrorType, NorFlashError, NorFlashErrorKind};
    use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash, ReadNorFlash};
    use esp_storage::{FlashStorage, FlashStorageError};

    pub const SECTOR_SIZE: usize = FlashStorage::SECTOR_SIZE as usize;
    pub const WORD_SIZE: usize = FlashStorage::WORD_SIZE as usize;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EspFlashError {
        Storage(FlashStorageError),
        NotAligned,
        OutOfBounds,
    }

    impl NorFlashError for EspFlashError {
        fn kind(&self) -> NorFlashErrorKind {
            match self {
                EspFlashError::Storage(FlashStorageError::NotAligned)
                | EspFlashError::NotAligned => NorFlashErrorKind::NotAligned,
                EspFlashError::Storage(FlashStorageError::OutOfBounds)
                | EspFlashError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
                EspFlashError::Storage(_) => NorFlashErrorKind::Other,
            }
        }
    }

    impl From<FlashStorageError> for EspFlashError {
        fn from(err: FlashStorageError) -> Self {
            Self::Storage(err)
        }
    }

    pub struct EspFlash {
        range: Range<u32>,
        storage: FlashStorage<'static>,
    }

    impl EspFlash {
        pub fn new(range: Range<u32>) -> Self {
            // SAFETY: FlashStorage wraps ROM SPI flash behind a critical section
            // (and Core 1 auto-park on dual-core chips); instances cover disjoint
            // KVS ranges used sequentially on Core 0.
            let storage = FlashStorage::new(unsafe { esp_hal::peripherals::FLASH::steal() });
            #[cfg(feature = "esp32s3")]
            let storage = storage.multicore_auto_park();
            Self { range, storage }
        }

        pub fn range(&self) -> &Range<u32> {
            &self.range
        }

        pub fn erase_range_blocking(&mut self) -> Result<(), EspFlashError> {
            self.check_erase_bounds(self.range.start, self.range.end)?;
            self.storage
                .erase(self.range.start, self.range.end)
                .map_err(EspFlashError::from)
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
            self.storage
                .read_nor(offset, bytes)
                .map_err(EspFlashError::from)
        }

        fn capacity(&self) -> usize {
            (self.range.end.saturating_sub(self.range.start)) as usize
        }
    }

    impl NorFlash for EspFlash {
        const WRITE_SIZE: usize = WORD_SIZE;
        const ERASE_SIZE: usize = SECTOR_SIZE;

        async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
            self.check_erase_bounds(from, to)?;
            self.storage.erase(from, to).map_err(EspFlashError::from)
        }

        async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            if bytes.is_empty() {
                return Ok(());
            }
            self.check_rw_bounds(offset, bytes.len())?;
            self.storage
                .write_nor(offset, bytes)
                .map_err(EspFlashError::from)
        }
    }

    impl MultiwriteNorFlash for EspFlash {}
}

#[cfg(target_os = "none")]
pub use target_impl::{EspFlash, EspFlashError};
