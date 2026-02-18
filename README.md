[![Crates.io](https://img.shields.io/crates/v/mom-rpc.svg)](https://crates.io/crates/mom-rpc)
[![Documentation](https://docs.rs/mom-rpc/badge.svg)](https://docs.rs/mom-rpc)
[![CI](https://github.com/JohnBasrai/mom-rpc/actions/workflows/ci.yml/badge.svg)](https://github.com/JohnBasrai/mom-rpc/actions)

# mom-rpc

**Transport-agnostic async RPC over message-oriented middleware.**

This crate provides a clean, strongly-typed RPC abstraction on top of pub/sub systems (such as MQTT or AMQP), without baking transport details into your application code.

It is designed to *tame* message brokers, not expose them to application level code.

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

## How is this different?

Unlike RPC libraries that target a single broker, `mom-rpc` provides transport abstraction—write your RPC code once, run it on MQTT, AMQP, DDS, or in-memory. The same application works across different brokers by changing feature flags, not code. This matters when you need to deploy the same edge application across different infrastructure, or when your broker choice changes between development and production.

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
 use mom_rpc::{TransportBuilder, RpcBrokerBuilder, Result};
 use serde::{Deserialize, Serialize};

 #[derive(Debug, Serialize, Deserialize)]
 struct ReadTemperature { unit: TemperatureUnit }

 #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
 enum TemperatureUnit { Celsius, Fahrenheit }

 #[derive(Debug, Serialize, Deserialize)]
 struct SensorReading { value: f32, unit: String, timestamp_ms: u64 }

 #[tokio::main]
 async fn main() -> Result<()> {
     //
     let transport = TransportBuilder::new()
         .uri("memory://")
         .node_id("env-sensor-42")
         .full_duplex()
         .build()
         .await?;

     let server = RpcBrokerBuilder::new(transport.clone()).build()?;
     server.register("read_temperature", |req: ReadTemperature| async move {
         let celsius = 22.0_f32; // Simulate reading hw sensor
         let (value, unit) = match req.unit {
             TemperatureUnit::Celsius => (celsius, "C"),
             TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
         };
         Ok(SensorReading { value, unit: unit.to_string(), timestamp_ms: 0 })
     })?;
     let _handle = server.spawn()?;

     let client = RpcBrokerBuilder::new(transport).build()?;
     let resp: SensorReading = client
         .request_to("env-sensor-42", "read_temperature", ReadTemperature {
             unit: TemperatureUnit::Celsius,
         }).await?;
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
./scripts/manual-tests/amqp.sh  transport_lapin
==> Checking prerequisites...
==> Starting RabbitMQ broker...
    Container: mom-rpc-test-rabbitmq
    AMQP port: 5672
    Management UI: http://localhost:15672 (guest/guest)
    Waiting for broker to be ready...
..
    ✓ RabbitMQ broker ready (took 5s)

==> Building examples with feature: transport_lapin
    ✓ Examples built successfully

==> Starting sensor_server...
    Server PID: 1484035
    ✓ Server running

==> Running sensor_client...

✅ AMQP integration test PASSED

Feature tested: transport_lapin
Broker URI: amqp://localhost:5672/%2f
Output:
Temperature: 21.5 C @ 1771360006536
Humidity:    55 % @ 1771360006580
Pressure:    101.3 kPa @ 1771360006624

==> Cleaning up...
Killing server (PID: 1484035)...
Stopping RabbitMQ container...
```
</details>

### MQTT Transport (Production)

For distributed deployments with an MQTT broker:

**Cargo.toml:**
```toml
[dependencies]
mom-rpc = { version = "0.8", features = ["transport_rumqttc"] }
```

**Server:**
```rust
use mom_rpc::{TransportBuilder, RpcBrokerBuilder, Result};

let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("env-sensor-42")
    .server_mode()
    .build()
    .await?;

let server = RpcBrokerBuilder::new(transport.clone()).build()?;

server.register("read_temperature", |req: ReadTemperature| async move {
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

// Run blocks until shutdown
server.run().await?;
transport.close().await?;
```

**Client:**
```rust
use mom_rpc::{TransportBuilder, RpcBrokerBuilder, Result};
use std::time::Duration;

let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("sensor-client")
    .client_mode()
    .build()
    .await?;

let client = RpcBrokerBuilder::new(transport.clone())
    .retry_max_attempts(5)
    .retry_initial_delay(Duration::from_millis(200))
    .retry_max_delay(Duration::from_secs(5))
    .request_total_timeout(Duration::from_secs(30))
    .build()?;

let resp: SensorReading = client
    .request_to(
        "env-sensor-42",
        "read_temperature",
        ReadTemperature {
            unit: TemperatureUnit::Celsius,
        },
    )
    .await?;

println!("Temperature: {} {}", resp.value, resp.unit);
transport.close().await?;
```
See complete working [examples](https://github.com/JohnBasrai/mom-rpc/tree/main/examples):
 - `examples/sensor_server.rs`
 - `examples/sensor_client.rs`
 - `examples/sensor_fullduplex.rs`

---

## Logging

By default, `mom-rpc` emits structured logs via the `tracing` crate.

### Reduce Verbosity

Add a subscriber in your application:

```toml
[dependencies]
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

```rust
use tracing_subscriber::{fmt, EnvFilter};

fmt()
    .with_env_filter(EnvFilter::new("mom_rpc=warn"))
    .init();
```

---

### Runtime Control

You can control logging dynamically via the `RUST_LOG` environment variable:

```bash
# For just mom-rpc debug logs
RUST_LOG=mom_rpc=debug

# For all debug logs
RUST_LOG=debug

# For specific module
RUST_LOG=mom_rpc::retry=debug
```

If you are using a transport backend, you can configure multiple modules:

```bash
RUST_LOG=mom_rpc=debug,dust_dds=warn
```

---

### Disable Logging Entirely

Disable the `logging` feature:

```toml
[dependencies]
mom-rpc = { version = "0.8", default-features = false, features = ["transport_rumqttc"] }
```

`mom-rpc` does not install a global subscriber. The application is responsible for configuring `tracing`.

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

---

## Supported Transports

`mom-rpc` provides multiple transport backends. Each is feature-gated so you only compile what you use:

👉 The **memory transport is always available** - no feature flag required.

**You can enable multiple transports and choose at runtime:**
```toml
[dependencies]
mom-rpc = { version = "0.8", features = ["transport_rumqttc", "transport_lapin"] }
```

```rust
// TransportBuilder automatically tries enabled transports in order
let transport = TransportBuilder::new()
    .uri(&broker_uri)         // e.g., "mqtt://localhost:1883" or "amqp://localhost:5672/%2f"
    .node_id("client-id")
    .transport_type("lapin")  // or "rumqttc"
    .client_mode()
    .build()
    .await?;
```

The builder tries transports in this order: `dust_dds` → `rumqttc` → `lapin` → `memory`. The first compatible transport succeeds. For explicit control, use `.transport_type("rumqttc")`.

Applications can also run multiple transports concurrently (e.g., MQTT for IoT devices and AMQP for backend services) by creating separate transport instances.

**Transport implementation sizes (as of v0.8.2):**

| Transport | Feature Flag | SLOC | Use Case |
|:----------|:-------------|-----:|:---------|
| In-memory | *(always available)* | 107 | Testing, single-process |
| AMQP      | `transport_lapin`    | 313 | RabbitMQ, enterprise messaging |
| MQTT      | `transport_rumqttc`  | 404 | IoT, lightweight pub/sub |
| DDS       | `transport_dust_dds` | 703 | Real-time, mission-critical |

**Notes:**
 - *Core library: 1,381 lines, including In-memory.
 - *Total: 2,801 lines.*
 - *SLOC measured using `tokei` (crates.io methodology).*

Example: An application using only the MQTT transport compiles 1381 + 404 = 1785 lines of `mom-rpc` code.
With both MQTT and AMQP enabled: 1381 + 404 + 313 = 2098 lines.

---

## Overriding Default Timeout

`request_total_timeout` is a **total wall-clock budget** for the entire request, including all retry attempts. It is distinct from `retry_max_delay`, which caps the interval between individual retries.

The actual elapsed time may be less than the total timeout if the retry sequence exhausts its attempts first — whichever limit is reached first wins.

Configure timeouts per-request or at the broker level:


```rust
use std::time::Duration;


// Configure default timeout on the broker
let client = RpcBrokerBuilder::new(transport)
    .request_total_timeout(Duration::from_secs(5)) // global default timeout.
    .build()?;

// Per-request timeout (overrides the broker default)
let response: MyResponse = client
    .request_to_with_timeout(
        "service",
        "method",
        request,
        Duration::from_millis(1500),
    )
    .await?;

// Returns RpcError::Timeout if request exceeds timeout
```

---

## Full-Duplex Applications

For applications that both send and receive RPC calls (like device↔agent communication), use full-duplex mode:

```rust
use mom_rpc::{TransportBuilder, RpcBrokerBuilder, Result};

let transport = TransportBuilder::new()
    .uri(&broker_uri)
    .node_id("device-123")
    .full_duplex()  // ← Both request and response queues
    .build()
    .await?;

let broker = RpcBrokerBuilder::new(transport).build()?;

// Register handlers (acts as server)
broker.register("read-status", |req| async move {
    Ok(StatusResponse { /* ... */ })
})?;
broker.spawn()?;

// Make requests (acts as client)
let resp = broker.request_to("device-123", "read-status", req).await?;
```

See `examples/sensor_fullduplex.rs` for a complete full-duplex example, and [rust-edge-agent](https://github.com/JohnBasrai/rust-edge-agent/blob/main/src/agent/run.rs) for production usage.

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

## RPC Delivery Semantics

`mom-rpc` provides:

* **At-most-one response** delivered to the caller (first response wins).
* **No exactly-once guarantees.**

Handler invocation depends on the reliability of the underlying transport.
In failure or retry scenarios, a handler may be invoked more than once or not at all.

Applications requiring exactly-once effects must ensure idempotency or implement deduplication keyed by `correlation_id`.

---

## Transport-Specific Considerations

**Broker-based transports (MQTT, AMQP):**
Due to the star topology, there's a potential race condition during startup where
clients may publish before servers have subscribed. The unified broker API (0.8+) includes
built-in retry with exponential backoff to handle these races gracefully. Configure via
`RpcBrokerBuilder::retry_max_attempts()` and related methods.

**Peer-to-peer transports (DDS):**
Direct peer-to-peer discovery eliminates the startup race through explicit
reader/writer matching. DDS also eliminates the single point of failure and
serialization bottleneck inherent in broker-based architectures.

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

- [Complete API reference on docs.rs](https://docs.rs/mom-rpc)
- [Design patterns and module structure](docs/architecture.md)
- [Development guide and standards](CONTRIBUTING.md)
- [Release notes](https://github.com/JohnBasrai/mom-rpc/releases/tag/v0.8.2)

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
