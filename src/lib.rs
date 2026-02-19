//! Transport-agnostic async RPC over message-oriented middleware.
//!
//! This library provides a unified `RpcBroker` type for implementing RPC patterns
//! over pub/sub systems like MQTT, AMQP, and DDS. It handles correlation ID
//! generation, request/response matching, timeout handling, and concurrent
//! request processing.
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
//! | **Redis via redis**  | Redis Pub/Sub transport           | `transport_redis`    |
//!
//! **Note:** The `logging` feature (enabled by default) provides diagnostic output via `tracing`.
//! To disable logging, use `default-features = false` in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! mom-rpc = { version = "0.9", default-features = false, features = ["transport_rumqttc"] }
//! ```
//!
//! # Quick Start
//!
//! ```no_run
//! use mom_rpc::{TransportBuilder, RpcBrokerBuilder, Result};
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
//!     let transport = TransportBuilder::new()
//!         .uri("memory://")
//!         .node_id("env-sensor-42")
//!         .full_duplex()
//!         .build()
//!         .await?;
//!
//!     let server = RpcBrokerBuilder::new(transport.clone()).build()?;
//!     server.register_rpc_handler("read_temperature", |req: ReadTemperature| async move {
//!         let celsius = 22.0_f32;
//!         let (value, unit) = match req.unit {
//!             TemperatureUnit::Celsius => (celsius, "C"),
//!             TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
//!         };
//!         Ok(SensorReading { value, unit: unit.to_string(), timestamp_ms: 0 })
//!     })?;
//!     let _handle = server.spawn()?;
//!
//!     let client = RpcBrokerBuilder::new(transport).build()?;
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
//!  - `examples/sensor_client.rs`
//!  - `examples/sensor_fullduplex.rs`
//!  - `examples/sensor_memory.rs`
//!  - `examples/sensor_server.rs`

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::panic_in_result_fn
    )
)]

////////////////////////////////////////
// Submodules
////////////////////////////////////////

mod broker;
mod broker_builder;
mod broker_mode;
mod domain;
mod retry;
mod transport;
mod transport_builder;

mod correlation;
mod error;

////////////////////////////////////////
// Public API
////////////////////////////////////////

pub use broker::RpcBroker;
pub use broker_builder::RpcBrokerBuilder;
pub use broker_mode::BrokerMode;
pub(crate) use retry::RetryConfig;
pub use transport_builder::TransportBuilder;

pub use correlation::CorrelationId;
pub use error::{Result, RpcError};

pub use domain::{
    // ---
    Address,
    Envelope,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportBase,
    TransportConfig,
    TransportMode,
    TransportPtr,
};

////////////////////////////////////////
// Transport factory functions
////////////////////////////////////////

// Memory transport testing utilities
// WARNING: MemoryHub and create_memory_transport_with_hub are exposed only for
// mom-rpc's own integration tests and may change without notice.
// Production code should use TransportBuilder.
pub use transport::create_memory_transport_with_hub;
pub use transport::MemoryHub;

// Protocol transport factories - internal only; users go through TransportBuilder
pub(crate) use transport::create_dust_dds_transport;
pub(crate) use transport::create_lapin_transport;
pub(crate) use transport::create_memory_transport;
pub(crate) use transport::create_redis_transport;
pub(crate) use transport::create_rumqttc_transport;

////////////////////////////////////////
// Internal helpers
////////////////////////////////////////

pub(crate) use retry::retry_with_backoff;

mod macros;
pub(crate) use macros::{log_debug, log_error, log_info, log_warn};
