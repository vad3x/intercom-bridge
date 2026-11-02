//! ESP32-C6 I2S0 GDMA streaming with 16 kHz PCM in 32-bit left slots.
//!
//! Two 32-bit wire slots give BCLK = 64 * Fs = 1.024 MHz; MCLK = 256 * Fs =
//! 4.096 MHz. Channels::LEFT transfers only the left 32-bit word through DMA,
//! and fills the inactive right slot with zero. The codec uses 16-bit I2S words.
//!
//! PCB v2 uses GPIO22 MCLK, GPIO23 BCLK, GPIO15 WS, GPIO20 DOUT, GPIO21 DIN.
//! GPIO15 is MTDO. R11 holds CODEC_RST_N (GPIO11) low during ROM boot; start
//! these clocks before releasing the codec reset. RX shares TX's clocks through
//! signal loopback, so TX must stream silence continuously when idle.

use embassy_futures::select::{select, Either};
use esp_hal::dma::{DmaError, DmaRxStreamBuf, DmaTxStreamBuf};
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2s::master::{
    Channels, ConfigError, DataFormat, Error as I2sError, I2s, I2sMasterDmaChannel,
    I2sRxDmaTransfer, I2sTxDmaTransfer, Instance, TdmConfig,
};
use esp_hal::time::Rate;
use esp_hal::Async;
use intercom_core::domain::diagnostics::{is_tone_active, update_rx_levels_dbfs};
use intercom_media::{
    compute_frame_levels_dbfs, AudioFrameSink, AudioFrameSource, BenchToneGenerator, PcmFrame,
    RxAudioSender, TxAudioReceiver, AUDIO_SAMPLE_RATE_HZ,
};

use crate::pcm_slots::{decode_frame, encode_frame, DMA_FRAME_BYTES};

/// Owns the continuous RX and TX streams, including their static DMA buffers.
pub struct I2sAudioPipeline {
    rx_transfer: I2sRxDmaTransfer<'static, Async, DmaRxStreamBuf>,
    tx_transfer: I2sTxDmaTransfer<'static, Async, DmaTxStreamBuf>,
}

/// Configuration or DMA startup failure.
#[derive(Debug)]
pub enum PipelineError {
    Config(ConfigError),
    Start(I2sError),
}

impl I2sAudioPipeline {
    /// Starts shared I2S clocks and both DMA streams before codec reset is released.
    #[allow(clippy::too_many_arguments)]
    pub fn new<I: Instance + 'static>(
        i2s_periph: I,
        dma_channel: impl I2sMasterDmaChannel<'static, I> + 'static,
        mclk_pin: impl PeripheralOutput<'static>,
        bclk_pin: impl PeripheralOutput<'static>,
        ws_pin: impl PeripheralOutput<'static>,
        dout_pin: impl PeripheralOutput<'static>,
        din_pin: impl PeripheralInput<'static>,
    ) -> Result<Self, PipelineError> {
        // Four 20 ms descriptors provide 80 ms of scheduling headroom.
        let rx_buffer = esp_hal::dma_rx_stream_buffer!(DMA_STREAM_BYTES, DMA_FRAME_BYTES);
        let mut tx_buffer = esp_hal::dma_tx_stream_buffer!(DMA_STREAM_BYTES, DMA_FRAME_BYTES);
        tx_buffer.push_with(|buffer| {
            buffer.fill(0);
            buffer.len()
        });

        let config = TdmConfig::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(AUDIO_SAMPLE_RATE_HZ))
            .with_data_format(DataFormat::Data32Channel32)
            .with_channels(Channels::LEFT)
            .with_signal_loopback(true);
        let i2s = I2s::new(i2s_periph, dma_channel, config)
            .map_err(PipelineError::Config)?
            .into_async()
            .with_mclk(mclk_pin);
        let tx = i2s
            .i2s_tx
            .with_bclk(bclk_pin)
            .with_ws(ws_pin)
            .with_dout(dout_pin)
            .build();
        let rx = i2s.i2s_rx.with_din(din_pin).build();
        let tx_transfer = tx
            .write(tx_buffer)
            .map_err(|(error, _, _)| PipelineError::Start(error))?;
        let rx_transfer = rx
            .read(rx_buffer)
            .map_err(|(error, _, _)| PipelineError::Start(error))?;
        Ok(Self {
            rx_transfer,
            tx_transfer,
        })
    }

    /// Runs until a DMA failure. Dropping the pipeline then stops both streams.
    pub async fn run(
        mut self,
        rx_sender: RxAudioSender<'_>,
        tx_receiver: TxAudioReceiver<'_>,
    ) -> Result<(), DmaError> {
        // A failed stream cannot progress. Return its error so the composition
        // root can reset safely instead of repeatedly polling a stopped DMA.
        match select(
            Self::receive_frames(&mut self.rx_transfer, rx_sender),
            Self::transmit_frames(&mut self.tx_transfer, tx_receiver),
        )
        .await
        {
            Either::First(result) | Either::Second(result) => result,
        }
    }

    async fn receive_frames(
        transfer: &mut I2sRxDmaTransfer<'static, Async, DmaRxStreamBuf>,
        mut sender: RxAudioSender<'_>,
    ) -> Result<(), DmaError> {
        loop {
            while transfer.available_bytes() < DMA_FRAME_BYTES {
                transfer.wait_for_available_async().await?;
            }
            // One complete frame per descriptor keeps every read aligned and
            // avoids retaining an expanded raw frame in the async task storage.
            let raw = transfer
                .peek()
                .first_chunk::<DMA_FRAME_BYTES>()
                .ok_or(DmaError::BufferTooSmall)?;
            let frame = decode_frame(raw);
            transfer.consume(DMA_FRAME_BYTES);
            let levels = compute_frame_levels_dbfs(&frame);
            update_rx_levels_dbfs(levels.rms_dbfs, levels.peak_dbfs);
            let _ = sender.try_send_frame(frame);
        }
    }

    async fn transmit_frames(
        transfer: &mut I2sTxDmaTransfer<'static, Async, DmaTxStreamBuf>,
        mut receiver: TxAudioReceiver<'_>,
    ) -> Result<(), DmaError> {
        let mut tone = BenchToneGenerator::new();
        loop {
            // Wait before dequeuing PCM so neither a raw frame nor a PCM frame
            // remains live across the wait. Every descriptor is one full frame.
            while transfer.available_bytes() < DMA_FRAME_BYTES {
                transfer.wait_for_available_async().await?;
            }
            let frame = if is_tone_active() {
                tone.next_frame()
            } else {
                receiver.try_receive_frame().unwrap_or(PcmFrame::SILENCE)
            };
            let written = transfer.push_with(|buffer| {
                if let Some(raw) = buffer.first_chunk_mut::<DMA_FRAME_BYTES>() {
                    encode_frame(&frame, raw);
                    DMA_FRAME_BYTES
                } else {
                    0
                }
            });
            if written != DMA_FRAME_BYTES {
                return Err(DmaError::BufferTooSmall);
            }
        }
    }
}

const DMA_STREAM_BYTES: usize = DMA_FRAME_BYTES * 4;
