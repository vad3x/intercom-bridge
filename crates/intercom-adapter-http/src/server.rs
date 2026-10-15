//! Asynchronous HTTP Web and REST API Server
//!
//! Provides a single-connection HTTP server running on Embassy networking,
//! serving the pre-compressed web dashboard and REST API endpoints for telemetry,
//! domain actions, and firmware OTA flashing.

use core::fmt::Write;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle};
use intercom_core::ports::ota::{OtaFlashDriver, OtaStream};
use intercom_core::service::DiagnosticsService;
use log::{info, warn};

use crate::router::{
    format_status_json, parse_route, Route, HTTP_200_JSON_OK, HTTP_400_BAD_REQUEST,
    HTTP_404_NOT_FOUND,
};
use crate::DASHBOARD_HTML_GZ;

// ============================================================================
// Public Configuration & Types
// ============================================================================

/// Configuration for `HttpServer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpServerConfig {
    /// TCP port to listen on (default: 8080).
    pub port: u16,
}

impl HttpServerConfig {
    /// Create a new configuration with the specified TCP port.
    pub const fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self { port: 8080 }
    }
}

impl From<u16> for HttpServerConfig {
    fn from(port: u16) -> Self {
        Self { port }
    }
}

// ============================================================================
// Main Public Server Struct & Implementation
// ============================================================================

/// Asynchronous HTTP Server with injected core services.
///
/// Binds a single-connection TCP socket on the configured port (default 8080).
/// Dispatches incoming requests to the pre-compressed dashboard HTML,
/// system diagnostics snapshots, live logs, access controller actions,
/// and streaming OTA firmware updates.
pub struct HttpServer<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver> {
    stack: Option<Stack<'static>>,
    access_handle: AccessControllerHandle<'a>,
    diagnostics: &'a DiagnosticsService<'a, H, E>,
    ota_driver: &'a mut O,
    log_provider: fn(&mut [u8]) -> usize,
    config: HttpServerConfig,
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver>
    HttpServer<'a, H, E, O>
{
    /// Create a new `HttpServer` bound to an `embassy_net::Stack`.
    pub fn new(
        stack: Stack<'static>,
        access_handle: AccessControllerHandle<'a>,
        diagnostics: &'a DiagnosticsService<'a, H, E>,
        ota_driver: &'a mut O,
        log_provider: fn(&mut [u8]) -> usize,
        config: HttpServerConfig,
    ) -> Self {
        Self {
            stack: Some(stack),
            access_handle,
            diagnostics,
            ota_driver,
            log_provider,
            config,
        }
    }

    /// Create an unbound `HttpServer` without a network stack, suitable for testing.
    pub fn new_unbound(
        access_handle: AccessControllerHandle<'a>,
        diagnostics: &'a DiagnosticsService<'a, H, E>,
        ota_driver: &'a mut O,
        log_provider: fn(&mut [u8]) -> usize,
        config: HttpServerConfig,
    ) -> Self {
        Self {
            stack: None,
            access_handle,
            diagnostics,
            ota_driver,
            log_provider,
            config,
        }
    }

    /// Process a single incoming client connection on an abstract `OtaStream`.
    pub async fn handle_request<S: OtaStream>(&mut self, stream: &mut S) -> Result<(), ()> {
        let mut read_buf = [0u8; 1024];
        let mut read_len = 0;

        let body_offset = loop {
            let n = stream
                .read(&mut read_buf[read_len..])
                .await
                .map_err(|_| ())?;
            if n == 0 {
                let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                let _ = stream.flush().await;
                return Err(());
            }
            read_len += n;

            if let Some(offset) = find_headers_end(&read_buf[..read_len]) {
                break offset;
            }

            if read_len == read_buf.len() {
                let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                let _ = stream.flush().await;
                return Err(());
            }
        };

        let header_bytes = &read_buf[..body_offset];
        let header_str = core::str::from_utf8(header_bytes).map_err(|_| ())?;
        let request_line = header_str.lines().next().ok_or(())?;
        let route = parse_route(request_line);

        match route {
            Route::Dashboard => {
                let mut header_buf = [0u8; 256];
                let mut writer = BufferWriter::new(&mut header_buf);
                write!(
                    writer,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    DASHBOARD_HTML_GZ.len()
                )
                .map_err(|_| ())?;
                let written = writer.written();

                stream.write_all(&header_buf[..written]).await?;
                stream.write_all(DASHBOARD_HTML_GZ).await?;
                stream.flush().await?;
                Ok(())
            }
            Route::Status => {
                let diag = self.diagnostics.snapshot();
                let mut json_buf = [0u8; 1024];
                let json_len = format_status_json(&diag, &mut json_buf).map_err(|_| ())?;

                let mut header_buf = [0u8; 256];
                let mut writer = BufferWriter::new(&mut header_buf);
                write!(
                    writer,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    json_len
                )
                .map_err(|_| ())?;
                let written = writer.written();

                stream.write_all(&header_buf[..written]).await?;
                stream.write_all(&json_buf[..json_len]).await?;
                stream.flush().await?;
                Ok(())
            }
            Route::Logs => {
                let mut log_buf = [0u8; 2048];
                let log_len = (self.log_provider)(&mut log_buf);

                let mut header_buf = [0u8; 256];
                let mut writer = BufferWriter::new(&mut header_buf);
                write!(
                    writer,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    log_len
                )
                .map_err(|_| ())?;
                let written = writer.written();

                stream.write_all(&header_buf[..written]).await?;
                stream.write_all(&log_buf[..log_len]).await?;
                stream.flush().await?;
                Ok(())
            }
            Route::ActionRing => {
                self.access_handle.send(AccessCommand::Ring);
                stream.write_all(HTTP_200_JSON_OK).await?;
                stream.flush().await?;
                Ok(())
            }
            Route::ActionUnlock => {
                self.access_handle.send(AccessCommand::Unlock);
                stream.write_all(HTTP_200_JSON_OK).await?;
                stream.flush().await?;
                Ok(())
            }
            Route::OtaFlash => {
                let content_len = match parse_content_length(header_str) {
                    Some(len) => len,
                    None => {
                        let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                        let _ = stream.flush().await;
                        return Err(());
                    }
                };

                let prefix = &read_buf[body_offset..read_len];
                let mut prefixed_stream = PrefixedStream::new(prefix, stream);

                match self
                    .ota_driver
                    .handle_flash_stream(&mut prefixed_stream, content_len)
                    .await
                {
                    Ok(()) => {
                        stream.write_all(HTTP_200_JSON_OK).await?;
                        stream.flush().await?;
                        Ok(())
                    }
                    Err(_) => {
                        let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                        let _ = stream.flush().await;
                        Err(())
                    }
                }
            }
            Route::NotFound => {
                stream.write_all(HTTP_404_NOT_FOUND).await?;
                stream.flush().await?;
                Ok(())
            }
        }
    }

    /// Run the sequential HTTP server accept loop forever.
    pub async fn run(&mut self) -> ! {
        let stack = self
            .stack
            .expect("HttpServer requires a network stack to run");
        let mut rx_buffer = [0u8; 4096];
        let mut tx_buffer = [0u8; 1024];

        loop {
            let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
            socket.set_timeout(Some(Duration::from_secs(60)));

            info!("HTTP Server listening on TCP port {}", self.config.port);
            if let Err(e) = socket.accept(self.config.port).await {
                warn!("HTTP socket accept error: {:?}", e);
                Timer::after_millis(100).await;
                continue;
            }

            info!("HTTP Server: client connection accepted");
            let mut stream = SocketStream(&mut socket);
            let _ = self.handle_request(&mut stream).await;
            socket.close();
        }
    }
}

// ============================================================================
// Private Helpers & Stream Adapters
// ============================================================================

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    if buffer.len() < 2 {
        return None;
    }
    for i in 0..buffer.len() {
        if i + 4 <= buffer.len() && &buffer[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
        if i + 2 <= buffer.len() && &buffer[i..i + 2] == b"\n\n" {
            return Some(i + 2);
        }
    }
    None
}

fn parse_content_length(header_str: &str) -> Option<usize> {
    for line in header_str.lines() {
        if let Some((name, val)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                if let Ok(len) = val.trim().parse::<usize>() {
                    return Some(len);
                }
            }
        }
    }
    None
}

struct BufferWriter<'b> {
    buf: &'b mut [u8],
    pos: usize,
}

impl<'b> BufferWriter<'b> {
    fn new(buf: &'b mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn written(&self) -> usize {
        self.pos
    }
}

impl<'b> Write for BufferWriter<'b> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len().saturating_sub(self.pos);
        if bytes.len() > remaining {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

struct SocketStream<'s, 'b>(&'s mut TcpSocket<'b>);

impl<'s, 'b> OtaStream for SocketStream<'s, 'b> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.0.read(buf).await.map_err(|_| ())
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), ()> {
        while !buf.is_empty() {
            let n = self.0.write(buf).await.map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            buf = &buf[n..];
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        self.0.flush().await.map_err(|_| ())
    }
}

struct PrefixedStream<'s, S: OtaStream> {
    prefix: &'s [u8],
    prefix_pos: usize,
    inner: &'s mut S,
}

impl<'s, S: OtaStream> PrefixedStream<'s, S> {
    fn new(prefix: &'s [u8], inner: &'s mut S) -> Self {
        Self {
            prefix,
            prefix_pos: 0,
            inner,
        }
    }
}

impl<'s, S: OtaStream> OtaStream for PrefixedStream<'s, S> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        if self.prefix_pos < self.prefix.len() {
            let available = self.prefix.len() - self.prefix_pos;
            let to_copy = core::cmp::min(available, buf.len());
            buf[..to_copy]
                .copy_from_slice(&self.prefix[self.prefix_pos..self.prefix_pos + to_copy]);
            self.prefix_pos += to_copy;
            Ok(to_copy)
        } else {
            self.inner.read(buf).await
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.inner.write_all(buf).await
    }

    async fn flush(&mut self) -> Result<(), ()> {
        self.inner.flush().await
    }
}
