//! DDS protocol transports.
//!
//! This module contains transport implementations for DDS (Data Distribution Service).
//! DDS is a brokerless, peer-to-peer middleware using RTPS wire protocol.
//! Currently supports:
//! - dustdds - Pure Rust DDS implementation with RTPS over UDP

#[cfg(feature = "transport_dustdds")]
mod dustdds;

#[cfg(feature = "transport_dustdds")]
pub use dustdds::create_transport as create_dustdds_transport;
