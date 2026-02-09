[![Crates.io](https://img.shields.io/crates/v/mom-rpc.svg)](https://crates.io/crates/mom-rpc)
[![Documentation](https://docs.rs/mom-rpc/badge.svg)](https://docs.rs/mom-rpc)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# mom-rpc

**Transport-agnostic async RPC over message-oriented middleware.**

This crate provides a clean, strongly-typed RPC abstraction on top of unreliable or awkward pub/sub systems (such as MQTT), without baking transport details into your application code.

It is designed to *tame* message brokers, not expose them.

---

## Lean by Design

While this crate supports multiple transport implementations, **applications only compile the transports they enable**. The crate size shown on crates.io is the total of all transport implementations combined, but thanks to Cargo features, your application will only include the code for the transports you actually use. A typical application using a single transport will compile to approximately 45-55 KiB of mom-rpc code regardless of how many total transports the library supports.


---

## Why this exists

Message-oriented middleware (MQTT, pub/sub systems, etc.) is great for decoupling — but painful for request/response workflows:

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
use mom_rpc::{create_transport, RpcClient, RpcConfig, RpcServer, Result};

let config = RpcConfig::memory("math");
let transport = create_transport(&config).await?;

let server = RpcServer::with_transport(transport.clone(), "math");
server.register("add", |req: AddRequest| async move {
    Ok(AddResponse { sum: req.a + req.b })
});
let _handle = server.spawn();

let client = RpcClient::with_transport(transport.clone(), "client-1").await?;
let resp: AddResponse = client.request_to("math", "add", AddRequest { a: 2, b: 3 }).await?;
```

<details>
<summary><b>View complete memory example</b></summary>

```rust
use mom_rpc::{create_transport, Result, RpcClient, RpcConfig, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse {
    sum: i32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = RpcConfig::memory("math");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    let _handle = server.spawn();

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    assert_eq!(resp.sum, 5);
    Ok(())
}
```

</details>

**Run it:**
```bash
cargo run --example math_memory
```

### Testing with RabbitMQ

   See [scripts/manual-tests/README.md](scripts/manual-tests/README.md) for automated test scripts.

<details>
<summary><b>Expected test output</b></summary>

```bash
./scripts/manual-tests/rabbitmq.sh transport_lapin
==> Checking prerequisites...
==> Starting RabbitMQ broker...
    Container: mom-rpc-test-rabbitmq
    AMQP port: 5672
    Management UI: http://localhost:15672 (guest/guest)
    Waiting for broker to be ready...
    ✓ RabbitMQ broker ready

==> Building examples with feature: transport_lapin
    ✓ Examples built successfully

==> Starting math_server...
    Server PID: 272977
    Waiting for server to initialize...
    ✓ Server running

==> Running math_client...

✅ RabbitMQ integration test PASSED

Feature tested: transport_lapin
Broker URI: amqp://localhost:5672/%2f
Output:2 + 3 = 5

==> Cleaning up...
Killing server (PID: 272977)...
Stopping RabbitMQ container...
```
</details>

### MQTT Transport (Production)

For distributed deployments with an MQTT broker:

**Cargo.toml:**
```toml
[dependencies]
mom-rpc = { version = "0.4", features = ["transport_rumqttc"] }
```

**Basic usage:**
```rust
// Server
let config = RpcConfig::with_broker("mqtt://localhost:1883", "math-server");
let transport = create_transport(&config).await?;
let server = RpcServer::with_transport(transport.clone(), "math");
server.register("add", |req: AddRequest| async move { /* ... */ });
server.run().await?;

// Client
let config = RpcConfig::with_broker("mqtt://localhost:1883", "math-client");
let transport = create_transport(&config).await?;
let client = RpcClient::with_transport(transport.clone(), "client-1").await?;
let resp: AddResponse = client.request_to("math", "add", AddRequest { a: 2, b: 3 }).await?;
```

<details>
<summary><b>View complete server with broker example</b></summary>
```rust

use mom_rpc::{create_transport, RpcConfig, RpcServer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest { a: i32, b: i32 }

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse { sum: i32 }

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    env_logger::init();

    let broker_uri = std::env::var("BROKER_URI")
        .unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "math-server");
    let transport = create_transport(&config).await?;

    let server = RpcServer::with_transport(transport.clone(), "math");

    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    // Setup graceful shutdown
    let server_clone = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl+C");
        server_clone.shutdown().await.expect("shutdown failed");
    });

    server.run().await?;
    transport.close().await?;
    Ok(())
}
```

</details>

<details>
<summary><b>View complete client with broker example</b></summary>

```rust
use mom_rpc::{create_transport, RpcClient, RpcConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AddRequest { a: i32, b: i32 }

#[derive(Debug, Serialize, Deserialize)]
struct AddResponse { sum: i32 }

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    env_logger::init();

    let broker_uri =
        std::env::var("BROKER_URI")
        .unwrap_or_else(|_| "mqtt://localhost:1883".to_string());

    let config = RpcConfig::with_broker(&broker_uri, "math-client");
    let transport = create_transport(&config).await?;

    let client = RpcClient::with_transport(transport.clone(), "client-1").await?;

    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    println!("2 + 3 = {}", resp.sum);
    transport.close().await?;
    Ok(())
}
```

</details>

**Run it:**
```bash
# Terminal 1: Start MQTT broker
docker run -p 1883:1883 eclipse-mosquitto

# Terminal 2: Start server
cargo run --example math_server --features transport_rumqttc

# Terminal 3: Send requests
cargo run --example math_client --features transport_rumqttc
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
mom-rpc = { version = "0.4", default-features = false, features = ["transport_rumqttc"] }
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
  mom-rpc = { version = "0.4", features = ["transport_rumqttc"] }
  ```

Additional transports may be added in the future behind feature flags.

---

## Feature flags

| Flag | Description | Status |
|:-----|:------------|:-------|
| `transport_rumqttc`  | MQTT via rumqttc | 🌟 **Recommended** |
| `transport_lapin` | AMQP via lapin (RabbitMQ) | ✅ Available |
| `logging` | Enable log output (uses `log` crate) | ✅ Default |

The **memory transport is always available** - no feature flag required.

### Choosing Features

**For production MQTT deployments:**
```toml
[dependencies]
mom-rpc = { version = "0.4", features = ["transport_rumqttc"] }
```

**For testing without a broker:**
```toml
[dependencies]
mom-rpc = "0.4"  # Memory transport included by default
```

**Note:** If multiple transport features are enabled, `transport_rumqttc` takes priority, then memory as fallback.

---

## Timeout Handling

The RPC layer does not impose timeouts. Use `tokio::time::timeout` to add them:

<details>
<summary><b>Show timeout example</b></summary>

```rust
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(5),
    client.request_to("service", "method", request)
).await;

match result {
    Ok(Ok(response)) => { /* success */ }
    Ok(Err(e)) => { /* RPC error */ }
    Err(_) => { /* timeout */ }
}
```

</details>

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

The `rumqttc` transport supports TLS but delegates certificate validation and connection security to the broker. Transport security is intentionally treated as an external concern to avoid coupling RPC semantics to cryptographic policy.

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

MIT
