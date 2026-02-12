//! Sensor RPC client example using a message broker.
//!
//! Demonstrates reading sensor data from a remote node over a broker transport.
//!
//! Run with: cargo run --example sensor_client --features transport_rumqttc
//!
//! Requires:
//! - An MQTT broker running on localhost:1883
//! - sensor_server example running (node_id "env-sensor-42")

mod common;

use common::{ReadHumidity, ReadPressure, ReadTemperature, SensorReading, TemperatureUnit};
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // ---
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .with_ansi(false)
        .with_line_number(true)
        .init();

    let broker_uri =
        std::env::var("BROKER_URI").unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "sensor-client");
    let transport = create_transport(&config).await?;

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

    transport.close().await?;
    Ok(())
}
