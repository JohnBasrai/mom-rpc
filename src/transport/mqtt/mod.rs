//! MQTT protocol transports.
//!
//! This module contains transport implementations for MQTT brokers.
//! Currently supports:
//! - rumqttc - MQTT 3.1.1 and 5.0

#[cfg(feature = "transport_rumqttc")]
mod rumqttc;

#[cfg(feature = "transport_rumqttc")]
pub use rumqttc::create_transport as create_rumqttc_transport;

#[cfg(not(feature = "transport_rumqttc"))]
pub async fn create_rumqttc_transport(
    _config: crate::TransportConfig,
) -> crate::Result<crate::TransportPtr> {
    Err(crate::RpcError::Transport(
        "transport_rumqttc feature is not enabled".into(),
    ))
}
