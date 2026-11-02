//! Live 16 kHz G.711 mu-law WebSocket Push-to-Talk endpoint (`/api/ws/audio`).

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_futures::select::{select, Either as EmbEither};
use intercom_core::domain::intercom::SessionOwner;
use intercom_core::events::{
    CallStateReceiver, CallStateWatch, SystemState, SystemStateReceiver, SystemStateWatch,
};
use intercom_core::ports::inbound::IntercomControllerHandle;
use intercom_media::{
    AudioFrameSink, AudioFrameSource, RxAudioChannel, TxAudioChannel, AUDIO_FRAME_SAMPLES,
};
use picoserve::futures::Either as WsEither;
use picoserve::response::ws::{Message, SocketRx, SocketTx, WebSocketCallback};

use crate::g711::{decode_ulaw_frame, encode_ulaw_frame};

pub const WS_CTRL_HEARTBEAT: u8 = 0x00;
pub const WS_CTRL_START_TALK: u8 = 0x01;
pub const WS_CTRL_STOP_TALK: u8 = 0x02;

#[derive(Clone, Copy)]
pub struct HttpAudioMedia<'a> {
    pub rx_audio: &'a RxAudioChannel,
    pub tx_audio: &'a TxAudioChannel,
    pub call_watch: &'a CallStateWatch,
    pub state_watch: &'a SystemStateWatch,
    pub ws_active: &'a AtomicBool,
}

impl<'a> HttpAudioMedia<'a> {
    fn is_preempted(&self) -> bool {
        self.state_watch.try_get() == Some(SystemState::Updating)
            || self.call_watch.try_get().and_then(|s| s.active_owner)
                == Some(SessionOwner::MatterCall)
    }

    pub fn can_accept_ws(&self) -> bool {
        if self.is_preempted() {
            return false;
        }
        self.ws_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

pub struct WsAudioSession<'a> {
    pub access_handle: IntercomControllerHandle<'a>,
    pub media: HttpAudioMedia<'a>,
    started: bool,
    reserved: bool,
}

impl<'a> WsAudioSession<'a> {
    pub(crate) fn new(
        access_handle: IntercomControllerHandle<'a>,
        media: HttpAudioMedia<'a>,
    ) -> Self {
        Self {
            access_handle,
            media,
            started: false,
            reserved: true,
        }
    }

    async fn run_media<R: picoserve::io::Read, W: picoserve::io::Write<Error = R::Error>>(
        &mut self,
        rx: &mut SocketRx<R>,
        tx: &mut SocketTx<W>,
    ) -> Result<SessionEnd, W::Error> {
        let mut rx_audio = self.media.rx_audio.receiver();
        let mut tx_audio = self.media.tx_audio.sender();
        let (Some(mut call_rx), Some(mut state_rx)) = (
            self.media.call_watch.receiver(),
            self.media.state_watch.receiver(),
        ) else {
            return Ok(SessionEnd::Unavailable);
        };
        if self.media.is_preempted() {
            return Ok(SessionEnd::Preempted);
        }
        self.access_handle.start_listen(SessionOwner::HttpDebug);
        self.started = true;
        let mut msg_buf = [0u8; AUDIO_FRAME_SAMPLES];

        loop {
            if self.media.is_preempted() {
                return Ok(SessionEnd::Preempted);
            }
            let preempt_fut = wait_for_preemption(&mut call_rx, &mut state_rx);

            // next_message is only signal-aware before the frame header. Dropping
            // its entire future on preemption also interrupts a partial/fragmented
            // frame; this connection is closed, so parsing is never resumed.
            let event = select(
                preempt_fut,
                rx.next_message(&mut msg_buf, rx_audio.receive_frame()),
            )
            .await;
            let event = match event {
                EmbEither::First(()) => {
                    return Ok(SessionEnd::Preempted);
                }
                EmbEither::Second(event) => event?,
            };
            let send_audio = match event {
                WsEither::First(Ok(Message::Binary(data))) => {
                    if data.len() == 1 {
                        match data[0] {
                            WS_CTRL_START_TALK => {
                                self.access_handle.start_talk(SessionOwner::HttpDebug);
                            }
                            WS_CTRL_STOP_TALK => {
                                self.access_handle.stop_talk(SessionOwner::HttpDebug);
                            }
                            WS_CTRL_HEARTBEAT => {
                                self.access_handle
                                    .session_heartbeat(SessionOwner::HttpDebug);
                            }
                            _ => {}
                        }
                    } else if let Ok(chunk) = <&[u8; AUDIO_FRAME_SAMPLES]>::try_from(data) {
                        let pcm = decode_ulaw_frame(chunk);
                        let _ = tx_audio.try_send_frame(pcm);
                        self.access_handle
                            .session_heartbeat(SessionOwner::HttpDebug);
                    }
                    false
                }
                WsEither::First(Ok(Message::Ping(payload))) => {
                    match select(
                        wait_for_preemption(&mut call_rx, &mut state_rx),
                        tx.send_pong(payload),
                    )
                    .await
                    {
                        EmbEither::First(()) => {
                            return Ok(SessionEnd::Preempted);
                        }
                        EmbEither::Second(result) => result?,
                    }
                    false
                }
                WsEither::First(Ok(Message::Close(_))) | WsEither::First(Err(_)) => {
                    return Ok(SessionEnd::Normal);
                }
                WsEither::First(Ok(_)) => false,
                WsEither::Second(pcm_frame) => {
                    encode_ulaw_frame(&pcm_frame, &mut msg_buf);
                    true
                }
            };
            // End the PCM frame's scope before awaiting network backpressure.
            // Receive and transmit frames share this session's one wire buffer.
            if send_audio {
                match select(
                    wait_for_preemption(&mut call_rx, &mut state_rx),
                    tx.send_binary(&msg_buf),
                )
                .await
                {
                    EmbEither::First(()) => {
                        return Ok(SessionEnd::Preempted);
                    }
                    EmbEither::Second(result) => result?,
                }
            }
        }
    }

    fn release(&mut self) {
        if self.reserved {
            if self.started {
                self.access_handle.end_session(SessionOwner::HttpDebug);
            }
            self.media.ws_active.store(false, Ordering::Release);
            self.reserved = false;
        }
    }
}

// The callback owns the reservation even before the 101 response is written.
// Cancellation releases that reservation and attempts non-blocking session cleanup;
// normal completion uses awaited delivery instead.
impl Drop for WsAudioSession<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

enum SessionEnd {
    Normal,
    Preempted,
    Unavailable,
}

impl WebSocketCallback for WsAudioSession<'_> {
    async fn run<R: picoserve::io::Read, W: picoserve::io::Write<Error = R::Error>>(
        mut self,
        mut rx: SocketRx<R>,
        mut tx: SocketTx<W>,
    ) -> Result<(), W::Error> {
        // The media helper is gone before command-queue cleanup waits, so PCM
        // frames and watch receivers never remain allocated across that wait.
        let result = self.run_media(&mut rx, &mut tx).await;
        // Normal completion, preemption and I/O errors retain the reservation
        // until EndSession has entered the controller queue. Drop remains the
        // best-effort fallback if this task is cancelled while waiting.
        if self.started {
            self.access_handle
                .end_session_wait(SessionOwner::HttpDebug)
                .await;
            self.started = false;
        }
        self.release();
        match result? {
            SessionEnd::Normal => tx.close(None).await,
            SessionEnd::Preempted => tx.close((1013, "preempted")).await,
            SessionEnd::Unavailable => tx.close((1013, "audio watches unavailable")).await,
        }
    }
}

async fn wait_for_preemption(
    call_rx: &mut CallStateReceiver<'_>,
    state_rx: &mut SystemStateReceiver<'_>,
) {
    loop {
        match select(call_rx.changed(), state_rx.changed()).await {
            EmbEither::First(snapshot)
                if snapshot.active_owner == Some(SessionOwner::MatterCall) =>
            {
                return
            }
            EmbEither::Second(SystemState::Updating) => return,
            _ => {}
        }
    }
}
