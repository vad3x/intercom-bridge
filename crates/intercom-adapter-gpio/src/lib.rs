//! Intercom Bridge - Hardware GPIO Adapter
//!
//! Provides concrete drivers implementing `intercom_core::ports` traits
//! for physical intercom hardware (e.g. IR204E) via `esp-hal`.

#![no_std]
#![allow(async_fn_in_trait)]

pub mod boot_button;
pub mod config;
pub mod door;
pub mod flash;
pub mod led;
pub mod ring;

pub use boot_button::{GpioBootButtonConfig, GpioBootButtonDriver};
pub use door::{GpioDoorConfig, GpioDoorDriver};
pub use flash::{EspFlash, EspFlashError};
pub use led::GpioLedDriver;
pub use ring::{GpioRingConfig, GpioRingDriver};
