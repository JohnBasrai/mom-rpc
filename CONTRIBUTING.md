# Contributing to mom-rpc

Thanks for considering contributing!

## Quick Start

This is a pure Rust library crate providing RPC semantics over message-oriented middleware (MOM). It supports multiple transport backends including in-memory (for testing) and MQTT. No cross-compilation or embedded targets required.

### Cross-Compilation

This library is cross-platform and suitable for embedded/edge deployments. Cross-compilation and platform-specific concerns are handled at the consuming crate level. Once this crate stabilizes, [rust-edge-agent](https://github.com/JohnBasrai/rust-edge-agent) will be refactored to use mom-rpc as a reference example for cross-compiled projects.

## Local Development

### Running the Memory Transport Example

The quickest way to get started is with the in-memory transport (no broker needed):

```bash
cargo run --example math_memory
```

This example demonstrates the full RPC lifecycle with client and server in a single process.

### Running Unit Tests

```bash
# Run all unit tests
cargo test --lib

# Run all tests including integration tests
cargo test
```

### Testing Feature Combinations

This crate supports optional features. Test all combinations:

```bash
# Test with MQTT transport enabled
cargo build --features transport_mqttac

# Test minimal build (no default features)
cargo build --no-default-features

# Test with all features
cargo build --all-features
```

### Working with Broker-Based Transports

If you're developing or testing a broker-based transport (MQTT, AMQP, etc.):

```bash
# Start your broker (example for MQTT/mosquitto)
./scripts/start-mosquitto.sh

# Terminal 1: Start server
cargo run --example math_server --features transport_mqttac

# Terminal 2: Send requests
cargo run --example math_client --features transport_mqttac
```

**Note:** The `math_server` and `math_client` examples require an external broker and cannot use the in-memory transport.

### Before Submitting a Pull Request

Run local CI scripts:

```bash
./scripts/ci-lint.sh   # Format and clippy
./scripts/ci-test.sh   # Run all tests
```

- Keep commits focused and descriptive
- Update `CHANGELOG.md` under [Unreleased] if behavior changes
- Add tests for new features

We follow [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Semantic Versioning](https://semver.org/).

## Code Formatting

This project uses `rustfmt` for consistent code formatting.

### Visual Separators

Since `rustfmt` removes blank lines at the start of impl blocks, function bodies, and module blocks, we use comment separators `// ---` for visual clarity:

```rust
// Use blocks
use crate::{
    // ---
    Address,
    Envelope,
    PublishOptions,
    Result,
};

// Struct definitions
struct MemoryTransport {
    // ---
    subscriptions: RwLock<HashMap<Subscription, Vec<mpsc::Sender<Envelope>>>>,
    transport_id: String,
}

// Impl blocks
#[async_trait::async_trait]
impl Transport for MemoryTransport {
    // ---
    fn transport_id(&self) -> &str {
        self.transport_id.as_str()
    }

    async fn publish(&self, env: Envelope, _opts: PublishOptions) -> Result<()> {
        // ---
        let subs = self.subscriptions.read().await;

        for (sub, senders) in subs.iter() {
            if sub.0 == env.address.0 {
                for sender in senders {
                    sender.send(env.clone()).await?;
                }
            }
        }

        Ok(())
    }
}

// Function definitions
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    let transport_id = config.transport_id.clone();
    
    let transport = MemoryTransport {
        // ---
        transport_id,
        subscriptions: RwLock::new(HashMap::new()),
    };

    Ok(Arc::new(transport))
}

// Test modules
#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[tokio::test]
    async fn test_subscription_delivery() {
        // ---
        // test body
    }
}
```

**Style Guidelines:**
1. Use `// ---` for visual separation in **use blocks**, **impl blocks**, **struct definitions**, and **function bodies**
2. Place separators after the opening brace and before the first meaningful line
3. Use between meaningful steps of logic processing within function bodies
4. Do NOT use separators between fields in struct literals
5. Keep separators consistent across the codebase

## Documentation and Doc Comments

This project follows a **production-grade documentation standard** for Rust code.

### Required Doc Comments

Use Rust doc comments (`///`) for:

- Public structs and enums (especially `RpcClient`, `RpcServer`, message types)
- Public functions (especially `request()`, `handle()`, `run()`)
- Public modules that define architectural boundaries
- Critical system behavior (correlation matching, timeout handling, handler spawning)

Doc comments should describe **intent, guarantees, and failure semantics** — not restate what the code obviously does.

### RPC/Messaging-Specific Documentation

For RPC and messaging code, doc comments should explicitly describe:

- **Failure modes** - What happens on timeout, connection loss, transport unavailable?
- **Message flow** - Which part of request/response cycle is this?
- **Concurrency** - Can multiple requests be in-flight? How are handlers executed?
- **Correlation semantics** - How are requests matched to responses?
- **Transport behavior** - Does this depend on specific transport semantics?

Example:
```rust
/// Sends an RPC request and returns a Future that resolves when the response arrives.
///
/// This method generates a unique correlation ID, subscribes to the response address
/// if not already subscribed, and publishes the request to the target service.
///
/// # Behavior
///
/// - Correlation ID: UUID v4 (36-byte string format)
/// - Response address: `{service_name}/responses/{client_id}`
/// - Request published to: `{service_name}/{method}`
/// - Concurrent requests: Supported via internal HashMap tracking
///
/// # Errors
///
/// Returns an error if:
/// - Transport is disconnected
/// - Request serialization fails
/// - Publish fails
///
/// # Timeouts
///
/// Timeouts are handled at the application layer. Consider using
/// `tokio::time::timeout()` when awaiting the returned future.
pub async fn request<Req, Resp>(&self, service: &str, method: &str, payload: &Req) 
    -> Result<Resp>
where
    Req: Serialize,
    Resp: DeserializeOwned,
{
    // ---
    // implementation
}
```

### Optional (Encouraged) Doc Comments

Doc comments are encouraged for:

- Internal functions with concurrency implications
- Correlation ID generation and matching logic
- Handler registration and execution
- Timeout and error handling

### Not Required

Doc comments are not required for:

- Trivial helpers
- Simple getters or obvious pass-through functions
- Test code (assert messages should be sufficient)

### General Guidance

- Prefer documenting *why* over *how*
- Be explicit about failure behavior and recovery
- Keep comments accurate and up to date
- Avoid over-documenting trivial code
- For RPC patterns, describe correlation and timeout semantics clearly

## Architecture Guidelines

This project uses the [Explicit Module Boundary Pattern (EMBP)](https://github.com/JohnBasrai/architecture-patterns/blob/main/rust/embp.md) for module organization. Please review the EMBP documentation before making structural changes.

### Key EMBP Principles

- Each module's public API is defined in its `mod.rs` gateway file
- Sibling modules import from each other using `super::`
- External modules import through `crate::module::`
- Never bypass module gateways with deep imports

### mom-rpc Module Structure

```
src/
├── lib.rs            # Public API exports
├── error.rs          # Error types
├── rpc_config.rs     # Configuration
├── correlation.rs    # Correlation ID handling
├── domain/           # Domain types and traits
│   ├── mod.rs        # Gateway: Transport trait, core types
│   └── transport.rs  # Transport abstraction
├── client/           # RPC client
│   ├── mod.rs        # Gateway: RpcClient
│   └── pending.rs    # In-flight request tracking
├── server/           # RPC server
│   ├── mod.rs        # Gateway: RpcServer
│   └── handler.rs    # Handler execution
└── transport/        # Transport implementations
    ├── mod.rs        # Transport factory
    ├── memory/       # In-memory transport (always available)
    │   ├── mod.rs
    │   └── transport.rs
    └── mqtt_async_client/  # MQTT transport (optional feature)
        ├── mod.rs
        └── transport.rs
```

## Test Coverage

This project uses a **layered testing approach** with memory transport as the primary test backend.

### Current Test Strategy

**Unit Tests (Primary):**
- Transport-agnostic RPC behavior using in-memory transport
- Correlation ID generation and matching
- Message serialization/deserialization
- Error type conversions
- Concurrent request handling
- Handler execution and response matching

**Integration Tests:**
- Full client ↔ server roundtrips with memory transport
- Timeout behavior and error handling
- Multiple concurrent clients
- Handler spawning and lifecycle

**Broker-Based Tests (Optional):**
- Real MQTT broker integration (when `transport_mqttac` feature is enabled)
- Network failure scenarios
- Cross-process communication

### When to Add Tests

**Add unit tests when:**
- Adding new RPC patterns or features
- Changing correlation or timeout logic
- Modifying handler execution behavior
- Complex logic needs isolated testing

**Add integration tests when:**
- Testing full request/response cycles
- Validating multi-client scenarios
- Testing edge cases difficult to trigger via unit tests

**Add broker-based tests when:**
- Implementing a new transport backend
- Testing transport-specific failure modes
- Validating network behavior

### Test Organization

```
tests/
  integration.rs           # Full roundtrip tests
  transport_memory.rs      # Memory transport specific tests

scripts/
  ci-lint.sh              # Formatting and clippy
  ci-test.sh              # All tests + examples
  start-mosquitto.sh      # Start MQTT broker (for MQTT transport testing)
```

**Running tests:**
```bash
# Linting
./scripts/ci-lint.sh

# All tests (memory transport, no broker needed)
cargo test

# Test specific features
cargo test --features transport_mqttac

# Just unit tests
cargo test --lib

# Just integration tests
cargo test --test integration
```

## Testing RPC Behavior

When testing RPC flows:

- Test both successful and error responses
- Verify timeout handling
- Test concurrent requests from same client
- Test multiple clients simultaneously
- Include tests for transport disconnection scenarios
- Validate correlation ID matching with multiple in-flight requests
- For new transports: verify semantics match memory transport reference implementation

## Dependencies

When adding dependencies:

- Prefer well-maintained crates with recent updates
- Check for tokio compatibility
- Avoid breaking changes to public API
- Update CI scripts if new system dependencies are needed
