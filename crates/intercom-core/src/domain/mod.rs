//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;
pub mod diagnostics;
pub mod ecosystem;

pub use access::{AccessEffects, AccessStateMachine};
pub use diagnostics::{SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, MAX_ECOSYSTEM_PROPERTIES,
};
