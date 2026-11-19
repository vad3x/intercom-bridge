//! Intercom Bridge - Firmware Library
//!
//! Exposes utility modules, configurations, and version metadata.
//! This library target enables host-based unit testing of non-hardware logic.
#![no_std]

#[cfg(test)]
extern crate std;

pub mod config;
pub mod cpu_load;
pub mod devkit_led;
pub mod disabled_ota;
pub mod hardware_info;
pub mod platform;
pub mod reset;
pub mod utils;
pub mod version;
