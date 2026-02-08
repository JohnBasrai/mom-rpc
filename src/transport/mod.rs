//! Transport implementations.
//!
//! This module provides concrete implementations of the domain-level
//! `Transport` trait. All transports are organized by protocol, then
//! by library, and hidden behind feature flags.
//!
//! The memory transport is flat (always available, brokerless).
//! Protocol-based transports are organized under protocol directories.
//!
//! Domain code must not depend on transport-specific types.

mod memory;

#[cfg(feature = "transport_rumqttc")]
mod mqtt;

#[cfg(feature = "transport_lapin")]
mod amqp;

#[allow(unused)]
pub use memory::create_transport as create_memory_transport;

#[cfg(feature = "transport_rumqttc")]
pub use mqtt::create_rumqttc_transport;

#[cfg(feature = "transport_lapin")]
pub use amqp::create_lapin_transport;
