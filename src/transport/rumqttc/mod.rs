//! MQTT transport implementation based on rumqttc.
//!
//! This module adapts the rumqttc API to the domain-level `Transport` trait
//! without leaking MQTT concepts upward.
//!
//! # Features
//!
//! - Actor-based concurrency model with single EventLoop ownership
//! - SUBACK confirmation before returning from subscribe()
//! - Info-level logging for subscription events
//! - Lazy connection on EventLoop startup
//!
//! # Usage
//!
//! Enable the `transport_rumqttc` feature in your Cargo.toml:
//!
//! ```toml
//! [dependencies]
//! mom-rpc = { version = "0.2", features = ["transport_rumqttc"] }
//! ```

mod transport;
pub use transport::create_transport;
