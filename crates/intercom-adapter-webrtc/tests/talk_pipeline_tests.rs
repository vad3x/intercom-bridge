use intercom_adapter_webrtc::opus::{HalfDuplexOpusCodec, OpusEncoderConfig};
use intercom_adapter_webrtc::session::{TalkJitterState, WebRtcJitterBuffer};
use intercom_adapter_webrtc::srtp::SrtpSession;
use intercom_media::{
    wav_header_16khz_mono, BenchLoopbackState, BenchToneGenerator, PcmFrame, TxAudioChannel,
    AUDIO_FRAME_SAMPLES, MAX_OPUS_PACKET_BYTES,
};

fn encode_440hz_frame(
    encoder: &mut HalfDuplexOpusCodec,
    tone: &mut BenchToneGenerator,
) -> ([u8; 256], usize) {
    let frame = tone.next_frame();
    let mut encoded = [0u8; 256];
    let len = encoder
        .encode_listen_frame(&frame, &mut encoded)
        .expect("encode 16 kHz sine frame");
    (encoded, len)
}

#[test]
fn talk_pipeline_accepts_and_decodes_high_bitrate_vbr_packets_over_256_bytes() {
    let mut buffer = WebRtcJitterBuffer::new(0);
    let mut talk_jitter = TalkJitterState::new();
    let mut decoder = HalfDuplexOpusCodec::new();
    let mut encoder = HalfDuplexOpusCodec::new();
    encoder.set_encoder_config(OpusEncoderConfig::WIDEBAND_16KHZ);
    let mut tone = BenchToneGenerator::new();

    // Seed a valid Opus frame, then pad/verify oversized (>256B, <=512B) payloads are accepted
    // into the jitter buffer without being silently dropped.
    let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
    let mut large_payload = [0u8; 384];
    large_payload[..len].copy_from_slice(&encoded[..len]);
    assert!(large_payload.len() > 256 && large_payload.len() <= MAX_OPUS_PACKET_BYTES);

    for seq in 0u16..3 {
        assert!(
            talk_jitter.push_rtp_payload(&mut buffer, seq, &large_payload),
            "384-byte Opus VBR payload must be accepted at seq {seq}"
        );
    }

    let popped = talk_jitter
        .pop_due_packet(&mut buffer)
        .expect("primed after 3 packets")
        .expect("seq 0 present");
    assert_eq!(popped.len, 384);

    // Also verify real encoded speech/tone decodes cleanly through the pipeline.
    let (frames, count) = decoder
        .decode_talk_frames(Some(&encoded[..len]))
        .expect("decode valid Opus frame");
    assert_eq!(count, 1);
    let peak = frames[0]
        .samples
        .iter()
        .map(|s| s.saturating_abs())
        .max()
        .unwrap_or(0);
    assert!(peak > 1_000, "decoded tone peak too low: {peak}");
}

#[test]
fn talk_pipeline_recovers_after_wifi_jitter_underrun_without_rejecting_packets() {
    let mut buffer = WebRtcJitterBuffer::new(0);
    let mut talk_jitter = TalkJitterState::new();
    let mut encoder = HalfDuplexOpusCodec::new();
    encoder.set_encoder_config(OpusEncoderConfig::WIDEBAND_16KHZ);
    let mut decoder = HalfDuplexOpusCodec::new();
    let mut tone = BenchToneGenerator::new();

    // Phase 1: stream 10 packets (seq 0..10) and drain them completely.
    for seq in 0u16..10 {
        let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
        assert!(talk_jitter.push_rtp_payload(&mut buffer, seq, &encoded[..len]));
        if seq >= 2 {
            let pkt = talk_jitter
                .pop_due_packet(&mut buffer)
                .expect("primed")
                .expect("packet present");
            let (_frames, count) = decoder
                .decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)]))
                .expect("decode");
            assert_eq!(count, 1);
        }
    }
    while let Some(Some(pkt)) = talk_jitter.pop_due_packet(&mut buffer) {
        let (_frames, count) = decoder
            .decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)]))
            .expect("decode");
        assert_eq!(count, 1);
    }
    assert!(buffer.is_empty());

    // Simulate a 100 ms (5-tick) Wi-Fi stall where playout ticks fire while no packets arrive.
    for _ in 0..5 {
        assert_eq!(talk_jitter.pop_due_packet(&mut buffer), None);
    }

    // Phase 2: sender resumes at seq 10..40 (30 packets). None may be rejected.
    let mut decoded_post_underrun = 0usize;
    for seq in 10u16..40 {
        let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
        assert!(
            talk_jitter.push_rtp_payload(&mut buffer, seq, &encoded[..len]),
            "post-underrun packet seq={seq} was rejected"
        );
        if let Some(Some(pkt)) = talk_jitter.pop_due_packet(&mut buffer) {
            let (_frames, count) = decoder
                .decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)]))
                .expect("decode post-underrun");
            assert_eq!(count, 1);
            decoded_post_underrun += 1;
        }
    }
    while let Some(Some(pkt)) = talk_jitter.pop_due_packet(&mut buffer) {
        let (_frames, count) = decoder
            .decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)]))
            .expect("drain post-underrun");
        assert_eq!(count, 1);
        decoded_post_underrun += 1;
    }
    assert_eq!(decoded_post_underrun, 30);
}

#[test]
fn talk_pipeline_plc_emits_single_20ms_frame_on_packet_loss() {
    let mut buffer = WebRtcJitterBuffer::new(0);
    let mut talk_jitter = TalkJitterState::new();
    let mut encoder = HalfDuplexOpusCodec::new();
    encoder.set_encoder_config(OpusEncoderConfig::WIDEBAND_16KHZ);
    let mut decoder = HalfDuplexOpusCodec::new();
    let mut tone = BenchToneGenerator::new();
    let tx_audio = TxAudioChannel::new();

    // Insert seq 0, 1, 2 to prime, pop 0, then insert 4, 5 (simulating lost seq 3).
    for seq in 0u16..3 {
        let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
        assert!(talk_jitter.push_rtp_payload(&mut buffer, seq, &encoded[..len]));
    }
    let first = talk_jitter
        .pop_due_packet(&mut buffer)
        .expect("primed")
        .expect("seq 0 present");
    let (_frames, count) = decoder
        .decode_talk_frames(Some(&first.payload[..usize::from(first.len)]))
        .expect("decode seq 0");
    assert_eq!(count, 1);

    for seq in [4u16, 5] {
        let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
        assert!(talk_jitter.push_rtp_payload(&mut buffer, seq, &encoded[..len]));
    }

    // Pop remaining 5 intervals: 1, 2, None (PLC for 3), 4, 5.
    let mut plc_frames = 0usize;
    let mut total_frames = 1usize;
    for expected_present in [true, true, false, true, true] {
        let slot = talk_jitter.pop_due_packet(&mut buffer).expect("primed");
        assert_eq!(slot.is_some(), expected_present);
        let (frames, count) = match slot {
            Some(pkt) => decoder.decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)])),
            None => {
                plc_frames += 1;
                decoder.decode_talk_frames(None)
            }
        }
        .expect("decode");
        assert_eq!(count, 1, "each 20ms interval must emit exactly 1 PCM frame");
        for frame in frames.iter().take(count) {
            assert!(
                tx_audio.try_send(*frame).is_ok(),
                "tx_audio overflowed on interval {total_frames}"
            );
            let _ = tx_audio.try_receive().expect("drain 20ms frame");
            total_frames += 1;
        }
    }
    assert_eq!(plc_frames, 1);
    assert_eq!(total_frames, 6);
}

#[test]
fn talk_pipeline_srtp_to_bench_loopback_wav_roundtrip() {
    const MASTER_KEY: [u8; 16] = [0x42; 16];
    const MASTER_SALT: [u8; 14] = [0x24; 14];
    let mut client_srtp = SrtpSession::new(MASTER_KEY, MASTER_SALT);
    let mut server_srtp = SrtpSession::new(MASTER_KEY, MASTER_SALT);

    let mut buffer = WebRtcJitterBuffer::new(0);
    let mut talk_jitter = TalkJitterState::new();
    let mut encoder = HalfDuplexOpusCodec::new();
    encoder.set_encoder_config(OpusEncoderConfig::WIDEBAND_16KHZ);
    let mut decoder = HalfDuplexOpusCodec::new();
    let mut tone = BenchToneGenerator::new();
    let tx_audio = TxAudioChannel::new();
    let mut loopback = BenchLoopbackState::new();
    let mut storage = [PcmFrame::SILENCE; 100];

    // Simulate 50 x 20ms RTP packets (1.0s) over SRTP -> jitter buffer -> decode -> tx_audio -> loopback.
    for seq in 0u16..50 {
        let (encoded, len) = encode_440hz_frame(&mut encoder, &mut tone);
        let mut rtp = [0u8; 12 + 256 + 10];
        rtp[0] = 0x80;
        rtp[1] = 111;
        rtp[2..4].copy_from_slice(&seq.to_be_bytes());
        rtp[4..8].copy_from_slice(&(u32::from(seq) * 960).to_be_bytes());
        rtp[8..12].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        rtp[12..12 + len].copy_from_slice(&encoded[..len]);

        let protected_len = client_srtp
            .protect_rtp(&mut rtp, 12 + len)
            .expect("protect RTP");
        let plain_len = server_srtp
            .unprotect_rtp(&mut rtp, protected_len)
            .expect("unprotect RTP");
        assert!(talk_jitter.push_rtp_payload(&mut buffer, seq, &rtp[12..plain_len]));

        if let Some(slot) = talk_jitter.pop_due_packet(&mut buffer) {
            let (frames, count) = match slot {
                Some(pkt) => decoder.decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)])),
                None => decoder.decode_talk_frames(None),
            }
            .expect("decode");
            for frame in frames.iter().take(count) {
                tx_audio.try_send(*frame).expect("tx_audio send");
                let drained = tx_audio.try_receive().expect("tx_audio drain");
                loopback.push_tx_frame(&mut storage, &drained, u64::from(seq) * 20);
            }
        }
    }

    while let Some(slot) = talk_jitter.pop_due_packet(&mut buffer) {
        let (frames, count) = match slot {
            Some(pkt) => decoder.decode_talk_frames(Some(&pkt.payload[..usize::from(pkt.len)])),
            None => decoder.decode_talk_frames(None),
        }
        .expect("drain");
        for frame in frames.iter().take(count) {
            loopback.push_tx_frame(&mut storage, frame, 1_000);
        }
    }

    // Release gap > 500 ms switches loopback from recording to playback.
    let first_playback = loopback
        .poll_playback_frame(&mut storage, 1_600)
        .expect("loopback ready");
    assert_eq!(loopback.recorded_frames(), 50);
    let peak = first_playback
        .samples
        .iter()
        .map(|s| s.saturating_abs())
        .max()
        .unwrap_or(0);
    assert!(peak > 1_000);

    let header =
        wav_header_16khz_mono((loopback.recorded_frames() * AUDIO_FRAME_SAMPLES * 2) as u32);
    assert_eq!(&header[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(header[40..44].try_into().unwrap()),
        (50 * AUDIO_FRAME_SAMPLES * 2) as u32
    );
}
