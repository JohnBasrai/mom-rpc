//! Math RPC server example
//!
//! This is a pedagogical example showing how to use mqtt-rpc-rs server API.
//! It demonstrates handler registration and concurrent request processing.
//!
//! Run with: cargo run --example math_server
//!
//! Requires: mosquitto running on localhost:1883

use mqtt_rpc_rs::{Error, RpcServer};
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct AddRequest {
    // ---
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize)]
struct MathResponse {
    // ---
    result: i32,
}

#[derive(Debug, Deserialize)]
struct MultiplyRequest {
    // ---
    a: i32,
    b: i32,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // ---
    println!("Starting math RPC server...");

    // Create MQTT client
    let mut mqtt_options = MqttOptions::new("math-server", "localhost", 1883);
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

    // Create RPC server
    let mut server = RpcServer::new(mqtt_client).await?;

    // Register addition handler
    server
        .handle("math/add", |req: AddRequest| async move {
            println!("Handling add: {} + {}", req.a, req.b);
            Ok(MathResponse {
                result: req.a + req.b,
            })
        })
        .await?;

    // Register multiplication handler
    server
        .handle("math/multiply", |req: MultiplyRequest| async move {
            println!("Handling multiply: {} * {}", req.a, req.b);
            Ok(MathResponse {
                result: req.a * req.b,
            })
        })
        .await?;

    println!("Math server ready!");
    println!("  - Listening on math/add/request");
    println!("  - Listening on math/multiply/request");

    // Run server (this would be the event loop processing messages)
    // For now, just keep alive
    tokio::signal::ctrl_c().await.ok();
    println!("\nShutting down...");

    Ok(())
}
