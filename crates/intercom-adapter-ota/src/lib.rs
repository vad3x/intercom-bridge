//! Smart Intercom - Over-The-Air (OTA) Update Adapter
//!
//! Provides bootloader state manipulation, streaming flash writing with Ed25519
//! signature verification, and an asynchronous HTTP server for firmware updates.

#![no_std]

extern crate alloc;

pub mod bootloader;
pub mod writer;

pub use bootloader::{
    ActiveSlot, BootloaderError, EspOtaSelectEntry, MemOtaFlash, OtaBootloader, OtaFlashStorage,
    OtaState, Slot, OTA_DATA_ENTRY_SIZE, OTA_DATA_SECTOR_SIZE,
};

pub use writer::{
    validate_esp_image_header, verify_image_signature, DefaultTargetFlash, MemTargetFlash,
    OtaError, OtaTargetFlash, OtaWriter, SectorBuffer, ESP32_C6_CHIP_ID, ESP_IMAGE_MAGIC,
    MAX_APP_SLOT_SIZE, OTA_0_OFFSET, OTA_1_OFFSET, OTA_HEADER_LEN, OTA_HEADER_MAGIC, OTA_SLOT_SIZE,
    SECTOR_SIZE,
};

#[cfg(target_arch = "riscv32")]
pub use writer::{EspFlashError, EspTargetFlash};
