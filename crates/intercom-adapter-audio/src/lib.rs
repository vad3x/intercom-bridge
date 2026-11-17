//! PCB v2 TLV320AIC3204 I2C codec driver and ESP32-C6 16 kHz I2S GDMA audio pipeline.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod aic3204;
#[cfg(all(target_arch = "riscv32", feature = "esp32c6"))]
pub mod i2s_pipeline;
pub mod registers;
#[cfg(feature = "esp32s31")]
pub mod synthetic_pipeline;

pub use aic3204::{Aic3204AudioDriver, CodecError};
#[cfg(all(target_arch = "riscv32", feature = "esp32c6"))]
pub use i2s_pipeline::I2sAudioPipeline;
#[cfg(feature = "esp32s31")]
pub use synthetic_pipeline::SyntheticAudioPipeline;

#[cfg(any(test, target_arch = "riscv32"))]
#[cfg(feature = "esp32c6")]
mod pcm_slots;
