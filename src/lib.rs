//! RPC semantics over MQTT pub/sub with automatic request/response correlation
//!
//! This library provides a simple, ergonomic API for implementing RPC patterns
//! over MQTT. It handles correlation ID generation, request/response matching,
//! timeout handling, and concurrent request processing.
//!
//! # Features
//!
//! - **Async/await API** - Built on tokio and rumqttc
//! - **Automatic correlation** - UUID-based request/response matching
//! - **Concurrent requests** - Multiple in-flight requests per client
//! - **Timeout support** - Per-request timeout handling
//! - **MQTT v5** - Uses native response_topic properties
//! - **JSON serialization** - Serde-based, interop with other languages
//! - **Concurrent handlers** - Server handlers execute in parallel
//!
//! # Quick Start
//!
//! ## Client Example
//!
//! ```no_run
//! use mqtt_rpc_rs::{RpcClient, Error};
//! use rumqttc::{MqttOptions, AsyncClient};
//! use serde::{Deserialize, Serialize};
//! use std::time::Duration;
//!
//! #[derive(Serialize)]
//! struct Request { value: i32 }
//!
//! #[derive(Deserialize)]
//! struct Response { result: i32 }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let mqtt_options = MqttOptions::new("client-01", "localhost", 1883);
//!     let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
//!     
//!     tokio::spawn(async move {
//!         loop { eventloop.poll().await.ok(); }
//!     });
//!     
//!     let rpc_client = RpcClient::new(mqtt_client, "client-01").await?;
//!     
//!     let response: Response = rpc_client
//!         .request("math/add", &Request { value: 5 })
//!         .timeout(Duration::from_secs(5))
//!         .await?;
//!     
//!     println!("Result: {}", response.result);
//!     Ok(())
//! }
//! ```
//!
//! ## Server Example
//!
//! ```no_run
//! use mqtt_rpc_rs::{RpcServer, Error};
//! use rumqttc::{MqttOptions, AsyncClient};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Deserialize)]
//! struct Request { value: i32 }
//!
//! #[derive(Serialize)]
//! struct Response { result: i32 }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let mqtt_options = MqttOptions::new("server-01", "localhost", 1883);
//!     let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
//!     
//!     tokio::spawn(async move {
//!         loop { eventloop.poll().await.ok(); }
//!     });
//!     
//!     let mut server = RpcServer::new(mqtt_client).await?;
//!     
//!     server.handle("math/add", |req: Request| async move {
//!         Ok(Response { result: req.value + 10 })
//!     }).await?;
//!     
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod protocol;
pub mod server;

// Re-export main types
pub use client::RpcClient;
pub use error::{Error, Result};
pub use protocol::{CorrelationId, RpcRequest, RpcResponse};
pub use server::RpcServer;
