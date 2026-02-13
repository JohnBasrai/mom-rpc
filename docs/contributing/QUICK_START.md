# Quick Start Guide

This is a pure Rust library crate providing RPC semantics over message-oriented middleware (MOM). It supports multiple transport backends including in-memory (for testing), MQTT, AMQP, and DDS.

## Running Examples

### Memory Transport (No Broker Needed)

The quickest way to get started:

```bash
cargo run --example sensor_memory
```

This example demonstrates the full RPC lifecycle with client and server in a single process.

### Transport-Specific Testing (Automated)

Use the manual test scripts which handle broker setup, example execution, and cleanup:

```bash
# MQTT transport (rumqttc)
./scripts/manual-tests/mqtt.sh transport_rumqttc

# AMQP transport (lapin)
./scripts/manual-tests/amqp.sh transport_lapin

# DDS transport (dust_dds)
./scripts/manual-tests/dds.sh transport_dust_dds
```

**Requirements:** Docker (for MQTT and AMQP tests)

These scripts automatically:
- Start the required broker in Docker (MQTT/AMQP) or verify multicast (DDS)
- Build and run sensor_server in background
- Run sensor_client and validate output
- Clean up processes and containers

### Manual Broker Testing

```bash
# Terminal 1: Start MQTT broker
docker run -p 1883:1883 eclipse-mosquitto

# Terminal 2: Start server
cargo run --example sensor_server --features transport_rumqttc

# Terminal 3: Send requests
cargo run --example sensor_client --features transport_rumqttc
```

## Running Tests

```bash
# Run all unit tests
cargo test --lib

# Run integration tests
cargo test --test '*'

# Run all tests
cargo test
```

## Testing Feature Combinations

This crate supports optional features:

```bash
# Test with MQTT transport
cargo build --features transport_rumqttc

# Test with AMQP transport
cargo build --features transport_lapin

# Test with DDS transport
cargo build --features transport_dust_dds

# Test minimal build (no default features)
cargo build --no-default-features

# Test with all features
cargo build --all-features
```

## Cross-Compilation

This library is cross-platform and suitable for embedded/edge deployments. Cross-compilation and platform-specific concerns are handled at the consuming crate level.

## Next Steps

- See [scripts/manual-tests/README.md](../../scripts/manual-tests/README.md) for transport testing details
- See [Local Testing](LOCAL_TESTING.md) for CI workflows
- See [Testing Strategy](TESTING.md) for when to add tests
- See [Code Style](CODE_STYLE.md) for formatting conventions
