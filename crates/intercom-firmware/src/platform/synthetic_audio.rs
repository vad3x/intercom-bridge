//! S31 synthetic audio and Opus codec path used while HAL I2S support is unavailable.

use intercom_adapter_audio::SyntheticAudioPipeline;
use intercom_adapter_webrtc::opus::{HalfDuplexOpusCodec, OpusError};
use intercom_media::{OpusPacket, PcmFrame};

/// Generates a benchmark PCM frame, encodes it to Opus, and decodes it again.
pub struct S31SyntheticAudioPipeline {
    source: SyntheticAudioPipeline,
    codec: HalfDuplexOpusCodec,
}

impl S31SyntheticAudioPipeline {
    pub const fn new() -> Self {
        Self {
            source: SyntheticAudioPipeline::new(),
            codec: HalfDuplexOpusCodec::new(),
        }
    }

    pub fn next_frame(&mut self) -> Result<(PcmFrame, OpusPacket, PcmFrame), OpusError> {
        let source = self.source.next_capture_frame();
        let mut packet = OpusPacket {
            len: 0,
            payload: [0; intercom_media::MAX_OPUS_PACKET_BYTES],
        };
        let len = self
            .codec
            .encode_listen_frame(&source, &mut packet.payload)?;
        packet.len = len as u16;

        let (decoded_frames, decoded_count) = self
            .codec
            .decode_talk_frames(Some(&packet.payload[..len]))?;
        debug_assert!(decoded_count > 0);

        Ok((source, packet, decoded_frames[0]))
    }
}

#[cfg(test)]
mod tests {
    use super::S31SyntheticAudioPipeline;
    use intercom_media::{compute_frame_levels_dbfs, OpusChannel};

    #[test]
    fn s31_pipeline_opus_roundtrip_crosses_the_ipc_channel() {
        let mut pipeline = S31SyntheticAudioPipeline::new();
        let (source, packet, decoded) = pipeline.next_frame().expect("Opus roundtrip");
        let channel = OpusChannel::new();

        channel.try_send(packet).expect("enqueue encoded frame");
        let transferred = channel.try_receive().expect("receive encoded frame");

        assert_eq!(compute_frame_levels_dbfs(&source).peak_dbfs, -3);
        assert!(transferred.len > 0);
        assert_eq!(transferred, packet);
        assert!(compute_frame_levels_dbfs(&decoded).rms_dbfs > -20);
    }
}
