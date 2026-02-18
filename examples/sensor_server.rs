//! Sensor RPC server example using a message broker.
//!
//! Demonstrates serving a bounded sensor/telemetry protocol over a broker transport.
//!
//! Run with: cargo run --example sensor_server --features transport_rumqttc
//!
//! Requires:
//! - An MQTT broker running on localhost:1883

// Allow unwrap in examples for clearer documentation
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{ReadHumidity, ReadPressure, ReadTemperature, SensorReading, TemperatureUnit};
use mom_rpc::{Result, RpcBrokerBuilder, TransportBuilder};
use tracing_subscriber::{fmt as tracing_format, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    tracing_format()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .with_line_number(true)
        .with_ansi(false)
        .init();

    let broker_uri =
        std::env::var("BROKER_URI").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let transport = TransportBuilder::new()
        .uri(&broker_uri)
        .node_id("env-sensor-42")
        .server_mode()
        .build()
        .await?;

    let server = RpcBrokerBuilder::new(transport.clone()).build()?;

    server.register_rpc_handler("read_temperature", |req: ReadTemperature| async move {
        // ---
        let celsius = 21.5_f32;
        let (value, unit) = match req.unit {
            TemperatureUnit::Celsius => (celsius, "C"),
            TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
        };
        Ok(SensorReading {
            value,
            unit: unit.to_string(),
            timestamp_ms: current_time_ms(),
        })
    })?;

    server.register_rpc_handler("read_humidity", |_req: ReadHumidity| async move {
        // ---
        Ok(SensorReading {
            value: 55.0,
            unit: "%".to_string(),
            timestamp_ms: current_time_ms(),
        })
    })?;

    server.register_rpc_handler("read_pressure", |_req: ReadPressure| async move {
        // ---
        Ok(SensorReading {
            value: 101.3,
            unit: "kPa".to_string(),
            timestamp_ms: current_time_ms(),
        })
    })?;

    println!("sensor_server listening as node_id=env-sensor-42");

    let broker_clone = server.clone();
    let transport_clone = transport.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
        broker_clone.shutdown().await;
        transport_clone
            .close()
            .await
            .expect("transport close failed");
    });

    // Blocks until shutdown() is called
    server.run().await?;
    Ok(())
}

fn current_time_ms() -> u64 {
    // ---
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
