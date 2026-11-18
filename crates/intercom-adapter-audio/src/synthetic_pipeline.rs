//! Synthetic 16 kHz audio source and sink for targets without an I2S driver.

use embassy_time::{Instant, Timer};
use intercom_core::domain::diagnostics::update_rx_levels_dbfs;
use intercom_media::{
    AudioCaptureSchedule, AudioFrameDropWindow, AudioFrameSink, AudioFrameSource,
    BenchToneGenerator, RxAudioSender, TxAudioReceiver, AUDIO_FRAME_DROP_WINDOW_FRAMES,
    AUDIO_SAMPLE_RATE_HZ,
};

/// A benchmark audio source and sink that exercises the firmware PCM channels.
pub struct SyntheticAudioPipeline {
    tone: BenchToneGenerator,
}

impl SyntheticAudioPipeline {
    pub const fn new() -> Self {
        Self {
            tone: BenchToneGenerator::new(),
        }
    }

    /// Generate the next 20 ms, 1 kHz benchmark frame.
    pub fn next_capture_frame(&mut self) -> intercom_media::PcmFrame {
        self.tone.next_frame()
    }

    /// Run the synthetic capture source and drain playback frames indefinitely.
    pub async fn run(
        mut self,
        mut rx_sender: RxAudioSender<'_>,
        mut tx_receiver: TxAudioReceiver<'_>,
    ) -> ! {
        let mut schedule = AudioCaptureSchedule::new(Instant::now().as_millis());
        let mut drop_window = AudioFrameDropWindow::new();
        let mut simulated_overruns = 0u64;

        loop {
            let now_ms = Instant::now().as_millis();
            Timer::after_millis(schedule.delay_until_next_ms(now_ms)).await;
            let batch = schedule.take_due_frames(Instant::now().as_millis());
            simulated_overruns = simulated_overruns.saturating_add(batch.overruns);

            for _ in 0..batch.frames {
                let frame = self.next_capture_frame();
                let levels = intercom_media::compute_frame_levels_dbfs(&frame);
                update_rx_levels_dbfs(levels.rms_dbfs, levels.peak_dbfs);
                let queue_dropped = rx_sender.try_send_frame(frame).is_err();
                let _ = tx_receiver.try_receive_frame();

                if let Some(queue_drops) = drop_window.record(queue_dropped) {
                    log::info!(
                        "Synthetic audio input frames={} sample_rate_hz={} simulated_overruns={} queue_drops={}",
                        AUDIO_FRAME_DROP_WINDOW_FRAMES,
                        AUDIO_SAMPLE_RATE_HZ,
                        simulated_overruns,
                        queue_drops,
                    );
                    simulated_overruns = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyntheticAudioPipeline;
    use intercom_media::compute_frame_levels_dbfs;

    #[test]
    fn synthetic_pipeline_generates_benchmark_tone_frames() {
        let mut pipeline = SyntheticAudioPipeline::new();

        let first = pipeline.next_capture_frame();
        let second = pipeline.next_capture_frame();

        let levels = compute_frame_levels_dbfs(&first);
        assert_eq!(levels.peak_dbfs, -3);
        assert_eq!(levels.rms_dbfs, -6);
        assert_eq!(first, second);
    }
}
