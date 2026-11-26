//! ESP32-S3 dual-core flash decorator that parks Core 1 during write/erase.

use core::ops::Range;

use embedded_storage::nor_flash::ErrorType;
use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash, ReadNorFlash};
use esp_hal::peripherals::CPU_CTRL;
use esp_hal::system::{is_running, Cpu, CpuControl};
use intercom_adapter_gpio::flash::{EspFlash, EspFlashError};

fn with_other_core_parked<R>(op: impl FnOnce() -> R) -> R {
    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let mut parked_cpu = None;
    for other_cpu in Cpu::other() {
        if is_running(other_cpu) {
            unsafe { cpu_ctrl.park_core(other_cpu) };
            parked_cpu = Some(other_cpu);
            break;
        }
    }

    let result = op();

    if let Some(other_cpu) = parked_cpu {
        let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
        cpu_ctrl.unpark_core(other_cpu);
    }

    result
}

/// [`EspFlash`] wrapper for ESP32-S3 that parks Core 1 around flash mutations.
pub struct S3Flash {
    inner: EspFlash,
}

impl S3Flash {
    pub fn new(range: Range<u32>) -> Self {
        Self {
            inner: EspFlash::new(range),
        }
    }

    pub fn erase_range_blocking(&mut self) -> Result<(), EspFlashError> {
        with_other_core_parked(|| self.inner.erase_range_blocking())
    }
}

impl ErrorType for S3Flash {
    type Error = EspFlashError;
}

impl ReadNorFlash for S3Flash {
    const READ_SIZE: usize = <EspFlash as ReadNorFlash>::READ_SIZE;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.inner.read(offset, bytes).await
    }

    fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

impl NorFlash for S3Flash {
    const WRITE_SIZE: usize = <EspFlash as NorFlash>::WRITE_SIZE;
    const ERASE_SIZE: usize = <EspFlash as NorFlash>::ERASE_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        with_other_core_parked(|| self.inner.erase_blocking(from, to))
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        with_other_core_parked(|| self.inner.write_blocking(offset, bytes))
    }
}

impl MultiwriteNorFlash for S3Flash {}
