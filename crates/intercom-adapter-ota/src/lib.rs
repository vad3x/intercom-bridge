//! Smart Intercom - Over-The-Air (OTA) Update Adapter
//!
//! Provides bootloader state manipulation, streaming flash writing with Ed25519
//! signature verification, and an asynchronous HTTP server for firmware updates.

#![no_std]

extern crate alloc;

pub mod bootloader;

pub use bootloader::{
    ActiveSlot, BootloaderError, EspOtaSelectEntry, MemOtaFlash, OtaBootloader, OtaFlashStorage,
    OtaState, Slot,
};
