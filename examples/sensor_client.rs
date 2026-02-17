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

use common::{
    //
    ReadHumidity,
    ReadPressure,
    ReadTemperature,
    SensorReading,
    TemperatureUnit,
};
use mom_rpc::{
    //
    create_transport,
    Result,
    RetryConfig,
    RpcClient,
    RpcConfig,
};
use std::time::Duration;
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

    let retry_config = RetryConfig {
        max_attempts: 20,
        multiplier: 2.0,
        initial_delay: Duration::from_millis(200),
        max_delay: Duration::from_millis(1000),
    };

    let config = RpcConfig::with_broker(&broker_uri, "sensor-client")
        .with_retry(retry_config)
        .with_request_timeout(Duration::from_millis(200));

    // Each request attempt:
    // - Publishes request
    // - Waits up to 200ms for response
    // - On timeout → returns TransportRetryable → retry kicks in
    // - Delay between retries: 200ms → 400ms → 800ms → 1000ms (capped)

    let transport = create_transport(&config).await?;

    let client = RpcClient::with_transport(transport.clone(), "client-1", config).await?;

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
