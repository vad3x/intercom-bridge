//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;

pub use access::{AccessEffects, AccessStateMachine};
