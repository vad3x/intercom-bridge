//! Smart Intercom - GPIO Drivers
//!
//! Hardware driver implementations for direct GPIO peripherals:
//! - `door`  — Door unlock via PhotoMOS relay
//! - `led`   — Status LED (active-low)
//! - `ring`  — Ring detection via optocoupler
//!
//! Driver traits are defined in `src/actors/` — each actor defines the
//! trait it needs, and the GPIO drivers here implement them.

pub mod door;
pub mod led;
pub mod ring;
