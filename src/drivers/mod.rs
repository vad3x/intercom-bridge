//! Smart Intercom - Drivers
//!
//! Hardware abstraction layer, organized by peripheral type:
//! - `gpio`   — GPIO drivers and traits (ring, door, LED)
//! - `matter` — Matter protocol clusters and node definition

pub mod gpio;
pub mod matter;
