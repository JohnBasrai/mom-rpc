//! Unified full-duplex sensor RPC example.
//!
//! Demonstrates the unified broker API in **full-duplex mode** — a single process
//! acting as both client and server simultaneously. The broker registers RPC
//! handlers, then makes requests to itself via the transport.
//!
//! This showcases the key advantage of the 0.8 unified API: no separate client
//! and server types. One `RpcBroker` does both.
//!
//! Run with any transport:
//!
//! ```bash
//! # Memory transport (default)
//! cargo run --example sensor_unified
//!
//! # MQTT (requires mosquitto or similar)
//! BROKER_URI=mqtt://localhost:1883 cargo run --example sensor_unified --features transport_rumqttc
//!
//! # AMQP (requires RabbitMQ or similar)
//! BROKER_URI=amqp://localhost:5672/%2f cargo run --example sensor_unified --features transport_lapin
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{ReadHumidity, ReadPressure, ReadTemperature, SensorReading, TemperatureUnit};
use mom_rpc::{Result, RpcBrokerBuilder, TransportBuilder};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

const NODE_ID: &str = "env-sensor-fd-42";

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_line_number(false)
        .with_ansi(true)
        .init();

    let broker_uri = std::env::var("BROKER_URI").unwrap_or_else(|_| "memory://".to_string());

    println!("🌡️  Full-Duplex Sensor Example");
    println!("   Transport: {broker_uri}");
    println!("   Node ID:   {NODE_ID}\n");

    // Create a full-duplex transport
    let transport = TransportBuilder::new()
        .uri(&broker_uri)
        .node_id(NODE_ID)
        .full_duplex() // ← Key: both request and response queues
        .build()
        .await?;

    // Create a single broker in full-duplex mode
    let broker = RpcBrokerBuilder::new(transport.clone())
        .retry_max_attempts(5)
        .retry_initial_delay(Duration::from_millis(100))
        .request_total_timeout(Duration::from_millis(500))
        .build()?;

    // Register RPC handlers (server-side)
    println!("📝 Registering RPC handlers...");
    broker.register_rpc_handler("read_temperature", handle_temperature)?;
    broker.register_rpc_handler("read_humidity", handle_humidity)?;
    broker.register_rpc_handler("read_pressure", handle_pressure)?;
    println!("   ✓ read_temperature");
    println!("   ✓ read_humidity");
    println!("   ✓ read_pressure\n");

    // Spawn the server receive loop in the background
    let _handle = broker.clone().spawn()?;
    println!("✓ Server running in background\n");

    // Give server time to subscribe (important for broker-based transports)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now act as client — make requests to ourselves!
    println!("📡 Making RPC calls to self...\n");

    // Read temperature in Celsius
    print!("  🌡️  Temperature (°C)... ");
    let temp_c: SensorReading = broker
        .request_to(
            NODE_ID, // ← Calling ourselves
            "read_temperature",
            ReadTemperature {
                unit: TemperatureUnit::Celsius,
            },
        )
        .await?;
    println!("✓ {} {}", temp_c.value, temp_c.unit);

    // Read temperature in Fahrenheit
    print!("  🌡️  Temperature (°F)... ");
    let temp_f: SensorReading = broker
        .request_to(
            NODE_ID,
            "read_temperature",
            ReadTemperature {
                unit: TemperatureUnit::Fahrenheit,
            },
        )
        .await?;
    println!("✓ {} {}", temp_f.value, temp_f.unit);

    // Read humidity
    print!("  💧 Humidity........... ");
    let humidity: SensorReading = broker
        .request_to(NODE_ID, "read_humidity", ReadHumidity)
        .await?;
    println!("✓ {} {}", humidity.value, humidity.unit);

    // Read pressure
    print!("  📊 Pressure........... ");
    let pressure: SensorReading = broker
        .request_to(NODE_ID, "read_pressure", ReadPressure)
        .await?;
    println!("✓ {} {}", pressure.value, pressure.unit);

    // Display results
    println!("\n📋 Sensor Readings:");
    println!(
        "   Temperature: {:.1}°C / {:.1}°F",
        temp_c.value, temp_f.value
    );
    println!("   Humidity:    {:.1}%", humidity.value);
    println!("   Pressure:    {:.1} kPa\n", pressure.value);

    // Clean shutdown
    println!("🛑 Shutting down...");
    broker.shutdown().await;
    transport.close().await?;
    println!("✓ Done\n");

    Ok(())
}

// ============================================================================
// RPC Handler Functions
// ============================================================================

async fn handle_temperature(req: ReadTemperature) -> Result<SensorReading> {
    // ---
    // Simulate sensor read with slight delay
    tokio::time::sleep(Duration::from_millis(50)).await;

    let celsius = 22.5_f32;
    let (value, unit) = match req.unit {
        TemperatureUnit::Celsius => (celsius, "°C"),
        TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "°F"),
    };

    Ok(SensorReading {
        value,
        unit: unit.to_string(),
        timestamp_ms: current_time_ms(),
    })
}

async fn handle_humidity(_req: ReadHumidity) -> Result<SensorReading> {
    // ---
    tokio::time::sleep(Duration::from_millis(30)).await;

    Ok(SensorReading {
        value: 55.0,
        unit: "%".to_string(),
        timestamp_ms: current_time_ms(),
    })
}

async fn handle_pressure(_req: ReadPressure) -> Result<SensorReading> {
    // ---
    tokio::time::sleep(Duration::from_millis(40)).await;

    Ok(SensorReading {
        value: 101.3,
        unit: "kPa".to_string(),
        timestamp_ms: current_time_ms(),
    })
}

fn current_time_ms() -> u64 {
    // ---
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
