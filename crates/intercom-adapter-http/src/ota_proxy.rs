//! Bounded transport for a single OTA driver shared by the two HTTP workers.

use core::cell::Cell;
use edf_core::ports::ota::{OtaDriverError, OtaFlashDriver, OtaStream};
use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel, signal::Signal};

// A single queued chunk provides backpressure without duplicating the driver's
// 4 KiB sector writer and 2 KiB input buffer in both HTTP connection futures.
#[cfg(feature = "large-ota-buffers")]
const CHUNK_SIZE: usize = 2048;

#[cfg(not(feature = "large-ota-buffers"))]
const CHUNK_SIZE: usize = 256;

struct Start {
    id: u64,
    worker: usize,
    len: usize,
}
struct Chunk {
    id: u64,
    bytes: [u8; CHUNK_SIZE],
    len: Result<usize, ()>,
}

pub(crate) struct OtaProxy {
    busy: Cell<bool>,
    next_id: Cell<u64>,
    cancelled_through: Cell<u64>,
    cancelled: Signal<NoopRawMutex, ()>,
    starts: Channel<NoopRawMutex, Start, 1>,
    chunks: Channel<NoopRawMutex, Chunk, 1>,
    outcomes: [Signal<NoopRawMutex, Result<(), OtaDriverError>>; 2],
}

impl OtaProxy {
    pub(crate) const fn new() -> Self {
        Self {
            busy: Cell::new(false),
            next_id: Cell::new(1),
            cancelled_through: Cell::new(0),
            cancelled: Signal::new(),
            starts: Channel::new(),
            chunks: Channel::new(),
            outcomes: [Signal::new(), Signal::new()],
        }
    }

    pub(crate) fn busy(&self) -> &Cell<bool> {
        &self.busy
    }

    pub(crate) fn client(&self, worker: usize) -> OtaClient<'_> {
        OtaClient {
            proxy: self,
            worker,
        }
    }

    async fn wait_cancelled(&self, id: u64) {
        while self.cancelled_through.get() < id {
            self.cancelled.wait().await;
        }
    }

    pub(crate) async fn run<O: OtaFlashDriver>(&self, driver: &mut O) -> ! {
        loop {
            let start = self.starts.receive().await;
            let mut stream = ProxyStream {
                proxy: self,
                id: start.id,
                chunk: None,
                offset: 0,
            };
            // Cancellation drops the driver's upload future, matching the original
            // direct streaming path. IDs keep stale chunks and wakeups isolated.
            if let Either::Second(outcome) = select(
                self.wait_cancelled(start.id),
                driver.handle_flash_stream(&mut stream, start.len),
            )
            .await
            {
                self.outcomes[start.worker].signal(outcome);
            }
        }
    }
}

struct TransferGuard<'a> {
    proxy: &'a OtaProxy,
    id: u64,
}
impl Drop for TransferGuard<'_> {
    fn drop(&mut self) {
        self.proxy.cancelled_through.set(self.id);
        self.proxy.cancelled.signal(());
        self.proxy.busy.set(false);
    }
}

pub(crate) struct OtaClient<'a> {
    proxy: &'a OtaProxy,
    worker: usize,
}
impl OtaFlashDriver for OtaClient<'_> {
    async fn handle_flash_stream<S: OtaStream>(
        &mut self,
        stream: &mut S,
        len: usize,
    ) -> Result<(), OtaDriverError> {
        if self.proxy.busy.replace(true) {
            return Err(OtaDriverError::InvalidHeader);
        }
        let id = self.proxy.next_id.get();
        self.proxy.next_id.set(id + 1);
        let _guard = TransferGuard {
            proxy: self.proxy,
            id,
        };
        self.proxy.outcomes[self.worker].reset();
        self.proxy
            .starts
            .send(Start {
                id,
                worker: self.worker,
                len,
            })
            .await;
        let pump = async {
            let mut remaining = len;
            while remaining > 0 {
                let mut chunk = Chunk {
                    id,
                    bytes: [0; CHUNK_SIZE],
                    len: Ok(0),
                };
                let capacity = remaining.min(CHUNK_SIZE);
                chunk.len = stream.read(&mut chunk.bytes[..capacity]).await;
                let n = chunk.len.unwrap_or(0);
                self.proxy.chunks.send(chunk).await;
                if n == 0 {
                    break;
                }
                remaining -= n;
            }
            core::future::pending::<()>().await
        };
        match select(self.proxy.outcomes[self.worker].wait(), pump).await {
            Either::First(outcome) => outcome,
            Either::Second(()) => unreachable!("OTA producer waits for the driver outcome"),
        }
    }
}

struct ProxyStream<'a> {
    proxy: &'a OtaProxy,
    id: u64,
    chunk: Option<Chunk>,
    offset: usize,
}
impl OtaStream for ProxyStream<'_> {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize, ()> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if let Some(chunk) = &self.chunk {
                let len = chunk.len?;
                if len == 0 {
                    return Ok(0);
                }
                let n = out.len().min(len - self.offset);
                out[..n].copy_from_slice(&chunk.bytes[self.offset..self.offset + n]);
                self.offset += n;
                if self.offset == len {
                    self.chunk = None;
                    self.offset = 0;
                }
                return Ok(n);
            }
            let chunk = self.proxy.chunks.receive().await;
            if chunk.id == self.id {
                self.chunk = Some(chunk);
            }
        }
    }
    async fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
        Ok(())
    }
    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use embassy_futures::select::{select, Either};
    use futures::{executor::block_on, Future};
    use std::{boxed::Box, vec::Vec};

    struct Source {
        bytes: Vec<u8>,
        offset: usize,
        fail: bool,
    }
    impl OtaStream for Source {
        async fn read(&mut self, out: &mut [u8]) -> Result<usize, ()> {
            if self.fail {
                return Err(());
            }
            let n = out.len().min(self.bytes.len() - self.offset);
            out[..n].copy_from_slice(&self.bytes[self.offset..self.offset + n]);
            self.offset += n;
            Ok(n)
        }
        async fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
            Ok(())
        }
        async fn flush(&mut self) -> Result<(), ()> {
            Ok(())
        }
    }
    struct Driver {
        received: Vec<u8>,
        fail: bool,
    }
    impl OtaFlashDriver for Driver {
        async fn handle_flash_stream<S: OtaStream>(
            &mut self,
            stream: &mut S,
            len: usize,
        ) -> Result<(), OtaDriverError> {
            let mut buf = [0; 137];
            while self.received.len() < len {
                let n = stream
                    .read(&mut buf)
                    .await
                    .map_err(|_| OtaDriverError::TruncatedStream)?;
                if n == 0 {
                    return Err(OtaDriverError::TruncatedStream);
                }
                self.received.extend_from_slice(&buf[..n]);
            }
            if self.fail {
                Err(OtaDriverError::VerificationFailed)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn streams_bounded_chunks_on_either_client_without_losing_partial_reads() {
        // Arrange
        for worker in [0, 1] {
            let proxy = OtaProxy::new();
            let mut client = proxy.client(worker);
            let payload: Vec<u8> = (0..2345).map(|i| i as u8).collect();
            let mut source = Source {
                bytes: payload.clone(),
                offset: 0,
                fail: false,
            };
            let mut driver = Driver {
                received: Vec::new(),
                fail: false,
            };
            // Act
            let result = block_on(select(
                proxy.run(&mut driver),
                client.handle_flash_stream(&mut source, payload.len()),
            ));
            // Assert
            assert!(matches!(result, Either::Second(Ok(()))));
            assert_eq!(driver.received, payload);
        }
    }

    #[test]
    fn propagates_source_truncation_and_driver_verification_errors() {
        // Arrange
        for (fail_source, len, fail_driver, expected) in [
            (true, 4, false, OtaDriverError::TruncatedStream),
            (false, 5, false, OtaDriverError::TruncatedStream),
            (false, 4, true, OtaDriverError::VerificationFailed),
        ] {
            let proxy = OtaProxy::new();
            let mut client = proxy.client(0);
            let mut source = Source {
                bytes: Vec::from(*b"DATA"),
                offset: 0,
                fail: fail_source,
            };
            let mut driver = Driver {
                received: Vec::new(),
                fail: fail_driver,
            };
            // Act
            let result = block_on(select(
                proxy.run(&mut driver),
                client.handle_flash_stream(&mut source, len),
            ));
            // Assert
            assert!(matches!(result, Either::Second(Err(error)) if error == expected));
        }
    }

    struct PendingDriver<'a> {
        entered: &'a Cell<bool>,
        dropped: &'a Cell<bool>,
    }
    impl OtaFlashDriver for PendingDriver<'_> {
        async fn handle_flash_stream<S: OtaStream>(
            &mut self,
            _: &mut S,
            _: usize,
        ) -> Result<(), OtaDriverError> {
            struct DropFlag<'a>(&'a Cell<bool>);
            impl Drop for DropFlag<'_> {
                fn drop(&mut self) {
                    self.0.set(true);
                }
            }
            let _guard = DropFlag(self.dropped);
            self.entered.set(true);
            core::future::pending().await
        }
    }

    #[test]
    fn cancellation_drops_the_driver_future_and_releases_busy_client() {
        // Arrange
        let proxy = OtaProxy::new();
        let mut client = proxy.client(0);
        let mut other = proxy.client(1);
        let mut source = Source {
            bytes: Vec::from(*b"DATA"),
            offset: 0,
            fail: false,
        };
        let mut second_source = Source {
            bytes: Vec::new(),
            offset: 0,
            fail: false,
        };
        let entered = Cell::new(false);
        let dropped = Cell::new(false);
        let mut driver = PendingDriver {
            entered: &entered,
            dropped: &dropped,
        };
        let mut backend = Box::pin(proxy.run(&mut driver));
        let mut request = Box::pin(client.handle_flash_stream(&mut source, 4));
        let waker = futures::task::noop_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        // Act
        assert!(request.as_mut().poll(&mut cx).is_pending());
        assert!(backend.as_mut().poll(&mut cx).is_pending());
        let result = block_on(other.handle_flash_stream(&mut second_source, 0));
        drop(request);
        assert!(backend.as_mut().poll(&mut cx).is_pending());
        // Assert
        assert!(entered.get());
        assert!(dropped.get());
        assert_eq!(result, Err(OtaDriverError::InvalidHeader));
    }
    #[test]
    fn cancelled_upload_cannot_leak_queued_bytes_into_a_new_upload() {
        struct RetryDriver<'a> {
            first: bool,
            entered: &'a Cell<bool>,
            received: Vec<u8>,
        }
        impl OtaFlashDriver for RetryDriver<'_> {
            async fn handle_flash_stream<S: OtaStream>(
                &mut self,
                stream: &mut S,
                len: usize,
            ) -> Result<(), OtaDriverError> {
                if self.first {
                    self.first = false;
                    self.entered.set(true);
                    core::future::pending::<()>().await;
                }
                let mut buf = [0; 17];
                while self.received.len() < len {
                    let n = stream
                        .read(&mut buf)
                        .await
                        .map_err(|_| OtaDriverError::TruncatedStream)?;
                    if n == 0 {
                        return Err(OtaDriverError::TruncatedStream);
                    }
                    self.received.extend_from_slice(&buf[..n]);
                }
                Ok(())
            }
        }
        // Arrange
        for next_worker in [0, 1] {
            let proxy = OtaProxy::new();
            let mut first = proxy.client(0);
            let mut next = proxy.client(next_worker);
            let mut old_source = Source {
                bytes: std::vec![0x11; 1000],
                offset: 0,
                fail: false,
            };
            let mut new_source = Source {
                bytes: std::vec![0x22; 700],
                offset: 0,
                fail: false,
            };
            let entered = Cell::new(false);
            let mut driver = RetryDriver {
                first: true,
                entered: &entered,
                received: Vec::new(),
            };
            let mut backend = Box::pin(proxy.run(&mut driver));
            let mut upload = Box::pin(first.handle_flash_stream(&mut old_source, 1000));
            let waker = futures::task::noop_waker();
            let mut cx = core::task::Context::from_waker(&waker);
            // Act
            assert!(upload.as_mut().poll(&mut cx).is_pending());
            assert!(backend.as_mut().poll(&mut cx).is_pending());
            assert!(entered.get());
            drop(upload);
            // Queue the retry before the backend observes the cancellation.
            let result = block_on(select(
                backend,
                next.handle_flash_stream(&mut new_source, 700),
            ));
            // Assert
            assert!(matches!(result, Either::Second(Ok(()))));
            assert_eq!(driver.received, std::vec![0x22; 700]);
        }
    }
}
