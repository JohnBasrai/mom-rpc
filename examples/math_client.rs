//! Math RPC client example.
//!
//! NOTE:
//! This example is intended for brokered transports (MQTT).
//! It cannot be used with MemoryTransport, which is in-process only.
//!
//! Run with: cargo run --example math_client --features transport_rumqttc
//!
//! Requires: an MQTT broker running on localhost:1883

use mom_rpc::{create_transport, RpcClient, RpcConfig};
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
    let config = RpcConfig::with_broker("mqtt://localhost:1883", "math-client");

    let transport = create_transport(&config).await?;

    let client = RpcClient::with_transport(transport.clone(), "client-1".to_string()).await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    println!("2 + 3 = {}", resp.sum);
    transport.close().await?;

    Ok(())
}
