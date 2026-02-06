//! Transport-agnostic async RPC over message-oriented middleware.
//!
//! This library provides a simple, ergonomic API for implementing RPC patterns
//! over unreliable pub/sub systems like MQTT. It handles correlation ID generation,
//! request/response matching, timeout handling, and concurrent request processing.
//!
//! # Supported Transports
//!
//! - **Memory** (default) - In-process testing transport, always available
//! - **MQTT via rumqttc** - Recommended MQTT backend (enable `transport_rumqttc`)
//! - **MQTT via mqtt-async-client** - Legacy backend (enable `transport_mqttac`, deprecated)
//!
//! # Quick Start
//!
//! ```no_run
//! use mom_rpc::{create_transport, RpcClient, RpcConfig, RpcServer, Result};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct AddRequest { a: i32, b: i32 }
//!
//! #[derive(Serialize, Deserialize)]
//! struct AddResponse { sum: i32 }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = RpcConfig::memory("example");
//!     let transport = create_transport(&config).await?;
//!
//!     let server = RpcServer::with_transport(transport.clone(), "math");
//!     server.register("add", |req: AddRequest| async move {
//!         Ok(AddResponse { sum: req.a + req.b })
//!     });
//!     let _handle = server.spawn();
//!
//!     let client = RpcClient::with_transport(transport.clone(), "client").await?;
//!     let resp: AddResponse = client.request_to("math", "add", AddRequest { a: 2, b: 3 }).await?;
//!     assert_eq!(resp.sum, 5);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Feature Flags
//!
//! - `transport_rumqttc` - MQTT via rumqttc (recommended for production)
//! - `transport_mqttac` - Legacy MQTT (deprecated, will be removed in v0.4.0)
//! - `logging` - Enable log output (enabled by default)
//!
//! # Examples
//!
//! See the `examples/` directory for complete working examples:
//! - `math_memory.rs` - In-memory transport (no broker needed)
//! - `math_server.rs` - MQTT server example
//! - `math_client.rs` - MQTT client example

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

/// Create a new in-memory transport.
///
/// This transport is always available (not feature-gated) and requires no
/// external resources.
pub use transport::create_memory_transport;

/// Legacy MQTT transport.
///
/// **Deprecated:** Use `create_rumqttc_transport` instead.
/// This feature will be removed in `v0.4.0`.
#[cfg(feature = "transport_mqttac")]
#[deprecated(
    since = "0.3.0",
    note = "Use transport_rumqttc feature instead. This will be removed in `v0.4.0`"
)]
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

/// Creates a transport based on the provided configuration and enabled features.
///
/// This is the primary transport factory function. It selects the appropriate
/// transport implementation based on feature flags with the following priority:
///
/// 1. `transport_rumqttc` - MQTT via rumqttc (recommended)
/// 2. `transport_mqttac` - Legacy MQTT (deprecated)
/// 3. Default - In-memory transport
///
/// If multiple transport features are enabled, rumqttc takes precedence.
///
/// # Examples
///
/// ```no_run
/// use mom_rpc::{create_transport, RpcConfig};
///
/// #[tokio::main]
/// async fn main() -> mom_rpc::Result<()> {
///     // Memory transport (default, no features needed)
///     let config = RpcConfig::memory("my-app");
///     let transport = create_transport(&config).await?;
///     
///     // MQTT transport (requires transport_rumqttc feature)
///     let config = RpcConfig::with_broker("mqtt://localhost:1883", "my-app");
///     let transport = create_transport(&config).await?;
///     
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - MQTT broker URL is invalid (for MQTT transports)
/// - Transport initialization fails
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
    #[cfg(all(not(feature = "transport_mqttac"), not(feature = "transport_rumqttc")))]
    {
        create_memory_transport(config).await
    }
}
