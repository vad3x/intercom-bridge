//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod diagnostics;
pub mod ecosystem;
pub mod intercom;
pub mod policy;
pub mod reset;

pub use diagnostics::{IntercomState, SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    ClusterInfo, EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, EndpointInfo,
    FabricInfo, FabricLabel, FabricSubscription, MAX_CLUSTERS_PER_ENDPOINT,
    MAX_ECOSYSTEM_ENDPOINTS, MAX_ECOSYSTEM_FABRICS, MAX_ECOSYSTEM_PROPERTIES,
    MAX_FABRIC_SUBSCRIPTIONS,
};
pub use reset::{classify_reset_hold, ResetScope};
