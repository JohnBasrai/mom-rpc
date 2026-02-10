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

Full client <--> server roundtrips with memory transport:

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
  ci-docs.sh                # Checks documentation code blocks
  ci-lint.sh                # Formatting and clippy
  ci-test.sh                # All tests + examples
  local-test.sh             # Local Testing Suite (mirrors CI)
  pre-publish.sh            # Checks if ready to publish to crates.io
  start-mosquitto.sh        # Start mosquitto MQTT broker for testing
  manual-tests/amqp.sh      # AMQP Manual Integration Test
  manual-tests/mqtt.sh      # MQTT Manual Integration Test
```

## Running Tests

```bash
# Linting
./scripts/ci-lint.sh

# All tests (memory transport, no broker needed)
./scripts/local-test.sh

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

|  Test Topic                 | What is tested          |
|:----------------------------|:------------------------|
| **Success paths**           | Normal request/response |
| **Error responses**         | Handler returns errors  |
| **Timeout handling**        | Requests that don't complete |
| **Concurrent requests**     | Multiple in-flight from same client |
| **Multiple clients**        | Simultaneous clients to same server |
| **Transport disconnection** | What happens when transport fails |
| **Correlation matching**    | Multiple in-flight requests don't mix |

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

Reference implementation: Memory transport (`src/transport/memory.rs`)

## Dependencies in Tests

Test dependencies in `Cargo.toml`:

```toml
[dev-dependencies]
anyhow       = "1.0"
env_logger   = "0.11"
futures      = "0.3.31"
tokio        = { version = "1", features = ["signal"] }
tokio-test   = "0.4"

```

Keep test dependencies minimal and well-maintained.
