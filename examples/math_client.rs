//! Math RPC client example using a message broker.
//!
//! Demonstrates connecting to a running math server via MQTT broker.
//!
//! Run with: cargo run --example math_client --features transport_rumqttc
//!
//! Requires:
//! - An MQTT broker running on localhost:1883
//! - math_server example running (or another server listening on node_id "math")

mod common;

use common::{AddRequest, AddResponse};
use mom_rpc::{create_transport, RpcClient, RpcConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---
    env_logger::init();

    let broker_uri =
        std::env::var("BROKER_URI").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "math-client");
    let transport = create_transport(&config).await?;

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    println!("2 + 3 = {}", resp.sum);
    transport.close().await?;

    Ok(())
}
