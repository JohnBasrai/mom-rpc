#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn
)]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use tracing::info;

#[allow(unused)]
use mom_rpc::{
    // ---
    MemoryHub,
    Result,
    RpcBroker,
    RpcBrokerBuilder,
    RpcError,
    TransportBuilder,
    TransportConfig,
    TransportMode,
    TransportPtr,
};

#[derive(Debug, Serialize, Deserialize)]
struct ReadTemperature {
    unit: TemperatureUnit,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReadHumidity;

#[derive(Debug, Serialize, Deserialize)]
struct ReadPressure;

#[derive(Debug, Serialize, Deserialize)]
struct SensorReading {
    value: f32,
    unit: String,
    timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}

/// Test fixture: a sensor server on an isolated [`MemoryHub`].
///
/// Each `SensorServer` owns the hub so tests are fully isolated from each other.
struct SensorServer {
    // ---
    _handle: JoinHandle<()>,
    broker: RpcBroker,
    node_id: String,
    hub: Arc<MemoryHub>,
}

impl SensorServer {
    // ---
    async fn new(id: &str) -> Result<Self> {
        // ---
        let node_id = format!("sensor-{id}");
        let hub = MemoryHub::new();

        let transport = server_transport(&node_id, hub.clone()).await?;
        let broker = RpcBrokerBuilder::new(transport).build()?;

        // Register temperature reading handler
        broker.register("temperature", |req: ReadTemperature| async move {
            let base_temp = 23.5; // Base temperature in Celsius
            let value = match req.unit {
                TemperatureUnit::Celsius => base_temp,
                TemperatureUnit::Fahrenheit => base_temp * 9.0 / 5.0 + 32.0,
            };
            let unit = match req.unit {
                TemperatureUnit::Celsius => "°C",
                TemperatureUnit::Fahrenheit => "°F",
            };
            Ok(SensorReading {
                value,
                unit: unit.to_string(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            })
        })?;

        // Register humidity reading handler
        broker.register("humidity", |_req: ReadHumidity| async move {
            Ok(SensorReading {
                value: 45.2,
                unit: "%RH".to_string(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            })
        })?;

        // Register pressure reading handler
        broker.register("pressure", |_req: ReadPressure| async move {
            Ok(SensorReading {
                value: 1013.25,
                unit: "hPa".to_string(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            })
        })?;

        let handle = broker.clone().spawn()?;

        // Give the server task time to subscribe before returning
        tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(Self {
            _handle: handle,
            broker,
            node_id,
            hub,
        })
    }

    async fn shutdown(self) -> Result<()> {
        // ---
        self.broker.shutdown().await;
        Ok(())
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn hub(&self) -> Arc<MemoryHub> {
        self.hub.clone()
    }
}

/// Create a server-mode transport on the given hub.
async fn server_transport(node_id: &str, hub: Arc<MemoryHub>) -> Result<TransportPtr> {
    // ---
    mom_rpc::create_memory_transport_with_hub(
        TransportConfig {
            uri: String::new(),
            node_id: node_id.into(),
            mode: TransportMode::Server,
            request_queue: Some(format!("requests/{node_id}")),
            response_queue: None,
            transport_type: None,
            keep_alive_secs: None,
        },
        hub,
    )
    .await
}

/// Create a client-mode transport on the given hub.
async fn client_transport(node_id: &str, hub: Arc<MemoryHub>) -> Result<TransportPtr> {
    // ---
    mom_rpc::create_memory_transport_with_hub(
        TransportConfig {
            uri: String::new(),
            node_id: node_id.into(),
            mode: TransportMode::Client,
            request_queue: None,
            response_queue: Some(format!("responses/{node_id}")),
            transport_type: None,
            keep_alive_secs: None,
        },
        hub,
    )
    .await
}

#[tokio::test]
async fn test_basic_request() -> Result<()> {
    // ---
    init_tracing();
    info!("Starting basic sensor reading test");

    let server = SensorServer::new("test_basic_request").await?;
    let client_transport = client_transport("controller", server.hub()).await?;
    let client = RpcBrokerBuilder::new(client_transport).build()?;

    info!("reading temperature in Celsius...");
    let resp: SensorReading = client
        .request_to(
            server.node_id(),
            "temperature",
            ReadTemperature {
                unit: TemperatureUnit::Celsius,
            },
        )
        .await?;
    info!("reading temperature in Celsius...done");

    assert_eq!(resp.value, 23.5);
    assert_eq!(resp.unit, "°C");
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_requests() -> Result<()> {
    // ---
    init_tracing();

    let server = SensorServer::new("test_concurrent_requests").await?;
    let node_id = server.node_id().to_string();
    let mut handles = Vec::new();

    for i in 0..10 {
        // ---
        let transport = client_transport(&format!("controller-{i}"), server.hub()).await?;
        let client = RpcBrokerBuilder::new(transport).build()?;
        let node_id = node_id.clone();

        handles.push(tokio::spawn(async move {
            // Alternate between temperature readings in different units
            let unit = if i % 2 == 0 {
                TemperatureUnit::Celsius
            } else {
                TemperatureUnit::Fahrenheit
            };
            let resp: SensorReading = client
                .request_to(&node_id, "temperature", ReadTemperature { unit })
                .await
                .unwrap();
            (resp.value, resp.unit)
        }));
    }

    for (i, task) in handles.into_iter().enumerate() {
        let (value, unit) = task.await.unwrap();
        if i % 2 == 0 {
            assert_eq!(value, 23.5);
            assert_eq!(unit, "°C");
        } else {
            assert_eq!(value, 74.3);
            assert_eq!(unit, "°F");
        }
    }

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_timeout() -> Result<()> {
    // ---
    init_tracing();

    let hub = MemoryHub::new();
    let server_node = "slow-sensor";
    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    // Register a slow temperature sensor that takes 1 second to respond
    server.register("temperature", |req: ReadTemperature| async move {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let value = match req.unit {
            TemperatureUnit::Celsius => 23.5,
            TemperatureUnit::Fahrenheit => 74.3,
        };
        let unit = match req.unit {
            TemperatureUnit::Celsius => "°C",
            TemperatureUnit::Fahrenheit => "°F",
        };
        Ok(SensorReading {
            value,
            unit: unit.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("controller", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    info!("test_timeout: reading temperature");
    let fut = client.request_to::<ReadTemperature, SensorReading>(
        server_node,
        "temperature",
        ReadTemperature {
            unit: TemperatureUnit::Celsius,
        },
    );
    let res = tokio::time::timeout(Duration::from_millis(200), fut).await;
    assert!(res.is_err(), "request unexpectedly completed");
    info!("test_timeout: {:?}", res);

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
#[ignore] // TODO: Implement error response protocol
async fn test_error_response() -> Result<()> {
    // ---
    init_tracing();

    let hub = MemoryHub::new();
    let server_node = "faulty-sensor";
    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    // Register a sensor that fails for specific conditions
    server.register("temperature", |req: ReadTemperature| async move {
        // Simulate sensor malfunction for Fahrenheit readings
        match req.unit {
            TemperatureUnit::Fahrenheit => Err(RpcError::InvalidRequest),
            TemperatureUnit::Celsius => Ok(SensorReading {
                value: 23.5,
                unit: "°C".to_string(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            }),
        }
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("controller", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    // Test error case
    let result: Result<SensorReading> = client
        .request_to(
            server_node,
            "temperature",
            ReadTemperature {
                unit: TemperatureUnit::Fahrenheit,
            },
        )
        .await;
    assert!(result.is_err(), "expected error for Fahrenheit reading");

    // Test success case
    let resp: SensorReading = client
        .request_to(
            server_node,
            "temperature",
            ReadTemperature {
                unit: TemperatureUnit::Celsius,
            },
        )
        .await?;
    assert_eq!(resp.value, 23.5);

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn test_multiple_clients() -> Result<()> {
    // ---
    init_tracing();

    let server = SensorServer::new("test_multiple_clients").await?;
    let node_id = server.node_id().to_string();

    let t1 = client_transport("controller-1", server.hub()).await?;
    let t2 = client_transport("controller-2", server.hub()).await?;
    let t3 = client_transport("controller-3", server.hub()).await?;

    let client1 = RpcBrokerBuilder::new(t1).build()?;
    let client2 = RpcBrokerBuilder::new(t2).build()?;
    let client3 = RpcBrokerBuilder::new(t3).build()?;

    let (r1, r2, r3) = tokio::join!(
        client1.request_to::<ReadTemperature, SensorReading>(
            &node_id,
            "temperature",
            ReadTemperature {
                unit: TemperatureUnit::Celsius
            }
        ),
        client2.request_to::<ReadHumidity, SensorReading>(&node_id, "humidity", ReadHumidity),
        client3.request_to::<ReadPressure, SensorReading>(&node_id, "pressure", ReadPressure),
    );

    let temp_reading = r1?;
    let humidity_reading = r2?;
    let pressure_reading = r3?;

    assert_eq!(temp_reading.value, 23.5);
    assert_eq!(temp_reading.unit, "°C");

    assert_eq!(humidity_reading.value, 45.2);
    assert_eq!(humidity_reading.unit, "%RH");

    assert_eq!(pressure_reading.value, 1013.25);
    assert_eq!(pressure_reading.unit, "hPa");

    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_request_with_timeout_success() -> Result<()> {
    // ---
    init_tracing();

    let server = SensorServer::new("test_timeout_success").await?;
    let node_id = server.node_id().to_string();

    let transport = client_transport("controller", server.hub()).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    let resp: SensorReading = client
        .request_to_with_timeout(&node_id, "humidity", ReadHumidity, Duration::from_secs(1))
        .await?;

    assert_eq!(resp.value, 45.2);
    assert_eq!(resp.unit, "%RH");
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_request_with_timeout_expires() -> Result<()> {
    // ---
    init_tracing();

    let hub = MemoryHub::new();
    let server_node = "slow-sensor-2";

    let transport = server_transport(server_node, hub.clone()).await?;
    let server = RpcBrokerBuilder::new(transport).build()?;

    // Register a slow pressure sensor
    server.register("pressure", |_req: ReadPressure| async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(SensorReading {
            value: 1013.25,
            unit: "hPa".to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    })?;

    let _handle = server.clone().spawn()?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    let transport = client_transport("controller", hub).await?;
    let client = RpcBrokerBuilder::new(transport).build()?;

    let result: Result<SensorReading> = client
        .request_to_with_timeout(
            server_node,
            "pressure",
            ReadPressure,
            Duration::from_millis(50),
        )
        .await;

    match result {
        Err(RpcError::Timeout) => {}
        Ok(_) => panic!("expected timeout but request succeeded"),
        Err(e) => panic!("expected Timeout error but got: {e}"),
    }

    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn test_run_blocks_until_shutdown() -> Result<()> {
    // ---
    use std::sync::atomic::{AtomicBool, Ordering};
    init_tracing();

    let hub = MemoryHub::new();
    let node_id = "sensor-hub";
    let transport = server_transport(node_id, hub).await?;
    let broker = RpcBrokerBuilder::new(transport).build()?;

    broker.register("temperature", |req: ReadTemperature| async move {
        let value = match req.unit {
            TemperatureUnit::Celsius => 23.5,
            TemperatureUnit::Fahrenheit => 74.3,
        };
        let unit = match req.unit {
            TemperatureUnit::Celsius => "°C",
            TemperatureUnit::Fahrenheit => "°F",
        };
        Ok(SensorReading {
            value,
            unit: unit.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    })?;

    let shutdown_called = Arc::new(AtomicBool::new(false));
    let shutdown_called_clone = shutdown_called.clone();
    let broker_clone = broker.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown_called_clone.store(true, Ordering::SeqCst);
        broker_clone.shutdown().await;
    });

    broker.run().await?;

    assert!(
        shutdown_called.load(Ordering::SeqCst),
        "run() returned before shutdown() was called"
    );
    Ok(())
}

/// Verify that TransportBuilder without an explicit transport_type falls back to
/// memory transport when no broker transport features are enabled.
#[cfg(not(any(
    feature = "transport_rumqttc",
    feature = "transport_lapin",
    feature = "transport_dust_dds"
)))]
#[tokio::test]
async fn test_transport_builder_fallback_to_memory() -> Result<()> {
    // ---
    init_tracing();

    // With no transport_type set, the builder tries dust_dds → rumqttc → lapin → memory.
    // In the default feature set (no broker transports enabled), all three broker
    // factories return Err immediately via Null Object stubs, so memory is used.
    let transport = TransportBuilder::new()
        .uri("memory://")
        .node_id("fallback-test")
        .full_duplex()
        .build()
        .await?;

    // Verify we got a working memory transport by doing a basic pub/sub round-trip
    use bytes::Bytes;
    use mom_rpc::{Address, Envelope, Subscription};

    let sub = transport
        .subscribe(Subscription::from("fallback/test"))
        .await?;

    let env = Envelope::response(
        Address::from("fallback/test"),
        Bytes::from_static(b"hello"),
        "corr-1".into(),
        "application/json".into(),
    );
    transport.publish(env).await?;

    let received = tokio::time::timeout(Duration::from_millis(100), {
        let mut sub = sub;
        async move { sub.inbox.recv().await }
    })
    .await
    .expect("timed out")
    .expect("channel closed");

    assert_eq!(received.payload, Bytes::from_static(b"hello"));
    Ok(())
}

use std::sync::Once;

static INIT: Once = Once::new();

fn init_tracing() {
    // ---
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_line_number(true)
            .with_ansi(false)
            .try_init();
    });
}
