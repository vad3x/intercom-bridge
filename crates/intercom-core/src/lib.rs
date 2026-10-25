//! Intercom Bridge - Universal Core
//!
//! Domain logic, state lifecycle watch, application services, and abstract port traits.
//! Zero hardware or network stack dependencies.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod app;
pub mod config;
pub mod domain;
pub mod events;
pub mod ports;
pub mod service;

pub use app::App;
pub use domain::{
    EcosystemId, EcosystemStatus, EcosystemType, EcosystemValue, IntercomState, SystemDiagnostics,
    WifiSsid, MAX_ECOSYSTEM_PROPERTIES,
};
pub use events::{SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch};
pub use service::DiagnosticsService;
