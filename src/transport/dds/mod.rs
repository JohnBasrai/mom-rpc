//! DDS protocol transports.
//!
//! This module contains transport implementations for DDS (Data Distribution Service).
//! DDS is a brokerless, peer-to-peer middleware using RTPS wire protocol.
//! Currently supports:
//! - dust_dds - Pure Rust DDS implementation with RTPS over UDP

#[cfg(feature = "transport_dust_dds")]
mod dust_dds;

#[cfg(feature = "transport_dust_dds")]
pub use dust_dds::create_transport as create_dust_dds_transport;
