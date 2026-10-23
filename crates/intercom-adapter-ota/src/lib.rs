//! Smart Intercom - Over-The-Air (OTA) Update Adapter
//!
//! Compatibility re-exports from `edf-ota`.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub use edf_ota::*;

pub mod bootloader {
    pub use edf_ota::bootloader::*;
}

pub mod driver {
    pub use edf_ota::driver::*;
}

pub mod writer {
    pub use edf_ota::writer::*;
}
