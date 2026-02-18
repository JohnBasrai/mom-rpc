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
//!
//! Each protocol sub-module uses the Null Object pattern: all factory
//! functions are always exported regardless of feature flags. When a
//! feature is disabled, the function returns a clear runtime error
//! instead of causing unresolved symbol errors at compile time.

mod amqp;
mod dds;
mod memory;
mod mqtt;

// Memory transport - process-global factory is internal; users use TransportBuilder
pub(crate) use memory::create_memory_transport;

// Memory transport testing utilities - pub for integration tests
// WARNING: These APIs are reserved for mom-rpc's own integration tests and may
// change without notice. Use TransportBuilder for production code.
pub use memory::create_memory_transport_with_hub;
pub use memory::MemoryHub;

// Protocol transports - always exported via Null Object pattern.
// Disabled transports return RpcError::Transport at runtime.
pub use amqp::create_lapin_transport;
pub use dds::create_dust_dds_transport;
pub use mqtt::create_rumqttc_transport;
