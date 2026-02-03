//! Math RPC server example
//!
//! This is a pedagogical example showing how to use mqtt-rpc-rs server API.
//! It demonstrates handler registration and concurrent request processing.
//!
//! Run with: cargo run --example math_server
//!
//! Requires: mosquitto running on localhost:1883

use serde::{Deserialize, Serialize};

use mqtt_rpc_rs::{RpcConfig, RpcServer, RpcServerBuilder};

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
async fn main() -> Result<()> {
    // ---
    // Build and start the RPC server
    let config = RpcConfig::new("mqtt://localhost:1883").with_keep_alive_secs(10);

    let server: RpcServer = RpcServerBuilder::new("math-server", config).start().await?;

    // Register a handler for the "add" method
    server.register("add", |bytes| {
        // ---
        let req: AddRequest = serde_json::from_slice(&bytes).expect("failed to decode request");

        let resp = AddResponse { sum: req.a + req.b };

        serde_json::to_vec(&resp).expect("failed to encode response")
    });

    println!("math_server running. Press Ctrl-C to exit.");

    // Keep the process alive
    tokio::signal::ctrl_c().await?;
    Ok(())
}
