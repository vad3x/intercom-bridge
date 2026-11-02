//! Pure no_std Media Plane PCM frame, static channel, dBFS meter, and 1 kHz tone primitives.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};

pub const AUDIO_SAMPLE_RATE_HZ: u32 = 16_000;
pub const AUDIO_FRAME_SAMPLES: usize = 320;
pub const RX_AUDIO_QUEUE_CAP: usize = 2;
pub const TX_AUDIO_QUEUE_CAP: usize = 3;
pub const SILENCE_DBFS: i8 = -96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmFrame {
    pub samples: [i16; AUDIO_FRAME_SAMPLES],
}

impl PcmFrame {
    pub const SILENCE: Self = Self {
        samples: [0; AUDIO_FRAME_SAMPLES],
    };
}

impl Default for PcmFrame {
    fn default() -> Self {
        Self::SILENCE
    }
}

pub trait AudioFrameSource {
    async fn receive_frame(&mut self) -> PcmFrame;
    fn try_receive_frame(&mut self) -> Option<PcmFrame>;
}

pub trait AudioFrameSink {
    fn try_send_frame(&mut self, frame: PcmFrame) -> Result<(), PcmFrame>;
}

pub type RxAudioChannel = Channel<CriticalSectionRawMutex, PcmFrame, RX_AUDIO_QUEUE_CAP>;
pub type RxAudioSender<'a> = Sender<'a, CriticalSectionRawMutex, PcmFrame, RX_AUDIO_QUEUE_CAP>;
pub type RxAudioReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, PcmFrame, RX_AUDIO_QUEUE_CAP>;
pub type TxAudioChannel = Channel<CriticalSectionRawMutex, PcmFrame, TX_AUDIO_QUEUE_CAP>;
pub type TxAudioSender<'a> = Sender<'a, CriticalSectionRawMutex, PcmFrame, TX_AUDIO_QUEUE_CAP>;
pub type TxAudioReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, PcmFrame, TX_AUDIO_QUEUE_CAP>;

impl<const N: usize> AudioFrameSource for Receiver<'_, CriticalSectionRawMutex, PcmFrame, N> {
    async fn receive_frame(&mut self) -> PcmFrame {
        self.receive().await
    }

    fn try_receive_frame(&mut self) -> Option<PcmFrame> {
        self.try_receive().ok()
    }
}

impl<const N: usize> AudioFrameSink for Sender<'_, CriticalSectionRawMutex, PcmFrame, N> {
    fn try_send_frame(&mut self, frame: PcmFrame) -> Result<(), PcmFrame> {
        self.try_send(frame)
            .map_err(|embassy_sync::channel::TrySendError::Full(frame)| frame)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLevelsDbfs {
    pub rms_dbfs: i8,
    pub peak_dbfs: i8,
}

pub fn compute_frame_levels_dbfs(frame: &PcmFrame) -> FrameLevelsDbfs {
    let mut sum_sq: u64 = 0;
    let mut peak: u16 = 0;
    let mut i = 0;
    while i < AUDIO_FRAME_SAMPLES {
        let mag = frame.samples[i].unsigned_abs();
        if mag > peak {
            peak = mag;
        }
        sum_sq += (mag as u64) * (mag as u64);
        i += 1;
    }
    let mean_sq = sum_sq / AUDIO_FRAME_SAMPLES as u64;
    let rms = isqrt_u64(mean_sq) as u16;
    FrameLevelsDbfs {
        rms_dbfs: amplitude_to_dbfs(rms),
        peak_dbfs: amplitude_to_dbfs(peak),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BenchToneGenerator {
    phase: usize,
}

impl BenchToneGenerator {
    pub const fn new() -> Self {
        Self { phase: 0 }
    }
    pub fn next_frame(&mut self) -> PcmFrame {
        let mut samples = [0i16; AUDIO_FRAME_SAMPLES];
        let mut idx = 0;
        while idx < AUDIO_FRAME_SAMPLES {
            samples[idx] = SINE_1KHZ_LUT[self.phase & 0x0f];
            self.phase = (self.phase + 1) & 0x0f;
            idx += 1;
        }
        PcmFrame { samples }
    }
}

/// 16-sample 1 kHz sine LUT at 16 kHz with peak 23,170 (-3.0 dBFS).
const SINE_1KHZ_LUT: [i16; 16] = [
    0, 8_867, 16_384, 21_406, 23_170, 21_406, 16_384, 8_867, 0, -8_867, -16_384, -21_406, -23_170,
    -21_406, -16_384, -8_867,
];

fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) >> 1;
    while y < x {
        x = y;
        y = (x + n / x) >> 1;
    }
    x
}

/// Integer approximation of `20 * log10(amp / 32767)` clamped to `[-96, 0]` dBFS.
fn amplitude_to_dbfs(amp: u16) -> i8 {
    if amp == 0 {
        return SILENCE_DBFS;
    }
    if amp >= 32_767 {
        return 0;
    }
    let octaves_down = (amp.leading_zeros() - 1) as i32;
    let norm = (amp as u32) << octaves_down;
    let frac_db = (((32_767u32 - norm) * 602) / 16_384) as i32;
    let total_hundredths = -(octaves_down * 602 + frac_db);
    ((total_hundredths - 50) / 100).clamp(SILENCE_DBFS as i32, 0) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_frame_measures_minus_96_dbfs() {
        // Arrange
        let frame = PcmFrame::SILENCE;
        // Act
        let levels = compute_frame_levels_dbfs(&frame);
        // Assert
        assert_eq!(
            levels,
            FrameLevelsDbfs {
                rms_dbfs: -96,
                peak_dbfs: -96
            }
        );
    }

    #[test]
    fn full_scale_square_wave_measures_zero_dbfs_rms_and_peak() {
        // Arrange
        let frame = PcmFrame {
            samples: [32_767; AUDIO_FRAME_SAMPLES],
        };
        // Act
        let levels = compute_frame_levels_dbfs(&frame);
        // Assert
        assert_eq!(
            levels,
            FrameLevelsDbfs {
                rms_dbfs: 0,
                peak_dbfs: 0
            }
        );
    }

    #[test]
    fn bench_tone_generator_emits_exact_16_sample_1khz_sine_at_minus_3_dbfs_peak() {
        // Arrange
        let mut gen = BenchToneGenerator::new();
        // Act
        let frame = gen.next_frame();
        let levels = compute_frame_levels_dbfs(&frame);
        // Assert
        assert_eq!(frame.samples[0], 0);
        assert_eq!(frame.samples[4], 23_170);
        assert_eq!(frame.samples[8], 0);
        assert_eq!(frame.samples[12], -23_170);
        assert_eq!(frame.samples[16], frame.samples[0]);
        assert!((-4..=-3).contains(&levels.peak_dbfs));
        assert!((-7..=-5).contains(&levels.rms_dbfs));
    }

    #[test]
    fn receive_channel_has_two_frame_nonblocking_capacity() {
        // Arrange
        let channel = RxAudioChannel::new();
        let sender = channel.sender();
        let receiver = channel.receiver();
        let first = PcmFrame {
            samples: [1; AUDIO_FRAME_SAMPLES],
        };
        let second = PcmFrame {
            samples: [2; AUDIO_FRAME_SAMPLES],
        };
        let overflow = PcmFrame {
            samples: [3; AUDIO_FRAME_SAMPLES],
        };
        // Act
        let first_result = sender.try_send(first);
        let second_result = sender.try_send(second);
        let overflow_result = sender
            .try_send(overflow)
            .map_err(|embassy_sync::channel::TrySendError::Full(frame)| frame);
        let received_first = receiver.try_receive().ok();
        let received_second = receiver.try_receive().ok();
        let received_empty = receiver.try_receive().ok();
        // Assert
        assert_eq!(first_result, Ok(()));
        assert_eq!(second_result, Ok(()));
        assert_eq!(overflow_result, Err(overflow));
        assert_eq!(received_first, Some(first));
        assert_eq!(received_second, Some(second));
        assert_eq!(received_empty, None);
    }

    #[test]
    fn transmit_channel_has_three_frame_nonblocking_capacity_and_trait_adapters() {
        // Arrange
        let channel = TxAudioChannel::new();
        let mut sender = channel.sender();
        let mut receiver = channel.receiver();
        let first = PcmFrame {
            samples: [11; AUDIO_FRAME_SAMPLES],
        };
        let second = PcmFrame {
            samples: [22; AUDIO_FRAME_SAMPLES],
        };
        let third = PcmFrame {
            samples: [33; AUDIO_FRAME_SAMPLES],
        };
        let overflow = PcmFrame {
            samples: [44; AUDIO_FRAME_SAMPLES],
        };
        // Act
        let first_result = AudioFrameSink::try_send_frame(&mut sender, first);
        let second_result = AudioFrameSink::try_send_frame(&mut sender, second);
        let third_result = AudioFrameSink::try_send_frame(&mut sender, third);
        let overflow_result = AudioFrameSink::try_send_frame(&mut sender, overflow);
        let received_first = AudioFrameSource::try_receive_frame(&mut receiver);
        let received_second = AudioFrameSource::try_receive_frame(&mut receiver);
        let received_third = AudioFrameSource::try_receive_frame(&mut receiver);
        let received_empty = AudioFrameSource::try_receive_frame(&mut receiver);
        // Assert
        assert_eq!(first_result, Ok(()));
        assert_eq!(second_result, Ok(()));
        assert_eq!(third_result, Ok(()));
        assert_eq!(overflow_result, Err(overflow));
        assert_eq!(received_first, Some(first));
        assert_eq!(received_second, Some(second));
        assert_eq!(received_third, Some(third));
        assert_eq!(received_empty, None);
    }

    #[test]
    fn pcm_frame_has_copy_default_and_equality_traits() {
        // Arrange
        fn assert_traits<T: Copy + Default + PartialEq + Eq>() {}
        // Act
        assert_traits::<PcmFrame>();
        let frame = PcmFrame::default();
        let copied = frame;
        // Assert
        assert_eq!(copied, PcmFrame::SILENCE);
    }
}
