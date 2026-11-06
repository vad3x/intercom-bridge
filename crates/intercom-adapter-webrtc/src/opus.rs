//! Zero-malloc fixed-point Opus 8 kHz listen encoder and 16 kHz talk decoder (`RV32IMAC`).

use core::ffi::{c_int, c_uchar, c_void};
use intercom_media::{AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ, PcmFrame};

pub const MAX_OPUS_PACKET_BYTES: usize = 256;
const OPUS_ARENA_U64_WORDS: usize = 3_077; // 24,616 B, 8-byte aligned.

const OPUS_OK: c_int = 0;
const OPUS_APPLICATION_VOIP: c_int = 2048;
const OPUS_SIGNAL_VOICE: c_int = 3001;
const OPUS_SET_BITRATE_REQUEST: c_int = 4002;
#[cfg(test)]
const OPUS_GET_BITRATE_REQUEST: c_int = 4003;
const OPUS_SET_COMPLEXITY_REQUEST: c_int = 4010;
#[cfg(test)]
const OPUS_GET_COMPLEXITY_REQUEST: c_int = 4011;
const OPUS_SET_INBAND_FEC_REQUEST: c_int = 4012;
#[cfg(test)]
const OPUS_GET_INBAND_FEC_REQUEST: c_int = 4013;
#[cfg(test)]
const OPUS_GET_SAMPLE_RATE_REQUEST: c_int = 4029;
const OPUS_SET_DTX_REQUEST: c_int = 4016;
const OPUS_SET_SIGNAL_REQUEST: c_int = 4024;
const LISTEN_OPUS_COMPLEXITY: c_int = 0;
const LISTEN_OPUS_BITRATE_BPS: c_int = 16_000;
const LISTEN_OPUS_INBAND_FEC: c_int = 1;
const LISTEN_OPUS_SAMPLE_RATE_HZ: u32 = 8_000;
const LISTEN_OPUS_SOURCE_FRAME_SAMPLES: usize =
    (AUDIO_FRAME_SAMPLES * LISTEN_OPUS_SAMPLE_RATE_HZ as usize) / AUDIO_SAMPLE_RATE_HZ as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpusError {
    ArenaTooSmall,
    InitFailed(i32),
    CodecFailed(i32),
}

pub struct HalfDuplexOpusCodec {
    arena: [u64; OPUS_ARENA_U64_WORDS],
    mode: ActiveCodecMode,
}

impl HalfDuplexOpusCodec {
    pub const fn new() -> Self {
        Self {
            arena: [0u64; OPUS_ARENA_U64_WORDS],
            mode: ActiveCodecMode::Uninitialized,
        }
    }

    pub fn encode_listen_frame(
        &mut self,
        pcm: &PcmFrame,
        out: &mut [u8; MAX_OPUS_PACKET_BYTES],
    ) -> Result<usize, OpusError> {
        self.ensure_encoder()?;
        let mut downsampled = [0i16; LISTEN_OPUS_SOURCE_FRAME_SAMPLES];
        for (index, output) in downsampled.iter_mut().enumerate() {
            let base = index * 2;
            let before = base.saturating_sub(1);
            let after = (base + 1).min(AUDIO_FRAME_SAMPLES - 1);
            let after_after = (base + 2).min(AUDIO_FRAME_SAMPLES - 1);
            let filtered = i32::from(pcm.samples[before])
                + 3 * i32::from(pcm.samples[base])
                + 3 * i32::from(pcm.samples[after])
                + i32::from(pcm.samples[after_after]);
            *output = (filtered / 8) as i16;
        }
        let enc_ptr = self.arena.as_mut_ptr() as *mut OpusEncoder;
        let rc = unsafe {
            opus_encode(
                enc_ptr,
                downsampled.as_ptr(),
                LISTEN_OPUS_SOURCE_FRAME_SAMPLES as c_int,
                out.as_mut_ptr(),
                MAX_OPUS_PACKET_BYTES as c_int,
            )
        };
        if rc < 0 {
            Err(OpusError::CodecFailed(rc))
        } else {
            Ok(rc as usize)
        }
    }

    pub fn decode_talk_frames(
        &mut self,
        packet: Option<&[u8]>,
    ) -> Result<([PcmFrame; 2], usize), OpusError> {
        self.ensure_decoder()?;
        let dec_ptr = self.arena.as_mut_ptr() as *mut OpusDecoder;
        let mut decoded_samples = [0i16; AUDIO_FRAME_SAMPLES * 2];
        let (ptr, len) = match packet {
            Some(bytes) => (bytes.as_ptr(), bytes.len() as c_int),
            None => (core::ptr::null(), 0),
        };
        let rc = unsafe {
            opus_decode(
                dec_ptr,
                ptr,
                len,
                decoded_samples.as_mut_ptr(),
                (AUDIO_FRAME_SAMPLES * 2) as c_int,
                0,
            )
        };
        let mut frames = [PcmFrame::SILENCE; 2];
        match rc as usize {
            AUDIO_FRAME_SAMPLES => {
                frames[0]
                    .samples
                    .copy_from_slice(&decoded_samples[..AUDIO_FRAME_SAMPLES]);
                Ok((frames, 1))
            }
            samples if samples == AUDIO_FRAME_SAMPLES * 2 => {
                frames[0]
                    .samples
                    .copy_from_slice(&decoded_samples[..AUDIO_FRAME_SAMPLES]);
                frames[1]
                    .samples
                    .copy_from_slice(&decoded_samples[AUDIO_FRAME_SAMPLES..]);
                Ok((frames, 2))
            }
            _ => Err(OpusError::CodecFailed(rc)),
        }
    }

    fn ensure_encoder(&mut self) -> Result<(), OpusError> {
        if self.mode == ActiveCodecMode::Encoder {
            return Ok(());
        }
        let needed = unsafe { opus_encoder_get_size(1) } as usize;
        if needed > core::mem::size_of_val(&self.arena) {
            return Err(OpusError::ArenaTooSmall);
        }
        let enc_ptr = self.arena.as_mut_ptr() as *mut OpusEncoder;
        unsafe {
            let err = opus_encoder_init(
                enc_ptr,
                LISTEN_OPUS_SAMPLE_RATE_HZ as i32,
                1,
                OPUS_APPLICATION_VOIP,
            );
            if err != OPUS_OK {
                return Err(OpusError::InitFailed(err));
            }
            let _ = opus_encoder_ctl(enc_ptr, OPUS_SET_BITRATE_REQUEST, LISTEN_OPUS_BITRATE_BPS);
            let _ = opus_encoder_ctl(enc_ptr, OPUS_SET_COMPLEXITY_REQUEST, LISTEN_OPUS_COMPLEXITY);
            let _ = opus_encoder_ctl(enc_ptr, OPUS_SET_SIGNAL_REQUEST, OPUS_SIGNAL_VOICE);
            let _ = opus_encoder_ctl(enc_ptr, OPUS_SET_INBAND_FEC_REQUEST, LISTEN_OPUS_INBAND_FEC);
            let _ = opus_encoder_ctl(enc_ptr, OPUS_SET_DTX_REQUEST, 1 as c_int);
        }
        self.mode = ActiveCodecMode::Encoder;
        Ok(())
    }

    fn ensure_decoder(&mut self) -> Result<(), OpusError> {
        if self.mode == ActiveCodecMode::Decoder {
            return Ok(());
        }
        let needed = unsafe { opus_decoder_get_size(1) } as usize;
        if needed > core::mem::size_of_val(&self.arena) {
            return Err(OpusError::ArenaTooSmall);
        }
        let dec_ptr = self.arena.as_mut_ptr() as *mut OpusDecoder;
        let err = unsafe { opus_decoder_init(dec_ptr, AUDIO_SAMPLE_RATE_HZ as i32, 1) };
        if err != OPUS_OK {
            return Err(OpusError::InitFailed(err));
        }
        self.mode = ActiveCodecMode::Decoder;
        Ok(())
    }
}

impl Default for HalfDuplexOpusCodec {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveCodecMode {
    Uninitialized,
    Encoder,
    Decoder,
}

#[repr(C)]
struct OpusEncoder {
    _private: [u8; 0],
}

#[repr(C)]
struct OpusDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn opus_encoder_get_size(channels: c_int) -> c_int;
    fn opus_encoder_init(
        st: *mut OpusEncoder,
        fs: i32,
        channels: c_int,
        application: c_int,
    ) -> c_int;
    fn opus_encode(
        st: *mut OpusEncoder,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut c_uchar,
        max_data_bytes: c_int,
    ) -> c_int;
    fn opus_encoder_ctl(st: *mut OpusEncoder, request: c_int, ...) -> c_int;
    fn opus_decoder_get_size(channels: c_int) -> c_int;
    fn opus_decoder_init(st: *mut OpusDecoder, fs: i32, channels: c_int) -> c_int;
    fn opus_decode(
        st: *mut OpusDecoder,
        data: *const c_uchar,
        len: c_int,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
}

#[allow(dead_code)]
const _: *const c_void = core::ptr::null();
#[cfg(test)]
mod tests {
    use super::*;

    use intercom_media::{BenchToneGenerator, compute_frame_levels_dbfs};

    #[test]
    fn fixed_point_opus_encodes_20ms_8khz_listen_packet_and_decodes_one_frame() {
        // Arrange
        let mut codec = HalfDuplexOpusCodec::new();
        let mut tone = BenchToneGenerator::new();
        let mut encoded_frames = [[0u8; MAX_OPUS_PACKET_BYTES]; 5];
        let mut lengths = [0usize; 5];

        // Act
        for i in 0..5 {
            let pcm = tone.next_frame();
            lengths[i] = codec
                .encode_listen_frame(&pcm, &mut encoded_frames[i])
                .expect("opus encode");
        }
        let last_len = lengths[4];
        let mut decoded = [PcmFrame::SILENCE; 2];
        for i in 0..5 {
            let (frames, count) = codec
                .decode_talk_frames(Some(&encoded_frames[i][..lengths[i]]))
                .expect("opus decode");
            assert_eq!(count, 1);
            decoded = frames;
        }
        let (plc_frames, plc_count) = codec.decode_talk_frames(None).expect("opus plc");

        // Assert
        assert!(
            last_len > 0 && last_len <= 160,
            "unexpected packet len: {last_len}"
        );
        let levels = compute_frame_levels_dbfs(&decoded[0]);
        assert!(
            levels.rms_dbfs > -20,
            "decoded signal too quiet: {:?}",
            levels
        );
        assert_eq!(plc_count, 2);
        assert_ne!(plc_frames[0].samples, PcmFrame::SILENCE.samples);
    }

    #[test]
    fn sustained_test_tone_is_not_suppressed_as_opus_dtx() {
        let mut codec = HalfDuplexOpusCodec::new();
        let mut tone = BenchToneGenerator::new();
        let mut encoded = [0u8; MAX_OPUS_PACKET_BYTES];
        let mut dtx_packets = 0;

        for _ in 0..100 {
            let pcm = tone.next_frame();
            let length = codec
                .encode_listen_frame(&pcm, &mut encoded)
                .expect("opus encode");
            dtx_packets += usize::from(length <= 3);
        }

        assert_eq!(dtx_packets, 0, "continuous tone produced DTX packets");
    }

    #[test]
    fn listen_encoder_uses_16kbps_bitrate() {
        let mut codec = HalfDuplexOpusCodec::new();
        codec.ensure_encoder().expect("initialize Opus encoder");
        let enc_ptr = codec.arena.as_mut_ptr() as *mut OpusEncoder;
        let mut bitrate = -1;

        let result = unsafe {
            opus_encoder_ctl(
                enc_ptr,
                OPUS_GET_BITRATE_REQUEST,
                &mut bitrate as *mut c_int,
            )
        };

        assert_eq!(result, OPUS_OK);
        assert_eq!(bitrate, 16_000);
    }

    #[test]
    fn listen_encoder_uses_lowest_opus_complexity() {
        let mut codec = HalfDuplexOpusCodec::new();
        codec.ensure_encoder().expect("initialize Opus encoder");
        let enc_ptr = codec.arena.as_mut_ptr() as *mut OpusEncoder;
        let mut complexity = -1;

        let result = unsafe {
            opus_encoder_ctl(
                enc_ptr,
                OPUS_GET_COMPLEXITY_REQUEST,
                &mut complexity as *mut c_int,
            )
        };

        assert_eq!(result, OPUS_OK);
        assert_eq!(complexity, 0);
    }

    #[test]
    fn listen_encoder_enables_inband_fec_for_loss_resilience() {
        let mut codec = HalfDuplexOpusCodec::new();
        codec.ensure_encoder().expect("initialize Opus encoder");
        let enc_ptr = codec.arena.as_mut_ptr() as *mut OpusEncoder;
        let mut inband_fec = -1;

        let result = unsafe {
            opus_encoder_ctl(
                enc_ptr,
                OPUS_GET_INBAND_FEC_REQUEST,
                &mut inband_fec as *mut c_int,
            )
        };

        assert_eq!(result, OPUS_OK);
        assert_eq!(inband_fec, 1);
    }

    #[test]
    fn listen_encoder_uses_8khz_mono_input() {
        let mut codec = HalfDuplexOpusCodec::new();
        codec.ensure_encoder().expect("initialize Opus encoder");
        let enc_ptr = codec.arena.as_mut_ptr() as *mut OpusEncoder;
        let mut sample_rate = 0;

        let result = unsafe {
            opus_encoder_ctl(
                enc_ptr,
                OPUS_GET_SAMPLE_RATE_REQUEST,
                &mut sample_rate as *mut c_int,
            )
        };

        assert_eq!(result, OPUS_OK);
        assert_eq!(sample_rate, 8_000);
    }

    #[test]
    fn listen_opus_packet_carries_20ms_of_audio() {
        assert_eq!(LISTEN_OPUS_SOURCE_FRAME_SAMPLES, 160);
    }
}
