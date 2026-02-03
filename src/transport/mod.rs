//! Transport implementations.
//!
//! This module provides concrete implementations of the domain-level
//! `Transport` trait. All transports are hidden behind feature flags
//! and exposed only through constructor functions.
//!
//! Domain code must not depend on transport-specific types.
mod memory;

#[cfg(feature = "transport-mqtt-async-client")]
mod mqtt_async_client;

#[cfg(feature = "transport-mqtt-async-client")]
pub use mqtt_async_client::create_transport as create_mqtt_async_client_transport;

#[allow(unused)]
pub use memory::create_transport as create_memory_transport;
