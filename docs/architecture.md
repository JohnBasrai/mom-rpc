# Architecture

## Overview

This crate implements a **transport-agnostic RPC layer** designed to run over message-oriented middleware (MOM) systems such as MQTT, RabbitMQ, or in-memory transports used for testing.

The design deliberately separates:

* **Transport mechanics** (publish / subscribe, delivery, fanout)
* **RPC semantics** (request / response, correlation, method dispatch)
* **User-facing APIs** (client and server)

This separation allows the crate to:

* Tame unreliable or awkward transports (e.g. MQTT)
* Provide a consistent RPC programming model
* Remain extensible to future transports without redesigning the client/server APIs

The architecture follows an **Explicit Module Boundary Pattern (EMBP)** throughout.

---

## Layering

```
  ┌─────────────────────────────┐
  │         User Code           │
  │  (RpcClient / RpcServer)    │
  └───────────────▲─────────────┘
                  │
  ┌───────────────┴─────────────┐
  │         RPC Layer           │
  │  - Correlation handling     │
  │  - Method dispatch          │
  │  - Pending request tracking │
  └───────────────▲─────────────┘
                  │
  ┌───────────────┴─────────────┐
  │      Transport Layer        │
  │  - Publish / Subscribe      │
  │  - Delivery semantics       │
  │  - Addressing               │
  └───────────────▲─────────────┘
                  │
  ┌───────────────┴─────────────┐
  │     Concrete Transports     │
  │  - memory                   │
  │  - mqtt-async-client        │
  │  - (future: rabbitmq, etc.) │
  └─────────────────────────────┘
```

---

## Transport Layer

### Transport Trait

The transport layer is defined by a small trait that supports:

* Publishing an `Envelope`
* Subscribing to an address
* Delivering envelopes to a consumer via an async runner

The transport layer **does not implement RPC semantics**. It is a generic message transport.

### Envelope

All messages are carried inside a single transport-neutral structure:

```rust
pub struct Envelope {
    pub address: Address,
    pub method: Option<Arc<str>>,
    pub payload: Bytes,
    pub correlation_id: Option<Arc<str>>,
    pub reply_to: Option<Address>,
    pub content_type: Option<Arc<str>>,
}
```

Key points:

* `Envelope` is **transport-facing**, not RPC-facing
* Many fields are `Option<>` to allow non-RPC messages
* RPC invariants are enforced *above* this layer

Helper constructors enforce intent:

* `Envelope::request(...)` — requires RPC fields
* `Envelope::response(...)` — requires correlation id

---

## Memory Transport (Reference Implementation)

The in-memory transport serves as the **reference implementation** for transport behavior.

It:

* Uses in-process routing via `HashMap` + async channels
* Does not require a broker
* Is always enabled by default
* Is used heavily for integration testing

### Reference Semantics

All other transports are expected to **approximate the memory transport’s behavior as closely as their underlying system allows**.

In particular:

* No message replay on subscribe
* Fanout delivers messages to all subscribers
* No implicit durability or persistence
* Best-effort delivery only

Any middleware features that violate these expectations (retained messages, durable queues, etc.) must be disabled or avoided.

---

## RPC Layer

### Correlation

RPC request/response matching is implemented using a generated correlation id (`CorrelationId`).

* Each outgoing request registers a pending response channel
* Incoming responses are matched by correlation id
* Pending entries are removed on completion

Duplicate responses are tolerated; only the first wins.

---

### Method Dispatch

RPC routing is based on a **logical method name**, carried inside the `Envelope.method` field.

* The transport address handles delivery
* The method handles dispatch

This allows:

* Multiple servers to share a transport address
* Cleaner separation of routing vs semantics
* Future support for method-based load splitting

---

## RpcClient

The client API:

* Creates RPC requests
* Manages correlation and pending responses
* Serializes request payloads
* Deserializes response payloads

The client:

* Owns exactly one transport instance
* Subscribes to a private response address
* Uses async/await for request handling
* Does not require callbacks

Timeouts and retries are intentionally out of scope for the initial release.

---

## RpcServer

The server:

* Owns exactly one transport instance
* Registers method handlers
* Listens for incoming request envelopes
* Dispatches based on `Envelope.method`
* Publishes responses using `reply_to` and `correlation_id`

Handlers are type-erased internally but strongly typed at registration time.

---

## Handler Model

Handlers:

* Receive deserialized request payloads
* Return serialized response payloads
* Are async
* Do not interact with the transport directly

Transport concerns (addresses, correlation, reply topics) are handled by the server runtime, not user handlers.

---

## Error Handling

This crate uses a **typed error model** (`crate::error::Error`) exclusively.

* `anyhow` is not used
* Transport-specific errors are mapped at boundaries
* Errors are meaningful, stable, and composable

Human-readable diagnostics belong in logging, not in error variants.

---

## Non-Goals (Explicit)

This crate intentionally does **not** provide:

* Exactly-once delivery
* Guaranteed ordering
* Durable message replay
* Transactional semantics
* Broker configuration management

It provides a **clean RPC abstraction over imperfect transports**, not a perfect distributed system.

---

## Future Directions

Potential future extensions include:

* Additional transport implementations (RabbitMQ, NATS, etc.)
* Optional response caching for idempotent requests
* Pluggable retry / timeout policies
* Full-duplex client/server unification

None of these are required for the current architecture.

---

## Summary

This architecture prioritizes:

* Clear separation of concerns
* Strong invariants enforced at the correct layer
* Transport neutrality
* Predictable, testable behavior

The memory transport defines the contract.
Other transports conform as best they can.
