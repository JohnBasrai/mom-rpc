//! MQTT transport implementation based on mqtt-async-client.
//!
//! This module adapts the mqtt-async-client API to the domain-level
//! `Transport` trait without leaking MQTT concepts upward.

mod transport;

use super::SubscriptionManager;
pub use transport::create_transport;
