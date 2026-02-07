# Testing Strategy

This project uses a **layered testing approach** with memory transport as the primary test backend.

## Test Layers

### Unit Tests (Primary)

Transport-agnostic RPC behavior using in-memory transport:

- Correlation ID generation and matching
- Message serialization/deserialization
- Error type conversions
- Concurrent request handling
- Handler execution and response matching

**Location:** Within module files (`#[cfg(test)] mod tests`)

### Integration Tests

Full client ↔ server roundtrips with memory transport:

- Full request/response cycles
- Timeout behavior and error handling
- Multiple concurrent clients
- Handler spawning and lifecycle

**Location:** `tests/` directory

### Broker-Based Tests (Optional)

Real MQTT broker integration (when `transport_rumqttc` feature is enabled):

- Network failure scenarios
- Cross-process communication
- Transport-specific behavior

**Location:** `tests/` directory with feature gates

## When to Add Tests

### Add Unit Tests When:

- Adding new RPC patterns or features
- Changing correlation or timeout logic
- Modifying handler execution behavior
- Complex logic needs isolated testing

### Add Integration Tests When:

- Testing full request/response cycles
- Validating multi-client scenarios
- Testing edge cases difficult to trigger via unit tests

### Add Broker-Based Tests When:

- Implementing a new transport backend
- Testing transport-specific failure modes
- Validating network behavior

## Test Organization

```
tests/
  integration.rs           # Full roundtrip tests
  transport_memory.rs      # Memory transport specific tests

scripts/
  ci-lint.sh              # Formatting and clippy
  ci-test.sh              # All tests + examples
  start-mosquitto.sh      # Start MQTT broker
```

## Running Tests

```bash
# Linting
./scripts/ci-lint.sh

# All tests (memory transport, no broker needed)
cargo test

# Test specific features
cargo test --features transport_rumqttc

# Just unit tests
cargo test --lib

# Just integration tests
cargo test --test integration

# Specific test
cargo test test_concurrent_requests
```

## Testing RPC Behavior

When testing RPC flows, ensure coverage of:

- **Success paths** - Normal request/response
- **Error responses** - Handler returns errors
- **Timeout handling** - Requests that don't complete
- **Concurrent requests** - Multiple in-flight from same client
- **Multiple clients** - Simultaneous clients to same server
- **Transport disconnection** - What happens when transport fails
- **Correlation matching** - Multiple in-flight requests don't mix

## Example Test Structure

```rust
#[tokio::test]
async fn test_concurrent_requests() {
    // Setup
    let transport = create_memory_transport().await.unwrap();
    let server = RpcServer::with_transport(transport.clone(), "test");
    let client = RpcClient::with_transport(transport.clone(), "client").await.unwrap();
    
    // Register handler
    server.register("echo", |req: String| async move {
        Ok(req)
    });
    
    // Test concurrent requests
    let req1 = client.request("test", "echo", "msg1".to_string());
    let req2 = client.request("test", "echo", "msg2".to_string());
    
    let (resp1, resp2) = tokio::join!(req1, req2);
    
    // Verify
    assert_eq!(resp1.unwrap(), "msg1");
    assert_eq!(resp2.unwrap(), "msg2");
}
```

## Test Guidelines

- **Use descriptive test names** - `test_concurrent_requests_from_single_client`
- **Test one behavior** - Each test should verify one thing
- **Arrange-Act-Assert** - Clear test structure
- **Clean up** - Shutdown servers, close transports
- **Document edge cases** - Comment why a test exists
- **Use assert messages** - Explain failures: `assert_eq!(x, y, "Correlation ID mismatch")`

## For New Transports

When implementing a new transport:

1. Copy memory transport test structure
2. Replace transport initialization
3. Verify all RPC semantics match memory transport
4. Add transport-specific failure mode tests
5. Gate tests with feature flag

Reference implementation: Memory transport (`src/transport/memory/`)

## Dependencies in Tests

Test dependencies in `Cargo.toml`:

```toml
[dev-dependencies]
tokio-test = "0.4"
futures = "0.3"
```

Keep test dependencies minimal and well-maintained.
