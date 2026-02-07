//! Math RPC server example.
//!
//! NOTE:
//! This example is intended for brokered transports (MQTT).
//! It cannot be used with MemoryTransport, which is in-process only.
//!
//! Run with: cargo run --example math_server --features transport_rumqttc
//!
//! Requires: an MQTT broker running on localhost:1883

use mom_rpc::{create_transport, RpcConfig, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse {
    sum: i32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---
    env_logger::init();
    let config = RpcConfig::with_broker("mqtt://localhost:1883", "math-server");

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
        println!("math_server: Received Ctrl+C, shutting down...");
        server_clone.shutdown().await.expect("shutdown failed");
    });

    // Run server - blocks until shutdown() is called
    // For Docker/production, also handle SIGTERM:
    // tokio::signal::unix::signal(SignalKind::terminate())
    server.run().await?;

    transport.close().await?;

    Ok(())
}
