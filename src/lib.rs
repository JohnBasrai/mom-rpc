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
//! - **AMQP via lapin**   - RabbitMQ and AMQP 0-9-1 brokers (enable `transport_lapin`)
//! - **DDS via rustdds**  - Brokerless peer-to-peer transport (enable `transport_rustdds`)
//!
//! # Quick Start
//!
//! ```no_run
//! use mom_rpc::{create_transport, RpcClient, RpcConfig, RpcServer, Result};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct ReadTemperature { unit: TemperatureUnit }
//!
//! #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
//! enum TemperatureUnit { Celsius, Fahrenheit }
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct SensorReading { value: f32, unit: String, timestamp_ms: u64 }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     //
//!     let config = RpcConfig::memory("sensor");
//!     let transport = create_transport(&config).await?;
//!
//!     let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");
//!     server.register("read_temperature", |req: ReadTemperature| async move {
//!         let celsius = 22.0_f32; // Simulate reading hw sensor
//!         let (value, unit) = match req.unit {
//!             TemperatureUnit::Celsius => (celsius, "C"),
//!             TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
//!         };
//!         Ok(SensorReading { value, unit: unit.to_string(), timestamp_ms: 0 })
//!     });
//!     let _handle = server.spawn();
//!
//!     let client = RpcClient::with_transport(transport.clone(), "client-1").await?;
//!     let resp: SensorReading = client
//!         .request_to("env-sensor-42", "read_temperature", ReadTemperature {
//!             unit: TemperatureUnit::Celsius,
//!         }).await?;
//!     println!("Temperature: {} {}", resp.value, resp.unit);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Feature Flags
//!
//! - `transport_rumqttc` - MQTT via rumqttc (recommended for production)
//! - `transport_lapin` - AMQP via lapin (for RabbitMQ and AMQP 0-9-1 brokers)
//! - `logging` - Enable log output (enabled by default)
//!
//! # Examples
//!
//! See the `examples/` directory for complete working examples:
//! - `sensor_memory.rs` - In-memory transport (no broker needed)
//! - `sensor_server.rs` - MQTT/AMQP server example
//! - `sensor_client.rs` - MQTT/AMQP client example

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
pub use error::{Result, RpcError};

/// Create a new in-memory transport.
///
/// This transport is always available (not feature-gated) and requires no
/// external resources.
pub use transport::create_memory_transport;

#[cfg(feature = "transport_rumqttc")]
pub use transport::create_rumqttc_transport;

#[cfg(feature = "transport_lapin")]
pub use transport::create_lapin_transport;

#[cfg(feature = "transport_rustdds")]
pub use transport::create_rustdds_transport;

// --- public re-exports
pub use domain::{
    //
    Address,
    Envelope,
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
/// 2. `transport_lapin` - AMQP via lapin
/// 3. `transport_rustdds` - DDS via rustdds
/// 4. Default - In-memory transport
///
/// If multiple transport features are enabled, rumqttc takes precedence over lapin.
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
///     // AMQP transport (requires transport_lapin feature)
///     let config = RpcConfig::with_broker("amqp://localhost:5672/%2f", "my-app");
///     let transport = create_transport(&config).await?;
///     
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Broker URI is invalid (for MQTT/AMQP transports)
/// - Transport initialization fails
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---

    #[cfg(feature = "transport_rumqttc")]
    return create_rumqttc_transport(config).await;

    #[cfg(all(feature = "transport_lapin", not(feature = "transport_rumqttc")))]
    return create_lapin_transport(config).await;

    #[cfg(all(
        feature = "transport_rustdds",
        not(any(feature = "transport_rumqttc", feature = "transport_lapin"))
    ))]
    return create_rustdds_transport(config).await;

    // Fallback / default
    #[cfg(not(any(
        feature = "transport_rumqttc",
        feature = "transport_lapin",
        feature = "transport_rustdds"
    )))]
    create_memory_transport(config).await
}
