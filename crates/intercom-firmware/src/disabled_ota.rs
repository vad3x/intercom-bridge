//! Disabled OTA driver for hardware targets without dual-bank OTA storage.

use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use log::warn;

/// Stub OTA flash driver that safely rejects firmware uploads.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledOtaDriver;

impl OtaFlashDriver for DisabledOtaDriver {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        _stream: &mut S,
        _content_length: usize,
    ) -> Result<(), OtaDriverError> {
        warn!("OTA upload rejected: OTA updates are disabled on this hardware profile");
        Err(OtaDriverError::FlashWriteFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyStream;
    impl OtaStream for DummyStream {
        async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, ()> {
            Ok(0)
        }
        async fn write_all(&mut self, _buf: &[u8]) -> Result<(), ()> {
            Ok(())
        }
        async fn flush(&mut self) -> Result<(), ()> {
            Ok(())
        }
    }

    #[test]
    fn test_disabled_ota_driver_rejects_stream() {
        let mut driver = DisabledOtaDriver;
        let mut stream = DummyStream;

        let mut future = core::pin::pin!(driver.handle_flash_stream(&mut stream, 1024));
        let waker = futures::task::noop_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        assert_eq!(
            core::future::Future::poll(future.as_mut(), &mut cx),
            core::task::Poll::Ready(Err(OtaDriverError::FlashWriteFailed))
        );
    }
}
