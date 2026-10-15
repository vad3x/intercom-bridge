//! Asynchronous HTTP Web and REST API Server
//!
//! Provides an Embassy-compatible HTTP server running on `picoserve`,
//! serving the pre-compressed web dashboard and REST API endpoints for telemetry,
//! domain actions, and firmware OTA flashing.

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle};
use intercom_core::ports::ota::{OtaFlashDriver, OtaStream};
use intercom_core::service::DiagnosticsService;
use log::{info, warn};
use picoserve::response::IntoResponse;

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

/// Shared application state passed to HTTP request handlers.
pub struct AppState<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver> {
    pub access_handle: AccessControllerHandle<'a>,
    pub diagnostics: &'a DiagnosticsService<'a, H, E>,
    pub ota_driver: &'a core::cell::RefCell<&'a mut O>,
    pub had_error: &'a core::cell::Cell<bool>,
    pub log_provider: fn(&mut [u8]) -> usize,
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver> Clone
    for AppState<'a, H, E, O>
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver> Copy
    for AppState<'a, H, E, O>
{
}

// ============================================================================
// Main Public Server Struct & Implementation
// ============================================================================

/// Asynchronous HTTP Server with injected core services.
///
/// Binds a single-connection TCP socket on the configured port (default 8080).
/// Dispatches incoming requests via `picoserve` to the pre-compressed dashboard HTML,
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
        let mut http_buffer = [0u8; 2048];
        let picoserve_config = picoserve::Config::new(picoserve::Timeouts {
            start_read_request: embassy_time::Duration::from_secs(5),
            persistent_start_read_request: embassy_time::Duration::from_secs(1),
            read_request: embassy_time::Duration::from_secs(5),
            write: embassy_time::Duration::from_secs(5),
        })
        .close_connection_after_response();

        let ota_driver_cell = core::cell::RefCell::new(&mut *self.ota_driver);
        let had_error = core::cell::Cell::new(false);
        let app_state = AppState {
            access_handle: self.access_handle,
            diagnostics: self.diagnostics,
            ota_driver: &ota_driver_cell,
            had_error: &had_error,
            log_provider: self.log_provider,
        };

        let router = build_router(app_state);
        let socket = SocketStream::new(stream);
        let serve_res = picoserve::Server::new(&router, &picoserve_config, &mut http_buffer)
            .serve(socket)
            .await;

        if serve_res.is_err() || had_error.get() {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Run the sequential HTTP server accept loop forever.
    pub async fn run(&mut self) -> ! {
        let stack = self
            .stack
            .expect("HttpServer requires a network stack to run");
        let mut rx_buffer = [0u8; 4096];
        let mut tx_buffer = [0u8; 1024];
        let mut http_buffer = [0u8; 2048];

        let picoserve_config = picoserve::Config::new(picoserve::Timeouts {
            start_read_request: embassy_time::Duration::from_secs(5),
            persistent_start_read_request: embassy_time::Duration::from_secs(1),
            read_request: embassy_time::Duration::from_secs(5),
            write: embassy_time::Duration::from_secs(5),
        })
        .close_connection_after_response();

        let ota_driver_cell = core::cell::RefCell::new(&mut *self.ota_driver);
        let had_error = core::cell::Cell::new(false);
        let app_state = AppState {
            access_handle: self.access_handle,
            diagnostics: self.diagnostics,
            ota_driver: &ota_driver_cell,
            had_error: &had_error,
            log_provider: self.log_provider,
        };

        let router = build_router(app_state);

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
            let _ = picoserve::Server::new(&router, &picoserve_config, &mut http_buffer)
                .serve(socket)
                .await;
        }
    }
}

// ============================================================================
// Private Helpers, Services & Stream Adapters
// ============================================================================

fn build_router<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver>(
    app_state: AppState<'a, H, E, O>,
) -> picoserve::Router<impl picoserve::routing::PathRouter + 'a> {
    picoserve::Router::new()
        .route(
            "/",
            picoserve::routing::get(|| async move {
                picoserve::response::Response::ok(DASHBOARD_HTML_GZ)
                    .with_content_type("text/html; charset=utf-8")
                    .with_header("Content-Encoding", "gzip")
            }),
        )
        .route(
            "/api/status",
            picoserve::routing::get(
                |picoserve::extract::State(state): picoserve::extract::State<AppState<'a, H, E, O>>| async move {
                    let diag = state.diagnostics.snapshot();
                    let mut json_buf = [0u8; 1024];
                    let json_len = crate::router::format_status_json(&diag, &mut json_buf).unwrap_or(0);
                    let json_str = alloc::string::String::from(
                        core::str::from_utf8(&json_buf[..json_len]).unwrap_or("{}"),
                    );
                    picoserve::response::Response::ok(json_str)
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/logs",
            picoserve::routing::get(
                |picoserve::extract::State(state): picoserve::extract::State<AppState<'a, H, E, O>>| async move {
                    let mut log_buf = [0u8; 2048];
                    let log_len = (state.log_provider)(&mut log_buf);
                    let log_str = alloc::string::String::from(
                        core::str::from_utf8(&log_buf[..log_len]).unwrap_or(""),
                    );
                    picoserve::response::Response::ok(log_str)
                        .with_content_type("text/plain; charset=utf-8")
                },
            ),
        )
        .route(
            "/api/action/ring",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<AppState<'a, H, E, O>>| async move {
                    state.access_handle.send(AccessCommand::Ring);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/unlock",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<AppState<'a, H, E, O>>| async move {
                    state.access_handle.send(AccessCommand::Unlock);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/v1/ota/flash",
            picoserve::routing::post_service(OtaFlashService),
        )
        .with_state(app_state)
}

struct OtaFlashService;

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver>
    picoserve::routing::RequestHandlerService<AppState<'a, H, E, O>> for OtaFlashService
{
    async fn call_request_handler_service<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        state: &AppState<'a, H, E, O>,
        _path_parameters: (),
        mut request: picoserve::request::Request<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        let content_len = match request
            .parts
            .headers()
            .get("content-length")
            .and_then(|val| val.as_str().ok()?.parse::<usize>().ok())
        {
            Some(len) => len,
            None => {
                state.had_error.set(true);
                return picoserve::response::Response::new(
                    picoserve::response::StatusCode::BAD_REQUEST,
                    "{\"error\":\"bad_request\"}",
                )
                .with_content_type("application/json")
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await;
            }
        };

        let reader = request.body_connection.body().reader();
        let mut stream = EmbeddedIoReader(reader);
        let flash_res = state
            .ota_driver
            .borrow_mut()
            .handle_flash_stream(&mut stream, content_len)
            .await;

        match flash_res {
            Ok(()) => {
                picoserve::response::Response::ok("{\"status\":\"ok\"}")
                    .with_content_type("application/json")
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await
            }
            Err(_) => {
                state.had_error.set(true);
                picoserve::response::Response::new(
                    picoserve::response::StatusCode::BAD_REQUEST,
                    "{\"error\":\"bad_request\"}",
                )
                .with_content_type("application/json")
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await
            }
        }
    }
}

struct EmbeddedIoReader<R>(R);

impl<R: picoserve::io::Read> OtaStream for EmbeddedIoReader<R> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.0.read(buf).await.map_err(|_| ())
    }

    async fn write_all(&mut self, _buf: &[u8]) -> Result<(), ()> {
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Debug)]
struct StreamIoError;

impl core::fmt::Display for StreamIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Stream IO Error")
    }
}

impl core::error::Error for StreamIoError {}

impl picoserve::io::Error for StreamIoError {
    fn kind(&self) -> picoserve::io::ErrorKind {
        picoserve::io::ErrorKind::Other
    }
}

struct StreamReadHalf<'a, 's, S: OtaStream>(&'a core::cell::RefCell<&'s mut S>);

impl<'a, 's, S: OtaStream> picoserve::io::ErrorType for StreamReadHalf<'a, 's, S> {
    type Error = StreamIoError;
}

impl<'a, 's, S: OtaStream> picoserve::io::Read for StreamReadHalf<'a, 's, S> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.borrow_mut().read(buf).await.map_err(|_| StreamIoError)
    }
}

struct StreamWriteHalf<'a, 's, S: OtaStream>(&'a core::cell::RefCell<&'s mut S>);

impl<'a, 's, S: OtaStream> picoserve::io::ErrorType for StreamWriteHalf<'a, 's, S> {
    type Error = StreamIoError;
}

impl<'a, 's, S: OtaStream> picoserve::io::BaseWrite for StreamWriteHalf<'a, 's, S> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.borrow_mut().write_all(buf).await.map_err(|_| StreamIoError)?;
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.borrow_mut().flush().await.map_err(|_| StreamIoError)
    }
}

impl<'a, 's, S: OtaStream> picoserve::io::Write for StreamWriteHalf<'a, 's, S> {
    async fn write_with<F: FnOnce(picoserve::mem::BorrowedCursor<'_>) -> Res, Res>(
        &mut self,
        f: F,
    ) -> Result<Res, Self::Error> {
        let mut buffer = [0u8; 1024];
        let mut buffer = picoserve::mem::BorrowedBuffer::new(&mut buffer);
        let output = f(buffer.unfilled());
        self.0.borrow_mut().write_all(buffer.filled()).await.map_err(|_| StreamIoError)?;
        Ok(output)
    }
}

struct SocketStream<'s, S: OtaStream> {
    cell: core::cell::RefCell<&'s mut S>,
}

impl<'s, S: OtaStream> SocketStream<'s, S> {
    fn new(stream: &'s mut S) -> Self {
        Self {
            cell: core::cell::RefCell::new(stream),
        }
    }
}

impl<'s, S: OtaStream> picoserve::io::Socket<picoserve::EmbassyRuntime> for SocketStream<'s, S> {
    type Error = StreamIoError;
    type ReadHalf<'a> = StreamReadHalf<'a, 's, S> where 's: 'a;
    type WriteHalf<'a> = StreamWriteHalf<'a, 's, S> where 's: 'a;

    fn split(&mut self) -> (Self::ReadHalf<'_>, Self::WriteHalf<'_>) {
        (StreamReadHalf(&self.cell), StreamWriteHalf(&self.cell))
    }

    async fn abort<Timer: picoserve::Timer<picoserve::EmbassyRuntime>>(
        self,
        _timeouts: &picoserve::Timeouts,
        _timer: &Timer,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        let _ = self.cell.borrow_mut().flush().await;
        Ok(())
    }

    async fn shutdown<Timer: picoserve::Timer<picoserve::EmbassyRuntime>>(
        self,
        _timeouts: &picoserve::Timeouts,
        _timer: &Timer,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        self.cell.borrow_mut().flush().await.map_err(|_| picoserve::Error::Write(StreamIoError))?;
        Ok(())
    }
}
