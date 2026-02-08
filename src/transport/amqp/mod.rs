//! AMQP protocol transports.
//!
//! This module contains transport implementations for AMQP-based brokers.
//! Currently supports:
//! - lapin - AMQP 0-9-1 (RabbitMQ)

#[cfg(feature = "transport_lapin")]
mod lapin;

#[cfg(feature = "transport_lapin")]
pub use lapin::create_transport as create_lapin_transport;
