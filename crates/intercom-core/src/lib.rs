//! Intercom Bridge - Universal Core
//!
//! Domain logic, state lifecycle watch, actors, and abstract port traits.
//! Zero hardware or network stack dependencies.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod actors;
pub mod app;
pub mod config;
pub mod domain;
pub mod events;
pub mod ports;

pub use app::App;
pub use events::{SystemState, SystemStateReceiver, SystemStateSender, SystemStateWatch};
