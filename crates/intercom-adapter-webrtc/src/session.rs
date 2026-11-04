//! Half-duplex WebRTC turn-taking policy and active media session runner.

use embassy_net::{udp::PacketMetadata, IpEndpoint, IpListenEndpoint, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Instant, Timer};
use intercom_core::domain::intercom::SessionOwner;
use intercom_core::ports::IntercomControllerHandle;
use intercom_media::{RxAudioReceiver, TxAudioSender};

use crate::dtls::{DtlsError, DtlsRole, DtlsSrtpEngine, DtlsSrtpResourceCell};
use crate::ice::IceLiteAgent;
use crate::mux::{classify_rfc7983_packet, DemuxedPacket};
use crate::opus::{HalfDuplexOpusCodec, MAX_OPUS_PACKET_BYTES};
use crate::srtp::SrtpSession;

const SESSION_IDLE: u8 = 0;
const SESSION_QUEUED: u8 = 1;
const SESSION_ACTIVE: u8 = 2;
const SESSION_STOP_QUEUED: u8 = 3;
const SESSION_STOP_ACTIVE: u8 = 4;

pub const OPUS_DTX_MAX_BYTES: usize = 3;
pub const OPUS_RTP_CLOCK_INCREMENT: u32 = 960;
pub const MEDIA_UDP_PORT: u16 = 50_000;
const UDP_RX_PACKET_BYTES: usize = 2_048;
const UDP_TX_PACKET_BYTES: usize = 2_048;
const UDP_METADATA_SLOTS: usize = 4;
const DTLS_PENDING_DATAGRAMS: usize = 2;
const DTLS_DATAGRAM_MAX_BYTES: usize = 1_500;
const MBEDTLS_ERR_SSL_WANT_READ: i32 = -0x6900;
const RTP_HEADER_BYTES: usize = 12;
const MEDIA_POLL_MS: u64 = 20;
const TURN_HANGOVER_MS: u64 = 400;
const CORE_SESSION_HEARTBEAT_INTERVAL_MS: u64 = 1_000;
// Keep ICE-lite sessions alive for RFC 7675 consent freshness expiry.
const SESSION_IDLE_TIMEOUT_MS: u64 = 30_000;

fn maybe_send_core_session_heartbeat(
    intercom: &IntercomControllerHandle<'_>,
    last_heartbeat_ms: &mut u64,
    now_ms: u64,
) {
    if now_ms.saturating_sub(*last_heartbeat_ms) >= CORE_SESSION_HEARTBEAT_INTERVAL_MS {
        intercom.session_heartbeat(SessionOwner::MatterCall);
        *last_heartbeat_ms = now_ms;
    }
}

fn should_process_dtls_datagram(peer_is_verified: bool, srtp_established: bool) -> bool {
    peer_is_verified && !srtp_established
}

/// Inputs from the Matter WebRTC signaling hook, copied into bounded storage.
#[derive(Clone, Debug)]
pub struct WebRtcSessionConfig {
    pub local_ufrag: heapless::String<32>,
    pub remote_ufrag: heapless::String<32>,
    pub local_password: heapless::String<64>,
    pub remote_fingerprint_sha256: [u8; 32],
    pub local_dtls_role: DtlsRole,
    pub dtls_seed: [u8; 32],
    pub payload_type: u8,
    pub local_port: u16,
}

impl WebRtcSessionConfig {
    pub fn new(
        local_ufrag: &str,
        remote_ufrag: &str,
        local_password: &str,
        remote_fingerprint_sha256: [u8; 32],
        local_dtls_role: DtlsRole,
        dtls_seed: [u8; 32],
        payload_type: u8,
    ) -> Result<Self, WebRtcSessionConfigError> {
        let mut local_ufrag_storage = heapless::String::new();
        let mut remote_ufrag_storage = heapless::String::new();
        let mut local_password_storage = heapless::String::new();
        local_ufrag_storage
            .push_str(local_ufrag)
            .map_err(|_| WebRtcSessionConfigError::ValueTooLong)?;
        remote_ufrag_storage
            .push_str(remote_ufrag)
            .map_err(|_| WebRtcSessionConfigError::ValueTooLong)?;
        local_password_storage
            .push_str(local_password)
            .map_err(|_| WebRtcSessionConfigError::ValueTooLong)?;
        if !(4..=32).contains(&local_ufrag.len())
            || !(4..=32).contains(&remote_ufrag.len())
            || !(22..=64).contains(&local_password.len())
            || !(96..=127).contains(&payload_type)
        {
            return Err(WebRtcSessionConfigError::InvalidValue);
        }
        Ok(Self {
            local_ufrag: local_ufrag_storage,
            remote_ufrag: remote_ufrag_storage,
            local_password: local_password_storage,
            remote_fingerprint_sha256,
            local_dtls_role,
            dtls_seed,
            payload_type,
            local_port: MEDIA_UDP_PORT,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebRtcSessionConfigError {
    ValueTooLong,
    InvalidValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebRtcSessionRequestError {
    Busy,
    NotActive,
}

/// Lightweight, clonable request port used by Matter signaling to start one call.
#[derive(Clone, Copy)]
pub struct WebRtcSessionHandle<'a> {
    sender: Sender<'a, CriticalSectionRawMutex, WebRtcSessionConfig, 1>,
    state: &'a portable_atomic::AtomicU8,
}

impl WebRtcSessionHandle<'_> {
    pub fn start(&self, config: WebRtcSessionConfig) -> Result<(), WebRtcSessionRequestError> {
        self.state
            .compare_exchange(
                SESSION_IDLE,
                SESSION_QUEUED,
                portable_atomic::Ordering::AcqRel,
                portable_atomic::Ordering::Acquire,
            )
            .map_err(|_| WebRtcSessionRequestError::Busy)?;
        if self.sender.try_send(config).is_err() {
            self.state
                .store(SESSION_IDLE, portable_atomic::Ordering::Release);
            Err(WebRtcSessionRequestError::Busy)
        } else {
            Ok(())
        }
    }

    /// Requests teardown of the active call, releasing all session-owned buffers.
    pub fn stop(&self) -> Result<(), WebRtcSessionRequestError> {
        loop {
            let current = self.state.load(portable_atomic::Ordering::Acquire);
            let requested = match current {
                SESSION_QUEUED => SESSION_STOP_QUEUED,
                SESSION_ACTIVE => SESSION_STOP_ACTIVE,
                SESSION_STOP_QUEUED | SESSION_STOP_ACTIVE => return Ok(()),
                _ => return Err(WebRtcSessionRequestError::NotActive),
            };
            if self
                .state
                .compare_exchange(
                    current,
                    requested,
                    portable_atomic::Ordering::AcqRel,
                    portable_atomic::Ordering::Acquire,
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Requests teardown and waits until the engine has dropped all session storage.
    pub async fn stop_and_wait(&self) {
        let _ = self.stop();
        self.wait_idle().await;
    }

    /// True only after the engine has dropped all session-owned storage.
    pub fn is_idle(&self) -> bool {
        self.state.load(portable_atomic::Ordering::Acquire) == SESSION_IDLE
    }

    /// Waits until the engine has dropped all session-owned storage.
    pub async fn wait_idle(&self) {
        while !self.is_idle() {
            embassy_futures::yield_now().await;
        }
    }

    pub fn is_stopping(&self) -> bool {
        matches!(
            self.state.load(portable_atomic::Ordering::Acquire),
            SESSION_STOP_QUEUED | SESSION_STOP_ACTIVE
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaBenchmarkStats {
    pub max_encode_us: u32,
    pub max_decode_us: u32,
    pub min_free_heap_bytes: u32,
    free_heap_sampled: bool,
}

impl MediaBenchmarkStats {
    pub fn observe_encode(&mut self, elapsed_us: u32) {
        self.max_encode_us = self.max_encode_us.max(elapsed_us);
    }

    pub fn observe_decode(&mut self, elapsed_us: u32) {
        self.max_decode_us = self.max_decode_us.max(elapsed_us);
    }

    pub fn observe_free_heap(&mut self, free_heap_bytes: u32) {
        self.min_free_heap_bytes = if !self.free_heap_sampled {
            free_heap_bytes
        } else {
            self.min_free_heap_bytes.min(free_heap_bytes)
        };
        self.free_heap_sampled = true;
    }
}

/// Shared media working buffers kept outside the general-purpose heap.
pub struct WebRtcMediaResources {
    codec: HalfDuplexOpusCodec,
    rx_metadata: [PacketMetadata; UDP_METADATA_SLOTS],
    tx_metadata: [PacketMetadata; UDP_METADATA_SLOTS],
    rx_buffer: [u8; UDP_RX_PACKET_BYTES],
    tx_buffer: [u8; UDP_TX_PACKET_BYTES],
    packet: [u8; UDP_RX_PACKET_BYTES],
    outbound: [u8; UDP_TX_PACKET_BYTES],
    pending_dtls: PendingDtlsDatagrams,
}

struct PendingDtlsDatagrams {
    packets: [[u8; DTLS_DATAGRAM_MAX_BYTES]; DTLS_PENDING_DATAGRAMS],
    lengths: [u16; DTLS_PENDING_DATAGRAMS],
    head: u8,
    count: u8,
}

impl PendingDtlsDatagrams {
    const fn new() -> Self {
        Self {
            packets: [[0; DTLS_DATAGRAM_MAX_BYTES]; DTLS_PENDING_DATAGRAMS],
            lengths: [0; DTLS_PENDING_DATAGRAMS],
            head: 0,
            count: 0,
        }
    }

    fn push(&mut self, datagram: &[u8]) -> bool {
        if datagram.is_empty()
            || datagram.len() > DTLS_DATAGRAM_MAX_BYTES
            || usize::from(self.count) == DTLS_PENDING_DATAGRAMS
        {
            return false;
        }
        let index = (usize::from(self.head) + usize::from(self.count)) % DTLS_PENDING_DATAGRAMS;
        self.packets[index][..datagram.len()].copy_from_slice(datagram);
        self.lengths[index] = datagram.len() as u16;
        self.count += 1;
        true
    }

    fn front(&self) -> Option<&[u8]> {
        (self.count != 0).then(|| {
            let index = usize::from(self.head);
            &self.packets[index][..usize::from(self.lengths[index])]
        })
    }

    fn pop(&mut self) {
        if self.count == 0 {
            return;
        }
        let index = usize::from(self.head);
        self.lengths[index] = 0;
        self.head = ((index + 1) % DTLS_PENDING_DATAGRAMS) as u8;
        self.count -= 1;
    }

    fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
        self.lengths = [0; DTLS_PENDING_DATAGRAMS];
    }
}

impl WebRtcMediaResources {
    pub const fn new() -> Self {
        Self {
            codec: HalfDuplexOpusCodec::new(),
            rx_metadata: [PacketMetadata::EMPTY; UDP_METADATA_SLOTS],
            tx_metadata: [PacketMetadata::EMPTY; UDP_METADATA_SLOTS],
            rx_buffer: [0; UDP_RX_PACKET_BYTES],
            tx_buffer: [0; UDP_TX_PACKET_BYTES],
            packet: [0; UDP_RX_PACKET_BYTES],
            outbound: [0; UDP_TX_PACKET_BYTES],
            pending_dtls: PendingDtlsDatagrams::new(),
        }
    }
}

/// Runs one active media session at a time using preallocated codec and packet storage.
pub struct WebRtcMediaEngine<'a> {
    requests: Receiver<'a, CriticalSectionRawMutex, WebRtcSessionConfig, 1>,
    intercom: IntercomControllerHandle<'a>,
    rx_audio: RxAudioReceiver<'a>,
    tx_audio: TxAudioSender<'a>,
    state: &'a portable_atomic::AtomicU8,
    resources: &'a mut WebRtcMediaResources,
    dtls_resources: &'a DtlsSrtpResourceCell<'a>,
    benchmark_stats: MediaBenchmarkStats,
}

impl<'a> WebRtcMediaEngine<'a> {
    pub fn new(
        request_channel: &'a Channel<CriticalSectionRawMutex, WebRtcSessionConfig, 1>,
        state: &'a portable_atomic::AtomicU8,
        intercom: IntercomControllerHandle<'a>,
        rx_audio: RxAudioReceiver<'a>,
        tx_audio: TxAudioSender<'a>,
        resources: &'a mut WebRtcMediaResources,
        dtls_resources: &'a DtlsSrtpResourceCell<'a>,
    ) -> (Self, WebRtcSessionHandle<'a>) {
        (
            Self {
                requests: request_channel.receiver(),
                intercom,
                rx_audio,
                tx_audio,
                state,
                resources,
                dtls_resources,
                benchmark_stats: MediaBenchmarkStats::default(),
            },
            WebRtcSessionHandle {
                sender: request_channel.sender(),
                state,
            },
        )
    }

    pub const fn benchmark_stats(&self) -> MediaBenchmarkStats {
        self.benchmark_stats
    }

    /// Waits without session-sized allocations until the commissioned Matter hook starts a call.
    pub async fn run(&mut self, stack: Stack<'static>, free_heap_bytes: fn() -> u32) {
        loop {
            let config = self.requests.receive().await;
            match self.state.compare_exchange(
                SESSION_QUEUED,
                SESSION_ACTIVE,
                portable_atomic::Ordering::AcqRel,
                portable_atomic::Ordering::Acquire,
            ) {
                Ok(_) => {}
                Err(SESSION_STOP_QUEUED) => {
                    self.state
                        .store(SESSION_IDLE, portable_atomic::Ordering::Release);
                    self.intercom
                        .end_session_wait(SessionOwner::MatterCall)
                        .await;
                    continue;
                }
                Err(_) => continue,
            }
            self.benchmark_stats = MediaBenchmarkStats::default();
            self.benchmark_stats.observe_free_heap(free_heap_bytes());
            let result = self.run_session(stack, config, free_heap_bytes).await;
            self.state
                .store(SESSION_IDLE, portable_atomic::Ordering::Release);
            self.intercom
                .end_session_wait(SessionOwner::MatterCall)
                .await;
            self.benchmark_stats.observe_free_heap(free_heap_bytes());
            log::info!(
                "WebRTC media benchmark encode_max_us={} decode_max_us={} min_free_heap_bytes={}",
                self.benchmark_stats.max_encode_us,
                self.benchmark_stats.max_decode_us,
                self.benchmark_stats.min_free_heap_bytes,
            );
            if let Err(error) = result {
                log::warn!("WebRTC session ended: {:?}", error);
            }
        }
    }

    async fn run_session(
        &mut self,
        stack: Stack<'static>,
        config: WebRtcSessionConfig,
        free_heap_bytes: fn() -> u32,
    ) -> Result<(), WebRtcMediaError> {
        let mut dtls_resources = self.dtls_resources.borrow_mut().await;
        let WebRtcMediaResources {
            codec,
            rx_metadata,
            tx_metadata,
            rx_buffer,
            tx_buffer,
            packet,
            outbound,
            pending_dtls,
        } = &mut *self.resources;
        pending_dtls.clear();
        let mut socket = embassy_net::udp::UdpSocket::new(
            stack,
            &mut rx_metadata[..],
            &mut rx_buffer[..],
            &mut tx_metadata[..],
            &mut tx_buffer[..],
        );
        socket
            .bind(IpListenEndpoint {
                addr: None,
                port: config.local_port,
            })
            .map_err(|_| WebRtcMediaError::UdpBind)?;

        let mut dtls = DtlsSrtpEngine::new(
            config.local_dtls_role,
            config.dtls_seed,
            &mut dtls_resources,
        )
        .map_err(|error| {
            log::warn!("WebRTC DTLS initialization failed: {:?}", error);
            WebRtcMediaError::Dtls
        })?;
        let mut ice = IceLiteAgent::new(
            &config.local_ufrag,
            &config.remote_ufrag,
            &config.local_password,
        )
        .map_err(|_| WebRtcMediaError::Ice)?;
        let mut srtp = None;
        let mut dtls_peer_transport_set = false;
        let mut ice_peer_logged = false;
        let mut first_audio_packet_logged = false;
        let mut turn = TurnTakingPolicy::new(TURN_HANGOVER_MS);
        let mut sequence = 0u16;
        let mut timestamp = 0u32;
        let ssrc = 0x5349_4e54u32;
        let mut liveness =
            SessionLiveness::new(Instant::now().as_millis(), SESSION_IDLE_TIMEOUT_MS);
        let mut last_core_heartbeat_ms = Instant::now().as_millis();

        self.intercom.start_listen(SessionOwner::MatterCall);
        self.benchmark_stats.observe_free_heap(free_heap_bytes());

        loop {
            if self.state.load(portable_atomic::Ordering::Acquire) == SESSION_STOP_ACTIVE {
                return Ok(());
            }
            let now_ms = Instant::now().as_millis();
            if liveness.is_expired(now_ms) {
                return Err(WebRtcMediaError::SessionTimeout);
            }
            maybe_send_core_session_heartbeat(&self.intercom, &mut last_core_heartbeat_ms, now_ms);
            if ice.verified_peer_addr().is_some() && srtp.is_none() {
                let dtls_output =
                    step_dtls_and_pop(&mut dtls, now_ms, &mut outbound[..]).map_err(|error| {
                        let (export_called, export_type, export_prf, export_error) =
                            dtls.export_diagnostic();
                        let (debug_level, debug_line, debug_message, debug_length) =
                            dtls.debug_diagnostic();
                        let debug_message = core::str::from_utf8(
                            &debug_message[..debug_length.min(debug_message.len())],
                        )
                        .unwrap_or("<invalid UTF-8>");
                        log::warn!(
                            "WebRTC DTLS handshake step failed: {:?}; key_export called={} type={} prf={} error={}; mbedtls level={} line={} message={}",
                            error,
                            export_called,
                            export_type,
                            export_prf,
                            export_error,
                            debug_level,
                            debug_line,
                            debug_message,
                        );
                        WebRtcMediaError::Dtls
                    })?;
                if let Some(out_length) = dtls_output {
                    if let Some(peer) = ice.verified_peer_addr() {
                        socket
                            .send_to(&outbound[..out_length], peer)
                            .await
                            .map_err(|_| WebRtcMediaError::UdpSend)?;
                    }
                }
                if dtls.peer_cert_sha256() == Some(config.remote_fingerprint_sha256) {
                    let keys = dtls.exported_srtp_keys().ok_or_else(|| {
                        log::warn!("WebRTC DTLS completed without SRTP keys");
                        WebRtcMediaError::Dtls
                    })?;
                    srtp = Some(SrtpSession::from_dtls_keys(keys, config.local_dtls_role));
                    log::info!("WebRTC: DTLS fingerprint verified; SRTP ready");
                } else if dtls.peer_cert_sha256().is_some() {
                    return Err(WebRtcMediaError::FingerprintMismatch);
                }
                if srtp.is_none() {
                    while let Some(datagram) = pending_dtls.front() {
                        match dtls.push_inbound_datagram(datagram) {
                            Ok(()) => pending_dtls.pop(),
                            Err(DtlsError::InputFailed(MBEDTLS_ERR_SSL_WANT_READ)) => break,
                            Err(error) => {
                                log::warn!("WebRTC queued DTLS datagram input failed: {:?}", error);
                                return Err(WebRtcMediaError::Dtls);
                            }
                        }
                    }
                }
            }
            let receive = socket.recv_from(&mut packet[..]);
            let poll = Timer::after_millis(MEDIA_POLL_MS);
            let selected = embassy_futures::select::select(receive, poll).await;
            if let embassy_futures::select::Either::First(Ok((length, metadata))) = selected {
                let remote = metadata.endpoint;
                let now_ms = Instant::now().as_millis();
                match classify_rfc7983_packet(&packet[..length]) {
                    DemuxedPacket::Stun => {
                        let mut response = [0u8; 128];
                        let previously_nominated = ice.verified_peer_addr();
                        if let Some(response_length) = ice.handle_stun_binding_request(
                            &packet[..length],
                            remote,
                            &mut response,
                        ) {
                            socket
                                .send_to(&response[..response_length], remote)
                                .await
                                .map_err(|_| WebRtcMediaError::UdpSend)?;
                            if !ice_peer_logged && ice.verified_peer_addr() == Some(remote) {
                                log::info!("WebRTC: ICE nominated peer {:?}", remote);
                                ice_peer_logged = true;
                            }
                            if config.local_dtls_role == DtlsRole::Server
                                && !dtls_peer_transport_set
                                && ice.verified_peer_addr() == Some(remote)
                            {
                                if previously_nominated.is_none() {
                                    dtls.set_peer_transport_id(&remote_peer_id(remote))
                                        .map_err(|error| {
                                            log::warn!(
                                                "WebRTC DTLS peer binding failed: {:?}",
                                                error
                                            );
                                            WebRtcMediaError::Dtls
                                        })?;
                                    dtls_peer_transport_set = true;
                                }
                            }
                            liveness.refresh(now_ms);
                        }
                    }
                    DemuxedPacket::Dtls => {
                        if !should_process_dtls_datagram(
                            ice.verified_peer_addr() == Some(remote),
                            srtp.is_some(),
                        ) {
                            // DTLS drives the handshake; late retransmits and close notifications
                            // are irrelevant once the fingerprint-bound SRTP session exists.
                            continue;
                        }
                        match dtls.push_inbound_datagram(&packet[..length]) {
                            Ok(()) => {}
                            Err(DtlsError::InputFailed(MBEDTLS_ERR_SSL_WANT_READ)) => {
                                if !pending_dtls.push(&packet[..length]) {
                                    log::warn!("WebRTC pending DTLS datagram queue is full");
                                    return Err(WebRtcMediaError::Dtls);
                                }
                            }
                            Err(error) => {
                                log::warn!("WebRTC DTLS datagram input failed: {:?}", error);
                                return Err(WebRtcMediaError::Dtls);
                            }
                        }
                    }
                    DemuxedPacket::RtpOrRtcp => {
                        if ice.verified_peer_addr() != Some(remote) {
                            continue;
                        }
                        if is_receiver_report_candidate(&packet[..length]) {
                            if let Some(srtp) = srtp.as_mut() {
                                let _ = accept_srtcp_receiver_report(
                                    srtp,
                                    &mut packet[..],
                                    length,
                                    &mut liveness,
                                    now_ms,
                                );
                            }
                            continue;
                        }
                        if !matches_payload_type(&packet[..length], config.payload_type) {
                            continue;
                        }
                        let Some(srtp) = srtp.as_mut() else {
                            continue;
                        };
                        let Ok(plain_length) = srtp.unprotect_rtp(&mut packet[..], length) else {
                            continue;
                        };
                        let range = rtp_payload_range(&packet[..plain_length])?;
                        let payload = &packet[range.start..range.end];
                        liveness.refresh(now_ms);
                        match turn.on_inbound_rtp(payload.len(), now_ms) {
                            Some(TurnAction::StartTalk) => {
                                self.intercom.start_talk(SessionOwner::MatterCall)
                            }
                            Some(TurnAction::StopTalk) | None => {}
                        }
                        let decode_start = Instant::now();
                        let frame = codec
                            .decode_talk_frame(Some(payload))
                            .map_err(|_| WebRtcMediaError::Opus)?;
                        self.benchmark_stats.observe_decode(
                            decode_start.elapsed().as_micros().min(u64::from(u32::MAX)) as u32,
                        );
                        let _ = self.tx_audio.try_send(frame);
                    }
                    DemuxedPacket::Unknown => {}
                }
            }

            let now_ms = Instant::now().as_millis();
            if liveness.is_expired(now_ms) {
                return Err(WebRtcMediaError::SessionTimeout);
            }
            if turn.poll_hangover(now_ms) == Some(TurnAction::StopTalk) {
                self.intercom.stop_talk(SessionOwner::MatterCall);
            }
            if !turn.is_talking() && srtp.is_some() {
                if let Some(frame) = self.rx_audio.try_receive().ok() {
                    let Some(srtp) = srtp.as_mut() else {
                        continue;
                    };
                    let encode_start = Instant::now();
                    let mut encoded = [0u8; MAX_OPUS_PACKET_BYTES];
                    let encoded_length = codec
                        .encode_listen_frame(&frame, &mut encoded)
                        .map_err(|_| WebRtcMediaError::Opus)?;
                    self.benchmark_stats.observe_encode(
                        encode_start.elapsed().as_micros().min(u64::from(u32::MAX)) as u32,
                    );
                    let packet_length = RTP_HEADER_BYTES + encoded_length;
                    outbound[0] = 0x80;
                    outbound[1] = config.payload_type & 0x7f;
                    outbound[2..4].copy_from_slice(&sequence.to_be_bytes());
                    outbound[4..8].copy_from_slice(&timestamp.to_be_bytes());
                    outbound[8..12].copy_from_slice(&ssrc.to_be_bytes());
                    outbound[RTP_HEADER_BYTES..packet_length]
                        .copy_from_slice(&encoded[..encoded_length]);
                    let protected_length = srtp
                        .protect_rtp(&mut outbound[..], packet_length)
                        .map_err(|_| WebRtcMediaError::Srtp)?;
                    if let Some(peer) = ice.verified_peer_addr() {
                        socket
                            .send_to(&outbound[..protected_length], peer)
                            .await
                            .map_err(|_| WebRtcMediaError::UdpSend)?;
                        if !first_audio_packet_logged {
                            log::info!(
                                "WebRTC: first outbound SRTP audio packet sent ({} bytes)",
                                protected_length
                            );
                            first_audio_packet_logged = true;
                        }
                    }
                    sequence = sequence.wrapping_add(1);
                    timestamp = timestamp.wrapping_add(OPUS_RTP_CLOCK_INCREMENT);
                }
            }
            self.benchmark_stats.observe_free_heap(free_heap_bytes());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebRtcMediaError {
    UdpBind,
    UdpSend,
    Ice,
    Dtls,
    FingerprintMismatch,
    Opus,
    Srtp,
    InvalidRtp,
    SessionTimeout,
}

fn remote_peer_id(remote: IpEndpoint) -> heapless::Vec<u8, 32> {
    use core::fmt::Write;
    let mut id = heapless::String::<32>::new();
    let _ = write!(id, "{}:{}", remote.addr, remote.port);
    id.into_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RtpPayloadRange {
    start: usize,
    end: usize,
}

fn rtp_payload_range(packet: &[u8]) -> Result<RtpPayloadRange, WebRtcMediaError> {
    if packet.len() < RTP_HEADER_BYTES || packet[0] >> 6 != 2 {
        return Err(WebRtcMediaError::InvalidRtp);
    }
    let csrc_count = usize::from(packet[0] & 0x0f);
    let mut offset = RTP_HEADER_BYTES + csrc_count * 4;
    if offset > packet.len() {
        return Err(WebRtcMediaError::InvalidRtp);
    }
    if packet[0] & 0x10 != 0 {
        if offset + 4 > packet.len() {
            return Err(WebRtcMediaError::InvalidRtp);
        }
        let extension_words =
            usize::from(u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]));
        offset = offset
            .checked_add(4 + extension_words * 4)
            .filter(|end| *end <= packet.len())
            .ok_or(WebRtcMediaError::InvalidRtp)?;
    }
    let padding = if packet[0] & 0x20 != 0 {
        let count = usize::from(*packet.last().ok_or(WebRtcMediaError::InvalidRtp)?);
        if count == 0 {
            return Err(WebRtcMediaError::InvalidRtp);
        }
        count
    } else {
        0
    };
    let end = packet
        .len()
        .checked_sub(padding)
        .filter(|end| *end >= offset)
        .ok_or(WebRtcMediaError::InvalidRtp)?;
    Ok(RtpPayloadRange { start: offset, end })
}

fn matches_payload_type(packet: &[u8], payload_type: u8) -> bool {
    packet.len() >= RTP_HEADER_BYTES && packet[0] >> 6 == 2 && packet[1] & 0x7f == payload_type
}

fn is_receiver_report_candidate(packet: &[u8]) -> bool {
    packet.len() >= 2 && packet[0] >> 6 == 2 && packet[1] == 201
}

fn accept_srtcp_receiver_report(
    srtp: &mut SrtpSession,
    packet: &mut [u8],
    packet_length: usize,
    liveness: &mut SessionLiveness,
    now_ms: u64,
) -> bool {
    if srtp.unprotect_rtcp(packet, packet_length).is_err() {
        return false;
    }
    liveness.refresh(now_ms);
    true
}

fn step_dtls_and_pop(
    dtls: &mut DtlsSrtpEngine,
    now_ms: u64,
    output: &mut [u8],
) -> Result<Option<usize>, crate::dtls::DtlsError> {
    dtls.step(now_ms)?;
    Ok(dtls.pop_outbound_datagram(output))
}

#[derive(Clone, Copy, Debug)]
struct SessionLiveness {
    timeout_ms: u64,
    deadline_ms: u64,
}

impl SessionLiveness {
    const fn new(now_ms: u64, timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            deadline_ms: now_ms.saturating_add(timeout_ms),
        }
    }

    fn refresh(&mut self, now_ms: u64) {
        self.deadline_ms = now_ms.saturating_add(self.timeout_ms);
    }

    const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnAction {
    StartTalk,
    StopTalk,
}

#[derive(Clone, Copy, Debug)]
pub struct TurnTakingPolicy {
    hangover_ms: u64,
    talking: bool,
    talk_release_deadline_ms: u64,
}

impl TurnTakingPolicy {
    pub const fn new(hangover_ms: u64) -> Self {
        Self {
            hangover_ms,
            talking: false,
            talk_release_deadline_ms: 0,
        }
    }

    pub const fn is_talking(&self) -> bool {
        self.talking
    }

    pub fn on_inbound_rtp(&mut self, opus_payload_len: usize, now_ms: u64) -> Option<TurnAction> {
        if opus_payload_len <= OPUS_DTX_MAX_BYTES {
            return None;
        }

        self.talk_release_deadline_ms = now_ms.saturating_add(self.hangover_ms);
        if self.talking {
            None
        } else {
            self.talking = true;
            Some(TurnAction::StartTalk)
        }
    }

    pub fn poll_hangover(&mut self, now_ms: u64) -> Option<TurnAction> {
        if self.talking && now_ms >= self.talk_release_deadline_ms {
            self.talking = false;
            Some(TurnAction::StopTalk)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DtlsRole, MediaBenchmarkStats, PendingDtlsDatagrams, RtpPayloadRange, SessionLiveness,
        TurnAction, TurnTakingPolicy, WebRtcSessionConfig, WebRtcSessionHandle,
        WebRtcSessionRequestError, OPUS_DTX_MAX_BYTES, SESSION_ACTIVE, SESSION_IDLE,
        SESSION_IDLE_TIMEOUT_MS, SESSION_STOP_ACTIVE, SESSION_STOP_QUEUED,
    };
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use intercom_core::domain::intercom::{IntercomAction, SessionOwner};
    use intercom_core::ports::{CommandChannel, IntercomControllerHandle};

    #[test]
    fn established_srtp_ignores_late_dtls_datagrams() {
        assert!(!super::should_process_dtls_datagram(true, true));
        assert!(super::should_process_dtls_datagram(true, false));
        assert!(!super::should_process_dtls_datagram(false, false));
    }

    #[test]
    fn media_resources_stay_within_main_stack_reserve() {
        assert!(core::mem::size_of::<super::WebRtcMediaResources>() <= 36 * 1024);
    }

    #[test]
    fn pending_dtls_datagram_queue_preserves_fifo_and_applies_backpressure() {
        // Arrange
        let mut queue = PendingDtlsDatagrams::new();

        // Act
        let first_queued = queue.push(b"first");
        let second_queued = queue.push(b"second");
        let full_queue_rejected = !queue.push(b"third");
        assert_eq!(queue.front(), Some(&b"first"[..]));
        queue.pop();
        assert_eq!(queue.front(), Some(&b"second"[..]));
        queue.pop();

        // Assert
        assert!(first_queued);
        assert!(second_queued);
        assert!(full_queue_rejected);
        assert_eq!(queue.front(), None);
    }

    #[test]
    fn active_webrtc_session_refreshes_core_heartbeat_before_timeout() {
        // Arrange
        let commands = CommandChannel::new();
        let intercom = IntercomControllerHandle::new(commands.sender());
        let mut last_heartbeat_ms = 0;

        // Act
        super::maybe_send_core_session_heartbeat(&intercom, &mut last_heartbeat_ms, 999);
        let before_deadline = commands.receiver().try_receive();
        super::maybe_send_core_session_heartbeat(&intercom, &mut last_heartbeat_ms, 1_000);
        let at_deadline = commands.receiver().try_receive();
        super::maybe_send_core_session_heartbeat(&intercom, &mut last_heartbeat_ms, 1_999);
        let before_next_deadline = commands.receiver().try_receive();
        super::maybe_send_core_session_heartbeat(&intercom, &mut last_heartbeat_ms, 2_000);
        let at_next_deadline = commands.receiver().try_receive();

        // Assert
        assert!(before_deadline.is_err());
        assert!(matches!(
            at_deadline,
            Ok(IntercomAction::SessionHeartbeat(SessionOwner::MatterCall))
        ));
        assert!(before_next_deadline.is_err());
        assert!(matches!(
            at_next_deadline,
            Ok(IntercomAction::SessionHeartbeat(SessionOwner::MatterCall))
        ));
    }

    #[test]
    fn session_liveness_tolerates_webrtc_ice_consent_interval() {
        // ICE consent freshness can legitimately leave longer gaps than the old five-second timeout.
        let liveness = SessionLiveness::new(0, SESSION_IDLE_TIMEOUT_MS);

        assert!(!liveness.is_expired(15_000));
        assert!(liveness.is_expired(30_000));
    }

    #[test]
    fn session_liveness_expires_after_five_seconds_without_valid_packets() {
        // Arrange
        let liveness = SessionLiveness::new(1_000, 5_000);

        // Act
        let at_deadline = liveness.is_expired(6_000);

        // Assert
        assert!(at_deadline);
    }

    #[test]
    fn valid_packet_refreshes_session_liveness_deadline() {
        // Arrange
        let mut liveness = SessionLiveness::new(1_000, 5_000);

        // Act
        liveness.refresh(5_900);
        let before_new_deadline = liveness.is_expired(10_899);
        let at_new_deadline = liveness.is_expired(10_900);

        // Assert
        assert!(!before_new_deadline);
        assert!(at_new_deadline);
    }

    #[test]
    fn nominated_client_starts_dtls_and_retransmits_without_inbound_packets() {
        // Arrange
        let _guard = crate::dtls::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut resources = crate::dtls::DtlsSrtpResources::new();
        let mut dtls =
            crate::dtls::DtlsSrtpEngine::new(DtlsRole::Client, [0x42; 32], &mut resources)
                .expect("client DTLS init");
        let mut datagram = [0; 1500];

        // Act
        let client_hello_len = super::step_dtls_and_pop(&mut dtls, 0, &mut datagram)
            .expect("initial DTLS step")
            .expect("ClientHello output");
        let retry_len = super::step_dtls_and_pop(&mut dtls, 8_000, &mut datagram)
            .expect("timer-driven DTLS step")
            .expect("ClientHello retransmission");

        // Assert
        assert!(client_hello_len > 13);
        assert_eq!(datagram[0], 22); // DTLS handshake record
        assert_eq!(datagram[13], 1); // ClientHello handshake message
        assert!(retry_len > 13);
        assert_eq!(datagram[0], 22);
        assert_eq!(datagram[13], 1);
    }

    #[test]
    fn authenticated_srtcp_receiver_report_refreshes_liveness_only_once() {
        // Arrange
        let mut sender = crate::srtp::SrtpSession::new([0x11; 16], [0x22; 14]);
        let mut receiver = crate::srtp::SrtpSession::new([0x11; 16], [0x22; 14]);
        let mut report = [0; 64];
        report[..8].copy_from_slice(&[0x80, 201, 0, 1, 0x01, 0x02, 0x03, 0x04]);
        let protected_len = sender.protect_rtcp(&mut report, 8).expect("protect RR");
        let mut liveness = SessionLiveness::new(1_000, 5_000);

        // Act
        let accepted = super::accept_srtcp_receiver_report(
            &mut receiver,
            &mut report,
            protected_len,
            &mut liveness,
            5_900,
        );
        let before_refreshed_deadline = liveness.is_expired(10_899);
        let at_refreshed_deadline = liveness.is_expired(10_900);

        // Assert
        assert!(accepted);
        assert!(!before_refreshed_deadline);
        assert!(at_refreshed_deadline);
    }

    #[test]
    fn invalid_or_replayed_srtcp_receiver_report_does_not_refresh_liveness() {
        // Arrange
        let mut sender = crate::srtp::SrtpSession::new([0x33; 16], [0x44; 14]);
        let mut receiver = crate::srtp::SrtpSession::new([0x33; 16], [0x44; 14]);
        let mut report = [0; 64];
        report[..8].copy_from_slice(&[0x80, 201, 0, 1, 0x01, 0x02, 0x03, 0x04]);
        let protected_len = sender.protect_rtcp(&mut report, 8).expect("protect RR");
        let mut liveness = SessionLiveness::new(1_000, 5_000);
        assert!(super::accept_srtcp_receiver_report(
            &mut receiver,
            &mut report,
            protected_len,
            &mut liveness,
            2_000,
        ));
        let refreshed_deadline = liveness.deadline_ms;

        // Act
        let replayed = super::accept_srtcp_receiver_report(
            &mut receiver,
            &mut report,
            protected_len,
            &mut liveness,
            4_000,
        );
        let mut tampered_report = [0; 64];
        tampered_report[..8].copy_from_slice(&[0x80, 201, 0, 1, 0x01, 0x02, 0x03, 0x04]);
        let tampered_length = sender
            .protect_rtcp(&mut tampered_report, 8)
            .expect("protect second RR");
        tampered_report[tampered_length - 1] ^= 1;
        let tampered = super::accept_srtcp_receiver_report(
            &mut receiver,
            &mut tampered_report,
            tampered_length,
            &mut liveness,
            4_500,
        );

        // Assert
        assert!(!replayed);
        assert!(!tampered);
        assert_eq!(liveness.deadline_ms, refreshed_deadline);
    }

    #[test]
    fn rtp_payload_range_excludes_padding_bytes() {
        // Arrange
        let packet = [0xA0, 111, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0x31, 0x32, 0, 2];

        // Act
        let range = super::rtp_payload_range(&packet);

        // Assert
        assert_eq!(range, Ok(RtpPayloadRange { start: 12, end: 14 }));
    }

    #[test]
    fn rtp_payload_range_rejects_zero_padding_count() {
        // Arrange
        let packet = [0xA0, 111, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0x31, 0];

        // Act
        let range = super::rtp_payload_range(&packet);

        // Assert
        assert_eq!(range, Err(super::WebRtcMediaError::InvalidRtp));
    }

    #[test]
    fn session_config_rejects_static_payload_type_for_opus() {
        // Arrange
        let inputs = (
            "local1",
            "remote1",
            "0123456789012345678901",
            [0x5a; 32],
            DtlsRole::Server,
            [0x42; 32],
        );

        // Act
        let config = WebRtcSessionConfig::new(
            inputs.0, inputs.1, inputs.2, inputs.3, inputs.4, inputs.5, 72,
        );

        // Assert
        assert!(matches!(
            config,
            Err(super::WebRtcSessionConfigError::InvalidValue)
        ));
    }

    #[test]
    fn session_config_binds_matter_audio_to_udp_port_50000() {
        // Arrange
        let config = WebRtcSessionConfig::new(
            "local1",
            "remote1",
            "0123456789012345678901",
            [0x5a; 32],
            DtlsRole::Server,
            [0x42; 32],
            111,
        )
        .expect("valid config");

        // Act
        let port = config.local_port;

        // Assert
        assert_eq!(port, 50_000);
    }

    #[test]
    fn media_demux_ignores_rtcp_payload_types_for_opus() {
        // Arrange
        let mut rtp = [0u8; 12];
        let mut rtcp = [0u8; 12];
        rtp[0] = 0x80;
        rtp[1] = 111;
        rtcp[0] = 0x80;
        rtcp[1] = 200;

        // Act
        let rtp_matches = super::matches_payload_type(&rtp, 111);
        let rtcp_matches = super::matches_payload_type(&rtcp, 111);

        // Assert
        assert!(rtp_matches);
        assert!(!rtcp_matches);
    }

    #[test]
    fn session_handle_cancels_a_queued_call_before_session_allocation() {
        // Arrange
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = portable_atomic::AtomicU8::new(SESSION_IDLE);
        let handle = WebRtcSessionHandle {
            sender: requests.sender(),
            state: &state,
        };
        let config = WebRtcSessionConfig::new(
            "local1",
            "remote1",
            "0123456789012345678901",
            [0x5a; 32],
            DtlsRole::Server,
            [0x42; 32],
            111,
        )
        .expect("valid session config");

        // Act
        let started = handle.start(config);
        let busy = handle.start(
            WebRtcSessionConfig::new(
                "local1",
                "remote1",
                "0123456789012345678901",
                [0x5a; 32],
                DtlsRole::Server,
                [0x42; 32],
                111,
            )
            .expect("valid session config"),
        );
        let stopped = handle.stop();

        // Assert
        assert_eq!(started, Ok(()));
        assert_eq!(busy, Err(WebRtcSessionRequestError::Busy));
        assert_eq!(stopped, Ok(()));
        assert_eq!(handle.stop(), Ok(()));
        assert_eq!(
            state.load(portable_atomic::Ordering::Acquire),
            SESSION_STOP_QUEUED
        );
        assert!(requests.receiver().try_receive().is_ok());
    }

    #[test]
    fn session_handle_cancels_active_call() {
        // Arrange
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = portable_atomic::AtomicU8::new(SESSION_ACTIVE);
        let handle = WebRtcSessionHandle {
            sender: requests.sender(),
            state: &state,
        };

        // Act
        let result = handle.stop();

        // Assert
        assert_eq!(result, Ok(()));
        assert_eq!(
            state.load(portable_atomic::Ordering::Acquire),
            SESSION_STOP_ACTIVE
        );
    }

    #[test]
    fn session_handle_reports_idle_only_after_engine_releases_storage() {
        // Arrange
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = portable_atomic::AtomicU8::new(SESSION_STOP_ACTIVE);
        let handle = WebRtcSessionHandle {
            sender: requests.sender(),
            state: &state,
        };

        // Act
        let while_stopping = handle.is_idle();
        state.store(SESSION_IDLE, portable_atomic::Ordering::Release);
        let after_cleanup = handle.is_idle();
        embassy_futures::block_on(handle.wait_idle());

        // Assert
        assert!(!while_stopping);
        assert!(after_cleanup);
    }

    #[test]
    fn benchmark_stats_track_worst_cpu_and_free_heap_samples() {
        // Arrange
        let mut stats = MediaBenchmarkStats::default();

        // Act
        stats.observe_encode(3_000);
        stats.observe_encode(7_500);
        stats.observe_decode(4_200);
        stats.observe_free_heap(80_000);
        stats.observe_free_heap(31_000);

        // Assert
        assert_eq!(
            stats,
            MediaBenchmarkStats {
                max_encode_us: 7_500,
                max_decode_us: 4_200,
                min_free_heap_bytes: 31_000,
                free_heap_sampled: true,
            }
        );
    }

    #[test]
    fn inbound_voice_rtp_switches_to_talk_then_hangover_returns_to_listen() {
        // Arrange
        let mut turn = TurnTakingPolicy::new(400);

        // Act
        let on_voice = turn.on_inbound_rtp(24, 1_000);
        let hold_mid = turn.poll_hangover(1_350);
        let release = turn.poll_hangover(1_400);

        // Assert
        assert_eq!(on_voice, Some(TurnAction::StartTalk));
        assert_eq!(hold_mid, None);
        assert_eq!(release, Some(TurnAction::StopTalk));
    }

    #[test]
    fn voice_rtp_renews_hangover_without_repeating_start_talk() {
        // Arrange
        let mut turn = TurnTakingPolicy::new(400);
        assert_eq!(turn.on_inbound_rtp(24, 1_000), Some(TurnAction::StartTalk));

        // Act
        let continued = turn.on_inbound_rtp(24, 1_300);
        let before_renewed_deadline = turn.poll_hangover(1_699);
        let release = turn.poll_hangover(1_700);

        // Assert
        assert_eq!(continued, None);
        assert_eq!(before_renewed_deadline, None);
        assert_eq!(release, Some(TurnAction::StopTalk));
    }

    #[test]
    fn dtx_payload_does_not_start_talk_or_extend_hangover() {
        // Arrange
        let mut turn = TurnTakingPolicy::new(400);

        // Act
        let action = turn.on_inbound_rtp(OPUS_DTX_MAX_BYTES, 1_000);
        let idle = turn.poll_hangover(1_400);

        // Assert
        assert_eq!(action, None);
        assert_eq!(idle, None);
        assert!(!turn.is_talking());
    }
}
