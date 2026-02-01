//! Math RPC client example
//!
//! This is a pedagogical example showing how to use mqtt-rpc-rs client API.
//! It demonstrates sending requests with timeouts and handling responses.
//!
//! Run with: cargo run --example math_client
//!
//! Requires: math_server example running and mosquitto on localhost:1883

use mqtt_rpc_rs::{Error, RpcClient};
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize)]
struct AddRequest {
    // ---
    a: i32,
    b: i32,
}

#[derive(Debug, Deserialize)]
struct MathResponse {
    // ---
    result: i32,
}

#[derive(Debug, Serialize)]
struct MultiplyRequest {
    // ---
    a: i32,
    b: i32,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // ---
    println!("Starting math RPC client...");

    // Create MQTT client
    let mut mqtt_options = MqttOptions::new("math-client-01", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

    // Spawn eventloop in background
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("MQTT eventloop error: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Create RPC client
    let rpc_client = RpcClient::new(mqtt_client, "math-client-01").await?;

    println!("Client ready!");

    // Test addition
    println!("\nSending add request: 5 + 3");
    let add_req = AddRequest { a: 5, b: 3 };
    let response: MathResponse = rpc_client
        .request_timeout("math/add", &add_req, Some(Duration::from_secs(5)))
        .await?;
    println!("  Result: {}", response.result);

    // Test multiplication
    println!("\nSending multiply request: 7 * 6");
    let multiply_req = MultiplyRequest { a: 7, b: 6 };
    let response: MathResponse = rpc_client
        .request_timeout("math/multiply", &multiply_req, Some(Duration::from_secs(5)))
        .await?;
    println!("  Result: {}", response.result);

    println!("\nAll requests completed successfully!");

    Ok(())
}
