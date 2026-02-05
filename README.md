# mom-rpc

**Transport-agnostic async RPC over message-oriented middleware.**

This crate provides a clean, strongly-typed RPC abstraction on top of unreliable or awkward pub/sub systems (such as MQTT), without baking transport details into your application code.

It is designed to *tame* message brokers, not expose them.

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

## Transports

The crate includes a **memory transport** by default.

The memory transport:

* requires no broker
* is always enabled
* is used for integration testing
* is **in-process only**
* requires all participants to share the same transport instance

It provides a deterministic loopback environment for testing and examples.
It does **not** model an external broker.

Broker-backed transports (e.g. MQTT, RabbitMQ) are implemented behind feature
flags and run out-of-process, with shared state managed by the broker itself.
All transports conform to the same RPC contract.

---

## Example

### Server

```rust
use mom_rpc::{create_transport, RpcConfig, RpcServer, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Create transport (memory for testing, MQTT for production)
    let config = RpcConfig::memory("math-server");
    let transport = create_transport(&config).await?;

    // Create server with transport and service name
    let server = RpcServer::with_transport(transport.clone(), "math".to_owned());

    // Register typed handlers
    server.register("add", |req: AddRequest| async move {
        Ok(AddResponse { sum: req.a + req.b })
    });

    // Setup graceful shutdown
    let server_clone = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl+C");
        server_clone.shutdown().await.expect("shutdown failed");
    });

    // Run server (blocks until shutdown)
    server.run().await?;
    transport.close().await?;
    Ok(())
}
```

### Client

```rust
use mom_rpc::{create_transport, RpcClient, RpcConfig, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Create transport (shared with server for memory, broker for MQTT)
    let config = RpcConfig::memory("math-client");
    let transport = create_transport(&config).await?;

    // Create client with transport and node ID
    let client = RpcClient::with_transport(transport.clone(), "client-1".to_string()).await?;

    // Make typed RPC call
    let resp: AddResponse = client
        .request_to("math", "add", AddRequest { a: 2, b: 3 })
        .await?;

    assert_eq!(resp.sum, 5);
    transport.close().await?;
    Ok(())
}
```

**Note:** Examples above show actual API. For complete working code, see [`examples/math_memory.rs`](examples/math_memory.rs).

---

## What this crate is **not**

This crate intentionally does **not** provide:

* exactly-once delivery
* durable message replay
* transactional guarantees
* broker configuration
* distributed consensus

It provides a **clean RPC abstraction**, not a distributed systems framework.

---

## Architecture

The design separates:

* transport mechanics
* RPC semantics
* user-facing APIs

For details, see:
**`docs/architecture.md`**

---

## Security note

The MQTT transport uses `mqtt-async-client`, which depends on an end-of-life TLS stack (rustls 0.19). This is a known ecosystem limitation.

`mom-rpc` intentionally treats transport security as an external concern.
In production deployments, users should:

- terminate TLS at the broker (recommended), or
- provide a custom transport backed by a maintained MQTT or message broker client

This avoids baking TLS policy and cryptographic maintenance into the RPC layer.


---

## Status

* Early development / API unstable
* Feedback welcome
