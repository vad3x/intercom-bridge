//! Smart Intercom - Actors
//!
//! Each actor is a separate struct with an async `run()` method.
//! Actors communicate through the `PubSubChannel` event bus, mirroring
//! the C++ actor/observer pattern.
//!
//! Actors receive event bus handles (Publisher/Subscriber) as `run()` parameters,
//! making them fully testable without statics.

pub mod access_control;
pub mod door;
pub mod indicator;
pub mod ring;
