//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;
pub mod diagnostics;
pub mod ecosystem;
pub mod reset;

pub use access::{AccessEffects, AccessStateMachine};
pub use diagnostics::{IntercomState, SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo, FabricInfo,
    FabricLabel, MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS, MAX_ECOSYSTEM_PROPERTIES,
};
pub use reset::{classify_reset_hold, ResetScope};
