//! Math RPC client example
//!
//! This example sends a single RPC request to the math server and
//! prints the response.
//!
//! Run with: cargo run --example math_client
//!
//! Requires: mosquitto running on localhost:1883

use anyhow::Result;
use serde::{Deserialize, Serialize};

use mqtt_rpc_rs::{RpcClient, RpcClientBuilder, RpcConfig};

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
    // Build and start the RPC client
    let config = RpcConfig::new("mqtt://localhost:1883").with_keep_alive_secs(10);

    let client: RpcClient = RpcClientBuilder::new("math-client", config).start().await?;

    // ---
    // Send an RPC request
    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    println!("2 + 3 = {}", resp.sum);

    Ok(())
}
