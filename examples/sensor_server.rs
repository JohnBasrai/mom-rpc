//! Sensor RPC server example using a message broker.
//!
//! Demonstrates serving a bounded sensor/telemetry protocol over a broker transport.
//!
//! Run with: cargo run --example sensor_server --features transport_rumqttc
//!
//! Requires:
//! - An MQTT broker running on localhost:1883

mod common;

use common::{ReadHumidity, ReadPressure, ReadTemperature, SensorReading, TemperatureUnit};
use mom_rpc::{create_transport, Result, RpcConfig, RpcServer};

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    env_logger::init();

    let broker_uri =
        std::env::var("BROKER_URI").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "env-sensor-42");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");

    // Spawn server in background so we can use the main task for client
    let _handle = server.spawn();

    server.register("read_temperature", |req: ReadTemperature| async move {
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
    });

    server.register("read_humidity", |_req: ReadHumidity| async move {
        // ---
        Ok(SensorReading {
            value: 55.0,
            unit: "%".to_string(),
            timestamp_ms: current_time_ms(),
        })
    });

    server.register("read_pressure", |_req: ReadPressure| async move {
        // ---
        Ok(SensorReading {
            value: 101.3,
            unit: "kPa".to_string(),
            timestamp_ms: current_time_ms(),
        })
    });

    println!("sensor_server listening as node_id=env-sensor-42");

    // Block until Ctrl+C
    tokio::signal::ctrl_c().await.ok();

    server.shutdown().await?;
    transport.close().await?;
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
