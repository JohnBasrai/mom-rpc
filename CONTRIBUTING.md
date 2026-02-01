# Contributing to mqtt-rpc-rs

Thanks for considering contributing!

## Quick Start

This is a pure Rust library crate for RPC over MQTT. No cross-compilation or embedded targets required.

## Local Development

### Starting MQTT Broker

Before running examples or tests:

```bash
# Start mosquitto (MQTT v5 required)
./scripts/start-mosquitto.sh

# Or manually
mosquitto -v
```

### Running Examples

```bash
# Terminal 1: Start server
cargo run --example math_server

# Terminal 2: Send requests
cargo run --example math_client
```

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
// Module blocks
mod client {
    // ---
    use super::*;

    pub fn send_request() {
        // ---
        // function body
    }
}

// Struct definitions
pub struct RpcClient {
    // ---
    mqtt: AsyncClient,
    pending: Arc<Mutex<HashMap<CorrelationId, Sender>>>,
}

// Impl blocks
impl RpcClient {
    // ---
    pub async fn request<T>(&self, topic: &str, payload: T) {
        // ---
        // implementation
    }
}

// Test modules
#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[tokio::test]
    async fn test_correlation_matching() {
        // ---
        // test body
    }
}
```

**Style Guidelines:**
1. Use `// ---` for visual separation in **module blocks**, **impl blocks**, **struct definitions**, and **function bodies**
2. Place separators after the opening brace and before the first meaningful line
3. Between meaningful steps of logic processing
4. Do NOT use separators inside struct literals (during construction)
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

For RPC and MQTT code, doc comments should explicitly describe:

- **Failure modes** - What happens on timeout, connection loss, broker unavailable?
- **Message flow** - Which part of request/response cycle is this?
- **Concurrency** - Can multiple requests be in-flight? How are handlers executed?
- **Correlation semantics** - How are requests matched to responses?

Example:
```rust
/// Sends an RPC request and returns a Future that resolves when the response arrives.
///
/// This method generates a unique correlation ID, subscribes to the response topic
/// if not already subscribed, and publishes the request with MQTT v5 response_topic
/// property set.
///
/// # Behavior
///
/// - Correlation ID: UUID v4 (36-byte string format)
/// - Response topic: `responses/{client_id}`
/// - Request published to: `{topic}/request`
/// - Concurrent requests: Supported via internal HashMap tracking
///
/// # Errors
///
/// Returns an error if:
/// - MQTT client is disconnected
/// - Request serialization fails
/// - Publish fails
///
/// # Timeouts
///
/// Use `.timeout()` method on returned `RequestFuture` to add timeout handling.
pub async fn request<Req, Resp>(&self, topic: &str, payload: &Req) 
    -> Result<RequestFuture<Resp>>
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

### mqtt-rpc-rs Module Structure

```
src/
├── lib.rs            # Public API exports
├── error.rs          # Error types
├── protocol/         # Correlation IDs and message types
│   ├── mod.rs        # Gateway
│   ├── correlation.rs
│   └── message.rs
├── client/           # RPC client
│   ├── mod.rs        # Gateway: RpcClient
│   └── pending.rs    # In-flight request tracking
└── server/           # RPC server
    ├── mod.rs        # Gateway: RpcServer
    └── handler.rs    # Handler execution
```

## Test Coverage

This project uses a **layered testing approach**:

### Current Test Strategy

**Integration Tests:**
- Full client ↔ server roundtrips with real MQTT broker
- Timeout behavior and error handling
- Concurrent request handling
- Handler execution and response matching

**Unit Tests:**
- Correlation ID generation
- Message serialization/deserialization
- Error type conversions

### When to Add Tests

**Add integration tests when:**
- Adding new RPC patterns
- Changing correlation or timeout logic
- Modifying handler execution behavior

**Add unit tests when:**
- Complex logic needs isolated testing
- Edge cases are difficult to trigger via integration tests

### Test Organization

```
tests/
  integration.rs        # Full roundtrip tests with mosquitto

scripts/
  ci-lint.sh            # Formatting and clippy
  ci-test.sh            # All tests + examples
  start-mosquitto.sh    # Start MQTT broker for testing
```

**Running tests:**
```bash
# Linting
./scripts/ci-lint.sh

# All tests (requires mosquitto on localhost:1883)
./scripts/ci-test.sh

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
- Include tests for connection loss scenarios
- Validate correlation ID matching with multiple in-flight requests

## Dependencies

When adding dependencies:

- Prefer well-maintained crates with recent updates
- Check for tokio compatibility
- Avoid breaking changes to public API
- Update CI scripts if new system dependencies are needed
