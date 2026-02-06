//! RPC semantics over MQTT pub/sub with automatic request/response correlation
//!
//! This library provides a simple, ergonomic API for implementing RPC patterns
//! over MQTT. It handles correlation ID generation, request/response matching,
//! timeout handling, and concurrent request processing.
//!

// Import all sub modules once...
mod client;
mod domain;
mod server;
mod transport;

mod rpc_config;

mod correlation;
mod error;

// Re-export main types
pub use client::RpcClient;
pub use server::RpcServer;

pub use rpc_config::RpcConfig;

pub use correlation::CorrelationId;
pub use error::{Error, Result};

pub use transport::create_memory_transport;

#[cfg(feature = "transport_mqttac")]
pub use transport::create_mqtt_async_client_transport;

#[cfg(feature = "transport_rumqttc")]
pub use transport::create_rumqttc_transport;

// --- public re-exports
pub use domain::{
    //
    Address,
    Envelope,
    PublishOptions,
    SubscribeOptions,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};

pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    #[cfg(feature = "transport_rumqttc")]
    {
        return create_rumqttc_transport(config).await;
    }

    #[cfg(all(feature = "transport_mqttac", not(feature = "transport_rumqttc")))]
    {
        return create_mqtt_async_client_transport(config).await;
    }

    // Fallback / default
    #[cfg(all(
        not(feature = "transport_mqttac"),
        not(feature = "transport_rumqttc"),
        not(feature = "transport_acme")
    ))]
    {
        create_memory_transport(config).await
    }
}
