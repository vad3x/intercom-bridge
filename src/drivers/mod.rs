//! Smart Intercom - Drivers
//!
//! Hardware abstraction layer, organized by peripheral type:
//! - `flash`  — ESP32 internal SPI flash (NOR flash for Matter persistence)
//! - `gpio`   — GPIO drivers and traits (ring, door, LED)
//! - `matter` — Matter protocol clusters and node definition

pub mod flash;
pub mod gpio;
pub mod matter;
