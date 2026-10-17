//! Smart Intercom - Domain Logic
//!
//! Pure, synchronous business logic decoupled from async runtimes and I/O.

pub mod access;
pub mod commissioning;
pub mod diagnostics;
pub mod ecosystem;
pub mod reset;

pub use access::{AccessEffects, AccessStateMachine};
pub use commissioning::{
    discriminator_from_mac, hostname_from_mac, is_forbidden_passcode, mac_hash, passcode_from_mac,
    serial_no_from_mac, FORBIDDEN_PASSCODES, MAC_HOSTNAME_LEN, MAC_SERIAL_NO_LEN,
    MAX_DISCRIMINATOR, MAX_PASSCODE, MIN_DISCRIMINATOR, MIN_PASSCODE, TEST_DISCRIMINATOR,
};
pub use diagnostics::{SystemDiagnostics, WifiSsid};
pub use ecosystem::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, MAX_ECOSYSTEM_PROPERTIES,
};
pub use reset::{classify_reset_hold, ResetScope};

