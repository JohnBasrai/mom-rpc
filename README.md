[![Crates.io](https://img.shields.io/crates/v/mom-rpc.svg)](https://crates.io/crates/mom-rpc)
[![Documentation](https://docs.rs/mom-rpc/badge.svg)](https://docs.rs/mom-rpc)
[![CI](https://github.com/JohnBasrai/mom-rpc/actions/workflows/ci.yml/badge.svg)](https://github.com/JohnBasrai/mom-rpc/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# mom-rpc

**Transport-agnostic async RPC over message-oriented middleware.**

This crate provides a clean, strongly-typed RPC abstraction on top of unreliable or awkward pub/sub systems (such as MQTT or AMQP), without baking transport details into your application code.

It is designed to *tame* message brokers, not expose them.

---

## Lean by Design

While this crate supports multiple transport implementations, **applications only compile the transports they enable**. The crate size shown on crates.io is the total of all transport implementations combined, but thanks to Cargo features, your application will only include the code for the transports you actually use. A typical application using a single transport will compile to approximately 45-55 KiB of mom-rpc code regardless of how many total transports the library supports.


---

## Why this exists

Message-oriented middleware (pub/sub systems, etc.) is great for decoupling — but painful for request/response workflows:

* no native RPC semantics
* no correlation handling
* unclear routing vs dispatch rules
* awkward client/server lifecycles

This crate solves that by providing:

* **async RPC semantics** (request / response)
* **correlation management**
* **method dispatch**
* **pluggable transports**
* **predictable behavior**, even over unreliable systems

---

## Key properties

* **Transport-agnostic**
  Works over multiple backends via a small transport trait.

* **Strongly typed RPC**
  Typed request and response payloads using `serde`.

* **Async-first**
  Built for Tokio, uses `async/await` throughout.

* **Cross-platform**
  Pure Rust with no platform-specific dependencies. Suitable for embedded/edge deployments.

* **No callbacks required**
  Client APIs return futures; servers use async handlers.

* **Explicit invariants**
  Correlation, routing, and dispatch rules are enforced by types.

---

## Quick Start

### In-Memory Transport (Testing)

Perfect for testing and single-process applications - no broker required:

```rust
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ReadTemperature { unit: TemperatureUnit }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TemperatureUnit { Celsius, Fahrenheit }

#[derive(Debug, Serialize, Deserialize)]
struct SensorReading { value: f32, unit: String, timestamp_ms: u64 }

#[tokio::main]
async fn main() -> Result<()> {
    let config = RpcConfig::memory("sensor");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");
    server.register("read_temperature", |req: ReadTemperature| async move {
        let celsius = 21.5_f32; // Simulate reading hw sensor
        let (value, unit) = match req.unit {
            TemperatureUnit::Celsius => (celsius, "C"),
            TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
        };
        Ok(SensorReading { value, unit: unit.to_string(), timestamp_ms: 0 })
    });
    let _handle = server.spawn();

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;
    let resp: SensorReading = client
        .request_to(
            "env-sensor-42",
            "read_temperature",
            ReadTemperature { unit: TemperatureUnit::Celsius },
        )
        .await?;

    println!("Temperature: {} {}", resp.value, resp.unit);
    Ok(())
}
```

**Run it:**
```bash
cargo run --example sensor_memory
```

### Testing with RabbitMQ

   See [scripts/manual-tests/README.md](scripts/manual-tests/README.md) for automated test scripts.

<details>
<summary><b>Expected test output</b></summary>

```bash
./scripts/manual-tests/amqp.sh transport_lapin
==> Checking prerequisites...
==> Starting RabbitMQ broker...
    Container: mom-rpc-test-rabbitmq
    AMQP port: 5672
    Management UI: http://localhost:15672 (guest/guest)
    Waiting for broker to be ready...
    ✓ RabbitMQ broker ready

==> Building examples with feature: transport_lapin
    ✓ Examples built successfully

==> Starting sensor_server...
    Server PID: 784650
    Waiting for server to initialize...
    ✓ Server running

==> Running sensor_client...

✅ RabbitMQ integration test PASSED

Feature tested: transport_lapin
Broker URI: amqp://localhost:5672/%2f
Output:
Temperature: 21.5 C @ 1770741387954
Humidity:    55 % @ 1770741387997
Pressure:    101.3 kPa @ 1770741388039

==> Cleaning up...
Killing server (PID: 784650)...
Stopping RabbitMQ container...
```
</details>

### MQTT Transport (Production)

For distributed deployments with an MQTT broker:

**Cargo.toml:**
```toml
[dependencies]
mom-rpc = { version = "0.x", features = ["transport_rumqttc"] }
```

**Basic broker usage:**

```rust
// Server
let config = RpcConfig::with_broker("mqtt://localhost:1883", "env-sensor-42");
let transport = create_transport(&config).await?;
let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");
server.register(
    "read_temperature",
    |req: ReadTemperature| async move { /* ... */ },
);
server.run().await?;

// Client
let config = RpcConfig::with_broker("mqtt://localhost:1883", "sensor-client");
let transport = create_transport(&config).await?;
let client = RpcClient::with_transport(transport.clone(), "client-1").await?;
let resp: SensorReading = client
    .request_to(
        "env-sensor-42",
        "read_temperature",
        ReadTemperature { unit: TemperatureUnit::Celsius },
    )
    .await?;
```

<details>
<summary><b>View complete server with broker example</b></summary>

```rust
use mom_rpc::{create_transport, Result, RpcConfig, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ReadTemperature { unit: TemperatureUnit }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TemperatureUnit { Celsius, Fahrenheit }

#[derive(Debug, Serialize, Deserialize)]
struct SensorReading { value: f32, unit: String, timestamp_ms: u64 }

#[tokio::main]
async fn main() -> Result<()> {

    env_logger::init();

    let broker_uri = std::env::var("BROKER_URI")
        .unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "env-sensor-42");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "env-sensor-42");

    server.register("read_temperature", |req: ReadTemperature| async move {
        let celsius = 21.5_f32; // Simulate reading hw sensor
        let (value, unit) = match req.unit {
            TemperatureUnit::Celsius => (celsius, "C"),
            TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
        };
        Ok(SensorReading {
            value,
            unit: unit.to_string(),
            timestamp_ms: 0,
        })
    });

    let server_clone = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
        server_clone.shutdown().await.expect("shutdown failed");
    });

    server.run().await?;
    transport.close().await
}
```

</details>

<details>
<summary><b>View complete client with broker example</b></summary>

```rust
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ReadTemperature { unit: TemperatureUnit }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum TemperatureUnit { Celsius, Fahrenheit }

#[derive(Debug, Serialize, Deserialize)]
struct SensorReading { value: f32, unit: String, timestamp_ms: u64 }

#[tokio::main]
async fn main() -> Result<()> {

    env_logger::init();

    let broker_uri = std::env::var("BROKER_URI")
        .unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "sensor-client");
    let transport = create_transport(&config).await?;

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let resp: SensorReading = client
        .request_to(
            "env-sensor-42",
            "read_temperature",
            ReadTemperature { unit: TemperatureUnit::Celsius },
        )
        .await?;

    println!("Temperature: {} {} @ {}", resp.value, resp.unit, resp.timestamp_ms);
    transport.close().await
}
```

</details>

**Run it:**
```bash
# Terminal 1: Start MQTT broker
docker run -p 1883:1883 eclipse-mosquitto

# Terminal 2: Start server
cargo run --example sensor_server --features transport_rumqttc

# Terminal 3: Send requests
cargo run --example sensor_client --features transport_rumqttc
```

### Logging

The `logging` feature (enabled by default) provides diagnostic output via the `log` crate at `INFO` level during normal operation.

**To reduce verbosity**, lower the log level to `WARN` or `ERROR`:
```toml
[dev-dependencies]
env_logger = "0.11"
```
```rust
// Reduce mom-rpc logs to warnings only
env_logger::builder()
    .filter_module("mom_rpc", log::LevelFilter::Warn)
    .init();
```

**Note:** Running with `RUST_LOG=debug` will produce verbose output. This is useful for troubleshooting but not recommended for production.

**To disable logging entirely**, omit the feature:
```toml
[dependencies]
mom-rpc = { version = "0.x", default-features = false, features = ["transport_rumqttc"] }
```
---

## Transports

The crate includes a **memory transport** by default.

The memory transport:

* requires no broker
* is always enabled
* is used for integration testing
* is **in-process only**
* requires all participants to share the same transport instance

It provides a deterministic loopback environment for testing and examples. It does **not** model an external broker.

Broker-backed transports (e.g. MQTT) are implemented behind feature flags and run out-of-process, with shared state managed by the broker itself. All transports conform to the same RPC contract and approximate the in-memory transport's delivery semantics as closely as the underlying system allows.

### Available brokered transports

* **rumqttc (MQTT)** — 🌟 **Recommended MQTT backend**

  Enable via the `transport_rumqttc` feature. This implementation provides:
  - Actor-based architecture with safe concurrency
  - Lazy connection initialization
  - SUBACK-confirmed subscriptions
  - Active maintenance and modern async patterns

  ```toml
  mom-rpc = { version = "0.x", features = ["transport_rumqttc"] }
  ```

Additional transports may be added in the future behind feature flags.

---

## Feature flags

| Flag                 | Description               | Default Enable |
|:---------------------|:--------------------------|:---------------|
| `transport_rumqttc`  | MQTT via rumqttc          | ❌ No          |
| `transport_lapin`    | AMQP via lapin (RabbitMQ) | ❌ No          |
| `transport_dust_dds` | DDS via (dust_dds)        | ❌ No          |
| `logging`            | Enable logging output     | ✅ Yes         |

👉 The **memory transport is always available** - no feature flag required.

### Choosing Features

**For production MQTT deployments:**
```toml
[dependencies]
mom-rpc = { version = "0.x", features = ["transport_rumqttc"] }
```

**For testing without a broker:**
```toml
[dependencies]
mom-rpc = "0.x"  # Memory transport included by default
```

---

## Timeout Handling

Use the built-in `request_with_timeout` method for convenient timeout handling:

```rust
use std::time::Duration;

// Recommended: built-in timeout method
let response: MyResponse = client
    .request_with_timeout(
        "service",
        "method",
        request,
        Duration::from_secs(5),
    )
    .await?;

// Returns RpcError::Timeout if request exceeds timeout
```

---

## What this crate is **not**

This crate intentionally does **not** provide:

* exactly-once delivery
* durable message replay
* transactional guarantees
* broker configuration
* distributed consensus

This crate focuses narrowly on RPC semantics. It intentionally avoids higher-level distributed-systems concerns such as discovery, consensus, retries, or topology management. The underlying transport is expected to provide any required distributed-systems features.

---

## Architecture

The design separates:

* transport mechanics
* RPC semantics
* user-facing APIs

For details, see `docs/architecture.md`.

---

## Security

The `rumqttc` and `lapin` transports support TLS but delegate certificate validation and connection security to the broker. Transport security is intentionally treated as an external concern to avoid coupling RPC semantics to cryptographic policy.

<details>
<summary><b>Security best practices</b></summary>

### TLS/SSL

For production deployments:

* Use TLS-enabled brokers (port 8883)
* Configure broker authentication (username/password, certificates)
* Terminate TLS at the broker or use mTLS

### Authentication

This library does not handle authentication. Delegate to:

* Broker-level auth (username/password, client certificates)
* Network-level security (VPN, firewall rules)
* Message-level encryption (application responsibility)

</details>

---

## Documentation

* [API Documentation](https://docs.rs/mom-rpc) - Complete API reference on docs.rs
* [Architecture](docs/architecture.md) - Design patterns and module structure
* [Contributing](CONTRIBUTING.md) - Development guide and standards

---

## Status

* Early development / API unstable
* Feedback welcome via [GitHub issues](https://github.com/JohnBasrai/mom-rpc/issues)

---

## License

Licensed under either of:

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate by you shall be dual licensed as above, without any additional terms or conditions.
