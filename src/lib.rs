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

#[cfg(feature = "transport-mqtt-async-client")]
pub use transport::create_mqtt_async_client_transport;

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

pub async fn create_transport(transport_id: &str) -> Result<TransportPtr> {
    // ---
    #[cfg(feature = "transport-mqtt-async-client")]
    {
        create_mqtt_async_client_transport(transport_id).await
    }

    // Future transport impls go here

    #[cfg(all(
        not(feature = "transport-mqtt-async-client"),
        //not(feature = "transport-rabbitmq")
    ))]
    {
        // Fallback / default
        create_memory_transport(transport_id).await
    }
}
