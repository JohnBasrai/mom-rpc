# Redis Pub/Sub Transport Design for mom-rpc

## Overview

Add a `transport_redis` feature implementing the `Transport` trait over Redis
Pub/Sub, consistent with existing transport semantics (memory, rumqttc, lapin,
dust_dds).

This document serves as both a design reference and implementation guide.
It is co-located with the implementation at `src/transport/redis/README.md`.

---

## Why Redis Pub/Sub (not Streams)

Redis has three messaging primitives:

| Primitive | Semantics | Fit for mom-rpc |
|-----------|-----------|-----------------|
| Pub/Sub | Fire-and-forget, topic-based | ✅ Best fit |
| Streams (`XADD`/`XREAD`) | Persistent, consumer groups | ❌ Poor fit |
| Lists | Point-to-point queue | ❌ Poor fit |

**Streams were rejected** because:
- `XREAD` returns all messages on a stream — every client sees every response,
  requiring correlation ID filtering in the application layer
- No topic-level isolation — any node can read `responses/{node-id}`
- Security: arbitrary nodes can eavesdrop on both sides of an exchange
- Adds boilerplate that mom-rpc exists to eliminate

**Pub/Sub was chosen** because:
- Topic semantics map directly to existing `Transport` trait (`publish`/`subscribe`)
- Only subscribers on a given topic receive messages — clean isolation
- Semantics match memory and MQTT transports (fire-and-forget, non-durable)
- `Transport` trait implementation is nearly identical to rumqttc

---

## Security Model

Redis Pub/Sub has no built-in topic-level ACLs by default. Any connected
client can subscribe to any topic — the same trust model as MQTT without
broker ACLs and AMQP without vhost permissions.

**This is not a mom-rpc concern.** Security is enforced at the broker
configuration layer, transparently to mom-rpc:

- Redis 6+ supports `ACL SETUSER` to restrict which channels a user can
  pub/sub on
- Credentials passed via URI: `redis://:password@localhost:6379`
- The `redis` crate passes credentials through; mom-rpc does not inspect them

The same application code works in open dev environments and ACL-secured
production deployments without any mom-rpc changes. This is consistent with
all other mom-rpc transports.

---

## Crate Selection

| Crate | Features needed |
|-------|----------------|
| `redis` | `tokio-comp`, `aio` |

No `r2d2` connection pool — that is for synchronous Redis usage. Async
connection management is handled directly via `redis::aio`. No transitive
`libc` dependency (unlike rdkafka).

```toml
[dependencies]
redis = { version = "0.25", features = ["tokio-comp", "aio"], optional = true }

[features]
transport_redis = ["dep:redis"]
```

Feature name follows library convention (crates.io enforces uniqueness of
library names, so `transport_redis` is unambiguous).

---

## URI Format

```
redis://localhost:6379
redis://:password@localhost:6379   # with auth
```

Consistent with other transports using scheme-prefixed URIs.

---

## Transport Trait Mapping

| Transport method | Redis operation |
|-----------------|-----------------|
| `publish(env)` | `PUBLISH <topic> <json-payload>` |
| `subscribe(sub)` | `SUBSCRIBE <topic>` + register local inbox |
| `close()` | Disconnect both connections, signal shutdown |

---

## Key Redis Pub/Sub Constraint: Two Connections Required

Redis **requires a dedicated connection** for Pub/Sub — a connection in
Pub/Sub mode cannot issue regular commands like `PUBLISH`. Two async
connections are required:

- `publish_conn` — regular `redis::aio::Connection`, used only for `PUBLISH`
- `pubsub_conn` — `redis::aio::PubSub` connection, used for `SUBSCRIBE`
  and receiving incoming messages

`publish_conn` is used imperatively inside the `Cmd::Publish` arm — awaited
inline, not polled via `select!`.

---

## Concurrency Model

Same actor pattern as `rumqttc` — a single background task owns both Redis
connections. All application interaction is serialized through an `mpsc`
command channel, preserving `Send + Sync` on the public `Transport` impl.

### Actor select! loop — 3 arms across 2 connections

```rust
loop {
    tokio::select! {
        cmd = self.cmd_rx.recv() => {
            // Publish (uses publish_conn inline), Subscribe, Close
            match cmd {
                Some(cmd) => {
                    if matches!(cmd.handle(&mut self).await, ActorStep::Stop) {
                        break;
                    }
                }
                None => break,
            }
        }

        msg = self.pubsub_conn.on_message() => {
            // Deserialize envelope, fan out to local subscribers
            Self::handle_incoming(
                self.transport_id.clone(),
                Arc::clone(&self.subscribers),
                msg,
            ).await;
        }

        _ = self.shutdown.notified() => {
            break;
        }
    }
}
```

| select! arm | Connection used | Purpose |
|-------------|----------------|---------|
| `cmd_rx.recv()` | `publish_conn` (inline) | Application commands |
| `pubsub_conn.on_message()` | `pubsub_conn` | Incoming broker messages → fanout |
| `shutdown.notified()` | — | Clean teardown |

---

## Actor Commands

```rust
enum Cmd {
    Publish {
        env: Envelope,
        resp: oneshot::Sender<Result<()>>,
    },
    Subscribe {
        topic: String,
        resp: oneshot::Sender<Result<Ack>>,
    },
    Close {
        resp: oneshot::Sender<Result<()>>,
    },
}
```

---

## Subscribe Serialization

Redis Pub/Sub sends a confirmation message when `SUBSCRIBE` completes,
analogous to MQTT SUBACK. Like SUBACK, it contains no topic name for
correlation when concurrent subscribes are in flight.

**Solution:** Same `Ack::Retry` pattern established in the rumqttc transport:

- If a subscribe is already in flight, actor responds `Ack::Retry`
- Caller in `Transport::subscribe` sleeps 200ms and retries
- Serializes subscribes without blocking the actor event loop
- No recursive async fn, no `Box::pin` required

```rust
pub enum Ack {
    Ok,
    Retry,
}
```

---

## Struct Layout

```rust
pub struct RedisTransport {
    base: TransportBase,
    cmd_tx: mpsc::Sender<Cmd>,
    subscribers: SubscriberMap,
    tasks: TaskList,
}

struct RedisActor {
    transport_id: String,
    publish_conn: redis::aio::Connection,
    pubsub_conn: redis::aio::PubSub,
    cmd_rx: mpsc::Receiver<Cmd>,
    subscribers: SubscriberMap,
    pending_subscribe: PendingSubscribe,
    shutdown: Arc<tokio::sync::Notify>,
    reconnect: bool,
}
```

Type aliases consistent with existing transports:

```rust
type SubscriberMap    = Arc<RwLock<HashMap<String, Vec<mpsc::Sender<Envelope>>>>>;
type TaskList         = Arc<RwLock<Vec<JoinHandle<()>>>>;
type PendingSubscribe = Arc<RwLock<Option<(String, oneshot::Sender<Result<Ack>>)>>>;
```

---

## Module Layout

Follows EMBP (Explicit Module Boundary Pattern):

```
src/transport/
    redis/
        mod.rs     # pub use + null object stub when feature disabled
        redis.rs   # RedisTransport + RedisActor implementation
        README.md  # This document
```

`mod.rs`:

```rust
//! Redis protocol transports.
//!
//! This module contains transport implementations for Redis.
//! Currently supports:
//! - redis - Redis Pub/Sub via redis library (redis.rs)

#[cfg(feature = "transport_redis")]
mod redis;

#[cfg(feature = "transport_redis")]
pub use redis::create_transport as create_redis_transport;

#[cfg(not(feature = "transport_redis"))]
pub async fn create_redis_transport(
    _config: crate::TransportConfig,
) -> crate::Result<crate::TransportPtr> {
    Err(crate::RpcError::Transport(
        "transport_redis feature is not enabled".into(),
    ))
}
```

---

## Reconnect Behavior

On connection error, actor sets `reconnect = true` and sleeps `RECONNECT_DELAY`
(2 seconds) before continuing the loop. On reconnect, re-subscribe to all
topics currently in the subscriber map — same pattern as rumqttc.

---

## Manual Test

Use `sensor_fullduplex` — it creates both server and client subscriptions
back-to-back, exercising the concurrent subscribe serialization path. This is
the primary stress case for the transport layer.

The manual test's job is to validate the **transport layer**, not `RpcBroker`
or RPC logic.

```bash
# Start broker
docker run -d --name mom-rpc-test-redis -p 6379:6379 redis:latest

# Run test
env BROKER_URI="redis://localhost:6379" \
    cargo run --example sensor_fullduplex --features transport_redis
```

Add `redis.sh` to `scripts/manual-tests/` following the same pattern as
`mqtt.sh` — Docker container lifecycle, build, run, validate output contains
Temperature / Humidity / Pressure, trap-based cleanup.

---

## TransportBuilder Integration

Add to the `match` dispatch in `TransportBuilder::build()`:

```rust
Some("redis") => crate::create_redis_transport(config).await,
```

And in the `None` auto-detect chain:

```rust
if let Ok(t) = crate::create_redis_transport(config.clone()).await {
    return Ok(t);
}
```

---

## Out of Scope

- Redis Streams (`XADD`/`XREAD`) — poor fit, see rationale above
- Persistent/durable delivery — Pub/Sub is fire-and-forget by design
- Retained messages — not supported by Redis Pub/Sub
- Pattern subscriptions (`PSUBSCRIBE`) — not needed for RPC use case
- `r2d2` connection pooling — sync only, not applicable
