//! Asynchronous HTTP Web and REST API Server
//!
//! Provides an Embassy-compatible HTTP server running on `picoserve`,
//! serving the pre-compressed web dashboard and REST API endpoints for telemetry,
//! domain actions, and firmware OTA flashing.

use edf_core::ports::ota::{OtaFlashDriver, OtaStream};
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use intercom_adapter_webrtc::session::WebRtcSessionHandle;
use intercom_adapter_webrtc::signaling::{SecureRandom, SignalingError, WebRtcSignalingFacade};
use intercom_core::domain::ecosystem::{EcosystemId, EcosystemType};
use intercom_core::domain::intercom::SessionOwner;
use intercom_core::events::SystemStateSender;
use intercom_core::ports::diagnostics::HardwareInfoDriver;
use intercom_core::ports::ecosystem::EcosystemInfoDriver;
use intercom_core::ports::inbound::{AccessCommand, AccessControllerHandle};
use intercom_core::ports::{BusSafetyDriver, ResetScope, SystemResetDriver};
use intercom_core::service::DiagnosticsService;
use log::{info, warn};
use picoserve::io::Read as _;
use picoserve::response::IntoResponse;

use crate::config::{
    HttpServerConfig, HTTP_BUFFER_SIZE, JSON_BUFFER_SIZE, LOG_BUFFER_SIZE,
    STREAM_WRITE_BUFFER_SIZE, TCP_RX_BUFFER_SIZE, TCP_TX_BUFFER_SIZE,
};
use crate::router::MemorySnapshot;
use crate::DASHBOARD_HTML_GZ;

// ============================================================================
// Internal State Types
// ============================================================================

/// Signaling bridge used to start and stop Debug UI sessions on the shared WebRTC engine.
pub trait DebugWebRtcSignaling {
    fn accept_offer(&self, offer: &str, answer_out: &mut [u8]) -> Result<usize, SignalingError>;
    fn stop(&self);
}

impl<R: SecureRandom> DebugWebRtcSignaling for WebRtcSignalingFacade<'_, R> {
    fn accept_offer(&self, offer: &str, answer_out: &mut [u8]) -> Result<usize, SignalingError> {
        self.accept_offer_for_owner(offer, answer_out, SessionOwner::HttpDebug)
    }

    fn stop(&self) {
        self.stop_for_owner(SessionOwner::HttpDebug);
    }
}

/// Shared application state passed to HTTP request handlers.
pub(crate) struct AppState<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver,
    O: OtaFlashDriver,
    T: EcosystemType = EcosystemId,
> {
    pub(crate) access_handle: AccessControllerHandle<'a>,
    pub(crate) bus_safety: &'a dyn BusSafetyDriver,
    pub(crate) diagnostics: &'a DiagnosticsService<'a, H, E, T>,
    pub(crate) memory_info: Option<fn() -> MemorySnapshot>,
    pub(crate) cpu_info: Option<fn() -> crate::CpuSnapshot>,
    pub(crate) ota_busy: Option<&'a core::cell::Cell<bool>>,
    pub(crate) ota_driver: &'a core::cell::RefCell<&'a mut O>,
    pub(crate) state_sender: Option<SystemStateSender<'a>>,
    pub(crate) tarpit_delay: Duration,
    pub(crate) read_request_timeout: Duration,
    pub(crate) had_error: &'a core::cell::Cell<bool>,
    pub(crate) reboot_requested: &'a core::cell::Cell<bool>,
    pub(crate) reset_requested: &'a core::cell::Cell<Option<ResetScope>>,
    pub(crate) audio_media: Option<crate::HttpAudioMedia<'a>>,
    pub(crate) webrtc_session: Option<&'a WebRtcSessionHandle<'a>>,
    pub(crate) debug_webrtc_signaling: Option<&'a dyn DebugWebRtcSignaling>,
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
/// Binds two concurrent TCP sockets on the configured port (default 8080).
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
    bus_safety: &'a dyn BusSafetyDriver,
    diagnostics: &'a DiagnosticsService<'a, H, E, T>,
    ota_driver: &'a mut O,
    reset_driver: Option<&'a R>,
    state_sender: Option<SystemStateSender<'a>>,
    log_provider: fn(&mut [u8]) -> usize,
    config: HttpServerConfig,
    audio_media: Option<crate::HttpAudioMedia<'a>>,
    webrtc_session: Option<&'a WebRtcSessionHandle<'a>>,
    debug_webrtc_signaling: Option<&'a dyn DebugWebRtcSignaling>,
    memory_info: Option<fn() -> MemorySnapshot>,
    cpu_info: Option<fn() -> crate::CpuSnapshot>,
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType>
    HttpServer<'a, H, E, O, (), T>
{
    /// Create an unbound `HttpServer` without a network stack or reset driver, suitable for testing.
    pub fn new_unbound(
        access_handle: AccessControllerHandle<'a>,
        bus_safety: &'a dyn BusSafetyDriver,
        diagnostics: &'a DiagnosticsService<'a, H, E, T>,
        ota_driver: &'a mut O,
        log_provider: fn(&mut [u8]) -> usize,
        mut config: HttpServerConfig,
    ) -> Self {
        config.tarpit_delay = Duration::from_millis(0);
        Self {
            stack: None,
            access_handle,
            bus_safety,
            diagnostics,
            ota_driver,
            reset_driver: None,
            state_sender: None,
            audio_media: None,
            webrtc_session: None,
            debug_webrtc_signaling: None,
            memory_info: None,
            cpu_info: None,
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
        bus_safety: &'a dyn BusSafetyDriver,
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
            bus_safety,
            diagnostics,
            ota_driver,
            reset_driver: Some(reset_driver),
            state_sender,
            audio_media: None,
            webrtc_session: None,
            debug_webrtc_signaling: None,
            memory_info: None,
            cpu_info: None,
            log_provider,
            config,
        }
    }

    /// Attach the static audio channels and lifecycle watches.
    pub fn with_audio_media(mut self, media: crate::HttpAudioMedia<'a>) -> Self {
        self.audio_media = Some(media);
        self
    }

    /// Attach a provider for internal RAM and external PSRAM allocator statistics.
    pub fn with_memory_info_provider(mut self, provider: fn() -> MemorySnapshot) -> Self {
        self.memory_info = Some(provider);
        self
    }

    /// Attach a provider for per-core CPU load telemetry.
    pub fn with_cpu_info_provider(mut self, provider: fn() -> crate::CpuSnapshot) -> Self {
        self.cpu_info = Some(provider);
        self
    }

    /// Attach the WebRTC session so OTA can wait for active media buffers to be released.
    pub fn with_webrtc_session(mut self, session: &'a WebRtcSessionHandle<'a>) -> Self {
        self.webrtc_session = Some(session);
        self
    }

    /// Attach the signaling facade used by the Debug UI's shared WebRTC session.
    pub fn with_debug_webrtc_signaling(mut self, signaling: &'a dyn DebugWebRtcSignaling) -> Self {
        self.debug_webrtc_signaling = Some(signaling);
        self
    }

    /// Attach a reset driver to the server (for reboot on OTA completion).
    pub fn with_reset_driver<R2: SystemResetDriver>(
        self,
        reset_driver: &'a R2,
    ) -> HttpServer<'a, H, E, O, R2, T> {
        HttpServer {
            stack: self.stack,
            access_handle: self.access_handle,
            bus_safety: self.bus_safety,
            diagnostics: self.diagnostics,
            ota_driver: self.ota_driver,
            reset_driver: Some(reset_driver),
            state_sender: self.state_sender,
            audio_media: self.audio_media,
            webrtc_session: self.webrtc_session,
            debug_webrtc_signaling: self.debug_webrtc_signaling,
            memory_info: self.memory_info,
            cpu_info: self.cpu_info,
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
            bus_safety: self.bus_safety,
            diagnostics: self.diagnostics,
            ota_driver: &ota_driver_cell,
            ota_busy: None,
            state_sender: self.state_sender,
            audio_media: self.audio_media,
            webrtc_session: self.webrtc_session,
            debug_webrtc_signaling: self.debug_webrtc_signaling,
            memory_info: self.memory_info,
            cpu_info: self.cpu_info,
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

    /// Run two independent accept workers so WebSocket audio leaves REST responsive.
    pub async fn run(&mut self) -> ! {
        let stack = self
            .stack
            .expect("HttpServer requires a network stack to run");
        let config = self.config;
        let reset_driver = self.reset_driver;
        let access_handle = self.access_handle;
        let bus_safety = self.bus_safety;
        let diagnostics = self.diagnostics;
        let state_sender = self.state_sender;
        let audio_media = self.audio_media;
        let webrtc_session = self.webrtc_session;
        let debug_webrtc_signaling = self.debug_webrtc_signaling;
        let memory_info = self.memory_info;
        let cpu_info = self.cpu_info;
        let log_provider = self.log_provider;
        let proxy = crate::ota_proxy::OtaProxy::new();
        let picoserve_config = picoserve::Config::new(picoserve::Timeouts {
            start_read_request: config.start_read_timeout,
            persistent_start_read_request: config.persistent_start_read_timeout,
            read_request: config.read_request_timeout,
            write: config.write_timeout,
        })
        .close_connection_after_response();
        let accept_worker = |worker| {
            let proxy = &proxy;
            let picoserve_config = &picoserve_config;
            async move {
                let mut rx_buffer = [0u8; TCP_RX_BUFFER_SIZE];
                let mut tx_buffer = [0u8; TCP_TX_BUFFER_SIZE];
                let mut http_buffer = [0u8; HTTP_BUFFER_SIZE];
                let mut ota_client = proxy.client(worker);
                let ota_driver_cell = core::cell::RefCell::new(&mut ota_client);
                let had_error = core::cell::Cell::new(false);
                let reboot_requested = core::cell::Cell::new(false);
                let reset_requested = core::cell::Cell::new(None);
                let app_state = AppState {
                    access_handle,
                    bus_safety,
                    diagnostics,
                    ota_driver: &ota_driver_cell,
                    ota_busy: Some(proxy.busy()),
                    state_sender,
                    audio_media,
                    webrtc_session,
                    debug_webrtc_signaling,
                    memory_info,
                    cpu_info,
                    tarpit_delay: config.tarpit_delay,
                    read_request_timeout: config.read_request_timeout,
                    had_error: &had_error,
                    reboot_requested: &reboot_requested,
                    reset_requested: &reset_requested,
                    log_provider,
                };
                let router = build_router(app_state);
                loop {
                    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
                    socket.set_timeout(Some(config.socket_timeout));
                    if let Err(e) = socket.accept(config.port).await {
                        warn!("HTTP socket accept error: {:?}", e);
                        Timer::after(config.accept_retry_delay).await;
                        continue;
                    }
                    had_error.set(false);
                    let _ = picoserve::Server::new(&router, picoserve_config, &mut http_buffer)
                        .serve(socket)
                        .await;
                    if let Some(scope) = reset_requested.get() {
                        Timer::after(config.reboot_delay).await;
                        if let Some(reset) = reset_driver {
                            reset.erase_credentials(scope);
                            reset.reboot();
                        }
                    }
                    if reboot_requested.get() {
                        Timer::after(config.reboot_delay).await;
                        if let Some(reset) = reset_driver {
                            reset.reboot();
                        }
                    }
                }
            }
        };
        info!(
            "HTTP Server listening on TCP port {} (two workers)",
            config.port
        );
        // Only this third future owns the large streaming/signature-validation
        // state. Both HTTP sockets forward bounded chunks to it without a heap.
        embassy_futures::join::join3(
            accept_worker(0),
            accept_worker(1),
            proxy.run(&mut *self.ota_driver),
        )
        .await;
        unreachable!("HTTP and OTA workers never return")
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
                    let memory = state.memory_info.map(|provider| provider()).unwrap_or(
                        MemorySnapshot {
                            internal: crate::router::MemoryRegion {
                                free_bytes: diag.free_heap_bytes,
                                total_bytes: diag.total_heap_bytes,
                            },
                            psram: None,
                        },
                    );
                    let cpu = state.cpu_info.map(|provider| provider());
                    let json_str = match crate::router::format_status_json_with_telemetry(
                        &diag,
                        memory,
                        cpu,
                        &mut json_buf,
                    ) {
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
            "/api/action/audio/gain",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >,
                 picoserve::extract::Query(q): picoserve::extract::Query<AudioGainQuery>| async move {
                    match (
                        intercom_core::domain::policy::RxPgaGain::try_new(q.rx_half_db),
                        intercom_core::domain::policy::TxDacGain::try_new(q.tx_half_db),
                    ) {
                        (Some(rx), Some(tx)) => {
                            state.access_handle.set_audio_gain(rx, tx);
                            picoserve::response::Response::ok("{\"status\":\"ok\"}")
                                .with_content_type("application/json")
                        }
                        _ => picoserve::response::Response::new(
                            picoserve::response::StatusCode::BAD_REQUEST,
                            "{\"error\":\"invalid_audio_gain\"}",
                        )
                        .with_content_type("application/json"),
                    }
                },
            ),
        )
        .route(
            "/api/debug/audio/tone",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >,
                 picoserve::extract::Query(q): picoserve::extract::Query<BenchToneQuery>| async move {
                    if q.enable {
                        state.access_handle.start_talk(SessionOwner::BenchTone);
                    } else {
                        state.access_handle.end_session(SessionOwner::BenchTone);
                    }
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route("/api/ws/audio", picoserve::routing::get_service(WsAudioService))
        .route(
            "/api/debug/audio/webrtc/offer",
            picoserve::routing::post_service(DebugWebRtcOfferService),
        )
        .route(
            "/api/debug/audio/webrtc/stop",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    if let Some(signaling) = state.debug_webrtc_signaling {
                        signaling.stop();
                    }
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/listen",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.start_listen(SessionOwner::HttpDebug);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/talk/start",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.start_talk(SessionOwner::HttpDebug);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/talk/stop",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.stop_talk(SessionOwner::HttpDebug);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/heartbeat",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.session_heartbeat(SessionOwner::HttpDebug);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/hangup",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.access_handle.end_session(SessionOwner::HttpDebug);
                    picoserve::response::Response::ok("{\"status\":\"ok\"}")
                        .with_content_type("application/json")
                },
            ),
        )
        .route(
            "/api/action/reset/matter",
            picoserve::routing::post(
                |picoserve::extract::State(state): picoserve::extract::State<
                    AppState<'a, H, E, O, T>,
                >| async move {
                    state.bus_safety.inhibit();
                    state.access_handle.emergency_stop();
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
                    state.bus_safety.inhibit();
                    state.access_handle.emergency_stop();
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
                    state.bus_safety.inhibit();
                    state.access_handle.emergency_stop();
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
struct AudioGainQuery {
    rx_half_db: u8,
    tx_half_db: i8,
}

#[derive(serde::Deserialize)]
struct BenchToneQuery {
    enable: bool,
}

#[derive(serde::Deserialize)]
struct DoorDurationQuery {
    seconds: u32,
}

struct WsAudioService;

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType>
    picoserve::routing::RequestHandlerService<AppState<'a, H, E, O, T>> for WsAudioService
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
        use picoserve::extract::FromRequest;
        let ws = match picoserve::response::ws::WebSocketUpgrade::from_request(
            state,
            request.parts,
            request.body_connection.body(),
        )
        .await
        {
            Ok(ws) => ws,
            Err(rejection) => {
                return rejection
                    .write_to(request.body_connection.finalize().await?, response_writer)
                    .await;
            }
        };
        match state.audio_media {
            Some(media) if media.can_accept_ws() => {
                ws.on_upgrade(crate::ws_audio::WsAudioSession::new(
                    state.access_handle,
                    media,
                ))
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await
            }
            _ => {
                picoserve::response::Response::new(
                    picoserve::response::StatusCode::CONFLICT,
                    "{\"error\":\"audio_busy\"}",
                )
                .with_content_type("application/json")
                .write_to(request.body_connection.finalize().await?, response_writer)
                .await
            }
        }
    }
}

const DEBUG_SDP_BUFFER_SIZE: usize = 3_072;

struct DebugWebRtcOfferService;

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType>
    picoserve::routing::RequestHandlerService<AppState<'a, H, E, O, T>>
    for DebugWebRtcOfferService
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
        let content_length = request
            .parts
            .headers()
            .get("content-length")
            .and_then(|value| value.as_str().ok()?.parse::<usize>().ok());
        let Some(content_length) =
            content_length.filter(|len| *len > 0 && *len <= DEBUG_SDP_BUFFER_SIZE)
        else {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::BAD_REQUEST,
                "invalid SDP body length",
            )
            .write_to(request.body_connection.finalize().await?, response_writer)
            .await;
        };

        let mut offer = [0u8; DEBUG_SDP_BUFFER_SIZE];
        let read_result = {
            let mut reader = request
                .body_connection
                .body()
                .reader()
                .with_different_timeout(state.read_request_timeout);
            let mut read = 0;
            while read < content_length {
                match reader.read(&mut offer[read..content_length]).await {
                    Ok(0) => break,
                    Ok(count) => read += count,
                    Err(_) => break,
                }
            }
            read
        };
        let body_connection = request.body_connection.finalize().await?;
        if read_result != content_length {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::BAD_REQUEST,
                "incomplete SDP body",
            )
            .write_to(body_connection, response_writer)
            .await;
        }
        let Ok(offer) = core::str::from_utf8(&offer[..read_result]) else {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::BAD_REQUEST,
                "SDP must be UTF-8",
            )
            .write_to(body_connection, response_writer)
            .await;
        };
        let Some(signaling) = state.debug_webrtc_signaling else {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::SERVICE_UNAVAILABLE,
                "WebRTC signaling unavailable",
            )
            .write_to(body_connection, response_writer)
            .await;
        };
        let mut answer = [0u8; DEBUG_SDP_BUFFER_SIZE];
        let answer_len = match signaling.accept_offer(offer, &mut answer) {
            Ok(len) => len,
            Err(SignalingError::Busy) => {
                return picoserve::response::Response::new(
                    picoserve::response::StatusCode::CONFLICT,
                    "WebRTC media session busy",
                )
                .write_to(body_connection, response_writer)
                .await;
            }
            Err(SignalingError::NotReady) => {
                return picoserve::response::Response::new(
                    picoserve::response::StatusCode::SERVICE_UNAVAILABLE,
                    "WebRTC signaling not ready",
                )
                .write_to(body_connection, response_writer)
                .await;
            }
            Err(_) => {
                return picoserve::response::Response::new(
                    picoserve::response::StatusCode::BAD_REQUEST,
                    "invalid WebRTC SDP offer",
                )
                .write_to(body_connection, response_writer)
                .await;
            }
        };
        let Ok(answer) = core::str::from_utf8(&answer[..answer_len]) else {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::INTERNAL_SERVER_ERROR,
                "invalid WebRTC SDP answer",
            )
            .write_to(body_connection, response_writer)
            .await;
        };
        picoserve::response::Response::new(picoserve::response::StatusCode::OK, answer)
            .with_content_type("application/sdp")
            .write_to(body_connection, response_writer)
            .await
    }
}

struct OtaFlashService<T: EcosystemType>(core::marker::PhantomData<fn() -> T>);

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, O: OtaFlashDriver, T: EcosystemType>
    picoserve::routing::RequestHandlerService<AppState<'a, H, E, O, T>> for OtaFlashService<T>
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
        // Force the physical outputs LOW before any upload validation or flash work.
        // The interlock remains inhibited until reboot even if this update fails or is cancelled.
        state.bus_safety.inhibit();
        if let Some(session) = state.webrtc_session {
            session.stop_and_wait().await;
        }
        state.access_handle.emergency_stop();
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

        if state.ota_busy.is_some_and(core::cell::Cell::get) {
            return picoserve::response::Response::new(
                picoserve::response::StatusCode::CONFLICT,
                "{\"error\":\"ota_busy\"}",
            )
            .with_content_type("application/json")
            .write_to(request.body_connection.finalize().await?, response_writer)
            .await;
        }

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
        self.0
            .borrow_mut()
            .read(buf)
            .await
            .map_err(|_| StreamIoError)
    }
}

struct StreamWriteHalf<'a, 's, S: OtaStream>(&'a core::cell::RefCell<&'s mut S>);

impl<'a, 's, S: OtaStream> picoserve::io::ErrorType for StreamWriteHalf<'a, 's, S> {
    type Error = StreamIoError;
}

impl<'a, 's, S: OtaStream> picoserve::io::BaseWrite for StreamWriteHalf<'a, 's, S> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0
            .borrow_mut()
            .write_all(buf)
            .await
            .map_err(|_| StreamIoError)?;
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
        self.0
            .borrow_mut()
            .write_all(buffer.filled())
            .await
            .map_err(|_| StreamIoError)?;
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
    type ReadHalf<'a>
        = StreamReadHalf<'a, 's, S>
    where
        's: 'a;
    type WriteHalf<'a>
        = StreamWriteHalf<'a, 's, S>
    where
        's: 'a;

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
        self.cell
            .borrow_mut()
            .flush()
            .await
            .map_err(|_| picoserve::Error::Write(StreamIoError))?;
        Ok(())
    }
}
