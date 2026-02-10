# Quick Start Guide

This is a pure Rust library crate providing RPC semantics over message-oriented middleware (MOM). It supports multiple transport backends including in-memory (for testing) and MQTT.

## Running Examples

### Memory Transport (No Broker Needed)

The quickest way to get started:

```bash
cargo run --example sensor_memory
```

This example demonstrates the full RPC lifecycle with client and server in a single process.

### MQTT Transport (Requires Broker)

```bash
# Terminal 1: Start MQTT broker
./scripts/start-mosquitto.sh

# Terminal 2: Start server
cargo run --example sensor_server --features transport_rumqttc

# Terminal 3: Send requests
cargo run --example sensor_client --features transport_rumqttc
```

**Note:** The `sensor_server` and `sensor_client` examples require an external broker and cannot use the in-memory transport.

## Running Tests

```bash
# Run all unit tests
cargo test --lib

# Run all tests including integration tests
cargo test
```

Also browse the testing scripts in `scripts/` dirctory.

## Testing Feature Combinations

This crate supports optional features:

```bash
# Test with MQTT transport enabled
cargo build --features transport_rumqttc

# Test minimal build (no default features)
cargo build --no-default-features

# Test with all features
cargo build --all-features
```

## Cross-Compilation

This library is cross-platform and suitable for embedded/edge deployments. Cross-compilation and platform-specific concerns are handled at the consuming crate level.

## Next Steps

- See [Local Testing](LOCAL_TESTING.md) for CI workflows
- See [Testing Strategy](TESTING.md) for when to add tests
- See [Code Style](CODE_STYLE.md) for formatting conventions
