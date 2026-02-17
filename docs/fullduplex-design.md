# Unified Broker API Design (0.8.0)

**Status:** Draft  
**Target Release:** 0.8.0  
**Author:** John Basrai  
**Date:** 2026-02-17

## Executive Summary

Redesign the public API to use a unified `RpcBroker` type that replaces both `RpcClient` and `RpcServer`. Mode (client/server/full-duplex) is inferred from queue subscriptions, eliminating the need for separate types and making full-duplex usage natural.

## Goals

1. **Unified API** - Single `RpcBroker` type for all use cases
2. **Mode inference** - Deduce client/server/full-duplex from queue configuration
3. **Cleaner configuration** - Separate transport concerns from broker concerns
4. **Better builder pattern** - Clear separation between `TransportBuilder` and `RpcBrokerBuilder`
5. **Preserve type safety** - Runtime validation for mode-incompatible operations

## Current API Problems (0.7.x)

### Problem 1: Separate Client/Server Types
```rust
// Two separate types for essentially the same underlying mechanism
let client = RpcClient::with_transport(transport, "client-1", config).await?;
let server = RpcServer::with_transport(transport, "server-42");
```

**Issues:**
- Full-duplex requires instantiating both types
- Code duplication between client and server implementations
- Conceptual split that doesn't match the underlying transport model

### Problem 2: Confused Configuration
```rust
let config = RpcConfig::with_broker("mqtt://...", "client")
    .with_retry(RetryConfig::default());  // Client concern
    
let transport = create_transport(&config).await?;  // Uses transport settings

let client = RpcClient::with_transport(transport, "client-1", config).await?;  // Uses retry settings
```

**Issues:**
- `RpcConfig` contains BOTH transport settings (URI, keep-alive) AND client settings (retry, timeout)
- Config used twice - once for transport, once for client
- Not clear which settings apply where
- Poor separation of concerns

### Problem 3: Transport Creation Confusion
```rust
// Feature-flag driven (implicit)
let transport = create_transport(&config).await?;

// OR explicit type selection
let transport = create_transport_for("transport_rumqttc", &config).await?;
```

**Issues:**
- Two different functions for same purpose
- Feature-flag version hides which transport is actually created
- Type-selection version requires string literal (not type-safe)

## Proposed API (0.8.0)

### Core Concepts

**1. TransportBuilder** - Builds transport, knows about queues and URIs
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("sensor-client")
    .response_queue("responses/sensor-client")  // Subscribes to responses
    .build()
    .await?;
```

```rust
let transport = TransportBuilder::new()
    .node_id("mstpd")
    .full_duplex()
    .build()
    .await?;
```

**2. RpcBrokerBuilder** - Builds broker, knows about retry and timeouts
```rust
let broker = RpcBrokerBuilder::new(transport)
    .retry_max_attempts(20)
    .retry_multiplier(2.0)
    .retry_initial_delay(Duration::from_millis(200))
    .retry_max_delay(Duration::from_secs(10))
    .request_timeout(Duration::from_secs(30))
    .build();
```

**3. Mode Inference** - Determined by queue subscriptions
- `request_queue()` only → `Mode::Server`
- `response_queue()` only → `Mode::Client`
- Both → `Mode::FullDuplex`
- Neither → Error at `build()`

### API Examples

#### Client Mode
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("sensor-client")
    .response_queue("responses/sensor-client")  // Client mode
    .build()
    .await?;

let client = RpcBrokerBuilder::new(transport)
    .retry_max_attempts(20)
    .request_timeout(Duration::from_millis(200))
    .build();

// Allowed in client mode
let temp: SensorReading = client
    .request_to("server-42", "read_temp", req)
    .await?;

// Returns Err(RpcError::InvalidMode) - client mode forbids register
client.register("handler", |req| async { Ok(resp) })?;
```

#### Server Mode
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("server-42")
    .request_queue("requests/server-42")  // Server mode
    .build()
    .await?;

let server = RpcBrokerBuilder::new(transport)
    .build();

// Allowed in server mode
server.register("read_temp", |req: ReadTemp| async {
    Ok(SensorReading { value: 21.5, unit: "C".into(), timestamp_ms: 0 })
})?;

server.spawn();

// Returns Err(RpcError::InvalidMode) - server mode forbids request_to
server.request_to("other", "method", req).await?;
```

#### Full-Duplex Mode
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("edge-agent")
    .request_queue("requests/edge-agent")   // First call: Server mode
    .response_queue("responses/edge-agent") // Second call: FullDuplex mode!
    .build()
    .await?;

let broker = RpcBrokerBuilder::new(transport)
    .retry_max_attempts(20)
    .build();

// Both allowed in full-duplex mode
broker.register("handle_command", |req: Command| async {
    Ok(CommandResult { success: true })
})?;

let status = broker.request_to("cloud", "report_status", data).await?;

broker.spawn();
```

#### Explicit Transport Type Selection
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .transport_type("transport_rumqttc")  // Explicit selection
    .node_id("client")
    .response_queue("responses/client")
    .build()
    .await?;
```

**Note:** If `transport_type()` is not called, uses current feature-flag driven logic from `create_transport()`.

## API Surface

### TransportBuilder

```rust
pub struct TransportBuilder {
    // Private fields
}

impl TransportBuilder {
    pub fn new() -> Self;
    
    // Required fields
    pub fn uri(self, uri: impl Into<String>) -> Self;
    pub fn node_id(self, id: impl Into<String>) -> Self;
    
    // Mode-determining fields (at least one required)
    pub fn request_queue(self, queue: impl Into<String>) -> Self;
    pub fn response_queue(self, queue: impl Into<String>) -> Self;
    
    // Optional fields
    pub fn transport_type(self, flag: impl Into<String>) -> Self;
    pub fn keep_alive_secs(self, secs: u16) -> Self;
    
    // Consumes self, validates required fields
    pub async fn build(self) -> Result<TransportPtr>;
}
```

**Validation at `build()`:**
- `uri` is required (returns `RpcError::MissingConfig("uri")`)
- `node_id` is required (returns `RpcError::MissingConfig("node_id")`)
- At least one of `request_queue` or `response_queue` required (returns `RpcError::MissingConfig("queue")`)

### RpcBrokerBuilder

```rust
pub struct RpcBrokerBuilder {
    // Private fields
}

impl RpcBrokerBuilder {
    pub fn new(transport: TransportPtr) -> Self;
    
    // Retry configuration (optional)
    pub fn retry_max_attempts(self, attempts: u32) -> Self;
    pub fn retry_multiplier(self, multiplier: f32) -> Self;
    pub fn retry_initial_delay(self, delay: Duration) -> Self;
    pub fn retry_max_delay(self, delay: Duration) -> Self;
    
    // Timeout configuration (optional, default: 30s)
    pub fn request_timeout(self, timeout: Duration) -> Self;
    
    // Consumes self
    pub fn build(self) -> RpcBroker;
}
```

### RpcBroker

```rust
pub struct RpcBroker {
    // Private fields including mode: BrokerMode
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrokerMode {
    Client,
    Server,
    FullDuplex,
}

impl RpcBroker {
    // Client operations (Mode::Client or Mode::FullDuplex)
    pub async fn request_to<TReq, TResp>(
        &self,
        target_node_id: &str,
        method: &str,
        req: TReq,
    ) -> Result<TResp>
    where
        TReq: Serialize,
        TResp: DeserializeOwned;
    
    // Server operations (Mode::Server or Mode::FullDuplex)
    pub fn register<TReq, TResp, F, Fut>(
        &self,
        method: &str,
        handler: F,
    ) -> Result<()>  // NEW: Returns Result for mode validation
    where
        TReq: DeserializeOwned + Send + 'static,
        TResp: Serialize + Send + 'static,
        F: Fn(TReq) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TResp>> + Send;
    
    // Lifecycle (Mode::Server or Mode::FullDuplex)
    pub fn spawn(self) -> JoinHandle<()>;
    pub async fn run(self);
    pub async fn shutdown(&self);
}
```

**Mode validation:**
- `request_to()` - Returns `Err(RpcError::InvalidMode)` if `Mode::Server`
- `register()` - Returns `Err(RpcError::InvalidMode)` if `Mode::Client`
- `spawn()`/`run()` - Returns `Err(RpcError::InvalidMode)` if `Mode::Client`

## Migration Guide (0.7.x → 0.8.0)

### Client Migration

**Before (0.7.x):**
```rust
let config = RpcConfig::with_broker("mqtt://localhost:1883", "client")
    .with_retry(RetryConfig::default());
let transport = create_transport(&config).await?;
let client = RpcClient::with_transport(transport, "client-1", config).await?;

let temp: SensorReading = client
    .request_to("server", "read_temp", req)
    .await?;
```

**After (0.8.0):**
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("client-1")
    .response_queue("responses/client-1")
    .build()
    .await?;

let client = RpcBrokerBuilder::new(transport)
    .retry_max_attempts(3)
    .retry_multiplier(2.0)
    .retry_initial_delay(Duration::from_millis(100))
    .retry_max_delay(Duration::from_secs(5))
    .build();

let temp: SensorReading = client
    .request_to("server", "read_temp", req)
    .await?;
```

### Server Migration

**Before (0.7.x):**
```rust
let config = RpcConfig::with_broker("mqtt://localhost:1883", "server");
let transport = create_transport(&config).await?;
let server = RpcServer::with_transport(transport, "server-42");

server.register("read_temp", |req: ReadTemp| async {
    Ok(SensorReading { value: 21.5, unit: "C".into(), timestamp_ms: 0 })
});

server.spawn();
```

**After (0.8.0):**
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("server-42")
    .request_queue("requests/server-42")
    .build()
    .await?;

let server = RpcBrokerBuilder::new(transport)
    .build();

server.register("read_temp", |req: ReadTemp| async {
    Ok(SensorReading { value: 21.5, unit: "C".into(), timestamp_ms: 0 })
})?;  // Now returns Result

server.spawn();
```

### Full-Duplex (NEW in 0.8.0)

**Before (0.7.x):** Required instantiating both client and server

**After (0.8.0):**
```rust
let transport = TransportBuilder::new()
    .uri("mqtt://localhost:1883")
    .node_id("edge-agent")
    .request_queue("requests/edge-agent")
    .response_queue("responses/edge-agent")
    .build()
    .await?;

let broker = RpcBrokerBuilder::new(transport)
    .retry_max_attempts(20)
    .build();

// Can both send and receive
broker.register("handle_cmd", handler)?;
let result = broker.request_to("cloud", "report", data).await?;
broker.spawn();
```

## Implementation Notes

### Phase 1: New API Alongside Old (Parallel Development)
- Implement `TransportBuilder`, `RpcBrokerBuilder`, `RpcBroker` as new types
- Keep existing `RpcClient`, `RpcServer`, `RpcConfig` unchanged
- Both APIs functional simultaneously
- Allows incremental migration and testing

### Phase 2: Deprecation Warnings
- Mark old API as `#[deprecated]` with migration guidance
- Update all examples to use new API
- Update documentation

### Phase 3: Removal (0.9.0 or 1.0.0)
- Remove deprecated old API
- Clean up any compatibility shims

### Key Implementation Details

**Mode Detection in TransportBuilder:**
```rust
impl TransportBuilder {
    fn infer_mode(&self) -> Option<BrokerMode> {
        match (self.request_queue.is_some(), self.response_queue.is_some()) {
            (true, true) => Some(BrokerMode::FullDuplex),
            (true, false) => Some(BrokerMode::Server),
            (false, true) => Some(BrokerMode::Client),
            (false, false) => None,  // Error at build()
        }
    }
}
```

**Runtime Mode Validation in RpcBroker:**
```rust
impl RpcBroker {
    pub async fn request_to<TReq, TResp>(&self, ...) -> Result<TResp> {
        match self.mode {
            BrokerMode::Client | BrokerMode::FullDuplex => {
                // Proceed with request
            }
            BrokerMode::Server => {
                return Err(RpcError::InvalidMode(
                    "request_to() not allowed in Server mode".into()
                ));
            }
        }
        // ... actual implementation
    }
    
    pub fn register<...>(&self, method: &str, handler: F) -> Result<()> {
        match self.mode {
            BrokerMode::Server | BrokerMode::FullDuplex => {
                // Proceed with registration
                Ok(())
            }
            BrokerMode::Client => {
                Err(RpcError::InvalidMode(
                    "register() not allowed in Client mode".into()
                ))
            }
        }
    }
}
```

## Open Questions

1. **Default queue names:** Should TransportBuilder provide default queue names based on `node_id`?
   - Pro: Less boilerplate for common case
   - Con: Less explicit, potential for confusion
   - **Decision:** TBD

2. **Retry builder verbosity:** Should we keep individual setters or accept `RetryConfig`?
   ```rust
   // Option A: Individual setters (more verbose)
   broker.retry_max_attempts(20)
         .retry_multiplier(2.0)
         .retry_initial_delay(Duration::from_millis(200))
         ...
   
   // Option B: Accept RetryConfig (less verbose)
   broker.retry(RetryConfig {
       max_attempts: 20,
       multiplier: 2.0,
       initial_delay: Duration::from_millis(200),
       max_delay: Duration::from_secs(10),
   })
   
   // Option C: Both
   broker.retry(RetryConfig::default())  // OR
   broker.retry_max_attempts(20)...
   ```
   - **Decision:** TBD

3. **Transport type selection:** Keep string-based or use enum?
   ```rust
   // String-based (current proposal)
   .transport_type("transport_rumqttc")
   
   // Enum-based (more type-safe)
   .transport_type(TransportType::Rumqttc)
   ```
   - **Decision:** TBD

## Success Criteria

- [ ] Single `RpcBroker` type replaces both `RpcClient` and `RpcServer`
- [ ] Mode automatically inferred from queue configuration
- [ ] Full-duplex usage is natural and well-documented
- [ ] Clear separation between transport and broker concerns
- [ ] All existing examples work with new API
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Migration guide complete and tested

## Timeline

- **Week 1:** Implement `TransportBuilder` and basic mode inference
- **Week 2:** Implement `RpcBroker` and mode validation
- **Week 3:** Migrate examples and tests
- **Week 4:** Documentation and release prep
