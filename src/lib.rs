//! Transport-agnostic async RPC over message-oriented middleware.
//!
//! This library provides a simple, ergonomic API for implementing RPC patterns
//! over unreliable pub/sub systems like MQTT. It handles correlation ID generation,
//! request/response matching, timeout handling, and concurrent request processing.
//!
//! # Supported Transports
//!
//! | Transport            | Description                       | Enable Flag          |
//! |:---------------------|:----------------------------------|:---------------------|
//! | **Memory** (default) | In-process testing transport      | **N/A** (always on)  |
//! |                      |                                   |                      |
//! | **AMQP via lapin**   | RabbitMQ and AMQP 0-9-1 brokers   | `transport_lapin`    |
//! | **DDS via dust_dds** | Brokerless peer-to-peer transport | `transport_dust_dds` |
//! | **MQTT via rumqttc** | MQTT broker-based transport       | `transport_rumqttc`  |
//!
//! **Note:** The `logging` feature (enabled by default) provides diagnostic output via `tracing`.
//! To disable logging, use `default-features = false` in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! mom-rpc = { version = "0.7", default-features = false, features = ["transport_rumqttc"] }
//! ```
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
//! # Examples
//!
//! See the [examples/](https://github.com/JohnBasrai/mom-rpc/blob/main/examples/)
//! - `sensor_memory.rs` - In-memory transport (no broker needed)
//! - `sensor_server.rs` - Transport-agnostic server example
//! - `sensor_client.rs` - Transport-agnostic client example

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::panic_in_result_fn
    )
)]

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

/// Create an in-memory transport.
///
/// This transport is always available (not feature-gated) and requires no
/// external resources.
pub use transport::create_memory_transport;

#[doc(hidden)]
#[cfg(feature = "transport_rumqttc")]
pub use transport::create_rumqttc_transport;

#[doc(hidden)]
#[cfg(feature = "transport_lapin")]
use transport::create_lapin_transport;

#[doc(hidden)]
#[cfg(feature = "transport_dust_dds")]
use transport::create_dust_dds_transport;

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

/// Create a transport based on the provided configuration and enabled features.
///
/// **Best for:** Applications with a single transport feature enabled.
///
/// When multiple transport features are enabled, auto-selects based on priority:
/// 1. `transport_dust_dds` - DDS via dust_dds
/// 2. `transport_rumqttc`  - MQTT via rumqttc
/// 3. `transport_lapin`    - AMQP via lapin
/// 4. `memory` (always available)
///
/// **For multi-transport applications:** Use `create_transport_for()` to explicitly
/// select the transport at runtime instead of relying on this priority order.
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
/// Returns `RpcError::Transport` if:
/// - Broker URI is invalid or malformed
/// - Transport-specific initialization fails (connection, authentication, etc.)
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---

    #[cfg(feature = "transport_rumqttc")]
    return create_rumqttc_transport(config).await;

    #[cfg(all(feature = "transport_lapin", not(feature = "transport_rumqttc")))]
    return create_lapin_transport(config).await;

    #[cfg(all(
        feature = "transport_dust_dds",
        not(any(feature = "transport_rumqttc", feature = "transport_lapin"))
    ))]
    return create_dust_dds_transport(config).await;

    // Fallback / default
    #[cfg(not(any(
        feature = "transport_rumqttc",
        feature = "transport_lapin",
        feature = "transport_dust_dds"
    )))]
    create_memory_transport(config).await
}

/// Creates a transport based on a runtime string flag.
///
/// Allows applications to select between multiple compiled transports at runtime.
/// The transport must be enabled via feature flags at compile time.
///
/// # Arguments
/// * `flag`   - Transport name: "memory", "dust-dds", "rumqttc", or "lapin"
/// * `config` - Transport configuration
///
/// # Errors
///
/// Returns `RpcError::Transport` if:
/// - The specified transport is not enabled via feature flags
/// - The transport name is unrecognized
/// - The broker URI format is invalid
/// - Transport initialization fails (connection, authentication, etc.)
///
/// # Examples
///
/// ```no_run
/// use mom_rpc::{create_transport_for, RpcConfig, Result};
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Build config normally
///     let config = RpcConfig::with_broker("mqtt://localhost:1883", "my-app");
///
///     // Explicit runtime selection (requires feature enabled)
///     let transport = create_transport_for("rumqttc", &config).await?;
///
///     Ok(())
/// }
/// ```
pub async fn create_transport_for(flag: &str, config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    let _not_enabled = format!("transport_{flag} feature not enabled");

    match flag {
        "dust-dds" => {
            #[cfg(feature = "transport_dust_dds")]
            return create_dust_dds_transport(config).await;

            #[cfg(not(feature = "transport_dust_dds"))]
            return Err(RpcError::Transport(_not_enabled));
        }
        "rumqttc" => {
            #[cfg(feature = "transport_rumqttc")]
            return create_rumqttc_transport(config).await;

            #[cfg(not(feature = "transport_rumqttc"))]
            return Err(RpcError::Transport(_not_enabled));
        }
        "lapin" => {
            #[cfg(feature = "transport_lapin")]
            return create_lapin_transport(config).await;

            #[cfg(not(feature = "transport_lapin"))]
            return Err(RpcError::Transport(_not_enabled));
        }
        "memory" => create_memory_transport(config).await,
        _ => Err(RpcError::Transport(format!(
            "unrecognized transport: {flag}, valid values: memory, dust-dds, rumqttc, lapin"
        ))),
    }
}

// src/lib.rs
// ---
// Internal infrastructure

mod macros;
pub(crate) use macros::{log_debug, log_error, log_info, log_warn};
