//! Smart Intercom - Over-The-Air (OTA) Update Adapter
//!
//! Provides bootloader state manipulation, streaming flash writing with Ed25519
//! signature verification, and an asynchronous HTTP server for firmware updates.

#![no_std]

extern crate alloc;

pub mod bootloader;
pub mod server;
pub mod writer;

pub use server::{
    find_headers_end, handle_connection_with, HttpParseError, OtaOutcome, OtaServer,
    OtaServerConfig, OtaServerError, OtaStream, RequestKind, parse_request_headers,
    COOPERATIVE_YIELD_DELAY, HTTP_200_OK, HTTP_400_BAD_REQUEST, MAX_OTA_PAYLOAD_SIZE,
    OTA_FLASH_PATH, REBOOT_DELAY, TARPIT_DELAY,
};

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
