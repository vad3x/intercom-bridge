//! ITU-T G.711 mu-law (8-bit, 16 kHz) tableless codec for WebSocket Push-to-Talk audio.

use intercom_media::{PcmFrame, AUDIO_FRAME_SAMPLES};

const ULAW_BIAS: i32 = 0x84;
const ULAW_CLIP: i32 = 32_635;

pub fn encode_ulaw_sample(pcm: i16) -> u8 {
    let sign: u8 = if pcm < 0 { 0x80 } else { 0x00 };
    let mag = (pcm as i32).abs().min(ULAW_CLIP) + ULAW_BIAS;
    let mut exponent: u8 = 7;
    let mut exp_mask: i32 = 0x4000;
    while (mag & exp_mask) == 0 && exponent > 0 {
        exponent -= 1;
        exp_mask >>= 1;
    }
    let mantissa = ((mag >> (exponent + 3)) & 0x0F) as u8;
    !(sign | (exponent << 4) | mantissa)
}

pub fn decode_ulaw_sample(ulaw: u8) -> i16 {
    let u = !ulaw;
    let sign = (u & 0x80) != 0;
    let exponent = ((u >> 4) & 0x07) as i32;
    let mantissa = (u & 0x0F) as i32;
    let mut magnitude = (((mantissa << 3) + ULAW_BIAS) << exponent) - ULAW_BIAS;
    if sign {
        magnitude = -magnitude;
    }
    magnitude.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

pub fn encode_ulaw_frame(frame: &PcmFrame, out: &mut [u8; AUDIO_FRAME_SAMPLES]) {
    let mut i = 0;
    while i < AUDIO_FRAME_SAMPLES {
        out[i] = encode_ulaw_sample(frame.samples[i]);
        i += 1;
    }
}

pub fn decode_ulaw_frame(ulaw: &[u8; AUDIO_FRAME_SAMPLES]) -> PcmFrame {
    let mut samples = [0i16; AUDIO_FRAME_SAMPLES];
    let mut i = 0;
    while i < AUDIO_FRAME_SAMPLES {
        samples[i] = decode_ulaw_sample(ulaw[i]);
        i += 1;
    }
    PcmFrame { samples }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulaw_silence_and_speech_amplitudes_round_trip_within_itu_step_tolerance() {
        // Arrange
        let test_samples: [i16; 7] = [0, 128, -128, 2_048, -2_048, 16_384, -32_000];

        // Act
        let decoded = test_samples.map(|sample| decode_ulaw_sample(encode_ulaw_sample(sample)));

        // Assert
        assert_eq!(decoded[0], 0);
        for (&sample, &actual) in test_samples[1..].iter().zip(decoded[1..].iter()) {
            let err = (sample as i32 - actual as i32).abs();
            let tol = ((sample as i32).abs() / 16).max(16);
            assert!(
                err <= tol,
                "sample={sample}, actual={actual}, err={err}, tol={tol}"
            );
        }
    }
    #[test]
    fn ulaw_matches_reference_silence_polarity_and_clipping_codes() {
        // Arrange
        let vectors = [
            (0, 0xff, 0),
            (128, 0xef, 132),
            (-128, 0x6f, -132),
            (2048, 0xbe, 2108),
            (16384, 0x8f, 16764),
            (i16::MAX, 0x80, 32124),
            (i16::MIN, 0x00, -32124),
        ];
        // Act / Assert
        for (pcm, code, decoded) in vectors {
            assert_eq!(encode_ulaw_sample(pcm), code);
            assert_eq!(decode_ulaw_sample(code), decoded);
        }
        assert_eq!(decode_ulaw_sample(0x7f), 0);
    }

    #[test]
    fn full_pcm_range_and_frame_conversion_keep_quantization_within_one_step() {
        // Arrange
        let frame = PcmFrame {
            samples: core::array::from_fn(|i| ((i as i32 * 197) - 31500) as i16),
        };
        let mut encoded = [0; AUDIO_FRAME_SAMPLES];
        // Act
        encode_ulaw_frame(&frame, &mut encoded);
        let decoded = decode_ulaw_frame(&encoded);
        // Assert
        for sample in i16::MIN..=i16::MAX {
            let actual = decode_ulaw_sample(encode_ulaw_sample(sample));
            let tolerance = (i32::from(sample).abs() / 16).max(16);
            assert!((i32::from(sample) - i32::from(actual)).abs() <= tolerance);
        }
        for (sample, actual) in frame.samples.iter().zip(decoded.samples) {
            assert!(
                (i32::from(*sample) - i32::from(actual)).abs()
                    <= (i32::from(*sample).abs() / 16).max(16)
            );
        }
    }
}
