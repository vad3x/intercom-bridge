//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;
pub mod diagnostics;

pub use access::{AccessEffects, AccessStateMachine};
pub use diagnostics::{SystemDiagnostics, WifiSsid};
