//! Conversion between 16-bit Media Plane PCM and 32-bit left-slot DMA words.

use intercom_media::{PcmFrame, AUDIO_FRAME_SAMPLES};

pub(crate) const DMA_FRAME_BYTES: usize = AUDIO_FRAME_SAMPLES * 4;

/// The codec's 16-bit I2S word occupies bits 31:16 of each 32-bit left slot.
/// esp-hal's little-endian DMA layout stores those bits in bytes 2 and 3.
pub(crate) fn decode_frame(raw: &[u8; DMA_FRAME_BYTES]) -> PcmFrame {
    let mut frame = PcmFrame::SILENCE;
    for (sample, slot) in frame.samples.iter_mut().zip(raw.as_chunks::<4>().0) {
        *sample = i16::from_le_bytes([slot[2], slot[3]]);
    }
    frame
}

pub(crate) fn encode_frame(frame: &PcmFrame, out: &mut [u8; DMA_FRAME_BYTES]) {
    for (sample, slot) in frame.samples.iter().zip(out.as_chunks_mut::<4>().0) {
        let [lo, hi] = sample.to_le_bytes();
        slot.copy_from_slice(&[0, 0, lo, hi]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_signed_pcm_in_high_half_of_little_endian_dma_words() {
        // Arrange
        let mut frame = PcmFrame::SILENCE;
        frame.samples[..6].copy_from_slice(&[0, 1, -1, i16::MIN, i16::MAX, 0x1234]);
        let mut raw = [0xff; DMA_FRAME_BYTES];

        // Act
        encode_frame(&frame, &mut raw);

        // Assert
        assert_eq!(
            &raw[..24],
            &[
                0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0xff, 0xff, 0, 0, 0, 0x80, 0, 0, 0xff, 0x7f, 0, 0,
                0x34, 0x12,
            ]
        );
        assert!(raw[24..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn decodes_high_half_and_ignores_unused_low_bits() {
        // Arrange
        let mut raw = [0; DMA_FRAME_BYTES];
        raw[..24].copy_from_slice(&[
            0xff, 0xff, 0, 0, 0xff, 0xff, 1, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0x80, 0xff,
            0xff, 0xff, 0x7f, 0xff, 0xff, 0x34, 0x12,
        ]);

        // Act
        let frame = decode_frame(&raw);

        // Assert
        assert_eq!(&frame.samples[..6], &[0, 1, -1, i16::MIN, i16::MAX, 0x1234]);
        assert!(frame.samples[6..].iter().all(|sample| *sample == 0));
    }

    #[test]
    fn every_i16_sample_round_trips_without_channel_duplication() {
        // Arrange
        let mut raw = [0; DMA_FRAME_BYTES];
        let mut frame = PcmFrame::SILENCE;

        // Act / Assert: use distinct adjacent samples and cover the entire signed range.
        for start in (0..=u16::MAX as usize).step_by(AUDIO_FRAME_SAMPLES) {
            for (index, sample) in frame.samples.iter_mut().enumerate() {
                *sample = (start + index) as i16;
            }
            encode_frame(&frame, &mut raw);
            assert_eq!(decode_frame(&raw), frame);
        }
    }
}
