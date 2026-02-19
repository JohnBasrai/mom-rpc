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
   │        (RpcBroker)          │
   └───────────────▲─────────────┘
                   │
   ┌───────────────┴─────────────┐
   │         RPC Layer           │
   │                             │
   │  - Correlation handling     │
   │  - Method dispatch          │
   │  - Pending request tracking │
   │  - Retry / timeouts         │
   └───────────────▲─────────────┘
                   │
   ┌───────────────┴─────────────┐
   │      Transport Layer        │
   │                             │
   │  - Publish / Subscribe      │
   │  - Delivery semantics       │
   │  - Addressing               │
   └───────────────▲─────────────┘
                   │
   ┌───────────────┴────────────────┐
   │     Concrete Transports        │
   │                                │
   │  -  memory (reference)         │
   │  -  AMQP   (lapin    )         │
   │  -  DDS    (dust_dds )         │
   │  -  MQTT   (rumqttc  )         │
   │  -  REDIS  (redis    )         │
   │  -   ⋮                         │
   └────────────────────────────────┘
```

Note: Each transport, other than memory, is feature-gated. Applications compile only what they use.

---

## Transport Layer

### Transport Trait

The transport layer is defined by a small trait that supports:

* Publishing an `Envelope`
* Subscribing to an address
* Closing/releasing resources
* Identifying itself via `transport_id()` for logging

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

### Transport Organization

Transports are organized by **protocol → library** hierarchy:

```
transport/
├── amqp/
│   ├── mod.rs
│   └── lapin.rs          # AMQP via lapin
├── dds/
│    ├── mod.rs
│    └── dust_dds.rs      # DDS via dust_dds
└── mqtt/
    ├── mod.rs
    └── rumqttc.rs        # MQTT via rumqttc
```

Future additions follow this same pattern. There may be multiple libraries used for a given transport.


```
└── mqtt/
│   ├── mod.rs
│   ├── rumqttc.rs
│   └── new-library-crate
⋮   ⋮
└── new-protocol
    ⋮
```

This allows multiple implementations per protocol while keeping feature names specific and unambiguous.

---

## Memory Transport (Reference Implementation)

The in-memory transport serves as the **reference implementation** for transport behavior.

It:

* Uses in-process routing via `HashMap` + async channels
* Does not require a broker
* Is always enabled by default
* Is used heavily for integration testing
* Accepts a `transport_id` parameter for debugging/logging

**Important for testing:** Each call to `create_transport(config)` creates an independent transport instance. To share state between client and server in tests, pass the same `TransportPtr` to both.

### Reference Semantics

Each brokered transport was derived from its predecessor, which structurally reinforces behavioral consistency rather than merely aspiring to it. That consistency targets the following reference semantics:

* No message replay on subscribe
* Fanout delivers messages to all subscribers
* No implicit durability or persistence
* Best-effort delivery only

Any middleware features that violate these expectations (retained messages, durable queues, etc.) must be disabled or avoided.

---

## Concrete Transport Implementations

<details>
<summary><strong>Rumqttc Transport (MQTT)</strong></summary>

The `rumqttc` transport adapts MQTT semantics to the transport contract defined by this crate.

### Concurrency model

* A single background actor owns the MQTT `EventLoop`
* All interaction with the MQTT client is serialized through the actor
* No other task touches the event loop directly

This preserves safety while keeping the public `Transport` trait `Send + Sync`.

### Connection behavior

* One broker connection per transport instance
* Connection is lazy and initiated when polling begins
* Connection success or failure is surfaced via logging

### Subscription semantics

* Subscriptions are registered one at a time
* Each subscribe waits for SUBACK confirmation
* Serialization is required because SUBACK packets do not include topic names

### Message delivery

* Incoming publishes are demultiplexed by topic
* Messages are fanned out to all local subscribers
* Delivery is best-effort and non-durable
* No retained-message or replay behavior

</details>

<details>
<summary><strong>Lapin Transport (AMQP)</strong></summary>

The `lapin` transport provides AMQP 0-9-1 support for RabbitMQ and compatible brokers.

### Concurrency model

* A single background actor owns the AMQP connection and channel
* All interaction with the AMQP client is serialized through the actor
* No other task touches the connection directly

This preserves safety while keeping the public `Transport` trait `Send + Sync`.

### Connection behavior

* One broker connection per transport instance
* Connection happens immediately during transport creation (not lazy)
* Connection success or failure is surfaced via logging

### Queue semantics

* Queues are declared with ephemeral options suitable for RPC:
  - `durable: false` - Messages not persisted to disk
  - `auto_delete: true` - Queue deleted when last consumer disconnects
  - `exclusive: false` - Multiple consumers allowed
* Queue names are derived from `transport_id` unless custom names are provided via `RpcConfig`
* Each queue gets a dedicated consumer task for message handling

### Message delivery

* Incoming AMQP messages are demultiplexed by queue name
* Messages are fanned out to all local subscribers for that queue
* Delivery is best-effort and non-durable
* Messages are acknowledged after successful deserialization
* Each subscription registers a new local inbox; multiple subscribers per queue are supported

</details>

<details>
<summary><strong>Dust_dds Transport (DDS)</strong></summary>

The `dust_dds` transport provides DDS/RTPS support for brokerless peer-to-peer communication.

### Concurrency model

* A single background actor owns the DDS `DomainParticipant`, `DataWriter`, and `DataReader` instances
* All interaction with the DDS entities is serialized through the actor
* No other task touches DDS entities directly

This preserves safety while keeping the public `Transport` trait `Send + Sync`.

### Connection behavior

* DDS is brokerless - no external broker required
* Peers discover each other automatically via RTPS multicast
* `DomainParticipant` joins the domain at creation time
* Discovery typically completes in 30-50ms on loopback networks

### Discovery and synchronization

* Uses `WaitSetAsync` with `StatusCondition` to ensure messages aren't lost due to timing races
* Writers wait for `PublicationMatched` status before publishing
* Readers poll for data availability using `WaitSetAsync` (no CPU spin)
* Discovery is fully automatic - no manual configuration required

### Message delivery

* Incoming DDS samples are demultiplexed by topic
* Messages are fanned out to all local subscribers for that topic
* Delivery uses `Reliability::Reliable` QoS (TCP-like with retries)
* No persistence (`Durability::Volatile`) - ephemeral RPC semantics
* `History::KeepLast(1)` - only latest message retained

### QoS Configuration

* **Reliability**: `Reliable`    - Ensures delivery with retries
* **History**:     `KeepLast(1)` - Prevents correlation confusion
* **Durability**:  `Volatile`    - No persistence, ephemeral messages

</details>

---

## RPC Layer

### Correlation

RPC request/response matching is implemented using a generated correlation id (`CorrelationId`).

* Each outgoing request registers a pending response channel
* Incoming responses are matched by correlation id
* Pending entries are removed on completion

Duplicate responses are tolerated; only the first wins.

---

### Method Dispatch and Addressing

RPC routing uses a two-level scheme:

* **Transport address** routes to a specific node: `requests/{node_id}`
* **Method field** (inside `Envelope`) selects the handler

For example:
* Client publishes to address `requests/env-sensor-42`
* Envelope contains `method: "read_humidity"`
* Server at node `env-sensor-42` dispatches to the `read_humidity` handler

This allows:

* Exact-match subscription semantics (compatible with memory transport reference implementation)
* Clean separation between routing (transport address) and dispatch (method name)
* Multiple methods served by a single server without subscription proliferation

---

## RpcBroker

`RpcBroker` is the unified entry point for all RPC operations. It replaces the separate `RpcClient` and `RpcServer` types from 0.7.x.

The broker:

* Owns exactly one transport instance
* Infers its operational mode (client, server, or full-duplex) from the queue configuration supplied to `TransportBuilder`
* Validates operations at runtime against its mode, returning `Err(RpcError::InvalidMode)` for disallowed calls

Mode is determined by which queues were subscribed during transport construction:

* `response_queue` only → `Mode::Client`
* `request_queue` only → `Mode::Server`
* Both queues → `Mode::FullDuplex`

### Client operations (`Mode::Client` or `Mode::FullDuplex`)

* Creates and sends RPC requests via `request_to()`
* Manages correlation IDs and pending responses
* Subscribes to a private response address: `responses/{node_id}`

### Server operations (`Mode::Server` or `Mode::FullDuplex`)

* Registers typed method handlers via `register()`
* Subscribes to incoming request address: `requests/{node_id}`
* Dispatches based on `Envelope.method`
* Publishes responses using `reply_to` and `correlation_id`

Handlers are type-erased internally but strongly typed at registration time.

**Lifecycle:**
1. Build transport via `TransportBuilder` (determines mode)
2. Build broker via `RpcBrokerBuilder::new(transport).build()`
3. Register handlers via `register(method, handler)?` (server/full-duplex)
4. Start processing via `spawn()` or `run()` (server/full-duplex)

**Construction:**
```rust
// Client mode
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("sensor-client")
    .response_queue("responses/sensor-client")
    .build()
    .await?;

let client = RpcBrokerBuilder::new(transport).build();

// Server mode
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("server-42")
    .request_queue("requests/server-42")
    .build()
    .await?;

let server = RpcBrokerBuilder::new(transport).build();
server.register("read_temp", |req: ReadTemp| async {
    Ok(SensorReading { value: 21.5, unit: "C".into(), timestamp_ms: 0 })
})?;
server.spawn();
```

---

## Handler Model

Handlers are async functions with typed request/response payloads:

```rust
server.register("read_temperature", |req: ReadTemperature| async move {
        // ---
        let celsius = 21.5_f32; // Simulate reading hw temperature sensor.
        let (value, unit) = match req.unit {
            TemperatureUnit::Celsius => (celsius, "C"),
            TemperatureUnit::Fahrenheit => (celsius * 9.0 / 5.0 + 32.0, "F"),
        };
        Ok(SensorReading {
            value,
            unit: unit.to_string(),
            timestamp_ms: current_time_ms(),
        })
    });
```

Handlers:

* Receive deserialized request payloads (type: `Req`)
* Return `Result<Resp>` where `Resp` is the response type
* Are async (return a `Future`)
* Do not interact with the transport directly
* Are automatically wrapped with serialization/deserialization logic

Transport concerns (addresses, correlation, reply topics) are handled by the server runtime, not user handlers.

---

This crate uses a **typed error model** (`crate::error::RpcError`) exclusively. This enables callers to handle errors programmatically via pattern matching.

* `anyhow` and `Box<dyn Error>` are not used. Both lose type information.
* Transport-specific errors are converted to `RpcError` variants (e.g., `Transport(String)`) to keep transport types out of the public API
* Error context is preserved as descriptive strings
* Errors are meaningful, stable, and composable

Error variants carry essential context. Detailed diagnostics (stack traces, full error chains) belong in logging.

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

* Additional transport implementations
* Optional response caching for idempotent requests
* Pluggable retry / timeout policies

None of these are required for the current architecture.

---

## Summary

This architecture prioritizes:

* Clear separation of concerns
* Strong invariants enforced at the correct layer
* Transport neutrality
* Predictable, testable behavior

The memory transport defines the contract. Other transports conform as best they can.
