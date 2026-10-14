//! Intercom Bridge - Universal Core
//!
//! Domain logic, event bus, actors, and abstract port traits.
//! Zero hardware or network stack dependencies.

#![cfg_attr(not(test), no_std)]
#![allow(async_fn_in_trait)]

pub mod actors;
pub mod app;
pub mod config;
pub mod events;
pub mod ports;

pub use app::App;
pub use events::{Event, EventBus, EventBusExt, SystemState};
