//! Endpoint 1 Matter 1.5 Audio Doorbell AV Stream (`0x0551`) & WebRTC Provider (`0x0553`) adapters.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU16, Ordering};

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::signal::Signal;
use heapless::Vec;
use intercom_core::domain::intercom::SessionOwner;
use intercom_core::domain::policy::{
    MAX_RX_PGA_HALF_DB, MAX_TX_DAC_HALF_DB, MIN_RX_PGA_HALF_DB, MIN_TX_DAC_HALF_DB, RxPgaGain,
    TxDacGain,
};
use intercom_core::ports::inbound::IntercomControllerHandle;
use log::{info, warn};
use rs_matter::dm::clusters::app::audio_av_stream::{
    AllocatedAudioStream, AudioAvStreamHandler, AudioAvStreamHooks, AudioEndpointCapabilities,
    HandlerAsyncAdaptor as AudioAvAdaptor,
};
use rs_matter::dm::clusters::app::webrtc_prov::{
    AnswerOutcome, IceCandidateSink, OfferParams, OutboundWork, SolicitOutcome, WebRtcError,
    WebRtcHooks, WebRtcProvHandler,
};
use rs_matter::dm::clusters::decl::globals::{ICECandidateStruct, WebRTCEndReasonEnum};
use rs_matter::dm::clusters::decl::{web_rtc_transport_provider, web_rtc_transport_requestor};
use rs_matter::dm::devices::DEV_TYPE_AUDIO_DOORBELL;
use rs_matter::dm::{
    AsyncHandler, Cluster, ClusterId, Dataver, DeviceType, HandlerContext, InvokeContext,
    InvokeReply, LifecycleOp, MatchContext, ReadContext, ReadReply, WriteContext,
};
use rs_matter::error::Error;
use rs_matter::tlv::TLVArray;

use crate::MatterCluster;
use crate::config::AUDIO_DOORBELL_ENDPOINT_ID;

pub const MAX_SDP_BYTES: usize = 2048;

/// Enforces `MAX_ACTIVE_MEDIA_SESSIONS = 1` across the `MinLimit = 3` Matter WebRTC session slots.
pub struct ActiveCallGate {
    active_session_id: AtomicU16,
}

impl ActiveCallGate {
    pub const fn new() -> Self {
        Self {
            active_session_id: AtomicU16::new(0),
        }
    }

    pub fn try_acquire(&self, session_id: u16) -> Result<(), WebRtcError> {
        if session_id == 0 {
            return Err(WebRtcError::InvalidCommand);
        }
        let current = self.active_session_id.load(Ordering::Acquire);
        if current == session_id {
            return Err(WebRtcError::InvalidInState);
        }
        self.active_session_id
            .compare_exchange(0, session_id, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| WebRtcError::ResourceExhausted)
    }

    pub fn release(&self, session_id: u16) -> bool {
        if session_id == 0 {
            return false;
        }
        self.active_session_id
            .compare_exchange(session_id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn active_session(&self) -> Option<u16> {
        let id = self.active_session_id.load(Ordering::Acquire);
        if id == 0 { None } else { Some(id) }
    }
}

impl Default for ActiveCallGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps Matter `0..=100` microphone volume + mute to `RxPgaGain` (`0..=95` half-dB).
pub const fn volume_percent_to_rx_gain(muted: bool, level: u8) -> RxPgaGain {
    if muted || level == 0 {
        return RxPgaGain::clamped(MIN_RX_PGA_HALF_DB);
    }
    let clamped = if level > 100 { 100 } else { level as i32 };
    let span = (MAX_RX_PGA_HALF_DB as i32) - (MIN_RX_PGA_HALF_DB as i32);
    let half_db = (MIN_RX_PGA_HALF_DB as i32) + ((clamped * span) / 100);
    RxPgaGain::clamped(half_db as u8)
}

/// Maps Matter `0..=100` speaker volume + mute to `TxDacGain` (`-127..=48` half-dB).
pub const fn volume_percent_to_tx_gain(muted: bool, level: u8) -> TxDacGain {
    if muted || level == 0 {
        return TxDacGain::clamped(MIN_TX_DAC_HALF_DB);
    }
    let clamped = if level > 100 { 100 } else { level as i32 };
    let span = (MAX_TX_DAC_HALF_DB as i32) - (MIN_TX_DAC_HALF_DB as i32);
    let half_db = (MIN_TX_DAC_HALF_DB as i32) + ((clamped * span) / 100);
    TxDacGain::clamped(half_db as i8)
}

/// Bridges `AudioAvStreamHandler` volume/mute attributes to `IntercomControllerHandle::set_audio_gain`.
pub struct IntercomAudioHooks<'a> {
    intercom: IntercomControllerHandle<'a>,
    last_allocated_stream_id: AtomicU16,
}

impl<'a> IntercomAudioHooks<'a> {
    pub const fn new(intercom: IntercomControllerHandle<'a>) -> Self {
        Self {
            intercom,
            last_allocated_stream_id: AtomicU16::new(0),
        }
    }
}

impl AudioAvStreamHooks for IntercomAudioHooks<'_> {
    fn on_audio_allocated(&self, stream: &AllocatedAudioStream) -> Result<(), Error> {
        info!(
            "AudioAV: on_audio_allocated stream_id={}, codec={:?}, rate={}, bitrate={}",
            stream.audio_stream_id, stream.audio_codec, stream.sample_rate, stream.bit_rate
        );
        self.last_allocated_stream_id
            .store(stream.audio_stream_id, Ordering::Release);
        Ok(())
    }

    fn on_audio_deallocated(&self, stream_id: u16) {
        info!("AudioAV: on_audio_deallocated stream_id={}", stream_id);
        let _ = self.last_allocated_stream_id.compare_exchange(
            stream_id,
            0,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
    }

    fn on_volume_or_mute_changed(
        &self,
        mic_muted: bool,
        mic_volume_level: u8,
        speaker_muted: bool,
        speaker_volume_level: u8,
    ) {
        let rx = volume_percent_to_rx_gain(mic_muted, mic_volume_level);
        let tx = volume_percent_to_tx_gain(speaker_muted, speaker_volume_level);
        self.intercom.set_audio_gain(rx, tx);
    }
}

/// Endpoint 1 `CameraAVStreamManagement` (`0x0551`) cluster wrapper.
pub struct IntercomAudioAvCluster<'a>(
    AudioAvAdaptor<AudioAvStreamHandler<IntercomAudioHooks<'a>, 2>>,
);

impl<'a> IntercomAudioAvCluster<'a> {
    pub const CLUSTER: Cluster<'static> =
        AudioAvStreamHandler::<IntercomAudioHooks<'static>, 2>::CLUSTER;

    pub const fn new(intercom: IntercomControllerHandle<'a>) -> Self {
        Self(
            AudioAvStreamHandler::new(
                Dataver::new(30),
                AudioEndpointCapabilities::opus_16khz_mono(),
                IntercomAudioHooks::new(intercom),
            )
            .adapt(),
        )
    }

    pub const fn handler(&self) -> &AudioAvStreamHandler<IntercomAudioHooks<'a>, 2> {
        &self.0.0
    }

    pub fn last_allocated_stream(&self) -> Option<u16> {
        let id = self.0.0.hooks().last_allocated_stream_id.load(Ordering::Acquire);
        if id != 0 {
            Some(id)
        } else {
            None
        }
    }
}

impl MatterCluster for IntercomAudioAvCluster<'_> {
    const ENDPOINT_ID: u16 = AUDIO_DOORBELL_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    const DEVICE_TYPE: DeviceType = DEV_TYPE_AUDIO_DOORBELL;
}

impl AsyncHandler for IntercomAudioAvCluster<'_> {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.0.read(ctx, reply).await
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.0.write(ctx).await
    }

    async fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.0.invoke(ctx, reply).await
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        self.0.bump_dataver(ctx);
    }

    fn lifecycle(&self, ctx: impl HandlerContext, op: LifecycleOp) -> Result<(), Error> {
        self.0.lifecycle(ctx, op)
    }

    fn run(
        &self,
        ctx: impl HandlerContext,
    ) -> impl core::future::Future<Output = Result<(), Error>> {
        self.0.run(ctx)
    }
}

/// Sans-IO media signalling port decoupling `IntercomWebRtcHooks` from concrete UDP sockets in unit tests.
pub trait MediaSessionPort {
    fn accept_remote_offer(
        &self,
        offer_sdp: &str,
        answer_out: &mut [u8],
    ) -> Result<usize, WebRtcError>;
    fn build_local_offer(&self, offer_out: &mut [u8]) -> Result<usize, WebRtcError>;
    fn accept_remote_answer(&self, answer_sdp: &str) -> Result<(), WebRtcError>;
    fn add_remote_ice_candidate(&self, candidate_sdp: &str) -> Result<(), WebRtcError>;
    fn stop_media_session(&self);
    #[allow(async_fn_in_trait)]
    async fn wait_media_idle(&self);
    fn is_media_idle(&self) -> bool;
}

/// Bridges `WebRtcProvHandler` (`0x0553`) to `AudioAvStreamHandler`, `IntercomControllerHandle`, and `MediaSessionPort`.
pub struct IntercomWebRtcHooks<'a, M: MediaSessionPort> {
    intercom: IntercomControllerHandle<'a>,
    audio_av: &'a IntercomAudioAvCluster<'a>,
    media: &'a M,
    gate: ActiveCallGate,
    state: Mutex<CriticalSectionRawMutex, RefCell<SignallingScratch>>,
    outbound_signal: Signal<NoopRawMutex, ()>,
}

impl<'a, M: MediaSessionPort> IntercomWebRtcHooks<'a, M> {
    pub const fn new(
        intercom: IntercomControllerHandle<'a>,
        audio_av: &'a IntercomAudioAvCluster<'a>,
        media: &'a M,
    ) -> Self {
        Self {
            intercom,
            audio_av,
            media,
            gate: ActiveCallGate::new(),
            state: Mutex::new(RefCell::new(SignallingScratch::new())),
            outbound_signal: Signal::new(),
        }
    }

    fn resolve_audio_stream_id(&self, params: &OfferParams) -> Result<u16, WebRtcError> {
        if let Some(Some(_)) = params.video_stream_id {
            info!("WebRTC: video requested on audio-only doorbell, proceeding without video");
        }
        if let Some(Some(id)) = params.audio_stream_id {
            info!("WebRTC: using requested audio_stream_id={}", id);
            return Ok(id);
        }
        if let Some(id) = self.audio_av.last_allocated_stream() {
            info!("WebRTC: fallback to last allocated audio_stream_id={}", id);
            return Ok(id);
        }
        warn!("WebRTC: no audio stream specified and none allocated");
        Err(WebRtcError::InvalidCommand)
    }

    pub fn active_session(&self) -> Option<u16> {
        self.gate.active_session()
    }

    async fn rollback_offer(&self, session_id: u16, audio_id: u16) {
        self.media.stop_media_session();
        self.media.wait_media_idle().await;
        self.state.lock(|cell| {
            let mut state = cell.borrow_mut();
            state.sdp_len = 0;
            state.media_started = false;
            state.outbound_queue.clear();
            state.bound_audio_stream_id = None;
        });
        let _ = self.audio_av.handler().release_audio(audio_id);
        self.gate.release(session_id);
    }

    async fn cleanup_session(&self, session_id: u16) -> bool {
        if self.gate.active_session() != Some(session_id) {
            return false;
        }
        self.media.stop_media_session();
        self.media.wait_media_idle().await;
        let bound_audio = self.state.lock(|cell| {
            let mut s = cell.borrow_mut();
            s.sdp_len = 0;
            s.media_started = false;
            s.outbound_queue.clear();
            s.bound_audio_stream_id.take()
        });
        if let Some(audio_id) = bound_audio {
            let _ = self.audio_av.handler().release_audio(audio_id);
        }
        self.intercom
            .end_session_wait(SessionOwner::MatterCall)
            .await;
        self.gate.release(session_id)
    }
}

impl<M: MediaSessionPort> WebRtcHooks for IntercomWebRtcHooks<'_, M> {
    async fn on_solicit_offer(
        &self,
        session_id: u16,
        params: &OfferParams,
    ) -> Result<SolicitOutcome, WebRtcError> {
        info!(
            "WebRTC: on_solicit_offer session={}, usage={:?}, orig_ep={}, audio={:?}, video={:?}",
            session_id, params.stream_usage, params.originating_endpoint_id, params.audio_stream_id, params.video_stream_id
        );
        let audio_id = self.resolve_audio_stream_id(params)?;
        self.gate.try_acquire(session_id)?;
        if self.audio_av.handler().acquire_audio(audio_id).is_err() {
            warn!("WebRTC: acquire_audio failed for stream {}", audio_id);
            self.gate.release(session_id);
            return Err(WebRtcError::DynamicConstraint);
        }

        let mut offer_buf = [0u8; MAX_SDP_BYTES];
        let offer_len = match self.media.build_local_offer(&mut offer_buf) {
            Ok(len) => len,
            Err(err) => {
                warn!("WebRTC: build_local_offer failed: {:?}", err);
                self.rollback_offer(session_id, audio_id).await;
                return Err(err);
            }
        };

        if offer_len == 0 || offer_len > offer_buf.len() {
            warn!("WebRTC: invalid offer length {}", offer_len);
            self.rollback_offer(session_id, audio_id).await;
            return Err(WebRtcError::ResourceExhausted);
        }

        info!(
            "WebRTC: local offer created ({} bytes), enqueuing outbound offer for session {}",
            offer_len, session_id
        );
        let queued = self.state.lock(|cell| {
            let mut s = cell.borrow_mut();
            s.outbound_queue.push(OutboundWork::Offer { session_id })?;
            s.bound_audio_stream_id = Some(audio_id);
            s.sdp_buf[..offer_len].copy_from_slice(&offer_buf[..offer_len]);
            s.sdp_len = offer_len;
            Ok::<(), OutboundWork>(())
        });
        if queued.is_err() {
            warn!("WebRTC: outbound queue full for session {}", session_id);
            self.rollback_offer(session_id, audio_id).await;
            return Err(WebRtcError::ResourceExhausted);
        }
        self.outbound_signal.signal(());

        Ok(SolicitOutcome {
            deferred: false,
            video_stream_id: None,
            audio_stream_id: Some(audio_id),
        })
    }

    async fn on_offer(
        &self,
        session_id: u16,
        sdp: &str,
        params: &OfferParams,
    ) -> Result<AnswerOutcome, WebRtcError> {
        info!(
            "WebRTC: on_offer session={}, sdp_len={}, audio={:?}, video={:?}",
            session_id, sdp.len(), params.audio_stream_id, params.video_stream_id
        );
        let audio_id = self.resolve_audio_stream_id(params)?;
        self.gate.try_acquire(session_id)?;
        if self.audio_av.handler().acquire_audio(audio_id).is_err() {
            warn!("WebRTC: acquire_audio failed for stream {}", audio_id);
            self.gate.release(session_id);
            return Err(WebRtcError::DynamicConstraint);
        }

        let mut answer_buf = [0u8; MAX_SDP_BYTES];
        let answer_len = match self.media.accept_remote_offer(sdp, &mut answer_buf) {
            Ok(len) => len,
            Err(err) => {
                warn!("WebRTC: accept_remote_offer failed: {:?}", err);
                self.rollback_offer(session_id, audio_id).await;
                return Err(err);
            }
        };

        if answer_len == 0 || answer_len > answer_buf.len() {
            warn!("WebRTC: invalid answer length {}", answer_len);
            self.rollback_offer(session_id, audio_id).await;
            return Err(WebRtcError::ResourceExhausted);
        }

        // Security invariant: ONLY starts Listen on Endpoint 1; NEVER triggers Unlock!
        let queued = self.state.lock(|cell| {
            let mut s = cell.borrow_mut();
            s.outbound_queue.push(OutboundWork::Answer { session_id })?;
            s.bound_audio_stream_id = Some(audio_id);
            s.media_started = true;
            s.sdp_buf[..answer_len].copy_from_slice(&answer_buf[..answer_len]);
            s.sdp_len = answer_len;
            Ok::<(), OutboundWork>(())
        });
        if queued.is_err() {
            warn!("WebRTC: outbound queue full for answer session {}", session_id);
            self.rollback_offer(session_id, audio_id).await;
            return Err(WebRtcError::ResourceExhausted);
        }
        info!("WebRTC: accepted offer -> starting listen on intercom bus");
        self.intercom.start_listen(SessionOwner::MatterCall);
        self.outbound_signal.signal(());

        Ok(AnswerOutcome {
            video_stream_id: None,
            audio_stream_id: Some(audio_id),
        })
    }

    async fn on_answer(&self, session_id: u16, sdp: &str) -> Result<(), WebRtcError> {
        info!("WebRTC: on_answer session={}, sdp_len={}", session_id, sdp.len());
        if self.gate.active_session() != Some(session_id) {
            warn!("WebRTC: on_answer called for inactive session {}", session_id);
            return Err(WebRtcError::InvalidInState);
        }
        self.media.accept_remote_answer(sdp)?;
        self.state
            .lock(|cell| cell.borrow_mut().media_started = true);
        info!("WebRTC: answer accepted -> starting listen on intercom bus");
        self.intercom.start_listen(SessionOwner::MatterCall);
        self.outbound_signal.signal(());
        Ok(())
    }

    async fn on_ice_candidates(
        &self,
        session_id: u16,
        candidates: &TLVArray<'_, ICECandidateStruct<'_>>,
    ) -> Result<(), WebRtcError> {
        info!("WebRTC: on_ice_candidates session={}", session_id);
        if self.gate.active_session() != Some(session_id) {
            warn!("WebRTC: on_ice_candidates for inactive session {}", session_id);
            return Err(WebRtcError::InvalidInState);
        }
        for item in candidates.iter() {
            let cand = item.map_err(|_| WebRtcError::InvalidCommand)?;
            let sdp = cand.candidate().map_err(|_| WebRtcError::InvalidCommand)?;
            self.media.add_remote_ice_candidate(sdp)?;
        }
        Ok(())
    }

    async fn on_end_session(
        &self,
        session_id: u16,
        reason: WebRTCEndReasonEnum,
    ) -> Result<(), WebRtcError> {
        info!("WebRTC: on_end_session session={}, reason={:?}", session_id, reason);
        self.cleanup_session(session_id).await;
        Ok(())
    }

    async fn next_outbound(&self) -> OutboundWork {
        loop {
            let next = self.state.lock(|cell| {
                let mut s = cell.borrow_mut();
                if s.outbound_queue.is_empty() {
                    None
                } else {
                    Some(s.outbound_queue.remove(0))
                }
            });
            if let Some(work) = next {
                return work;
            }
            let media_started = self.state.lock(|cell| cell.borrow().media_started);
            if let Some(session_id) = self.gate.active_session().filter(|_| media_started) {
                if self.media.is_media_idle() {
                    if self.cleanup_session(session_id).await {
                        return OutboundWork::End {
                            session_id,
                            reason: WebRTCEndReasonEnum::UnknownReason,
                        };
                    }
                } else {
                    let signal = self.outbound_signal.wait();
                    let idle = self.media.wait_media_idle();
                    let _ = embassy_futures::select::select(signal, idle).await;
                }
            } else {
                self.outbound_signal.wait().await;
            }
        }
    }

    async fn take_ice_candidates(
        &self,
        _session_id: u16,
        _out: &mut dyn IceCandidateSink,
    ) -> Result<(), WebRtcError> {
        Ok(())
    }

    async fn take_answer_sdp(
        &self,
        session_id: u16,
        sdp_out: &mut [u8],
    ) -> Result<usize, WebRtcError> {
        if self.gate.active_session() != Some(session_id) {
            return Err(WebRtcError::InvalidInState);
        }
        self.state.lock(|cell| {
            let mut s = cell.borrow_mut();
            if s.sdp_len == 0 {
                return Err(WebRtcError::InvalidInState);
            }
            if sdp_out.len() < s.sdp_len {
                return Err(WebRtcError::ResourceExhausted);
            }
            let len = s.sdp_len;
            sdp_out[..len].copy_from_slice(&s.sdp_buf[..len]);
            s.sdp_len = 0;
            Ok(len)
        })
    }

    async fn take_offer_sdp(
        &self,
        session_id: u16,
        sdp_out: &mut [u8],
    ) -> Result<usize, WebRtcError> {
        info!("WebRTC: take_offer_sdp session={}", session_id);
        self.take_answer_sdp(session_id, sdp_out).await
    }
}

/// Endpoint 1 `WebRTCTransportProvider` (`0x0553`) cluster wrapper advertising client `0x0554`.
pub struct IntercomWebRtcProvCluster<'a, M: MediaSessionPort>(
    web_rtc_transport_provider::HandlerAsyncAdaptor<
        WebRtcProvHandler<IntercomWebRtcHooks<'a, M>, 3, MAX_SDP_BYTES, 1024, 128, 4>,
    >,
);

impl<'a, M: MediaSessionPort> IntercomWebRtcProvCluster<'a, M> {
    pub const CLUSTER: Cluster<'static> =
        WebRtcProvHandler::<IntercomWebRtcHooks<'a, M>, 3, MAX_SDP_BYTES, 1024, 128, 4>::CLUSTER;

    pub const fn new(hooks: IntercomWebRtcHooks<'a, M>) -> Self {
        Self(WebRtcProvHandler::new(Dataver::new(40), AUDIO_DOORBELL_ENDPOINT_ID, hooks).adapt())
    }
}

impl<M: MediaSessionPort> MatterCluster for IntercomWebRtcProvCluster<'_, M> {
    const ENDPOINT_ID: u16 = AUDIO_DOORBELL_ENDPOINT_ID;
    const CLUSTER_DEF: Cluster<'static> = Self::CLUSTER;
    const DEVICE_TYPE: DeviceType = DEV_TYPE_AUDIO_DOORBELL;
    const CLIENT_CLUSTERS: &'static [ClusterId] = &[web_rtc_transport_requestor::FULL_CLUSTER.id];
}

impl<M: MediaSessionPort> AsyncHandler for IntercomWebRtcProvCluster<'_, M> {
    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.0.read(ctx, reply).await
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.0.write(ctx).await
    }

    async fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.0.invoke(ctx, reply).await
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        self.0.bump_dataver(ctx);
    }

    fn lifecycle(&self, ctx: impl HandlerContext, op: LifecycleOp) -> Result<(), Error> {
        self.0.lifecycle(ctx, op)
    }

    fn run(
        &self,
        ctx: impl HandlerContext,
    ) -> impl core::future::Future<Output = Result<(), Error>> {
        self.0.run(ctx)
    }
}

struct SignallingScratch {
    bound_audio_stream_id: Option<u16>,
    media_started: bool,
    sdp_buf: [u8; MAX_SDP_BYTES],
    sdp_len: usize,
    outbound_queue: Vec<OutboundWork, 4>,
}

impl SignallingScratch {
    const fn new() -> Self {
        Self {
            bound_audio_stream_id: None,
            media_started: false,
            sdp_buf: [0u8; MAX_SDP_BYTES],
            sdp_len: 0,
            outbound_queue: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use core::future::Future;
    use core::task::{Context, Poll, Waker};
    use std::sync::Arc;
    use std::task::Wake;

    use embassy_futures::block_on;
    use intercom_core::domain::intercom::SessionOwner;
    use intercom_core::ports::inbound::{
        CommandChannel, IntercomCommand, IntercomControllerHandle,
    };
    use rs_matter::dm::clusters::app::audio_av_stream::AudioCodecEnum;
    use rs_matter::dm::clusters::app::webrtc_prov::{
        OfferParams, OutboundWork, WebRtcError, WebRtcHooks,
    };
    use rs_matter::dm::clusters::decl::globals::{StreamUsageEnum, WebRTCEndReasonEnum};

    use super::{
        ActiveCallGate, IntercomAudioAvCluster, IntercomWebRtcHooks, MediaSessionPort,
        volume_percent_to_rx_gain, volume_percent_to_tx_gain,
    };

    struct MediaSessionPortMock {
        stopped: Cell<u32>,
        waited: Cell<u32>,
        answer_length: Cell<usize>,
        fail_offer: Cell<bool>,
        reject_answer: Cell<bool>,
    }

    impl MediaSessionPortMock {
        const fn new() -> Self {
            Self {
                stopped: Cell::new(0),
                waited: Cell::new(0),
                answer_length: Cell::new(8),
                fail_offer: Cell::new(false),
                reject_answer: Cell::new(false),
            }
        }
    }

    impl MediaSessionPort for MediaSessionPortMock {
        fn accept_remote_offer(&self, _offer: &str, out: &mut [u8]) -> Result<usize, WebRtcError> {
            if self.fail_offer.get() {
                return Err(WebRtcError::Failure);
            }
            let len = self.answer_length.get();
            if len <= out.len() {
                out[..len].fill(b'a');
            }
            Ok(len)
        }

        fn build_local_offer(&self, out: &mut [u8]) -> Result<usize, WebRtcError> {
            out[..3].copy_from_slice(b"v=0");
            Ok(3)
        }

        fn accept_remote_answer(&self, _answer: &str) -> Result<(), WebRtcError> {
            if self.reject_answer.get() {
                Err(WebRtcError::InvalidCommand)
            } else {
                Ok(())
            }
        }
        fn add_remote_ice_candidate(&self, _candidate: &str) -> Result<(), WebRtcError> {
            Ok(())
        }
        fn stop_media_session(&self) {
            self.stopped.set(self.stopped.get() + 1);
        }
        async fn wait_media_idle(&self) {
            self.waited.set(self.waited.get() + 1);
        }
        fn is_media_idle(&self) -> bool {
            self.waited.get() > 0
        }
    }

    fn params(stream_id: u16) -> OfferParams {
        OfferParams {
            stream_usage: StreamUsageEnum::LiveView,
            originating_endpoint_id: 1,
            video_stream_id: None,
            audio_stream_id: Some(Some(stream_id)),
            metadata_enabled: false,
        }
    }

    #[test]
    fn active_call_gate_rejects_a_second_session_and_releases_the_first() {
        // Arrange
        let gate = ActiveCallGate::new();
        gate.try_acquire(1).expect("first session");

        // Act
        let second = gate.try_acquire(2);
        let released = gate.release(1);
        let retry = gate.try_acquire(2);

        // Assert
        assert_eq!(second, Err(WebRtcError::ResourceExhausted));
        assert!(released);
        assert_eq!(retry, Ok(()));
    }

    #[test]
    fn active_call_gate_rejects_duplicate_offer_for_same_session() {
        // Arrange
        let gate = ActiveCallGate::new();
        gate.try_acquire(7).expect("first offer");

        // Act
        let duplicate = gate.try_acquire(7);

        // Assert
        assert_eq!(duplicate, Err(WebRtcError::InvalidInState));
    }

    #[test]
    fn matter_volume_endpoints_and_mute_map_to_codec_gain_limits() {
        // Arrange
        let levels = [0, 100];

        // Act
        let mic_low = volume_percent_to_rx_gain(false, levels[0]);
        let mic_high = volume_percent_to_rx_gain(false, levels[1]);
        let speaker_low = volume_percent_to_tx_gain(false, levels[0]);
        let speaker_high = volume_percent_to_tx_gain(false, levels[1]);
        let muted = volume_percent_to_tx_gain(true, levels[1]);

        // Assert
        assert_eq!(mic_low.half_db(), 0);
        assert_eq!(mic_high.half_db(), 95);
        assert_eq!(speaker_low.half_db(), -127);
        assert_eq!(speaker_high.half_db(), 48);
        assert_eq!(muted.half_db(), -127);
    }

    #[test]
    fn offer_starts_matter_listen_without_dispatching_unlock_and_releases_audio_on_end() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let receiver = commands.receiver();
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);

        // Act
        let result = block_on(hooks.on_offer(7, "v=0\r\n", &params(stream_id)));
        let while_active = audio.handler().deallocate_stream(stream_id);
        let command = receiver.try_receive().expect("listen command");
        let next_command = receiver.try_receive();
        block_on(hooks.on_end_session(7, WebRTCEndReasonEnum::UserHangup)).expect("end");
        let after_end = audio.handler().deallocate_stream(stream_id);

        // Assert
        assert_eq!(result.expect("offer").audio_stream_id, Some(stream_id));
        assert!(while_active.is_err());
        assert_eq!(
            command,
            IntercomCommand::StartListen(SessionOwner::MatterCall)
        );
        assert!(next_command.is_err());
        assert!(after_end.is_ok());
        assert_eq!(media.stopped.get(), 1);
        assert_eq!(media.waited.get(), 1);
    }

    #[test]
    fn oversized_answer_rejects_offer_and_releases_audio_and_gate() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        media.answer_length.set(super::MAX_SDP_BYTES + 1);
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);

        // Act
        let result = block_on(hooks.on_offer(7, "v=0\r\n", &params(stream_id)));
        let deallocate = audio.handler().deallocate_stream(stream_id);

        // Assert
        assert_eq!(result, Err(WebRtcError::ResourceExhausted));
        assert_eq!(hooks.active_session(), None);
        assert!(deallocate.is_ok());
        assert!(commands.receiver().try_receive().is_err());
    }

    #[test]
    fn media_offer_failure_releases_audio_and_gate() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        media.fail_offer.set(true);
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);

        // Act
        let result = block_on(hooks.on_offer(7, "v=0\r\n", &params(stream_id)));

        // Assert
        assert_eq!(result, Err(WebRtcError::Failure));
        assert_eq!(hooks.active_session(), None);
        assert!(audio.handler().deallocate_stream(stream_id).is_ok());
    }

    #[test]
    fn media_engine_ending_independently_releases_audio_and_notifies_peer() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);
        block_on(hooks.on_offer(7, "v=0\r\n", &params(stream_id))).expect("offer");

        // Act
        let answer = block_on(hooks.next_outbound());
        let end = block_on(hooks.next_outbound());
        let deallocate = audio.handler().deallocate_stream(stream_id);

        // Assert
        assert_eq!(answer, OutboundWork::Answer { session_id: 7 });
        assert_eq!(
            end,
            OutboundWork::End {
                session_id: 7,
                reason: WebRTCEndReasonEnum::UnknownReason,
            }
        );
        assert_eq!(hooks.active_session(), None);
        assert!(deallocate.is_ok());
    }

    struct NoopWakeMock;

    impl Wake for NoopWakeMock {
        fn wake(self: Arc<Self>) {}
    }

    #[test]
    fn end_keeps_gate_until_command_owner_release_is_queued() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let receiver = commands.receiver();
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);
        block_on(hooks.on_offer(7, "v=0\r\n", &params(stream_id))).expect("offer");
        for _ in 0..7 {
            handle.trigger_ring();
        }
        let waker = Waker::from(Arc::new(NoopWakeMock));
        let mut context = Context::from_waker(&waker);
        let mut end = core::pin::pin!(hooks.on_end_session(7, WebRTCEndReasonEnum::UserHangup));

        // Act
        let while_queue_full = end.as_mut().poll(&mut context);
        let competing = hooks.gate.try_acquire(8);
        receiver.try_receive().expect("make room for owner release");
        let after_capacity = end.as_mut().poll(&mut context);

        // Assert
        assert!(matches!(while_queue_full, Poll::Pending));
        assert_eq!(competing, Err(WebRtcError::ResourceExhausted));
        assert!(matches!(after_capacity, Poll::Ready(Ok(()))));
        assert_eq!(hooks.active_session(), None);
    }

    #[test]
    fn invalid_answer_keeps_local_offer_session_retryable() {
        // Arrange
        let commands = CommandChannel::new();
        let handle = IntercomControllerHandle::new(commands.sender());
        let audio = IntercomAudioAvCluster::new(handle);
        let stream_id = audio
            .handler()
            .allocate_stream(
                StreamUsageEnum::LiveView,
                AudioCodecEnum::OPUS,
                1,
                16_000,
                24_000,
                16,
            )
            .expect("allocate audio");
        let media = MediaSessionPortMock::new();
        let hooks = IntercomWebRtcHooks::new(handle, &audio, &media);
        block_on(hooks.on_solicit_offer(7, &params(stream_id))).expect("local offer");
        media.reject_answer.set(true);

        // Act
        let rejected = block_on(hooks.on_answer(7, "invalid SDP"));
        let held = hooks.active_session();
        let while_pending = audio.handler().deallocate_stream(stream_id);
        media.reject_answer.set(false);
        let retry = block_on(hooks.on_answer(7, "valid SDP"));

        // Assert
        assert_eq!(rejected, Err(WebRtcError::InvalidCommand));
        assert_eq!(held, Some(7));
        assert!(while_pending.is_err());
        assert_eq!(retry, Ok(()));
        assert_eq!(hooks.active_session(), Some(7));
    }
}
