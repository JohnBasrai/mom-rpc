# Architecture Design

This document explains the design decisions for mqtt-rpc-rs.

## Problem Statement

MQTT is a lightweight pub/sub protocol widely used in IoT, but it lacks built-in RPC semantics. Every developer using MQTT for request/response patterns must solve the same problems:

1. **Request/response correlation** - Matching responses to requests using correlation IDs
2. **Timeout handling** - Detecting when a request will never get a response
3. **Concurrent request handling** - Processing multiple in-flight requests without blocking
4. **Code duplication** - This logic is reimplemented in every MQTT client/server application

This library extracts that common pattern into a reusable abstraction.

## Design Goals

**Client-side:**
- Send request, get Future that resolves when response arrives
- Automatic correlation ID generation (UUID v4)
- Timeout support per request
- Concurrent requests (multiple in-flight)

**Server-side:**
- Register async handlers for request topics
- Handlers execute concurrently (spawned, not blocking event loop)
- Automatic correlation ID handling
- Response publishing to correct client

**Implementation:**
- Built on `rumqttc` (popular async Rust MQTT client)
- Tokio-based async runtime
- UUID correlation IDs (standard 36-byte string format)
- MQTT v5 native `response_topic` property
- JSON serialization via serde

## Core Architecture

### Request/Response Flow

```
Client:
1. Generate UUID correlation_id
2. Store oneshot::Sender in HashMap<CorrelationId, Sender>
3. Publish to {topic}/request with:
   - MQTT v5 property: response_topic = "responses/{client_id}"
   - JSON payload: { correlation_id, data }
4. Await on oneshot::Receiver

Server:
1. Receive request on {topic}/request
2. Extract correlation_id from JSON
3. Extract response_topic from MQTT v5 properties
4. Spawn handler task (concurrent execution)
5. Handler completes → publish response to response_topic
   - JSON payload: { correlation_id, result }

Client:
1. Receive response on "responses/{client_id}"
2. Extract correlation_id from JSON
3. Find oneshot::Sender in HashMap
4. Send response through channel
5. Remove from HashMap
```

### Topic Convention

```
Request topic:  {topic}/request
Response topic: responses/{client_id}  (client-specific)
```

**Example:**
- Client "gateway-01" subscribes to: `responses/gateway-01`
- Client publishes request to: `sensors/temp/request`
- Server publishes response to: `responses/gateway-01`

### Correlation Strategy

**UUID v4 (standard format):**
- Example: `550e8400-e29b-41d4-a9b6-446655440000`
- Size: 36 bytes as string
- Collision-free across reboots
- Standard, debuggable format

**Why UUID over counter-based IDs:**
- No collision risk on device reboot
- No session management required
- Standard format recognized everywhere
- Easy to debug (copy-paste into logs)

**Trade-off:** 36 bytes vs ~20 bytes for time-initialized counter, but debuggability and simplicity win for v0.1.0.

### MQTT v5 vs v3.1.1

**v0.1.0 uses JSON-encoded response_topic** (works with both MQTT v3.1.1 and v5)

Uses `response_topic` field in JSON payload instead of MQTT v5 property.

**Benefits:**
- Works with both MQTT v3.1.1 and v5
- Simpler implementation with current rumqttc API
- Still maintains clean separation through JSON structure
- Widely compatible with existing MQTT brokers

**Message Format:**
```json
// Request
{
  "correlation_id": "550e8400-...",
  "response_topic": "responses/client-01",
  "a": 5,
  "b": 3
}

// Response
{
  "correlation_id": "550e8400-...",
  "result": 8
}
```

**Future:** MQTT v5 native `response_topic` property support could be added in v0.2.0 if there's demand for it (would require different rumqttc API or version).

## Module Structure (EMBP)

```
src/
├── lib.rs              # Public API exports
├── error.rs            # Error types
├── protocol/           # Shared protocol types
│   ├── mod.rs          # Gateway: exports types
│   ├── correlation.rs  # CorrelationId(Uuid)
│   └── message.rs      # RpcRequest/RpcResponse
├── client/             # RPC client
│   ├── mod.rs          # Gateway: RpcClient
│   └── pending.rs      # In-flight request tracking
└── server/             # RPC server
    ├── mod.rs          # Gateway: RpcServer
    └── handler.rs      # Handler execution
```

**EMBP Principles Applied:**
- Private modules with selective exports via gateway `mod.rs`
- Sibling modules use `super::` imports
- External modules use `crate::module::` imports
- No deep imports bypassing gateways

## Concurrency Model

### Client Concurrency

**Multiple in-flight requests:**
```rust
// HashMap tracks pending requests
HashMap<CorrelationId, oneshot::Sender<Bytes>>
```

**Safe concurrent access:**
- `Arc<Mutex<HashMap>>` for thread-safe pending request tracking
- Each request gets unique UUID correlation_id
- Background task matches responses and sends through oneshot channels

**Timeout handling:**
```rust
tokio::time::timeout(duration, receiver.await)
```

On timeout, HashMap entry is cleaned up and error returned.

### Server Concurrency

**Handler execution:**
```rust
tokio::spawn(async move {
    let response = handler(request).await;
    mqtt.publish(response_topic, response_json).await;
});
```

**Key points:**
- Each request spawns independent task
- Handlers execute concurrently (no blocking)
- No task handle tracking needed (self-contained)
- MQTT eventloop not blocked by slow handlers

## Serialization

**JSON via serde:**
- Human-readable for debugging
- Interop with Go, Python, JavaScript MQTT clients
- Standard serde derive support
- Users define their own request/response types

**Wire format (request):**
```json
{
  "correlation_id": "550e8400-e29b-41d4-a9b6-446655440000",
  "response_topic": "responses/client-01",
  "a": 5,
  "b": 3
}
```

**Wire format (response):**
```json
{
  "correlation_id": "550e8400-e29b-41d4-a9b6-446655440000",
  "result": 8
}
```

**Note:** correlation_id is at JSON root level for easy parsing by hand-coded implementations.

## Error Handling

```rust
pub enum Error {
    Timeout,                           // Request timed out
    ConnectionLost,                    // MQTT disconnected
    Serialization(serde_json::Error),  // JSON encode/decode failed
    Mqtt(rumqttc::Error),              // MQTT client error
    HandlerNotFound(String),           // No handler registered
}
```

**Error semantics:**
- `Timeout`: Request exceeded configured timeout, HashMap cleaned up
- `ConnectionLost`: MQTT client disconnected, in-flight requests fail
- `Serialization`: User's request/response types failed to serialize
- `Mqtt`: Underlying rumqttc error (publish failed, subscribe failed, etc.)
- `HandlerNotFound`: Server received request for unregistered topic

## API Design

### Client API

```rust
// Create client
let client = RpcClient::new(mqtt_client, "client-01").await?;

// Send request with timeout
let response: Response = client
    .request("sensors/temp", &request)
    .timeout(Duration::from_secs(5))
    .await?;
```

**Design choices:**
- Builder pattern for timeout (optional, ergonomic)
- Generic over request/response types (serde bounds)
- Returns `RequestFuture` that can be awaited or timed out

### Server API

```rust
// Create server
let mut server = RpcServer::new(mqtt_client).await?;

// Register handler
server.handle("sensors/temp", |req: Request| async move {
    // Handler logic
    Ok(Response { value: 25 })
}).await?;

// Run event loop
server.run().await?;
```

**Design choices:**
- Handler is `Fn + Send + 'static` for spawning
- Generic over request/response types (serde bounds)
- `run()` is blocking event loop (typical server pattern)

## Scope and Non-Goals

**In scope:**
- Request/response RPC over MQTT
- Correlation ID management
- Timeout handling
- Concurrent request/response handling

**Out of scope (v0.1.0):**
- Async telemetry/events (pure pub/sub, not RPC)
- Message queuing or persistence
- Load balancing or connection pooling
- Metrics and observability hooks
- MQTT v3.1.1 support (response_topic in JSON)

**Future considerations:**
- MQTT v3.1.1 (encode response_topic in payload)
- Metrics/tracing hooks
- Connection pooling
- Binary serialization options (MessagePack, CBOR)

## Testing Strategy

**Unit tests:**
- Correlation ID generation and uniqueness
- Message serialization/deserialization
- Error type conversions

**Integration tests:**
- Full client ↔ server roundtrip with real mosquitto
- Timeout behavior (request never answered)
- Concurrent requests (multiple in-flight)
- Handler execution and spawning
- Connection loss scenarios

**Examples:**
- Math RPC (add/multiply) - pedagogical, shows API usage
- Future: IoT device control (valve, sensor) - realistic use case

## Design Rationale

### Why UUID over compact IDs?

**Decision:** Use standard UUID v4 (36 bytes) instead of custom compact formats.

**Reasoning:**
- Debuggability: Copy-paste into logs, easily recognizable
- Simplicity: No session management, no counter wrapping
- Standard: Works everywhere, interop with other languages
- Collision-free: Safe across reboots without extra logic

**Trade-off:** 36 bytes vs ~20 bytes for custom formats, but debuggability and simplicity justify the cost for v0.1.0.

### Why JSON-encoded response_topic?

**Decision:** Encode `response_topic` in JSON payload rather than using MQTT v5 property.

**Reasoning:**
- Works with both MQTT v3.1.1 and v5 (broader compatibility)
- Simpler implementation with current rumqttc 0.25 API
- Still maintains clean JSON structure
- Most existing MQTT RPC implementations use this approach
- Easy to debug (response_topic visible in payload)

**Trade-off:** Slightly larger payload (vs MQTT v5 property), but JSON serialization already chosen for its readability, so consistency wins.

### Why JSON?

**Decision:** Use JSON serialization via serde.

**Reasoning:**
- Human-readable for debugging
- Interop with non-Rust MQTT clients (Go, Python, JavaScript)
- Standard, well-understood format
- Easy to implement hand-coded correlation matching

**Trade-off:** Larger than binary formats (MessagePack, CBOR), but readability and interop justify the cost for v0.1.0.

### Why tokio-only?

**Decision:** Require tokio runtime (no sync API, no no_std).

**Reasoning:**
- Target: Edge gateways with Linux (rust-edge-agent use case)
- Simplicity: One runtime, one event loop model
- `rumqttc` async API is tokio-based
- Most MQTT RPC use cases have tokio available

**Trade-off:** Excludes bare-metal/RTOS systems (Hubris, Embassy), but that's a different tier. Can add sync API later if demand exists (YAGNI).

## Future Enhancements

### v0.2.0: MQTT v3.1.1 Support
- Encode `response_topic` in JSON payload
- Feature flag or runtime detection

### v0.3.0: Metrics and Observability
- Request latency histograms
- In-flight request count
- Error rate tracking
- Optional tracing integration

### v0.4.0: Connection Pooling
- Multiple MQTT connections for load balancing
- Automatic failover
- Connection health monitoring

### v0.5.0: Binary Serialization
- MessagePack or CBOR option
- Feature-gated for bandwidth-constrained scenarios
- Maintain JSON as default for debuggability

## References

- [MQTT v5 Request-Response Pattern](https://www.hivemq.com/blog/mqtt5-essentials-part9-request-response-pattern/)
- [MQTT v5 Specification](https://docs.oasis-open.org/mqtt/mqtt/v5.0/mqtt-v5.0.html)
- [rumqttc Documentation](https://docs.rs/rumqttc)
- [Explicit Module Boundary Pattern (EMBP)](https://github.com/JohnBasrai/architecture-patterns/blob/main/rust/embp.md)
