//! Asynchronous HTTP Web and REST API Server
//!
//! Provides an Embassy-compatible HTTP server running on `picoserve`,
//! serving the pre-compressed web dashboard and REST API endpoints for telemetry,
//! domain actions, and firmware OTA flashing.

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use intercom_core::domain::ecosystem::{EcosystemId, EcosystemType};
use intercom_core::events::SystemStateSender;
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle};
use edf_core::ports::ota::{OtaFlashDriver, OtaStream};
use intercom_core::ports::{ResetScope, SystemResetDriver};
use intercom_core::service::DiagnosticsService;
use log::{info, warn};
use picoserve::response::IntoResponse;

use crate::config::{
    HttpServerConfig, HTTP_BUFFER_SIZE, JSON_BUFFER_SIZE, LOG_BUFFER_SIZE,
    STREAM_WRITE_BUFFER_SIZE, TCP_RX_BUFFER_SIZE, TCP_TX_BUFFER_SIZE,
};
use crate::DASHBOARD_HTML_GZ;

// ============================================================================
// Internal State Types
// ============================================================================

/// Shared application state passed to HTTP request handlers.
pub(crate) struct AppState<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver,
    O: OtaFlashDriver,
    T: EcosystemType = EcosystemId,
> {
    pub(crate) access_handle: AccessControllerHandle<'a>,
    pub(crate) diagnostics: &'a DiagnosticsService<'a, H, E, T>,
    pub(crate) ota_driver: &'a core::cell::RefCell<&'a mut O>,
    pub(crate) state_sender: Option<SystemStateSender<'a>>,
    pub(crate) tarpit_delay: Duration,
    pub(crate) read_request_timeout: Duration,
    pub(crate) had_error: &'a core::cell::Cell<bool>,
    pub(crate) reboot_requested: &'a core::cell::Cell<bool>,
    pub(crate) reset_requested: &'a core::cell::Cell<Option<ResetScope>>,
    pub(crate) log_provider: fn(&mut [u8]) -> usize,
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType> Clone
    for AppState<'a, H, E, O, T>
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType> Copy
    for AppState<'a, H, E, O, T>
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
pub struct HttpServer<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver,
    O: OtaFlashDriver,
    R: SystemResetDriver = (),
    T: EcosystemType = EcosystemId,
> {
    stack: Option<Stack<'static>>,
    access_handle: AccessControllerHandle<'a>,
    diagnostics: &'a DiagnosticsService<'a, H, E, T>,
    ota_driver: &'a mut O,
    reset_driver: Option<&'a R>,
    state_sender: Option<SystemStateSender<'a>>,
    log_provider: fn(&mut [u8]) -> usize,
    config: HttpServerConfig,
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType>
    HttpServer<'a, H, E, O, (), T>
{
    /// Create an unbound `HttpServer` without a network stack or reset driver, suitable for testing.
    pub fn new_unbound(
        access_handle: AccessControllerHandle<'a>,
        diagnostics: &'a DiagnosticsService<'a, H, E, T>,
        ota_driver: &'a mut O,
        log_provider: fn(&mut [u8]) -> usize,
        mut config: HttpServerConfig,
    ) -> Self {
        config.tarpit_delay = Duration::from_millis(0);
        Self {
            stack: None,
            access_handle,
            diagnostics,
            ota_driver,
            reset_driver: None,
            state_sender: None,
            log_provider,
            config,
        }
    }
}

impl<
        'a,
        H: HardwareInfoDriver,
        E: EcosystemInfoDriver,
        O: OtaFlashDriver,
        R: SystemResetDriver,
        T: EcosystemType,
    > HttpServer<'a, H, E, O, R, T>
{
    /// Create a new `HttpServer` bound to an `embassy_net::Stack`.
    pub fn new(
        stack: Stack<'static>,
        access_handle: AccessControllerHandle<'a>,
        diagnostics: &'a DiagnosticsService<'a, H, E, T>,
        ota_driver: &'a mut O,
        reset_driver: &'a R,
        state_sender: Option<SystemStateSender<'a>>,
        log_provider: fn(&mut [u8]) -> usize,
        config: HttpServerConfig,
    ) -> Self {
        Self {
            stack: Some(stack),
            access_handle,
            diagnostics,
            ota_driver,
            reset_driver: Some(reset_driver),
            state_sender,
            log_provider,
            config,
        }
    }

    /// Attach a reset driver to the server (for reboot on OTA completion).
    pub fn with_reset_driver<R2: SystemResetDriver>(
        self,
        reset_driver: &'a R2,
    ) -> HttpServer<'a, H, E, O, R2, T> {
        HttpServer {
            stack: self.stack,
            access_handle: self.access_handle,
            diagnostics: self.diagnostics,
            ota_driver: self.ota_driver,
            reset_driver: Some(reset_driver),
            state_sender: self.state_sender,
            log_provider: self.log_provider,
            config: self.config,
        }
    }

    /// Attach a system state sender for broadcasting lifecycle transitions (e.g. Updating).
    pub fn with_state_sender(mut self, state_sender: SystemStateSender<'a>) -> Self {
        self.state_sender = Some(state_sender);
        self
    }

    /// Process a single incoming client connection on an abstract `OtaStream`.
    pub async fn handle_request<S: OtaStream>(&mut self, stream: &mut S) -> Result<(), ()> {
        let mut http_buffer = [0u8; HTTP_BUFFER_SIZE];
        let picoserve_config = picoserve::Config::new(picoserve::Timeouts {
            start_read_request: self.config.start_read_timeout,
            persistent_start_read_request: self.config.persistent_start_read_timeout,
            read_request: self.config.read_request_timeout,
            write: self.config.write_timeout,
        })
        .close_connection_after_response();

        let ota_driver_cell = core::cell::RefCell::new(&mut *self.ota_driver);
        let had_error = core::cell::Cell::new(false);
        let reboot_requested = core::cell::Cell::new(false);
        let reset_requested = core::cell::Cell::new(None);
        let app_state = AppState {
            access_handle: self.access_handle,
            diagnostics: self.diagnostics,
            ota_driver: &ota_driver_cell,
            state_sender: self.state_sender,
            tarpit_delay: self.config.tarpit_delay,
            read_request_timeout: self.config.read_request_timeout,
            had_error: &had_error,
            reboot_requested: &reboot_requested,
            reset_requested: &reset_requested,
            log_provider: self.log_provider,
        };

        let router = build_router(app_state);
        let socket = SocketStream::new(stream);
        let serve_res = picoserve::Server::new(&router, &picoserve_config, &mut http_buffer)
            .serve(socket)
            .await;

        if let Some(scope) = reset_requested.get() {
            if let Some(reset) = self.reset_driver {
                reset.erase_credentials(scope);
                reset.reboot();
            }
        }

        if reboot_requested.get() {
            if let Some(reset) = self.reset_driver {
                reset.reboot();
            }
        }

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
        let mut rx_buffer = [0u8; TCP_RX_BUFFER_SIZE];
        let mut tx_buffer = [0u8; TCP_TX_BUFFER_SIZE];
        let mut http_buffer = [0u8; HTTP_BUFFER_SIZE];

        let picoserve_config = picoserve::Config::new(picoserve::Timeouts {
            start_read_request: self.config.start_read_timeout,
            persistent_start_read_request: self.config.persistent_start_read_timeout,
            read_request: self.config.read_request_timeout,
            write: self.config.write_timeout,
        })
        .close_connection_after_response();

        let ota_driver_cell = core::cell::RefCell::new(&mut *self.ota_driver);
        let had_error = core::cell::Cell::new(false);
        let reboot_requested = core::cell::Cell::new(false);
        let reset_requested = core::cell::Cell::new(None);
        let app_state = AppState {
            access_handle: self.access_handle,
            diagnostics: self.diagnostics,
            ota_driver: &ota_driver_cell,
            state_sender: self.state_sender,
            tarpit_delay: self.config.tarpit_delay,
            read_request_timeout: self.config.read_request_timeout,
            had_error: &had_error,
            reboot_requested: &reboot_requested,
            reset_requested: &reset_requested,
            log_provider: self.log_provider,
        };

        let router = build_router(app_state);

        info!("HTTP Server listening on TCP port {}", self.config.port);

        loop {
            let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
            socket.set_timeout(Some(self.config.socket_timeout));

            if let Err(e) = socket.accept(self.config.port).await {
                warn!("HTTP socket accept error: {:?}", e);
                Timer::after(self.config.accept_retry_delay).await;
                continue;
            }

            had_error.set(false);
            let _ = picoserve::Server::new(&router, &picoserve_config, &mut http_buffer)
                .serve(socket)
                .await;

            if let Some(scope) = reset_requested.get() {
                info!(
                    "Reset requested ({:?}); erasing credentials and rebooting in {:?}...",
                    scope, self.config.reboot_delay
                );
                Timer::after(self.config.reboot_delay).await;
                if let Some(reset) = self.reset_driver {
                    reset.erase_credentials(scope);
                    reset.reboot();
                }
            }

            if reboot_requested.get() {
                info!("Reboot requested; rebooting in {:?}...", self.config.reboot_delay);
                Timer::after(self.config.reboot_delay).await;
                if let Some(reset) = self.reset_driver {
                    reset.reboot();
                }
            }
        }
    }
}

// ============================================================================
// Private Helpers, Services & Stream Adapters
// ============================================================================

fn build_router<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver,
    O: OtaFlashDriver,
    T: EcosystemType,
>(
    app_state: AppState<'a, H, E, O, T>,
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
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    let diag = state.diagnostics.snapshot();
                    let mut json_buf = [0u8; JSON_BUFFER_SIZE];
                    let json_str = match crate::router::format_status_json(&diag, &mut json_buf) {
                        Ok(len) => alloc::string::String::from(
                            core::str::from_utf8(&json_buf[..len]).unwrap_or("{}"),
                        ),
                        Err(_) => alloc::string::String::from("{}"),
                    };
                    picoserve::response::Response::ok(json_str)
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/logs",
            picoserve::routing::get(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    let mut log_buf = [0u8; LOG_BUFFER_SIZE];
                    let log_len = (state.log_provider)(&mut log_buf);
                    let log_str =
                        alloc::string::String::from_utf8_lossy(&log_buf[..log_len]).into_owned();
                    picoserve::response::Response::ok(log_str)
                        .with_content_type("text/plain; charset=utf-8")
                },
            ),
        )
        .route(
            "/api/action/ring",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.send(AccessCommand::Ring);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/unlock",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.send(AccessCommand::Unlock);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/door-duration",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >,
                 picoserve::extract::Query(query): picoserve::extract::Query<DoorDurationQuery>| async move {
                    if intercom_core::config::is_valid_door_unlock_duration(query.seconds) {
                        let _ = state.access_handle.set_unlock_duration(query.seconds);
                        picoserve::response::Response::ok("{\"status\":\"ok\"}")
                            .with_content_type("application/json")
                    } else {
                        picoserve::response::Response::new(
                            picoserve::response::StatusCode::BAD_REQUEST,
                            "{\"error\":\"invalid_duration\"}",
                        )
                        .with_content_type("application/json")
                    }
                },
            ),
        )
        .route(
            "/api/action/reset/matter",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    if let Some(sender) = state.state_sender {
                        sender.ecosystem_reset();
                    }
                    state.reset_requested.set(Some(ResetScope::Ecosystem));
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/reboot",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.reboot_requested.set(true);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/reset/factory",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    if let Some(sender) = state.state_sender {
                        sender.ecosystem_reset();
                    }
                    state.reset_requested.set(Some(ResetScope::Factory));
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/v1/ota/flash",
            picoserve::routing::post_service(OtaFlashService(core::marker::PhantomData)),
        )
        .with_state(app_state)
}

#[derive(serde::Deserialize)]
struct DoorDurationQuery {
    seconds: u32,
}

struct OtaFlashService<T: EcosystemType>(core::marker::PhantomData<fn() -> T>);

impl<
        'a,
        H: HardwareInfoDriver,
        E: EcosystemInfoDriver,
        O: OtaFlashDriver,
        T: EcosystemType,
    > picoserve::routing::RequestHandlerService<AppState<'a, H, E, O, T>> for OtaFlashService<T>
{
    async fn call_request_handler_service<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        state: &AppState<'a, H, E, O, T>,
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
                Timer::after(state.tarpit_delay).await;
                return picoserve::response::Response::new(
                    picoserve::response::StatusCode::BAD_REQUEST,
                    "{\"error\":\"bad_request\"}",
                )
                .with_content_type("application/json")
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await;
            }
        };

        if let Some(sender) = state.state_sender {
            sender.updating();
        }

        let reader = request
            .body_connection
            .body()
            .reader()
            .with_different_timeout(state.read_request_timeout);
        let mut stream = EmbeddedIoReader(reader);
        let flash_res = state
            .ota_driver
            .borrow_mut()
            .handle_flash_stream(&mut stream, content_len)
            .await;

        match flash_res {
            Ok(()) => {
                state.reboot_requested.set(true);
                picoserve::response::Response::ok("{\"status\":\"ok\"}")
                    .with_content_type("application/json")
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await
            }
            Err(_) => {
                state.had_error.set(true);
                if let Some(sender) = state.state_sender {
                    sender.operational();
                }
                Timer::after(state.tarpit_delay).await;
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
        let mut buffer = [0u8; STREAM_WRITE_BUFFER_SIZE];
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
