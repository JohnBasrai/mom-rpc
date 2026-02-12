//! Sensor RPC example using in-memory transport.
//!
//! Demonstrates both client and server in a single process using MemoryTransport.
//! This is useful for tests, simulations, and single-process applications.
//!
//! Run with: cargo run --example sensor_memory

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn
)]

mod common;

use common::{ReadHumidity, ReadPressure, ReadTemperature, SensorReading, TemperatureUnit};
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig, RpcServer};
use tracing_subscriber::{fmt as tracing_format, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    tracing_format()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .with_line_number(true)
        .init();

    let config = RpcConfig::memory("sensor");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");

    server.register("read_temperature", |req: ReadTemperature| async move {
        // ---
        let celsius = 22.0_f32;
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
            value: 50.0,
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

    // Spawn server in background so we can use the main task for client
    let _handle = server.spawn();

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let temp: SensorReading = client
        .request_to(
            "env-sensor-42",
            "read_temperature",
            ReadTemperature {
                unit: TemperatureUnit::Celsius,
            },
        )
        .await?;

    let humidity: SensorReading = client
        .request_to("env-sensor-42", "read_humidity", ReadHumidity)
        .await?;

    let pressure: SensorReading = client
        .request_to("env-sensor-42", "read_pressure", ReadPressure)
        .await?;

    println!(
        "Temperature: {} {} @ {}",
        temp.value, temp.unit, temp.timestamp_ms
    );
    println!(
        "Humidity:    {} {} @ {}",
        humidity.value, humidity.unit, humidity.timestamp_ms
    );
    println!(
        "Pressure:    {} {} @ {}",
        pressure.value, pressure.unit, pressure.timestamp_ms
    );

    // Clean shutdown
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
