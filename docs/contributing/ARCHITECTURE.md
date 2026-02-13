# Architecture Guidelines

This project uses the [Explicit Module Boundary Pattern (EMBP)](https://github.com/JohnBasrai/architecture-patterns/blob/350e668f/rust/embp.md) for module organization.

**Please review the EMBP documentation before making structural changes.**

## Key EMBP Principles

- Each module's public API is defined in its `mod.rs` gateway file
- Sibling modules import from each other using `super::`
- External modules import through `crate::module::`
- Never bypass module gateways with deep imports

## mom-rpc Module Structure

```
src/
├── lib.rs              # Public API exports
├── error.rs            # Error types
├── rpc_config.rs       # Configuration
├── correlation.rs      # Correlation ID handling
├── domain/             # Domain types and traits
│   ├── mod.rs          # Gateway: Transport trait, core types
│   └── transport.rs    # Transport abstraction
├── client/             # RPC client
│   ├── mod.rs          # Gateway: RpcClient
│   ├── pending.rs      # In-flight request tracking
│   └── rpc_client.rs   # RpcClient implementation
├── server/             # RPC server
│   ├── mod.rs          # Gateway: RpcServer
│   ├── rpc_server.rs   # RpcServer implementation
│   └── handler.rs      # Handler execution
└── transport/          # Transport implementations
    ├── mod.rs          # Top-level gateway, exports all transports
    ├── memory.rs       # In-memory transport (always available)
    ├── mqtt/           # MQTT protocol transports
    │   ├── mod.rs      # Protocol gateway (EMBP)
    │   └── rumqttc.rs  # MQTT via rumqttc library
    ├── amqp/           # AMQP protocol transports
    │    ├── mod.rs     # Protocol gateway (EMBP)
    │    └── lapin.rs   # AMQP via lapin library
    └── dds/            # AMQP protocol transports
        ├── mod.rs      # Protocol gateway (EMBP)
        ├── dust_dds.rs # DDS via dust_dds
        └── README.md   # DDS design notes
```

## Transport Organization

Transports are organized by **protocol → library** hierarchy:

- **Protocol level** (`mqtt/`, `amqp/`, etc.) - Contains gateway `mod.rs` and library implementations
- **Library level** (`rumqttc.rs`, `lapin.rs`) - Single-file transport implementations
- **Memory transport** (`memory.rs`) - Flat at top level (signals special/default status)

**Why this structure?**
- Allows multiple implementations per protocol
- Clear separation between protocols
- Feature names follow library convention (`transport_rumqttc`, `transport_lapin` ⋯ )
- EMBP maintained at protocol level

## Import Guidelines

For import guidelines see the _EMBP documentation_ linked at the top of this document.

## Module Responsibilities

### domain/
- Defines core abstractions: `Transport` trait
- Core domain types: `Envelope`, `Address`, `Subscription`
- No business logic, only contracts

### client/
- `RpcClient` - Send requests, receive responses
- Manages correlation IDs and pending requests
- Handles response routing

### server/
- `RpcServer` - Receive requests, dispatch to handlers
- Handler registration and execution
- Response publishing

### transport/
- Transport implementations
- Memory transport (testing/default)
- Protocol-based transports (MQTT, AMQP, etc.)
- Each protocol may have multiple library implementations

## Adding New Modules

When adding a new module:

1. Create module directory with `mod.rs`
2. Define public API in `mod.rs`
3. Keep implementation details in separate files
4. Export only what's needed via `mod.rs`
5. Document module purpose in `mod.rs`

## Adding New Transport

Transports are organized by **protocol → library**. Use an existing transport as a template — `lapin` was derived directly from `rumqttc`, which is the recommended starting point. This approach structurally enforces the reference semantics rather than requiring the implementer to reconstruct them from scratch.

### Adding First Implementation for a New Protocol

The steps below assume you are adding a new kafka transport.  Substitute your actual name for kafka in these steps.

**Example:** See issue #38 and commit `eb52cf9` for a complete working example of adding the DDS transport via dust_dds.

1. Create protocol directory: `src/transport/kafka/`
2. Create protocol gateway:   `src/transport/kafka/mod.rs`
3. Create new transport implementation: `src/transport/kafka/rdkafka.rs`
4. Update Cargo.toml / add feature flag: `transport_rdkafka = ["rdkafka"]`
5. Update transport-level gateway: `src/transport/mod.rs`
6. Update lib.rs `src/lib.rs`
7. Update tests `scripts/local-test.sh`
8. Go back and finish the implementation begun in step 3.
9. Add new feature to the test matrix in `ci.yml`

---

**Detailed instructions for each step:**

1,2. Create protocol directory and gateway (`src/transport/kafka/mod.rs`):

```rust
//! Kafka protocol transports.
//!
//! This module contains transport implementations for Kafka brokers.
//! Currently supports:
//! - rdkafka - Kafka via rdkafka library (rdkafka.rs)

#[cfg(feature = "transport_rdkafka")]
mod rdkafka;

#[cfg(feature = "transport_rdkafka")]
pub use rdkafka::create_transport as create_rdkafka_transport; // rename to avoid collision
```

3. Create new transport implementation (`src/transport/kafka/rdkafka.rs`):

```rust
//! Kafka transport implementation using rdkafka.

use crate::{Result, RpcConfig, Transport, TransportPtr};

pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // Implementation
}
```

Leave this stub now, you will finish it in step 8.

4. Update Cargo.toml / add feature flag

```
[dependencies]
rdkafka = { version = "0.39", optional = true }
libc    = { version = "0.2",  optional = true } # transitive dependency of rdkafka

[features]
transport_rdkafka = ["rdkafka", "libc"]
```

5. Update transport-level gateway (`src/transport/mod.rs`):

```rust
#[cfg(feature = "transport_rdkafka")]
mod kafka;

#[cfg(feature = "transport_rdkafka")]
pub use kafka::create_rdkafka_transport;
```

6. Update lib.rs `src/lib.rs`
   - Add re-export for the new transport following the pattern of existing transports
   - Update `create_transport()` function with feature guard
   - Update doc comments following the pattern of existing transports

The code below demonstrates the feature guard pattern for `create_transport()`:

```rust
#[cfg(feature = "transport_rdkafka")]
pub use transport::create_rdkafka_transport;

// Update feature guards in this method
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    #[cfg(feature = "transport_rumqttc")]
    { return create_rumqttc_transport(config).await; }

    #[cfg(all(feature = "transport_lapin", not(feature = "transport_rumqttc")))]
    { return create_lapin_transport(config).await; }

    #[cfg(all(feature = "transport_rdkafka",           // <-- NEW
              not(any(feature = "transport_rumqttc",   // <-- NEW
                      feature = "transport_lapin"))))] // <-- NEW
    { return create_rdkafka_transport(config).await; } // <-- NEW

    #[cfg(not(any(feature = "transport_rumqttc",
                  feature = "transport_lapin",
                  feature = "transport_rdkafka")))]    // <-- NEW
    { create_memory_transport(config).await }
}
```

7. Update tests `scripts/local-test.sh`

```
# 2. Feature matrix testing
echo "==> Feature Matrix"
run_test "default features" "default"
run_test "rumqttc" "transport_rumqttc"
run_test "rdkafka" "transport_rdkafka"  # <-- Add rdkafka
run_test "no default features" "no-default-features"
run_test "all features" "all-features"
```

8. Go back and finish the implementation begun in step 3.
9. Edit `.github/workflows/ci.yml`
   In the `test` job, find the `matrix.feature` list.
   Add `transport_kafka` to the list (in alphabetically order).

Study the reference implementation (`lapin` or `rumqttc`) to understand:

- Actor model structure (background task with channel communication)
- Transport trait implementation (publish, subscribe, unsubscribe)
- Error handling and connection management
- Serialization and correlation ID handling

### Adding Second Implementation for Existing Protocol

If a protocol directory already exists (e.g., `mqtt/`), just add the new library:

| Step/Action | Step from previous section  |
|--------------------------------------|----|
| 1. Update protocol gateway           | 2  (file exists)|
| 2. Create transport implementation   | 3  (directory exists)|
| 3. Update Cargo.toml / add feat flag | 4  |
| 4. Update transport-level gateway    | 5  |
| 5. Update src/lib.rs                 | 6  |
| 6. Update tests                      | 7  |
| 7. Finish transport implementation   | 8  |
| 8. Add new feature to `ci.yml`       | 9  |

### Feature Naming Convention

Features are named after the **library**, not the protocol:

| Feature name         | Rationale         | Rust Crate |
|:---------------------|:------------------|------------|
| `transport_rumqttc`  | ✅ (library name) | rumqttc    |
| `transport_dust_dds` | ✅ (library name) | dust_dds   |
| `transport_lapin`    | ✅ (library name) | lapin      |
| `transport_paho`     | ✅ (library name) | paho       |
| `transport_mqtt`     | ❌ (too generic)  | N/A        |
| `transport_amqp`     | ❌ (too generic)  | N/A        |

Note: Protocol is not needed in the feature name because crates.io will enforce uniqueness of library names.
This allows multiple implementations per protocol without naming conflicts.

## Testing Module Structure

Each module can have its own tests:

```rust
#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[test]
    fn test_something() {
        // test
    }
}
```

Integration tests go in `tests/` directory.
