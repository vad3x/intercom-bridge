//! Startup handshake between I2S DMA clocks and the external codec reset.

/// Error reported when the I2S DMA worker cannot start and provide clocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioWorkerStartupError {
    WorkerFailed,
}

/// Minimal reset control needed by the startup handshake.
pub trait CodecResetControl {
    fn release_reset(&mut self);
}

/// Release codec reset only after the I2S worker has started successfully.
pub fn release_codec_after_i2s_start(
    worker_result: Result<(), AudioWorkerStartupError>,
    codec: &mut impl CodecResetControl,
) -> Result<(), AudioWorkerStartupError> {
    worker_result?;
    codec.release_reset();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{release_codec_after_i2s_start, AudioWorkerStartupError, CodecResetControl};

    struct MockCodecReset {
        released: bool,
    }

    impl CodecResetControl for MockCodecReset {
        fn release_reset(&mut self) {
            self.released = true;
        }
    }

    #[test]
    fn waits_for_i2s_before_releasing_codec() {
        // Arrange
        let mut codec = MockCodecReset { released: false };

        // Act
        let started = release_codec_after_i2s_start(Ok(()), &mut codec);

        // Assert
        assert!(started.is_ok());
        assert!(codec.released);

        // Arrange
        let mut failed_codec = MockCodecReset { released: false };

        // Act
        let failed = release_codec_after_i2s_start(
            Err(AudioWorkerStartupError::WorkerFailed),
            &mut failed_codec,
        );

        // Assert
        assert_eq!(failed, Err(AudioWorkerStartupError::WorkerFailed));
        assert!(!failed_codec.released);
    }
}
