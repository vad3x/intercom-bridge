//! PCB v2 TLV320AIC3204 I2C codec driver and ESP32-C6 16 kHz I2S GDMA audio pipeline.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod aic3204;
#[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
pub mod i2s_pipeline;
pub mod registers;
#[cfg(feature = "esp32s31")]
pub mod synthetic_pipeline;

pub use aic3204::{Aic3204AudioDriver, CodecError};

/// Selects whether I2S input or the composition root supplies receive audio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2sCaptureSource {
    Codec,
    EmbeddedTestSample,
}

/// Whether captured I2S frames should be forwarded to the media receive queue.
pub const fn forwards_hardware_capture(source: I2sCaptureSource) -> bool {
    matches!(source, I2sCaptureSource::Codec)
}

#[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
pub use i2s_pipeline::I2sAudioPipeline;
#[cfg(feature = "esp32s31")]
pub use synthetic_pipeline::SyntheticAudioPipeline;

#[cfg(any(test, target_os = "none"))]
#[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
mod pcm_slots;

#[cfg(test)]
mod input_source_tests {
    use super::{forwards_hardware_capture, I2sCaptureSource};

    #[test]
    fn embedded_sample_replaces_codec_capture() {
        assert!(forwards_hardware_capture(I2sCaptureSource::Codec));
        assert!(!forwards_hardware_capture(
            I2sCaptureSource::EmbeddedTestSample
        ));
    }
}
