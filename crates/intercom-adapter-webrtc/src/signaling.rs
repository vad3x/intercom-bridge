//! Bounded Matter-facing SDP signaling for the single-session WebRTC media engine.

use alloc::boxed::Box;
use core::cell::RefCell;
use core::fmt::Write;
use core::sync::atomic::Ordering;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use heapless::String;
use portable_atomic::AtomicU32;

use crate::dtls::{DtlsRole, DtlsSrtpEngine, DtlsSrtpResourceCell, DtlsSrtpResources};
use crate::sdp::{
    parse_sdp_offer, write_sdp_answer, write_sdp_offer, DtlsSetupRole, LocalSdpParams, SdpError,
    SdpMediaDirection, LOCAL_OFFER_MID, LOCAL_OFFER_OPUS_PAYLOAD_TYPE,
};
use crate::session::{
    WebRtcSessionConfig, WebRtcSessionHandle, WebRtcSessionRequestError, MEDIA_UDP_PORT,
};

/// Cryptographically secure entropy supplied by the composition root.
pub trait SecureRandom {
    fn fill(&self, out: &mut [u8]) -> Result<(), SignalingError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalingError {
    NotReady,
    InvalidSdp,
    InvalidCandidate,
    BufferTooSmall,
    Busy,
    InvalidInState,
    Crypto,
}

#[derive(Clone)]
struct PendingOffer {
    local_ufrag: String<32>,
    local_password: String<64>,
    dtls_seed: [u8; 32],
    ssrc: u32,
}

struct SessionMaterial {
    local_ufrag: String<32>,
    local_password: String<64>,
    dtls_seed: [u8; 32],
    ssrc: u32,
}

/// Converts Matter SDP signaling into a single queued media session.
pub struct WebRtcSignalingFacade<'a, R: SecureRandom> {
    handle: WebRtcSessionHandle<'a>,
    random: &'a R,
    dtls_resources: Option<&'a DtlsSrtpResourceCell<'a>>,
    local_ipv4: AtomicU32,
    pending: Mutex<CriticalSectionRawMutex, RefCell<Option<PendingOffer>>>,
}

impl<'a, R: SecureRandom> WebRtcSignalingFacade<'a, R> {
    pub const fn new(handle: WebRtcSessionHandle<'a>, random: &'a R) -> Self {
        Self {
            handle,
            random,
            dtls_resources: None,
            local_ipv4: AtomicU32::new(0),
            pending: Mutex::new(RefCell::new(None)),
        }
    }

    /// Construct signaling that shares DTLS storage with the media engine.
    pub const fn new_with_resources(
        handle: WebRtcSessionHandle<'a>,
        random: &'a R,
        dtls_resources: &'a DtlsSrtpResourceCell<'a>,
    ) -> Self {
        Self {
            handle,
            random,
            dtls_resources: Some(dtls_resources),
            local_ipv4: AtomicU32::new(0),
            pending: Mutex::new(RefCell::new(None)),
        }
    }

    pub fn set_local_ipv4(&self, address: [u8; 4]) {
        self.local_ipv4
            .store(u32::from_be_bytes(address), Ordering::Release);
    }

    fn local_ipv4(&self) -> Result<[u8; 4], SignalingError> {
        let address = self.local_ipv4.load(Ordering::Acquire).to_be_bytes();
        if address == [0; 4] {
            Err(SignalingError::NotReady)
        } else {
            Ok(address)
        }
    }

    fn new_material(&self) -> Result<SessionMaterial, SignalingError> {
        let mut random = [0u8; 60];
        self.random.fill(&mut random)?;
        let mut local_ufrag = String::<32>::new();
        let mut local_password = String::<64>::new();
        for byte in &random[32..40] {
            write!(local_ufrag, "{byte:02x}").map_err(|_| SignalingError::Crypto)?;
        }
        for byte in &random[40..56] {
            write!(local_password, "{byte:02x}").map_err(|_| SignalingError::Crypto)?;
        }
        let mut dtls_seed = [0u8; 32];
        dtls_seed.copy_from_slice(&random[..32]);
        Ok(SessionMaterial {
            local_ufrag,
            local_password,
            dtls_seed,
            ssrc: u32::from_be_bytes(
                random[56..60]
                    .try_into()
                    .map_err(|_| SignalingError::Crypto)?,
            ),
        })
    }

    fn local_fingerprint(
        &self,
        seed: [u8; 32],
        role: DtlsRole,
    ) -> Result<[u8; 32], SignalingError> {
        // The media engine uses the same seed to recreate this certificate when it starts.
        let fingerprint = if let Some(resources) = self.dtls_resources {
            let mut resources = resources.try_borrow_mut().ok_or(SignalingError::Busy)?;
            let identity = DtlsSrtpEngine::new(role, seed, &mut resources)
                .map_err(|_| SignalingError::Crypto)?;
            let fingerprint = identity.local_cert_sha256();
            drop(identity);
            fingerprint
        } else {
            let mut resources = Box::new(DtlsSrtpResources::new());
            let identity = DtlsSrtpEngine::new(role, seed, &mut resources)
                .map_err(|_| SignalingError::Crypto)?;
            let fingerprint = identity.local_cert_sha256();
            drop(identity);
            fingerprint
        };
        Ok(fingerprint)
    }

    fn local_params<'b>(
        ipv4: [u8; 4],
        material: &'b SessionMaterial,
        fingerprint: [u8; 32],
    ) -> LocalSdpParams<'b> {
        LocalSdpParams {
            ipv4,
            ipv6: [0; 8],
            port: MEDIA_UDP_PORT,
            ice_ufrag: &material.local_ufrag,
            ice_pwd: &material.local_password,
            sha256_fingerprint: fingerprint,
            ssrc: material.ssrc,
        }
    }

    pub fn accept_offer(
        &self,
        offer: &str,
        answer_out: &mut [u8],
    ) -> Result<usize, SignalingError> {
        let ipv4 = self.local_ipv4()?;
        if !self.handle.is_idle() || self.pending.lock(|p| p.borrow().is_some()) {
            return Err(SignalingError::Busy);
        }
        let remote = parse_sdp_offer(offer).map_err(map_sdp_error)?;
        let role = match remote.setup {
            DtlsSetupRole::Passive => DtlsRole::Client,
            DtlsSetupRole::Active | DtlsSetupRole::ActPass => DtlsRole::Server,
        };
        let material = self.new_material()?;
        let fingerprint = self.local_fingerprint(material.dtls_seed, role)?;
        let params = Self::local_params(ipv4, &material, fingerprint);
        let len = write_sdp_answer(&remote, &params, answer_out).map_err(map_sdp_error)?;
        let config = WebRtcSessionConfig::new(
            &material.local_ufrag,
            remote.ice_ufrag,
            &material.local_password,
            remote.sha256_fingerprint,
            role,
            material.dtls_seed,
            material.ssrc,
            remote.opus_payload_type,
        )
        .map_err(|_| SignalingError::InvalidSdp)?;
        self.handle.start(config).map_err(map_request_error)?;
        Ok(len)
    }

    pub fn create_offer(&self, offer_out: &mut [u8]) -> Result<usize, SignalingError> {
        let ipv4 = self.local_ipv4()?;
        if !self.handle.is_idle() || self.pending.lock(|p| p.borrow().is_some()) {
            return Err(SignalingError::Busy);
        }
        let material = self.new_material()?;
        let fingerprint = self.local_fingerprint(material.dtls_seed, DtlsRole::Server)?;
        let params = Self::local_params(ipv4, &material, fingerprint);
        let len = write_sdp_offer(&params, offer_out).map_err(map_sdp_error)?;
        self.pending.lock(|p| {
            *p.borrow_mut() = Some(PendingOffer {
                local_ufrag: material.local_ufrag,
                local_password: material.local_password,
                dtls_seed: material.dtls_seed,
                ssrc: material.ssrc,
            });
        });
        Ok(len)
    }

    pub fn accept_answer(&self, answer: &str) -> Result<(), SignalingError> {
        let remote = parse_sdp_offer(answer).map_err(map_sdp_error)?;
        if remote.mid != LOCAL_OFFER_MID
            || remote.opus_payload_type != LOCAL_OFFER_OPUS_PAYLOAD_TYPE
            || remote.direction != SdpMediaDirection::SendRecv
        {
            return Err(SignalingError::InvalidSdp);
        }
        let role = match remote.setup {
            DtlsSetupRole::Active => DtlsRole::Server,
            DtlsSetupRole::Passive => DtlsRole::Client,
            DtlsSetupRole::ActPass => return Err(SignalingError::InvalidSdp),
        };
        self.pending.lock(|pending| {
            let pending = pending.borrow();
            let local = pending.as_ref().ok_or(SignalingError::InvalidInState)?;
            let config = WebRtcSessionConfig::new(
                &local.local_ufrag,
                remote.ice_ufrag,
                &local.local_password,
                remote.sha256_fingerprint,
                role,
                local.dtls_seed,
                local.ssrc,
                remote.opus_payload_type,
            )
            .map_err(|_| SignalingError::InvalidSdp)?;
            self.handle.start(config).map_err(map_request_error)
        })?;
        self.pending.lock(|pending| *pending.borrow_mut() = None);
        Ok(())
    }

    pub fn add_candidate(&self, candidate: &str) -> Result<(), SignalingError> {
        if self.handle.is_idle() && !self.pending.lock(|p| p.borrow().is_some()) {
            return Err(SignalingError::InvalidInState);
        }
        validate_remote_candidate(candidate)
    }

    pub fn stop(&self) {
        self.pending.lock(|pending| *pending.borrow_mut() = None);
        let _ = self.handle.stop();
    }

    pub fn is_idle(&self) -> bool {
        self.handle.is_idle()
    }

    pub async fn wait_idle(&self) {
        self.handle.wait_idle().await;
    }
}

fn map_sdp_error(error: SdpError) -> SignalingError {
    match error {
        SdpError::BufferTooSmall => SignalingError::BufferTooSmall,
        _ => SignalingError::InvalidSdp,
    }
}

fn map_request_error(error: WebRtcSessionRequestError) -> SignalingError {
    match error {
        WebRtcSessionRequestError::Busy => SignalingError::Busy,
        WebRtcSessionRequestError::NotActive => SignalingError::InvalidInState,
    }
}

/// ICE-Lite accepts candidate signaling but chooses its peer only from authenticated STUN.
pub fn validate_remote_candidate(candidate: &str) -> Result<(), SignalingError> {
    let candidate = candidate.strip_prefix("a=").unwrap_or(candidate);
    let fields = candidate
        .strip_prefix("candidate:")
        .ok_or(SignalingError::InvalidCandidate)?;
    let mut parts = fields.split_ascii_whitespace();
    let foundation = parts.next().ok_or(SignalingError::InvalidCandidate)?;
    let component = parts.next().and_then(|v| v.parse::<u8>().ok());
    let transport = parts.next();
    let priority = parts.next().and_then(|v| v.parse::<u32>().ok());
    let address = parts.next().ok_or(SignalingError::InvalidCandidate)?;
    let port = parts.next().and_then(|v| v.parse::<u16>().ok());
    let typ = parts.next();
    let kind = parts.next();
    if foundation.is_empty()
        || component != Some(1)
        || !transport.is_some_and(|v| v.eq_ignore_ascii_case("UDP"))
        || priority.is_none()
        || address.is_empty()
        || port.is_none_or(|v| v == 0)
        || typ != Some("typ")
        || !matches!(kind, Some("host" | "srflx" | "prflx" | "relay"))
    {
        return Err(SignalingError::InvalidCandidate);
    }
    Ok(())
}

#[cfg(feature = "matter-port")]
mod matter_port {
    use super::{SecureRandom, SignalingError, WebRtcSignalingFacade};
    use intercom_adapter_matter::MediaSessionPort;
    use intercom_adapter_matter::WebRtcError;

    fn map_error(error: SignalingError) -> WebRtcError {
        match error {
            SignalingError::NotReady | SignalingError::InvalidInState => {
                WebRtcError::InvalidInState
            }
            SignalingError::InvalidSdp | SignalingError::InvalidCandidate => {
                WebRtcError::InvalidCommand
            }
            SignalingError::BufferTooSmall | SignalingError::Busy => WebRtcError::ResourceExhausted,
            SignalingError::Crypto => WebRtcError::Failure,
        }
    }

    impl<R: SecureRandom> MediaSessionPort for WebRtcSignalingFacade<'_, R> {
        fn accept_remote_offer(
            &self,
            offer_sdp: &str,
            answer_out: &mut [u8],
        ) -> Result<usize, WebRtcError> {
            self.accept_offer(offer_sdp, answer_out).map_err(map_error)
        }

        fn build_local_offer(&self, offer_out: &mut [u8]) -> Result<usize, WebRtcError> {
            self.create_offer(offer_out).map_err(map_error)
        }

        fn accept_remote_answer(&self, answer_sdp: &str) -> Result<(), WebRtcError> {
            self.accept_answer(answer_sdp).map_err(map_error)
        }

        fn add_remote_ice_candidate(&self, candidate_sdp: &str) -> Result<(), WebRtcError> {
            self.add_candidate(candidate_sdp).map_err(map_error)
        }

        fn stop_media_session(&self) {
            self.stop();
        }

        async fn wait_media_idle(&self) {
            self.wait_idle().await;
        }

        fn is_media_idle(&self) -> bool {
            self.is_idle()
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use intercom_core::ports::inbound::{AccessControllerHandle, CommandChannel};
    use intercom_media::{RxAudioChannel, TxAudioChannel};
    use portable_atomic::AtomicU8;

    use super::{validate_remote_candidate, SecureRandom, SignalingError, WebRtcSignalingFacade};
    use crate::dtls::DtlsSrtpResources;
    use crate::session::{WebRtcMediaEngine, WebRtcSessionConfig};

    const CONTROLLER_OFFER: &str = "v=0\r\n\
o=- 1234 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=ice-ufrag:ctrlUfrag\r\n\
a=ice-pwd:ctrlPassword0123456789abcdef\r\n\
a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99\r\n\
a=setup:actpass\r\n\
a=rtcp-mux\r\n\
a=rtpmap:111 opus/48000/2\r\n";

    const HOME_ASSISTANT_OFFER: &str = "v=0\r\n\
o=- 381697197521272464 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111 63 9 0 8 13 110 126\r\n\
c=IN IP4 0.0.0.0\r\n\
a=rtcp:9 IN IP4 0.0.0.0\r\n\
a=ice-ufrag:ctrlUfrag\r\n\
a=ice-pwd:controllerPassword0123456789ab\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 0A:19:E8:17:F6:7D:4F:32:D9:9D:09:30:4A:D4:C5:E6:B8:C1:A1:A4:7A:5C:E7:44:6C:D5:47:D0:D2:05:2D:E4\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n\
a=extmap:1 urn:ietf:params:rtp-hdrext:ssrc-audio-level\r\n\
a=extmap:2 http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time\r\n\
a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01\r\n\
a=extmap:4 urn:ietf:params:rtp-hdrext:sdes:mid\r\n\
a=sendrecv\r\n\
a=msid:- 621af9a1-27a2-4ec8-9433-d9f87063ac7f\r\n\
a=rtcp-mux\r\n\
a=rtcp-rsize\r\n\
a=rtcp-xr:rcvr-rtt=all\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=rtcp-fb:111 transport-cc\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=rtpmap:63 red/48000/2\r\n\
a=fmtp:63 111/111\r\n\
a=rtpmap:9 G722/8000\r\n\
a=rtpmap:0 PCMU/8000\r\n\
a=rtpmap:8 PCMA/8000\r\n\
a=rtpmap:13 CN/8000\r\n\
a=rtpmap:110 telephone-event/48000\r\n\
a=rtpmap:126 telephone-event/8000\r\n\
a=ssrc:3210117597 cname:DNnSCHWCW0hzfTB4\r\n\
a=ssrc:3210117597 msid:- 621af9a1-27a2-4ec8-9433-d9f87063ac7f\r\n";

    struct SecureRandomMock(Cell<u8>);

    impl SecureRandom for SecureRandomMock {
        fn fill(&self, out: &mut [u8]) -> Result<(), SignalingError> {
            let next = self.0.get().wrapping_add(1);
            self.0.set(next);
            out.fill(next);
            Ok(())
        }
    }

    #[test]
    fn offer_rejects_until_local_ipv4_is_ready_without_queuing_media() {
        // Arrange
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        let mut answer = [0u8; 2048];

        // Act
        let result = facade.accept_offer("v=0\r\n", &mut answer);

        // Assert
        assert_eq!(result, Err(SignalingError::NotReady));
        assert!(handle.is_idle());
    }

    #[test]
    fn accepted_offer_queues_media_with_fresh_credentials_and_matching_sdp() {
        // Arrange
        let _guard = crate::dtls::DTLS_TEST_LOCK.lock().expect("DTLS lock");
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        facade.set_local_ipv4([192, 168, 1, 50]);
        let mut answer = [0u8; 2048];

        // Act
        let len = facade
            .accept_offer(CONTROLLER_OFFER, &mut answer)
            .expect("answer");
        let config = requests.receiver().try_receive().expect("media queued");
        assert!(dtls_resources.try_borrow_mut().is_some());
        let answer = core::str::from_utf8(&answer[..len]).expect("UTF-8 SDP");

        // Assert
        assert!(answer.contains("c=IN IP4 192.168.1.50"));
        assert!(answer.contains("a=ice-ufrag:"));
        assert_eq!(config.remote_ufrag.as_str(), "ctrlUfrag");
        assert_eq!(config.payload_type, 111);
        let answer_ssrc = answer
            .lines()
            .find_map(|line| {
                line.strip_prefix("a=ssrc:")?
                    .split_ascii_whitespace()
                    .next()?
                    .parse::<u32>()
                    .ok()
            })
            .expect("answer should advertise the local RTP SSRC");
        assert_eq!(config.local_ssrc, answer_ssrc);
        assert!(!handle.is_idle());
    }

    #[test]
    fn accepts_home_assistant_live_view_offer() {
        let _guard = crate::dtls::DTLS_TEST_LOCK.lock().expect("DTLS lock");
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        facade.set_local_ipv4([192, 168, 0, 101]);
        let mut answer = [0u8; 2048];

        crate::sdp::parse_sdp_offer(HOME_ASSISTANT_OFFER).expect("Home Assistant SDP should parse");

        let len = facade
            .accept_offer(HOME_ASSISTANT_OFFER, &mut answer)
            .expect("Home Assistant offer should be accepted");

        assert!(len > 0);
        assert!(requests.receiver().try_receive().is_ok());
    }

    #[test]
    fn short_answer_buffer_rejects_offer_without_queuing_media() {
        // Arrange
        let _guard = crate::dtls::DTLS_TEST_LOCK.lock().expect("DTLS lock");
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        facade.set_local_ipv4([192, 168, 1, 50]);
        let mut answer = [0u8; 4];

        // Act
        let result = facade.accept_offer(CONTROLLER_OFFER, &mut answer);

        // Assert
        assert_eq!(result, Err(SignalingError::BufferTooSmall));
        assert!(handle.is_idle());
        assert!(requests.receiver().try_receive().is_err());
    }

    #[test]
    fn local_offer_starts_media_only_after_valid_remote_answer() {
        // Arrange
        let _guard = crate::dtls::DTLS_TEST_LOCK.lock().expect("DTLS lock");
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        facade.set_local_ipv4([192, 168, 1, 50]);
        let mut offer = [0u8; 2048];

        // Act
        let len = facade.create_offer(&mut offer).expect("local offer");
        let before_answer = requests.receiver().try_receive();
        let answer = CONTROLLER_OFFER.replace("a=setup:actpass", "a=setup:active");
        let accepted = facade.accept_answer(&answer);
        let config = requests.receiver().try_receive().expect("media queued");

        // Assert
        assert!(core::str::from_utf8(&offer[..len])
            .unwrap()
            .contains("a=setup:actpass"));
        assert!(before_answer.is_err());
        assert_eq!(accepted, Ok(()));
        assert_eq!(config.local_dtls_role, crate::dtls::DtlsRole::Server);
    }

    #[test]
    fn candidate_validation_accepts_udp_host_and_rejects_malformed_candidates() {
        // Arrange
        let valid = "candidate:1 1 UDP 2130706431 192.168.1.50 50000 typ host";
        let missing_port = "candidate:1 1 UDP 2130706431 192.168.1.50 typ host";

        // Act
        let accepted = validate_remote_candidate(valid);
        let rejected = validate_remote_candidate(missing_port);

        // Assert
        assert_eq!(accepted, Ok(()));
        assert_eq!(rejected, Err(SignalingError::InvalidCandidate));
    }

    struct AnswerRetryObservation {
        rejected: Result<(), SignalingError>,
        queued_after_rejection: bool,
        retry: Result<(), SignalingError>,
        queued_after_retry: bool,
    }

    fn try_invalid_answer_then_retry(invalid_answer: &str) -> AnswerRetryObservation {
        let _guard = crate::dtls::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let commands = CommandChannel::new();
        let requests = Channel::<CriticalSectionRawMutex, WebRtcSessionConfig, 1>::new();
        let state = AtomicU8::new(0);
        let rx = RxAudioChannel::new();
        let tx = TxAudioChannel::new();
        let mut dtls_storage = DtlsSrtpResources::new();
        let dtls_resources = crate::dtls::DtlsSrtpResourceCell::new(&mut dtls_storage);
        let mut resources = crate::session::WebRtcMediaResources::new();
        let (_engine, handle) = WebRtcMediaEngine::new(
            &requests,
            &state,
            AccessControllerHandle::new(commands.sender()),
            rx.receiver(),
            tx.sender(),
            &mut resources,
            &dtls_resources,
        );
        let random = SecureRandomMock(Cell::new(0));
        let facade = WebRtcSignalingFacade::new_with_resources(handle, &random, &dtls_resources);
        facade.set_local_ipv4([192, 168, 1, 50]);
        let mut offer = [0u8; 2048];
        facade.create_offer(&mut offer).expect("local offer");

        // Act
        let rejected = facade.accept_answer(invalid_answer);
        let queued_after_rejection = requests.receiver().try_receive().is_ok();
        let valid_answer = CONTROLLER_OFFER.replace("a=setup:actpass", "a=setup:active");
        let retry = facade.accept_answer(&valid_answer);
        let queued_after_retry = requests.receiver().try_receive().is_ok();

        AnswerRetryObservation {
            rejected,
            queued_after_rejection,
            retry,
            queued_after_retry,
        }
    }

    #[test]
    fn answer_with_wrong_mid_is_rejected_without_losing_pending_offer() {
        // Arrange
        let answer = CONTROLLER_OFFER
            .replace("a=setup:actpass", "a=setup:active")
            .replace("a=mid:0", "a=mid:1");

        // Act
        let observation = try_invalid_answer_then_retry(&answer);

        // Assert
        assert_eq!(observation.rejected, Err(SignalingError::InvalidSdp));
        assert!(!observation.queued_after_rejection);
        assert_eq!(observation.retry, Ok(()));
        assert!(observation.queued_after_retry);
    }

    #[test]
    fn answer_with_wrong_opus_payload_type_is_rejected_without_queuing_media() {
        // Arrange
        let answer = CONTROLLER_OFFER
            .replace("a=setup:actpass", "a=setup:active")
            .replace("SAVPF 111", "SAVPF 112")
            .replace("rtpmap:111", "rtpmap:112");

        // Act
        let observation = try_invalid_answer_then_retry(&answer);

        // Assert
        assert_eq!(observation.rejected, Err(SignalingError::InvalidSdp));
        assert!(!observation.queued_after_rejection);
        assert_eq!(observation.retry, Ok(()));
        assert!(observation.queued_after_retry);
    }

    #[test]
    fn answer_with_actpass_role_is_rejected_and_can_be_retried() {
        // Arrange
        let answer = CONTROLLER_OFFER;

        // Act
        let observation = try_invalid_answer_then_retry(answer);

        // Assert
        assert_eq!(observation.rejected, Err(SignalingError::InvalidSdp));
        assert!(!observation.queued_after_rejection);
        assert_eq!(observation.retry, Ok(()));
        assert!(observation.queued_after_retry);
    }

    #[test]
    fn inactive_answer_is_rejected_for_two_way_audio() {
        // Arrange
        let answer =
            CONTROLLER_OFFER.replace("a=setup:actpass", "a=setup:active") + "a=inactive\r\n";

        // Act
        let observation = try_invalid_answer_then_retry(&answer);

        // Assert
        assert_eq!(observation.rejected, Err(SignalingError::InvalidSdp));
        assert!(!observation.queued_after_rejection);
        assert_eq!(observation.retry, Ok(()));
        assert!(observation.queued_after_retry);
    }
}
