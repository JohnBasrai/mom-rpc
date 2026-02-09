//! Math RPC server example using a message broker.
//!
//! Demonstrates running an RPC server that listens for requests via MQTT broker.
//!
//! Run with: cargo run --example math_server --features transport_rumqttc
//!
//! Requires: An MQTT broker running on localhost:1883
//!
//! Note: For Docker/production deployments, also handle SIGTERM:
//! ```ignore
//! use tokio::signal::unix::{signal, SignalKind};
//! signal(SignalKind::terminate()).unwrap().recv().await;
//! ```
mod common;

use common::{AddRequest, AddResponse};
use mom_rpc::{create_transport, RpcConfig, RpcServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---
    env_logger::init();

    let broker_uri =
        std::env::var("BROKER_URI").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "math-server");

    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    // Setup signal handling for graceful shutdown
    let server_clone = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
        println!("Received Ctrl+C, shutting down...");
        server_clone.shutdown().await.expect("shutdown failed");
    });

    // Run server - blocks until shutdown() is called
    server.run().await?;

    transport.close().await?;

    Ok(())
}
