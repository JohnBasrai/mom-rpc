//! DDS protocol transports.
//!
//! This module contains transport implementations for DDS (Data Distribution Service).
//! DDS is a brokerless, peer-to-peer middleware using RTPS wire protocol.
//! Currently supports:
//! - rustdds - Pure Rust DDS implementation with RTPS over UDP

#[cfg(feature = "transport_rustdds")]
mod rustdds;

#[cfg(feature = "transport_rustdds")]
pub use rustdds::create_transport as create_rustdds_transport;
