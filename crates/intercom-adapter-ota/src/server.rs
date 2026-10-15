//! Asynchronous HTTP OTA Server
//!
//! Provides a single-connection HTTP/TCP server on port 8080 streaming firmware updates
//! directly into `OtaWriter`, updating system lifecycle states, and coordinating
//! bootloader commit and chip reset.

use crate::bootloader::{BootloaderError, OtaBootloader, OtaFlashStorage};
#[cfg(not(target_arch = "riscv32"))]
use crate::bootloader::MemOtaFlash;
use crate::writer::{MAX_APP_SLOT_SIZE, OTA_HEADER_LEN, OtaError, OtaTargetFlash, OtaWriter};
#[cfg(not(target_arch = "riscv32"))]
use crate::writer::MemTargetFlash;
#[cfg(target_arch = "riscv32")]
use crate::writer::EspTargetFlash;
use core::fmt;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use intercom_core::events::{SystemState, SystemStateSender};
use log::{info, warn};

// ============================================================================
// Main Public Struct & Implementation
// ============================================================================

/// Asynchronous HTTP OTA Server listening for local firmware flashing requests.
///
/// Binds a single-connection TCP socket on port 8080. When a valid signed payload
/// arrives at `/api/v1/ota/flash`, it streams incoming chunks directly to flash,
/// validates the Ed25519 signature, sets the target slot to pending verification,
/// responds with 200 OK, and reboots the device into the new image.
pub struct OtaServer<'a> {
    stack: Stack<'static>,
    state_sender: SystemStateSender<'a>,
    config: OtaServerConfig,
}

impl<'a> OtaServer<'a> {
    /// Construct a new OTA HTTP server bound to the given network stack.
    ///
    /// Accepts either an `OtaServerConfig` or a raw Ed25519 public key array `[u8; 32]`.
    pub fn new(
        stack: Stack<'static>,
        state_sender: SystemStateSender<'a>,
        config: impl Into<OtaServerConfig>,
    ) -> Self {
        Self {
            stack,
            state_sender,
            config: config.into(),
        }
    }

    /// Run the OTA server loop forever.
    ///
    /// Limits concurrency to 1 active socket. Incoming connections are handled sequentially.
    /// On error or failed verification, enforces a 5-second tarpit backoff before accepting
    /// the next connection.
    pub async fn run(&self) -> ! {
        let mut rx_buffer = [0u8; 4096];
        let mut tx_buffer = [0u8; 1024];

        loop {
            let mut socket = TcpSocket::new(self.stack, &mut rx_buffer, &mut tx_buffer);
            socket.set_timeout(Some(Duration::from_secs(60)));

            info!("OTA Server listening on TCP port {}", self.config.port);
            if let Err(e) = socket.accept(self.config.port).await {
                warn!("OTA socket accept error: {:?}", e);
                Timer::after_millis(100).await;
                continue;
            }

            info!("OTA Server: client connection accepted");

            #[cfg(target_arch = "riscv32")]
            let bootloader_res = OtaBootloader::load();
            #[cfg(not(target_arch = "riscv32"))]
            let bootloader_res = OtaBootloader::new(MemOtaFlash::blank());

            let mut bootloader = match bootloader_res {
                Ok(b) => b,
                Err(e) => {
                    warn!("OTA Server: failed to load bootloader: {:?}", e);
                    let _ = socket.write_all(HTTP_400_BAD_REQUEST).await;
                    let _ = socket.flush().await;
                    socket.close();
                    Timer::after(TARPIT_DELAY).await;
                    continue;
                }
            };

            #[cfg(target_arch = "riscv32")]
            let target_flash = EspTargetFlash;
            #[cfg(not(target_arch = "riscv32"))]
            let target_flash = MemTargetFlash::new();

            let outcome = handle_connection_with(
                &mut socket,
                &mut bootloader,
                target_flash,
                self.config.public_key,
                &self.state_sender,
                true,
            )
            .await;

            match outcome {
                Ok(OtaOutcome::Success) => {
                    info!("OTA firmware flash successful! Awaiting reset...");
                    socket.close();
                    let _ = socket.flush().await;
                    Timer::after(REBOOT_DELAY).await;
                    reboot_device();
                }
                Err(e) => {
                    warn!("OTA flash failed: {:?}. Entering 5s tarpit backoff...", e);
                    socket.close();
                    let _ = socket.flush().await;
                    Timer::after(TARPIT_DELAY).await;
                }
            }
        }
    }
}

// ============================================================================
// Supporting Public Types
// ============================================================================

/// Configuration options for the OTA HTTP Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtaServerConfig {
    /// TCP port to bind (default: 8080).
    pub port: u16,
    /// 32-byte Ed25519 public key for firmware verification.
    pub public_key: [u8; 32],
}

impl OtaServerConfig {
    /// Default TCP port for the OTA server.
    pub const DEFAULT_PORT: u16 = 8080;

    /// Create a new configuration with default port 8080 and the specified public key.
    pub const fn new(public_key: [u8; 32]) -> Self {
        Self {
            port: Self::DEFAULT_PORT,
            public_key,
        }
    }

    /// Override the TCP port.
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

/// Parsed HTTP request kind for OTA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    /// Flash update request (`POST /api/v1/ota/flash`) with expected Content-Length in bytes.
    Flash(usize),
}

/// Errors encountered while parsing incoming HTTP request headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpParseError {
    /// Request headers are incomplete (missing trailing `\r\n\r\n`).
    Incomplete,
    /// HTTP method is not allowed (only `POST` is accepted for flash).
    InvalidMethod,
    /// Request URI path is not recognized.
    InvalidPath,
    /// HTTP protocol version is invalid or unsupported.
    InvalidVersion,
    /// `Content-Length` header is missing.
    MissingContentLength,
    /// `Content-Length` header value is malformed or invalid.
    InvalidContentLength,
    /// `Content-Length` exceeds maximum permissible partition slot size.
    PayloadTooLarge,
    /// General header formatting error.
    InvalidHeader,
}

/// Outcome of processing an OTA client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaOutcome {
    /// Update verified and committed into bootloader; 200 OK sent; device ready to reboot.
    Success,
}

/// Errors occurring during an OTA connection and flashing session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaServerError {
    /// HTTP request line or header parsing failed.
    Http(HttpParseError),
    /// Streaming flash writer or signature verification error.
    Writer(OtaError),
    /// Bootloader state commit error.
    Bootloader(BootloaderError),
    /// Underlying TCP stream I/O failure.
    Stream,
    /// Client disconnected or payload was shorter than Content-Length.
    TruncatedBody,
}

/// Abstract streaming transport abstraction for OTA connections.
///
/// Implemented by `embassy_net::tcp::TcpSocket` on target, and by mock in-memory
/// transports for host testing.
#[allow(async_fn_in_trait)]
pub trait OtaStream {
    /// Read incoming bytes into `buf`, returning the number of bytes read (0 on EOF).
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;
    /// Write the entire slice `buf` to the stream.
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()>;
    /// Flush any buffered outgoing bytes to the network.
    async fn flush(&mut self) -> Result<(), ()>;
}

// ============================================================================
// Public Constants & Functions
// ============================================================================

/// Expected HTTP route for firmware flashing.
pub const OTA_FLASH_PATH: &str = "/api/v1/ota/flash";

/// Maximum allowed payload size for HTTP Content-Length (wire header + app slot).
pub const MAX_OTA_PAYLOAD_SIZE: usize = MAX_APP_SLOT_SIZE + OTA_HEADER_LEN;

/// HTTP 200 OK response sent upon successful verification and bootloader commit.
pub const HTTP_200_OK: &[u8] = b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}";

/// HTTP 400 Bad Request response sent upon failure or signature rejection.
pub const HTTP_400_BAD_REQUEST: &[u8] = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 24\r\n\r\n{\"error\":\"bad_request\"}";

/// Tarpit delay imposed on failed transfers before accepting the next connection (5 seconds).
pub const TARPIT_DELAY: Duration = Duration::from_secs(5);

/// Delay between 200 OK transmission and software reset (500 milliseconds).
pub const REBOOT_DELAY: Duration = Duration::from_millis(500);

/// Cooperative yield duration between sector chunks (1 millisecond).
pub const COOPERATIVE_YIELD_DELAY: Duration = Duration::from_millis(1);

/// Find the index immediately following the `\r\n\r\n` header terminator in `buffer`.
///
/// Returns `Some(offset)` where `offset` points to the first byte of the HTTP body,
/// or `None` if the header terminator has not been received yet.
pub fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    if buffer.len() < 4 {
        return None;
    }
    for i in 0..=buffer.len() - 4 {
        if &buffer[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

/// Parse HTTP request headers and extract the request kind and content length.
///
/// Validates:
/// - Presence of `\r\n\r\n` header delimiter
/// - HTTP `POST` method
/// - Request path matching `/api/v1/ota/flash`
/// - Valid HTTP version (`HTTP/1.0` or `HTTP/1.1`)
/// - Valid `Content-Length` header between 72 bytes and 3.4 MB + 72 bytes
pub fn parse_request_headers(buffer: &[u8]) -> Result<RequestKind, HttpParseError> {
    let header_end = find_headers_end(buffer).ok_or(HttpParseError::Incomplete)?;
    let header_bytes = &buffer[..header_end - 4]; // exclude trailing \r\n\r\n

    let header_str =
        core::str::from_utf8(header_bytes).map_err(|_| HttpParseError::InvalidHeader)?;
    let mut lines = header_str.split("\r\n");

    // 1. Parse Request Line: METHOD PATH VERSION
    let request_line = lines.next().ok_or(HttpParseError::Incomplete)?;
    let mut parts = request_line.split_whitespace();

    let method = parts.next().ok_or(HttpParseError::InvalidHeader)?;
    if method != "POST" {
        return Err(HttpParseError::InvalidMethod);
    }

    let path = parts.next().ok_or(HttpParseError::InvalidPath)?;
    if path != OTA_FLASH_PATH {
        return Err(HttpParseError::InvalidPath);
    }

    let version = parts.next().ok_or(HttpParseError::InvalidVersion)?;
    if !version.starts_with("HTTP/1.") {
        return Err(HttpParseError::InvalidVersion);
    }

    // 2. Parse Headers
    let mut content_length: Option<usize> = None;

    for line in lines {
        if line.is_empty() {
            continue;
        }

        if let Some((name, val)) = line.split_once(':') {
            let name = name.trim();
            let val = val.trim();

            if name.eq_ignore_ascii_case("content-length") {
                let len: usize = val.parse().map_err(|_| HttpParseError::InvalidContentLength)?;
                content_length = Some(len);
            }
        }
    }

    let len = content_length.ok_or(HttpParseError::MissingContentLength)?;

    if len < OTA_HEADER_LEN {
        return Err(HttpParseError::InvalidContentLength);
    }

    if len > MAX_OTA_PAYLOAD_SIZE {
        return Err(HttpParseError::PayloadTooLarge);
    }

    Ok(RequestKind::Flash(len))
}

/// Process an incoming OTA client connection on an abstract stream.
///
/// Coordinates reading HTTP headers, transitions `SystemState` to `Updating`,
/// streams incoming chunks into `OtaWriter`, verifies the Ed25519 signature, commits
/// the image to `OtaBootloader`, and sends `HTTP_200_OK` or `HTTP_400_BAD_REQUEST`.
pub async fn handle_connection_with<S, F, TF>(
    stream: &mut S,
    bootloader: &mut OtaBootloader<F>,
    target_flash: TF,
    public_key: [u8; 32],
    state_sender: &SystemStateSender<'_>,
    check_esp_magic: bool,
) -> Result<OtaOutcome, OtaServerError>
where
    S: OtaStream,
    F: OtaFlashStorage,
    TF: OtaTargetFlash,
{
    // 1. Read HTTP headers until \r\n\r\n delimiter is found
    let mut header_buf = [0u8; 1024];
    let mut header_len = 0;
    let body_offset;

    loop {
        let n = stream
            .read(&mut header_buf[header_len..])
            .await
            .map_err(|_| OtaServerError::Stream)?;

        if n == 0 {
            let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
            let _ = stream.flush().await;
            return Err(OtaServerError::Http(HttpParseError::Incomplete));
        }
        header_len += n;

        if let Some(offset) = find_headers_end(&header_buf[..header_len]) {
            body_offset = offset;
            break;
        }

        if header_len == header_buf.len() {
            let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
            let _ = stream.flush().await;
            return Err(OtaServerError::Http(HttpParseError::InvalidHeader));
        }
    }

    // 2. Parse request line & headers
    let req = match parse_request_headers(&header_buf[..body_offset]) {
        Ok(r) => r,
        Err(e) => {
            let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
            let _ = stream.flush().await;
            return Err(OtaServerError::Http(e));
        }
    };

    let expected_len = match req {
        RequestKind::Flash(len) => len,
    };

    // 3. Resolve target slot and sequence
    let target_slot = bootloader.get_target_slot();
    let next_seq = bootloader.get_next_seq();
    info!(
        "OTA Server: targeting slot={:?}, seq={}, expected_len={}",
        target_slot, next_seq, expected_len
    );

    // 4. Remember initial lifecycle state and signal Updating
    let initial_state = state_sender.current_state().unwrap_or(SystemState::Ready);
    let revert_state = match initial_state {
        SystemState::Updating | SystemState::FactoryReset => SystemState::Ready,
        state => state,
    };
    state_sender.updating();

    // 5. Initialize OtaWriter
    let mut writer =
        OtaWriter::new(target_flash, target_slot, public_key).with_esp_magic_check(check_esp_magic);

    // 6. Write any body bytes already read into header_buf
    let mut total_body_received = 0;
    if body_offset < header_len {
        let initial_body = &header_buf[body_offset..header_len];
        if let Err(e) = writer.write_chunk(initial_body) {
            let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
            let _ = stream.flush().await;
            state_sender.send(revert_state);
            return Err(OtaServerError::Writer(e));
        }
        total_body_received += initial_body.len();
    }

    // 7. Stream remaining body bytes with cooperative yielding
    let mut chunk_buf = [0u8; 2048];
    while total_body_received < expected_len {
        Timer::after(COOPERATIVE_YIELD_DELAY).await;

        let to_read = core::cmp::min(chunk_buf.len(), expected_len - total_body_received);
        let n = match stream.read(&mut chunk_buf[..to_read]).await {
            Ok(0) => {
                let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                let _ = stream.flush().await;
                state_sender.send(revert_state);
                return Err(OtaServerError::TruncatedBody);
            }
            Ok(n) => n,
            Err(_) => {
                let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
                let _ = stream.flush().await;
                state_sender.send(revert_state);
                return Err(OtaServerError::Stream);
            }
        };

        if let Err(e) = writer.write_chunk(&chunk_buf[..n]) {
            let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
            let _ = stream.flush().await;
            state_sender.send(revert_state);
            return Err(OtaServerError::Writer(e));
        }
        total_body_received += n;
    }

    // 8. Finalize flash write and Ed25519 signature verification
    if let Err(e) = writer.finish() {
        warn!("OTA signature or format validation failed: {:?}", e);
        let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
        let _ = stream.flush().await;
        state_sender.send(revert_state);
        return Err(OtaServerError::Writer(e));
    }

    // 9. Valid signature! Commit to bootloader and send HTTP 200 OK
    if let Err(e) = bootloader.mark_pending_verify(target_slot, next_seq) {
        warn!("OTA bootloader commit failed: {:?}", e);
        let _ = stream.write_all(HTTP_400_BAD_REQUEST).await;
        let _ = stream.flush().await;
        state_sender.send(revert_state);
        return Err(OtaServerError::Bootloader(e));
    }

    stream.write_all(HTTP_200_OK).await.map_err(|_| OtaServerError::Stream)?;
    stream.flush().await.map_err(|_| OtaServerError::Stream)?;

    Ok(OtaOutcome::Success)
}

// ============================================================================
// Private Helpers
// ============================================================================

fn reboot_device() -> ! {
    #[cfg(target_arch = "riscv32")]
    esp_hal::system::software_reset();

    #[cfg(not(target_arch = "riscv32"))]
    panic!("Host OTA reboot triggered");
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl OtaStream for TcpSocket<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.read(buf).await.map_err(|_| ())
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), ()> {
        while !buf.is_empty() {
            let n = self.write(buf).await.map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            buf = &buf[n..];
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        self.flush().await.map_err(|_| ())
    }
}

impl From<[u8; 32]> for OtaServerConfig {
    fn from(public_key: [u8; 32]) -> Self {
        Self::new(public_key)
    }
}

impl fmt::Display for HttpParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => write!(f, "incomplete HTTP headers"),
            Self::InvalidMethod => write!(f, "invalid or unsupported HTTP method"),
            Self::InvalidPath => write!(f, "invalid request path"),
            Self::InvalidVersion => write!(f, "invalid HTTP version"),
            Self::MissingContentLength => write!(f, "missing Content-Length header"),
            Self::InvalidContentLength => write!(f, "invalid Content-Length value"),
            Self::PayloadTooLarge => write!(f, "payload exceeds maximum slot capacity"),
            Self::InvalidHeader => write!(f, "malformed HTTP header"),
        }
    }
}

impl fmt::Display for OtaServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(e) => write!(f, "HTTP error: {}", e),
            Self::Writer(e) => write!(f, "flash writer error: {}", e),
            Self::Bootloader(e) => write!(f, "bootloader error: {:?}", e),
            Self::Stream => write!(f, "network stream I/O failure"),
            Self::TruncatedBody => write!(f, "truncated payload body from client"),
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootloader::{OtaState, Slot};
    use crate::writer::{ESP32_C6_CHIP_ID, ESP_IMAGE_MAGIC, OTA_HEADER_MAGIC};
    use alloc::vec;
    use alloc::vec::Vec;
    use ed25519_dalek::{Signer, SigningKey};
    use intercom_core::events::SystemStateWatch;

    struct MockStream {
        incoming: Vec<u8>,
        read_pos: usize,
        chunk_size: usize,
        outgoing: Vec<u8>,
    }

    impl MockStream {
        fn new(data: Vec<u8>) -> Self {
            Self {
                incoming: data,
                read_pos: 0,
                chunk_size: 4096,
                outgoing: Vec::new(),
            }
        }

        fn with_chunk_size(mut self, size: usize) -> Self {
            self.chunk_size = size;
            self
        }
    }

    impl OtaStream for MockStream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
            if self.read_pos >= self.incoming.len() {
                return Ok(0);
            }
            let available = self.incoming.len() - self.read_pos;
            let to_read = core::cmp::min(buf.len(), core::cmp::min(self.chunk_size, available));
            buf[..to_read].copy_from_slice(&self.incoming[self.read_pos..self.read_pos + to_read]);
            self.read_pos += to_read;
            Ok(to_read)
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), ()> {
            self.outgoing.extend_from_slice(buf);
            Ok(())
        }

        async fn flush(&mut self) -> Result<(), ()> {
            Ok(())
        }
    }

    fn generate_test_keys() -> ([u8; 32], SigningKey) {
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(13);
        }
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        (verifying_key.to_bytes(), signing_key)
    }

    fn build_test_esp_binary(len: usize) -> Vec<u8> {
        let mut bin = vec![0xbb; len];
        bin[0] = ESP_IMAGE_MAGIC;
        if len >= 14 {
            bin[12] = (ESP32_C6_CHIP_ID & 0xff) as u8;
            bin[13] = ((ESP32_C6_CHIP_ID >> 8) & 0xff) as u8;
        }
        bin
    }

    fn build_signed_wire_payload(image: &[u8], signing_key: &SigningKey) -> Vec<u8> {
        let sig = signing_key.sign(image);
        let mut packet = Vec::with_capacity(OTA_HEADER_LEN + image.len());
        packet.extend_from_slice(OTA_HEADER_MAGIC);
        packet.extend_from_slice(&(image.len() as u32).to_le_bytes());
        packet.extend_from_slice(&sig.to_bytes());
        packet.extend_from_slice(image);
        packet
    }

    fn build_http_request(payload: &[u8]) -> Vec<u8> {
        let header = alloc::format!(
            "POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
            payload.len()
        );
        let mut req = Vec::from(header.as_bytes());
        req.extend_from_slice(payload);
        req
    }

    #[test]
    fn parses_valid_ota_post_request() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 1048576\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Ok(RequestKind::Flash(1048576)));
    }

    #[test]
    fn parses_request_with_multiple_headers_and_case_insensitivity() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nHost: 192.168.1.100:8080\r\ncontent-length: 204800\r\nContent-Type: application/octet-stream\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Ok(RequestKind::Flash(204800)));
    }

    #[test]
    fn parses_request_with_body_bytes_following_headers() {
        // Arrange
        let mut request = Vec::from(&b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 100\r\n\r\n"[..]);
        request.extend_from_slice(b"ICOT\x00\x00\x00\x1c");

        // Act
        let parsed = parse_request_headers(&request);

        // Assert
        assert_eq!(parsed, Ok(RequestKind::Flash(100)));
    }

    #[test]
    fn rejects_incomplete_request_headers() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 1048576\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::Incomplete));
    }

    #[test]
    fn rejects_non_post_methods() {
        // Arrange
        let request = b"GET /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 1048576\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::InvalidMethod));
    }

    #[test]
    fn rejects_invalid_request_paths() {
        // Arrange
        let request = b"POST /api/v1/other HTTP/1.1\r\nContent-Length: 1048576\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::InvalidPath));
    }

    #[test]
    fn rejects_invalid_http_version() {
        // Arrange
        let request = b"POST /api/v1/ota/flash FTP/1.0\r\nContent-Length: 1048576\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::InvalidVersion));
    }

    #[test]
    fn rejects_missing_content_length() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nHost: test\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::MissingContentLength));
    }

    #[test]
    fn rejects_malformed_content_length() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: not-a-number\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::InvalidContentLength));
    }

    #[test]
    fn rejects_content_length_smaller_than_header() {
        // Arrange
        let request = b"POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: 71\r\n\r\n";

        // Act
        let parsed = parse_request_headers(request);

        // Assert
        assert_eq!(parsed, Err(HttpParseError::InvalidContentLength));
    }

    #[test]
    fn rejects_content_length_larger_than_slot_capacity() {
        // Arrange
        let too_large = MAX_OTA_PAYLOAD_SIZE + 1;
        let request = alloc::format!("POST /api/v1/ota/flash HTTP/1.1\r\nContent-Length: {}\r\n\r\n", too_large);

        // Act
        let parsed = parse_request_headers(request.as_bytes());

        // Assert
        assert_eq!(parsed, Err(HttpParseError::PayloadTooLarge));
    }

    #[test]
    fn successful_ota_flash_workflow() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(8192);
        let wire_payload = build_signed_wire_payload(&image, &privkey);
        let request_bytes = build_http_request(&wire_payload);

        let mut stream = MockStream::new(request_bytes);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let mut target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            &mut target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert_eq!(outcome, Ok(OtaOutcome::Success));
        assert!(stream.outgoing.starts_with(b"HTTP/1.1 200 OK"));
        assert!(stream.outgoing.windows(15).any(|w| w == b"{\"status\":\"ok\"}"));

        // Verify target slot was updated to Ota1, marked OtaState::New with sequence 2 (ESP-IDF (seq-1)%2 == 1 for Ota1)
        let active = bootloader.get_active();
        assert_eq!(active.slot, Slot::Ota1);
        assert_eq!(active.seq, 2);
        assert_eq!(active.state, OtaState::New);

        // Verify flashed data in target slot matches binary
        let flashed = target_flash.read_bytes(Slot::Ota1.flash_offset(), 8192);
        assert_eq!(flashed, image);
    }

    #[test]
    fn rejects_tampered_signature_in_ota_stream() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(4096);
        let mut wire_payload = build_signed_wire_payload(&image, &privkey);
        // Tamper with signature at offset 8 (magic: 0..4, len: 4..8, sig: 8..72)
        wire_payload[8] ^= 0xff;
        let request_bytes = build_http_request(&wire_payload);

        let mut stream = MockStream::new(request_bytes);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert_eq!(outcome, Err(OtaServerError::Writer(OtaError::InvalidSignature)));
        assert!(stream.outgoing.starts_with(b"HTTP/1.1 400 Bad Request"));

        // Bootloader state must remain untouched
        let active = bootloader.get_active();
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.seq, 0);
    }

    #[test]
    fn rejects_truncated_body_stream() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(4096);
        let wire_payload = build_signed_wire_payload(&image, &privkey);
        let mut request_bytes = build_http_request(&wire_payload);
        // Truncate stream by 1000 bytes
        request_bytes.truncate(request_bytes.len() - 1000);

        let mut stream = MockStream::new(request_bytes);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert_eq!(outcome, Err(OtaServerError::TruncatedBody));
        assert!(stream.outgoing.starts_with(b"HTTP/1.1 400 Bad Request"));

        // Bootloader state must remain untouched
        let active = bootloader.get_active();
        assert_eq!(active.seq, 0);
    }

    #[test]
    fn rejects_bad_wire_magic_in_stream() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(4096);
        let mut wire_payload = build_signed_wire_payload(&image, &privkey);
        wire_payload[0..4].copy_from_slice(b"BAD!");
        let request_bytes = build_http_request(&wire_payload);

        let mut stream = MockStream::new(request_bytes);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert_eq!(outcome, Err(OtaServerError::Writer(OtaError::InvalidHeader)));
        assert!(stream.outgoing.starts_with(b"HTTP/1.1 400 Bad Request"));
    }

    #[test]
    fn handles_incremental_chunked_streaming() {
        // Arrange: 64-byte chunks simulates slow/fragmented TCP network packet arrival
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(5000);
        let wire_payload = build_signed_wire_payload(&image, &privkey);
        let request_bytes = build_http_request(&wire_payload);

        let mut stream = MockStream::new(request_bytes).with_chunk_size(64);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let mut target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            &mut target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert_eq!(outcome, Ok(OtaOutcome::Success));
        assert!(stream.outgoing.starts_with(b"HTTP/1.1 200 OK"));
        let flashed = target_flash.read_bytes(Slot::Ota1.flash_offset(), 5000);
        assert_eq!(flashed, image);
    }

    #[test]
    fn server_config_builder_and_conversion() {
        // Arrange
        let key = [0x42u8; 32];

        // Act 1: Default port
        let config1 = OtaServerConfig::new(key);

        // Assert 1
        assert_eq!(config1.port, 8080);
        assert_eq!(config1.public_key, key);

        // Act 2: Custom port
        let config2 = config1.with_port(9000);

        // Assert 2
        assert_eq!(config2.port, 9000);

        // Act 3: From conversion
        let config3: OtaServerConfig = key.into();

        // Assert 3
        assert_eq!(config3.port, 8080);
        assert_eq!(config3.public_key, key);
    }

    #[test]
    fn ota_failure_reverts_to_prior_system_state() {
        // Arrange
        let (pubkey, privkey) = generate_test_keys();
        let image = build_test_esp_binary(4096);
        let mut wire_payload = build_signed_wire_payload(&image, &privkey);
        // Tamper with signature to force verification failure
        wire_payload[8] ^= 0xff;
        let request_bytes = build_http_request(&wire_payload);

        let mut stream = MockStream::new(request_bytes);
        let mut bootloader = OtaBootloader::new(MemOtaFlash::blank()).unwrap();
        let target_flash = MemTargetFlash::new();
        let watch = SystemStateWatch::new();
        let state_sender = SystemStateSender::new(&watch);
        // Device was in Connected state before OTA started
        state_sender.connected();

        // Act
        let outcome = futures::executor::block_on(handle_connection_with(
            &mut stream,
            &mut bootloader,
            target_flash,
            pubkey,
            &state_sender,
            true,
        ));

        // Assert
        assert!(outcome.is_err());
        assert_eq!(state_sender.current_state(), Some(SystemState::Connected));
    }
}
