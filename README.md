# mqtt-rpc-rs

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

* **No callbacks required**
  Client APIs return futures; servers use async handlers.

* **Explicit invariants**
  Correlation, routing, and dispatch rules are enforced by types.

---

## Transports

The crate includes a **memory transport** by default, which:

* requires no broker
* is always enabled
* is used for integration testing
* defines the reference semantics for other transports

Additional transports (e.g. MQTT) are implemented behind feature flags and conform to the same contract.

---

## Example

### Server

```rust
use mqtt_rpc_rs::{RpcServer, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut server = RpcServer::new();

    server.register("math.add", |(a, b): (i32, i32)| async move {
        Ok(a + b)
    });

    server.start().await
}
```

### Client

```rust
use mqtt_rpc_rs::{RpcClient, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = RpcClient::new();

    let sum: i32 = client
        .request_to("server", "math.add", (2, 3))
        .await?;

    assert_eq!(sum, 5);
    Ok(())
}
```

(Examples use the in-memory transport by default.)

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

## Status

* Actively developed
* API still evolving
* Feedback welcome
