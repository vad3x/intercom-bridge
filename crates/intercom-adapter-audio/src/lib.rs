//! PCB v2 TLV320AIC3204 I2C codec driver and ESP32-C6 16 kHz I2S GDMA audio pipeline.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod aic3204;
#[cfg(target_arch = "riscv32")]
pub mod i2s_pipeline;
pub mod registers;

pub use aic3204::{Aic3204AudioDriver, CodecError};
#[cfg(target_arch = "riscv32")]
pub use i2s_pipeline::I2sAudioPipeline;

#[cfg(any(test, target_arch = "riscv32"))]
mod pcm_slots;
